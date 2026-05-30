use anyhow::{anyhow, Context, Result};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use crate::{
    artifacts::run_ai_tool,
    cli::{AiTool, ReviewInput},
    ui::{print_interactive_help, BLUE_BOLD, GREEN_BOLD, LINE, RESET, YELLOW_BOLD},
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
    tool: &AiTool,
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
    tool: &AiTool,
) -> Result<()> {
    let session = ReviewSession::new(artifact_dir)?;

    println!("{LINE}");
    println!(
        "{}Interactive review session started with {}{}",
        GREEN_BOLD,
        tool.display_name(),
        RESET
    );
    println!(
        "{}Type /exit to leave. Type /full to force full diff context once.{}",
        YELLOW_BOLD, RESET
    );
    println!("{LINE}");

    loop {
        print!("{}pr-review> {}", BLUE_BOLD, RESET);
        io::stdout().flush()?;

        let mut user_question = String::new();
        io::stdin().read_line(&mut user_question)?;

        let user_question = user_question.trim();

        if user_question.is_empty() {
            continue;
        }

        match user_question {
            "/help" => {
                print_interactive_help(tool);
                continue;
            }
            "/exit" => {
                println!("{LINE}");
                println!(
                    "{}Conversation saved to:{} {}",
                    GREEN_BOLD,
                    RESET,
                    session.conversation_path.display()
                );
                println!("{LINE}");
                break;
            }
            "/quit" => {
                println!("Use /exit to leave the session.");
                continue;
            }
            "" => continue,
            _ => process_user_question(input, tool, &session, user_question)?,
        }

        process_user_question(input, tool, &session, user_question)?;
    }

    Ok(())
}

fn process_user_question(
    input: &ReviewInput,
    tool: &AiTool,
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
    let answer = run_ai_tool(tool, &prompt)?;
    println!("\n{}\n", answer);
    session.append_ai_message(&answer)?;
    update_conversation_summary(session, tool)?;
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

fn summarize_review(input: &ReviewInput, review: &str, tool: &AiTool) -> Result<String> {
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

    run_ai_tool(tool, &prompt)
}

fn update_conversation_summary(session: &ReviewSession, tool: &AiTool) -> Result<()> {
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

    let summary = run_ai_tool(tool, &prompt)?;

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

fn read_or_default(path: &Path, default: &str) -> Result<String> {
    if path.exists() {
        Ok(fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?)
    } else {
        Ok(default.to_string())
    }
}

fn write_diff_by_file(diff_by_file_dir: &Path, diff: &str) -> Result<()> {
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
        .replace('\\', "__")
        .replace('/', "__")
        .replace(':', "_")
        .replace('*', "_")
        .replace('?', "_")
        .replace('"', "_")
        .replace('<', "_")
        .replace('>', "_")
        .replace('|', "_")
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

pub fn resume_interactive_session(artifact_dir: &Path, tool: &AiTool) -> Result<()> {
    let conversation_path = artifact_dir.join("conversation.md");
    let review_summary_path = artifact_dir.join("review-summary.md");
    let conversation_summary_path = artifact_dir.join("conversation-summary.md");
    let diff_by_file_dir = artifact_dir.join("diff-by-file");

    if !conversation_path.exists() {
        return Err(anyhow!(
            "No conversation.md found. This review does not have an interactive session yet: {}",
            conversation_path.display()
        ));
    }

    if !review_summary_path.exists() {
        return Err(anyhow!(
            "No review-summary.md found. This review is missing session artifacts: {}",
            review_summary_path.display()
        ));
    }

    if !conversation_summary_path.exists() {
        return Err(anyhow!(
            "No conversation-summary.md found. This review is missing session artifacts: {}",
            conversation_summary_path.display()
        ));
    }

    if !diff_by_file_dir.exists() {
        return Err(anyhow!(
            "No diff-by-file directory found. This review is missing split diff artifacts: {}",
            diff_by_file_dir.display()
        ));
    }

    println!(
        "Resuming interactive review session from: {}",
        artifact_dir.display()
    );

    let input = load_review_input_from_session(artifact_dir)?;

    run_interactive_session(&input, artifact_dir, tool)
}

fn load_review_input_from_session(artifact_dir: &Path) -> Result<ReviewInput> {
    let diff = std::fs::read_to_string(artifact_dir.join("diff.patch"))?;

    let review_name = artifact_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown-review")
        .to_string();

    Ok(ReviewInput {
        diff,
        metadata: format!("Resumed review session: {review_name}"),
        prompt_scope: "Resumed interactive review session.".to_string(),
        artifact_prefix: review_name.clone(),
        repository: "resumed-session".to_string(),
        source: "existing-review".to_string(),
        target: "existing-review".to_string(),
        review_ref: review_name,
    })
}
