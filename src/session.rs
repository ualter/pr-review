use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use crate::ai_backend::{AiEvent, backend_for_tool};
use crate::{
    artifacts::{AiRunResult, load_review_meta},
    cli::{AiRuntime, ReviewInput},
    markdown_viewer::{
        open_markdown_text, open_markdown_viewer, open_markdown_viewer_at_end,
        print_markdown_document, print_markdown_text,
    },
    ui::{
        BLACK_BOLD, BLUE_BOLD, GREEN_BOLD, LINE, RESET, YELLOW, YELLOW_BOLD,
        print_interactive_help, render_interactive_prompt, start_spinner,
    },
};

pub struct SelectedContext {
    pub review_summary: String,
    pub conversation_summary: String,
    pub relevant_diff: Option<String>,
}

#[allow(dead_code)]
pub struct ReviewSession {
    pub artifact_dir: PathBuf,
    pub conversation_path: PathBuf,
    pub conversation_summary_path: PathBuf,
    pub review_summary_path: PathBuf,
    pub diff_by_file_dir: PathBuf,
}

impl ReviewSession {
    pub fn new(artifact_dir: &Path) -> Result<Self> {
        let conversation_path = artifact_dir.join("conversation.md");
        let conversation_summary_path = artifact_dir.join("conversation-summary.md");
        let review_summary_path = artifact_dir.join("review-summary.md");
        let diff_by_file_dir = artifact_dir.join("diff-by-file");

        if !conversation_path.exists() {
            fs::write(&conversation_path, "# Review Conversation\n\n")?;
        }

        if !conversation_summary_path.exists() {
            fs::write(&conversation_summary_path, "No conversation yet.")?;
        }

        Ok(Self {
            artifact_dir: artifact_dir.to_path_buf(),
            conversation_path,
            conversation_summary_path,
            review_summary_path,
            diff_by_file_dir,
        })
    }

    pub fn append_user_message(&self, message: &str) -> Result<()> {
        append_message(&self.conversation_path, "USER", message)
    }

    pub fn append_ai_message(&self, message: &str) -> Result<()> {
        append_message(&self.conversation_path, "AI", message)
    }
}

pub fn prepare_session_artifacts(
    artifact_dir: &Path,
    input: &ReviewInput,
    review: &str,
    tool: &AiRuntime,
) -> Result<()> {
    let session = ReviewSession::new(artifact_dir)?;

    write_diff_by_file(&session.diff_by_file_dir, &input.diff)?;

    if !session.review_summary_path.exists() {
        let summary = summarize_review(input, review, tool)?;
        fs::write(&session.review_summary_path, summary).with_context(|| {
            format!("Failed to write {}", session.review_summary_path.display())
        })?;
    }

    Ok(())
}

