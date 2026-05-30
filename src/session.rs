use anyhow::{Context, Result};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use crate::{
    artifacts::run_ai_tool,
    cli::{AiTool, ReviewInput},
    ui::{BLUE_BOLD, GREEN_BOLD, LINE, RESET, YELLOW_BOLD},
};

pub struct ReviewSession {
    pub artifact_dir: PathBuf,
    pub conversation_path: PathBuf,
    pub summary_path: PathBuf,
}

impl ReviewSession {
    pub fn new(artifact_dir: &Path) -> Result<Self> {
        let conversation_path = artifact_dir.join("conversation.md");
        let summary_path = artifact_dir.join("conversation-summary.md");

        if !conversation_path.exists() {
            fs::write(&conversation_path, "# PR Review Conversation\n\n")?;
        }

        if !summary_path.exists() {
            fs::write(&summary_path, "No conversation summary yet.\n")?;
        }

        Ok(Self {
            artifact_dir: artifact_dir.to_path_buf(),
            conversation_path,
            summary_path,
        })
    }

    pub fn append_user_message(&self, message: &str) -> Result<()> {
        append_message(&self.conversation_path, "USER", message)
    }

    pub fn append_ai_message(&self, message: &str) -> Result<()> {
        append_message(&self.conversation_path, "AI", message)
    }

    pub fn read_summary(&self) -> Result<String> {
        fs::read_to_string(&self.summary_path)
            .with_context(|| format!("Failed to read {}", self.summary_path.display()))
    }
}

fn append_message(path: &Path, role: &str, message: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .with_context(|| format!("Failed to open conversation file: {}", path.display()))?;

    writeln!(file, "\n## {role}\n\n{message}\n")?;
    Ok(())
}

pub fn run_interactive_session(
    input: &ReviewInput,
    artifact_dir: &Path,
    tool: &AiTool,
    review: &str,
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
        "{}Type /exit to leave. Type /full to ask using the full diff once.{}",
        YELLOW_BOLD, RESET
    );
    println!("{LINE}");

    loop {
        print!("{}pr-review> {}", BLUE_BOLD, RESET);
        io::stdout().flush()?;

        let mut user_input = String::new();
        io::stdin().read_line(&mut user_input)?;

        let user_input = user_input.trim();

        if user_input.is_empty() {
            continue;
        }

        if matches!(user_input, "/exit" | "exit" | "quit") {
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

        session.append_user_message(user_input)?;

        let include_full_diff = user_input == "/full";
        let actual_question = if include_full_diff {
            "Review the whole diff again and answer with any important missed concerns."
        } else {
            user_input
        };

        let prompt = build_interactive_prompt(
            input,
            review,
            &session.read_summary()?,
            actual_question,
            include_full_diff,
        );

        let answer = run_ai_tool(tool, &prompt)?;
        println!("\n{}\n", answer);

        session.append_ai_message(&answer)?;
    }

    Ok(())
}

fn build_interactive_prompt(
    input: &ReviewInput,
    initial_review: &str,
    conversation_summary: &str,
    user_question: &str,
    include_full_diff: bool,
) -> String {
    let diff_context = if include_full_diff {
        format!(
            r#"
Full diff:
```diff
{}

"#,
            input.diff
        )
    } else {
        format!(
            r#"
Changed files:
{}

Do not request the full diff unless it is truly necessary.
Use the initial review and metadata first.
"#,
            changed_files_from_diff(&input.diff).join("\n")
        )
    };

    format!(
        r#"

You are continuing an existing AI-assisted PR/commit review session.

Important rules:

Keep the answer focused on the user's follow-up question.
Do not repeat the full review.
Do not invent context.
If there is not enough context, say exactly what file/function/diff chunk is needed.
Prefer practical engineering guidance.
Be concise but specific.

Review metadata:
{}

Initial review result:
{}

Conversation summary:
{}

{}

User question:
{}
"#,
        input.metadata, initial_review, conversation_summary, diff_context, user_question
    )
}

fn changed_files_from_diff(diff: &str) -> Vec<String> {
    diff.lines()
        .filter_map(|line| line.strip_prefix("diff --git a/"))
        .filter_map(|rest| rest.split_once(" b/"))
        .map(|(left, right)| format!("- {left} -> {right}"))
        .collect()
}