pub fn run_interactive_session(
    input: &ReviewInput,
    artifact_dir: &Path,
    tool: &AiRuntime,
) -> Result<()> {
    let session = ReviewSession::new(artifact_dir)?;
    let mut conversation_summary_dirty = false;

    println!(
        "{GREEN_BOLD}Interactive review session started with {}{RESET}",
        tool
    );

    println!("{BLACK_BOLD}Commands:{RESET}");
    println!("  {YELLOW}/help{RESET}  {YELLOW}/exit{RESET}  {YELLOW}/full{RESET}");
    println!("{BLACK_BOLD}Views:{RESET}");
    println!("  {YELLOW}/summary{RESET}           {YELLOW}/summary-print{RESET}");
    println!("  {YELLOW}/review{RESET}            {YELLOW}/review-print{RESET}");
    println!("  {YELLOW}/review-summary{RESET}    {YELLOW}/review-summary-print{RESET}");
    println!("  {YELLOW}/last [N]{RESET}          {YELLOW}/last-print [N]{RESET}");

    println!(
        "{BLACK_BOLD}Tip:{RESET} Ask follow-up questions naturally — only use {YELLOW}/full{RESET} when the AI needs the complete diff context again."
    );
    println!("{LINE}");

    loop {
        print!("{}", render_interactive_prompt(tool));
        io::stdout().flush()?;

        let mut user_question = String::new();
        io::stdin().read_line(&mut user_question)?;

        let user_question = user_question.trim();

        if user_question.is_empty() {
            continue;
        }

        if let Some(last_count) = parse_last_command(user_question, "/last")? {
            match last_count {
                None => {
                    open_markdown_viewer_at_end(
                        "💬 Last Conversation",
                        &session.conversation_path,
                    )?;
                }
                Some(count) => {
                    let conversation =
                        load_last_conversation_markdown(&session.conversation_path, Some(count))?;
                    let title = format!("💬 Last {count} Conversation Exchanges");
                    open_markdown_text(&title, &conversation, true)?;
                }
            }
            continue;
        }

        if let Some(last_count) = parse_last_command(user_question, "/last-print")? {
            let conversation =
                load_last_conversation_markdown(&session.conversation_path, last_count)?;
            let title = match last_count {
                Some(count) => format!("Last {count} Conversation Exchanges"),
                None => "Last Conversation".to_string(),
            };
            print_markdown_text(&title, &conversation)?;
            continue;
        }

        match user_question {
            "/help" => {
                print_interactive_help(tool);
                continue;
            }

            "/summary" => {
                open_markdown_viewer(
                    "📋 Conversation Summary",
                    &artifact_dir.join("conversation-summary.md"),
                )?;
                continue;
            }
            "/summary-print" => {
                print_markdown_document(
                    "Conversation Summary",
                    &artifact_dir.join("conversation-summary.md"),
                )?;
                continue;
            }

            "/review" => {
                open_markdown_viewer("🧠 Full Review", &artifact_dir.join("review.md"))?;
                continue;
            }
            "/review-print" => {
                print_markdown_document("Full Review", &artifact_dir.join("review.md"))?;
                continue;
            }
            "/review-summary" => {
                open_markdown_viewer("🧠 Review Summary", &artifact_dir.join("review-summary.md"))?;
                continue;
            }
            "/review-summary-print" => {
                print_markdown_document("Review Summary", &artifact_dir.join("review-summary.md"))?;
                continue;
            }

            "/exit" | "exit" => {
                if conversation_summary_dirty {
                    let spinner = start_spinner(
                        tool.status_icon(),
                        format!("{tool} is updating the conversation summary..."),
                    );
                    update_conversation_summary(&session, tool)?;
                    spinner.stop();
                }
                println!("{BLUE_BOLD}{LINE}{RESET}");
                println!(
                    "{}Conversation saved to:{} {}",
                    GREEN_BOLD,
                    RESET,
                    session.conversation_path.display()
                );
                println!("{BLUE_BOLD}{LINE}{RESET}");
                break;
            }

            "/quit" => {
                println!("Use /exit to leave the session.");
                continue;
            }
            "" => continue,
            _ => {
                process_user_question(input, tool, &session, user_question)?;
                conversation_summary_dirty = true;
            }
        }
    }

    Ok(())
}

fn process_user_question(
    input: &ReviewInput,
    tool: &AiRuntime,
    session: &ReviewSession,
    user_question: &str,
) -> Result<(), anyhow::Error> {
    let force_full_diff = user_question == "/full";
    let actual_question = if force_full_diff {
        "Review the whole diff again and tell me if there is anything important we missed."
    } else {
        user_question
    };
    session.append_user_message(actual_question)?;
    let context = select_context_for_question(session, input, actual_question, force_full_diff)?;
    let prompt = build_chat_prompt(input, actual_question, &context);
    let backend = backend_for_tool(tool);
    let spinner_label = format!("{tool} is thinking...");
    let mut spinner = Some(start_spinner(tool.status_icon(), spinner_label.clone()));
    let mut streamed_anything = false;
    let answer = backend.run_review(&prompt, &mut |event| match event {
        AiEvent::TextDelta(chunk) => {
            if let Some(active_spinner) = spinner.take() {
                active_spinner.stop();
                println!();
            }

            streamed_anything = true;
            print!("{chunk}");
            let _ = io::stdout().flush();
        }
        AiEvent::Status(message) => {
            if crate::debug::DEBUG {
                if let Some(active_spinner) = spinner.take() {
                    active_spinner.stop();
                    println!();
                }
                println!("{YELLOW}[debug]{RESET} {message}");
                if !streamed_anything {
                    spinner = Some(start_spinner(tool.status_icon(), spinner_label.clone()));
                }
            } else if tool.shows_live_status_updates()
                && let Some(active_spinner) = spinner.as_ref()
            {
                active_spinner.set_status(message);
            }
        }
        AiEvent::Failed(message) => {
            if crate::debug::DEBUG {
                if let Some(active_spinner) = spinner.take() {
                    active_spinner.stop();
                    println!();
                }
                println!("{YELLOW}[debug]{RESET} failure: {message}");
            } else if tool.shows_live_status_updates()
                && let Some(active_spinner) = spinner.as_ref()
            {
                active_spinner.set_status(format!("failure: {message}"));
            }
        }
        AiEvent::Started | AiEvent::Finished => {}
    })?;

    if let Some(active_spinner) = spinner.take() {
        active_spinner.stop();
    } else if streamed_anything {
        println!();
    }

    if streamed_anything {
        println!();
    }
    session.append_ai_message(&answer)?;
    Ok(())
}

fn select_context_for_question(
    session: &ReviewSession,
    input: &ReviewInput,
    question: &str,
    force_full_diff: bool,
) -> Result<SelectedContext> {
    let review_summary =
        read_or_default(&session.review_summary_path, "No review summary available.")?;

    let conversation_summary = read_or_default(
        &session.conversation_summary_path,
        "No conversation summary available.",
    )?;

    let relevant_diff = if force_full_diff {
        Some(input.diff.clone())
    } else {
        select_relevant_diff_by_file(&session.diff_by_file_dir, question)?
    };

    Ok(SelectedContext {
        review_summary,
        conversation_summary,
        relevant_diff,
    })
}

fn build_chat_prompt(input: &ReviewInput, question: &str, context: &SelectedContext) -> String {
    format!(
        r#"
You are continuing an existing AI-assisted PR/commit review session.

Rules:
- Answer only the user's follow-up question.
- Do not repeat the full review.
- Do not invent missing context.
- If more context is needed, say exactly what file/function/diff chunk is needed.
- Be concise and practical.
- Focus on engineering review quality: correctness, architecture, security, maintainability, tests, deployment risk.

Review metadata:
{}

Review summary:
{}

Conversation summary:
{}

Relevant diff context:
```diff
{}
```

User question:
{}
"#,
        input.metadata,
        context.review_summary,
        context.conversation_summary,
        context
            .relevant_diff
            .as_deref()
            .unwrap_or("No specific diff chunk selected."),
        question
    )
}

fn summarize_review(input: &ReviewInput, review: &str, tool: &AiRuntime) -> Result<String> {
    let prompt = format!(
        r#"
Summarize this AI code review into compact memory for future follow-up questions.

Rules:
- Keep it concise.
- Preserve concrete findings.
- Preserve file/function/resource names.
- Preserve blocking vs non-blocking classification.
- Preserve test recommendations.
- Preserve deployment/migration risks.
- Do not add new findings.

Review metadata:
{}

Full review:
{}
"#,
        input.metadata, review
    );

    Ok(run_ai_tool_silently(tool, &prompt)?.output)
}

fn update_conversation_summary(session: &ReviewSession, tool: &AiRuntime) -> Result<()> {
    let conversation = read_or_default(&session.conversation_path, "")?;

    let prompt = format!(
        r#"
Update the conversation summary for this PR review chat.

Rules:
- Keep only useful long-term context.
- Preserve decisions, clarifications, important conclusions, and unresolved questions.
- Remove casual chatter.
- Keep it compact.

Conversation:
{}
"#,
        conversation
    );

    let summary = run_ai_tool_silently(tool, &prompt)?.output;

    fs::write(&session.conversation_summary_path, summary).with_context(|| {
        format!(
            "Failed to write {}",
            session.conversation_summary_path.display()
        )
    })?;

    Ok(())
}

fn append_message(path: &Path, role: &str, message: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .with_context(|| format!("Failed to open {}", path.display()))?;

    writeln!(file, "\n## {role}\n\n{message}\n")?;

    Ok(())
}

fn run_ai_tool_silently(tool: &AiRuntime, prompt: &str) -> Result<AiRunResult> {
    let backend = backend_for_tool(tool);
    let output = backend.run_review(prompt, &mut |_| {})?;
    Ok(AiRunResult { output })
}

fn parse_last_command(input: &str, command: &str) -> Result<Option<Option<usize>>> {
    if input == command {
        return Ok(Some(None));
    }

    let Some(rest) = input.strip_prefix(command) else {
        return Ok(None);
    };

    if rest.starts_with('-') {
        return Ok(None);
    }

    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(Some(None));
    }

    let count = rest.parse::<usize>().with_context(|| {
        format!("Invalid `{command}` argument `{rest}`. Use `{command}` or `{command} <N>`.")
    })?;

    if count == 0 {
        anyhow::bail!("Invalid `{command}` argument `0`. Use a value greater than zero.");
    }

    Ok(Some(Some(count)))
}

fn load_last_conversation_markdown(path: &Path, exchange_count: Option<usize>) -> Result<String> {
    let conversation =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

    match exchange_count {
        None => Ok(conversation),
        Some(count) => slice_last_conversation_exchanges(&conversation, count),
    }
}

fn slice_last_conversation_exchanges(conversation: &str, exchange_count: usize) -> Result<String> {
    let entries = parse_conversation_entries(conversation);
    let user_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(idx, entry)| (entry.role == "USER").then_some(idx))
        .collect();

    if user_indices.is_empty() {
        return Ok("# Review Conversation\n\nNo saved conversation exchanges yet.\n".to_string());
    }

    let start_user_pos = user_indices.len().saturating_sub(exchange_count);
    let start_idx = user_indices[start_user_pos];

    let mut markdown = String::from("# Review Conversation\n\n");
    for entry in entries.iter().skip(start_idx) {
        markdown.push_str("## ");
        markdown.push_str(&entry.role);
        markdown.push_str("\n\n");
        markdown.push_str(entry.message.trim());
        markdown.push_str("\n\n");
    }

    Ok(markdown)
}

fn parse_conversation_entries(conversation: &str) -> Vec<ConversationEntry> {
    let mut entries = Vec::new();
    let mut current_role: Option<&str> = None;
    let mut current_message: Vec<&str> = Vec::new();

    for line in conversation.lines() {
        match line {
            "## USER" | "## AI" => {
                if let Some(role) = current_role.take() {
                    entries.push(ConversationEntry {
                        role: role.to_string(),
                        message: current_message.join("\n").trim().to_string(),
                    });
                }

                current_role = Some(line.trim_start_matches("## ").trim());
                current_message.clear();
            }
            _ if current_role.is_some() => current_message.push(line),
            _ => {}
        }
    }

    if let Some(role) = current_role {
        entries.push(ConversationEntry {
            role: role.to_string(),
            message: current_message.join("\n").trim().to_string(),
        });
    }

    entries
}

struct ConversationEntry {
    role: String,
    message: String,
}

fn read_or_default(path: &Path, default: &str) -> Result<String> {
    if path.exists() {
        Ok(fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?)
    } else {
        Ok(default.to_string())
    }
}

fn write_diff_by_file(diff_by_file_dir: &Path, diff: &str) -> Result<()> {
    if diff_by_file_dir.exists() {
        fs::remove_dir_all(diff_by_file_dir)
            .with_context(|| format!("Failed to clear {}", diff_by_file_dir.display()))?;
    }

    fs::create_dir_all(diff_by_file_dir)
        .with_context(|| format!("Failed to create {}", diff_by_file_dir.display()))?;

    let mut current_file: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            if let Some(file) = current_file.take() {
                write_single_file_diff(diff_by_file_dir, &file, &current_lines)?;
                current_lines.clear();
            }

            current_file = extract_file_from_git_diff_header(line);
        }

        current_lines.push(line.to_string());
    }

    if let Some(file) = current_file {
        write_single_file_diff(diff_by_file_dir, &file, &current_lines)?;
    }

    Ok(())
}

fn extract_file_from_git_diff_header(line: &str) -> Option<String> {
    let rest = line.strip_prefix("diff --git a/")?;
    let (_left, right) = rest.split_once(" b/")?;

    Some(right.to_string())
}

fn write_single_file_diff(
    diff_by_file_dir: &Path,
    file_path: &str,
    lines: &[String],
) -> Result<()> {
    let safe_name = sanitize_file_name(file_path);
    let output_path = diff_by_file_dir.join(format!("{safe_name}.patch"));

    fs::write(&output_path, lines.join("\n"))
        .with_context(|| format!("Failed to write {}", output_path.display()))?;

    Ok(())
}

fn sanitize_file_name(file_path: &str) -> String {
    file_path
        .replace(['\\', '/'], "__")
        .replace([':', '*', '?', '"', '<', '>', '|'], "_")
}

fn select_relevant_diff_by_file(diff_by_file_dir: &Path, question: &str) -> Result<Option<String>> {
    if !diff_by_file_dir.exists() {
        return Ok(None);
    }

    let question_lower = question.to_lowercase();

    let files = load_diff_files(diff_by_file_dir)?;

    for (file_name, diff_content) in &files {
        let file_name_lower = file_name.to_lowercase();

        if question_lower.contains(&file_name_lower) {
            return Ok(Some(diff_content.clone()));
        }

        let unsanitized_guess = file_name_lower.replace("__", "/").replace(".patch", "");

        if question_lower.contains(&unsanitized_guess) {
            return Ok(Some(diff_content.clone()));
        }

        let basename = file_name_lower
            .trim_end_matches(".patch")
            .split("__")
            .last()
            .unwrap_or("");

        if !basename.is_empty() && question_lower.contains(basename) {
            return Ok(Some(diff_content.clone()));
        }
    }

    // Fallback heuristic matching:
    //
    // At this point, no direct filename match was found from the user's question.
    //
    // Example:
    //   User asks:
    //     "is there a concurrency issue in the transaction handling?"
    //
    // The question may not explicitly mention a file like:
    //     transaction_service.py
    //
    // So we attempt a lightweight semantic-ish match using keywords.
    //
    // Steps:
    //
    // 1. Extract meaningful keywords from the user's question
    //    while removing very common/noisy words.
    //
    //    Example:
    //      "is there a concurrency issue in transaction handling?"
    //
    //    becomes roughly:
    //      ["concurrency", "transaction", "handling"]
    //
    // 2. Iterate over every diff file content.
    //
    // 3. Count how many extracted keywords appear inside each diff.
    //
    //    Example:
    //      transaction_service.py diff contains:
    //        - transaction
    //        - concurrency
    //
    //      score = 2
    //
    // 4. Keep the diff with the highest keyword match score.
    //
    // This is intentionally simple and cheap:
    // - no embeddings
    // - no vector DB
    // - no RAG
    // - no AI call
    //
    // Goal:
    // cheaply inject ONLY the most likely relevant diff chunk
    // instead of replaying the entire PR diff every chat turn.
    //
    // NOTE:
    // This is a *heuristic only.
    //
    //   heuristic == "a practical approximation/rule-of-thumb to make a good-enough decision cheaply"
    //             != "a precise method that always picks the perfect chunk every time"
    //
    // It may produce false positives/negatives,
    // but is already much better than always injecting the full diff.
    let keywords = extract_keywords(&question_lower);

    if keywords.is_empty() {
        return Ok(None);
    }

    let mut best_match: Option<(usize, String)> = None;

    for (_file_name, diff_content) in files {
        let diff_lower = diff_content.to_lowercase();

        let score = keywords
            .iter()
            .filter(|keyword| diff_lower.contains(keyword.as_str()))
            .count();

        if score > 0 {
            match &best_match {
                Some((best_score, _)) if score <= *best_score => {}
                _ => best_match = Some((score, diff_content)),
            }
        }
    }

    Ok(best_match.map(|(_, diff)| diff))
}

fn load_diff_files(diff_by_file_dir: &Path) -> Result<HashMap<String, String>> {
    let mut files = HashMap::new();

    for entry in fs::read_dir(diff_by_file_dir)
        .with_context(|| format!("Failed to read {}", diff_by_file_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|x| x.to_str()) else {
            continue;
        };

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        files.insert(file_name.to_string(), content);
    }

    Ok(files)
}

fn extract_keywords(question_lower: &str) -> Vec<String> {
    question_lower
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|word| word.len() >= 4)
        .filter(|word| {
            !matches!(
                *word,
                "what"
                    | "when"
                    | "where"
                    | "which"
                    | "would"
                    | "could"
                    | "should"
                    | "about"
                    | "there"
                    | "their"
                    | "this"
                    | "that"
                    | "with"
                    | "from"
                    | "have"
                    | "does"
                    | "review"
                    | "issue"
                    | "file"
                    | "code"
                    | "change"
                    | "changes"
                    | "blocking"
                    | "non"
                    | "risk"
            )
        })
        .map(ToString::to_string)
        .collect()
}

pub fn resume_interactive_session(artifact_dir: &Path, tool: &AiRuntime) -> Result<()> {
    let conversation_path = artifact_dir.join("conversation.md");
    let review_summary_path = artifact_dir.join("review-summary.md");
    let conversation_summary_path = artifact_dir.join("conversation-summary.md");
    let diff_by_file_dir = artifact_dir.join("diff-by-file");
    let diff_path = artifact_dir.join("diff.patch");
    let meta_path = artifact_dir.join("meta.json");

    if !conversation_path.exists() {
        display_no_session_warning(artifact_dir, "conversation.md");
        std::process::exit(0);
    }

    if !review_summary_path.exists() {
        display_no_session_warning(artifact_dir, "review-summary.md");
        std::process::exit(0);
    }

    if !conversation_summary_path.exists() {
        display_no_session_warning(artifact_dir, "conversation-summary.md");
        std::process::exit(0);
    }

    if !diff_by_file_dir.exists() {
        display_no_session_warning(artifact_dir, "diff-by-file");
        std::process::exit(0);
    }

    if !diff_path.exists() {
        display_no_session_warning(artifact_dir, "diff.patch");
        std::process::exit(0);
    }

    if !meta_path.exists() {
        display_no_session_warning(artifact_dir, "meta.json");
        std::process::exit(0);
    }

    println!(
        "🚀 Resuming interactive review session from: {}",
        artifact_dir.display()
    );

    let input = load_review_input_from_session(artifact_dir)?;

    run_interactive_session(&input, artifact_dir, tool)
}

fn display_no_session_warning(artifact_dir: &Path, missing_file: &str) {
    println!("{LINE}");
    println!(
        "{} ⚠  No interactive session found for this review.{}",
        YELLOW_BOLD, RESET
    );

    println!();
    println!("{}Review:{} {}", BLUE_BOLD, RESET, artifact_dir.display());

    println!();
    println!("👉 {}Missing file:{} {}", YELLOW, missing_file, RESET);

    println!();
    println!(
        "{}This PR/commit review was generated without an interactive AI session.{}",
        BLACK_BOLD, RESET
    );

    println!(
        "{}Start a new interactive session first using:{}",
        GREEN_BOLD, RESET
    );

    println!();
    println!(
        "  {}pr-review pr <PR_NUMBER> --ai codex{}",
        BLUE_BOLD, RESET
    );

    println!();
    println!(
        "{}Then you can resume the conversation later from the saved artifacts.{}",
        BLACK_BOLD, RESET
    );

    println!("{LINE}");

    println!(
        "{}👋 Exiting. Please start a new interactive session to continue.{}",
        YELLOW_BOLD, RESET
    );
}

fn load_review_input_from_session(artifact_dir: &Path) -> Result<ReviewInput> {
    let diff = std::fs::read_to_string(artifact_dir.join("diff.patch"))?;
    let loaded_meta = load_review_meta(artifact_dir)?;

    if let Some(warning) = &loaded_meta.warning {
        println!("{LINE}");
        println!("{YELLOW_BOLD}⚠  Legacy session detected.{RESET}");
        println!("{BLACK_BOLD}{warning}{RESET}");
        println!("{LINE}");
    }

    let meta = loaded_meta.meta;

    Ok(ReviewInput {
        diff,
        metadata: meta.metadata,
        prompt_scope: "Resumed interactive review session.".to_string(),
        artifact_prefix: meta.artifact_prefix,
        review_kind: meta.review_kind,
        repository: meta.repository,
        source: meta.source,
        target: meta.target,
        review_ref: meta.review_ref,
        remote: meta.remote,
        pr_id: meta.pr_id,
        sha: meta.sha,
    })
}
