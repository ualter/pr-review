use crate::ui::{BLACK_BOLD, BLUE_BOLD, RESET, YELLOW, YELLOW_BOLD};
use anyhow::{anyhow, Context, Result};
use chrono::Local;
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tempfile;

use crate::cli::{AiTool, ReviewInput};

pub fn write_review_meta(
    artifact_dir: &Path,
    input: &ReviewInput,
    tool: &Option<AiTool>,
) -> Result<()> {
    let timestamp = Local::now().to_rfc3339();
    let tool_name = tool.as_ref().map(|t| t.display_name()).unwrap_or("none");

    let meta = serde_json::json!({
        "timestamp":  timestamp,
        "tool":       tool_name,
        "repository": input.repository,
        "source":     input.source,
        "target":     input.target,
        "review_ref": input.review_ref,
    });

    let meta_path = artifact_dir.join("meta.json");
    fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)
        .with_context(|| format!("Failed to write meta file: {}", meta_path.display()))
}

const MAX_COPILOT_PROMPT_BYTES: usize = 129_000;

// - copilot takes the prompt as a CLI argument: copilot -p "...prompt..." → run_command is enough
// - codex review - the - flag means "read from stdin" → needs run_command_with_stdin to pipe the prompt in
//
// If codex had a flag like codex review -p "prompt" we wouldn't need it. The difference is purely dictated by how each CLI wasdesigned to receive its input.
//
//  copilot -p "<prompt>"          ← prompt lives in argv
//  codex review -                 ← prompt comes through stdin pipe
//
// That's the only reason for the two functions — one writes to args, the other writes to stdin.

pub fn run_ai_tool(tool: &AiTool, prompt: &str) -> Result<String> {
    match tool {
        AiTool::Copilot => {
            if prompt.len() > MAX_COPILOT_PROMPT_BYTES {
                // Copilot has a hard limit on prompt size, and performance degrades well before that limit. For very large PRs,
                // we write the prompt to a temporary file and pass instructions to read from that file instead.
                eprintln!(
    "\n{YELLOW}⚠️  Large PR detected ({}) bytes.{RESET}\n\
{BLACK_BOLD}Copilot may struggle with very large reviews or hit prompt limits.{RESET}\n\
{BLUE_BOLD}Tip:{RESET} For large PRs, consider using {YELLOW_BOLD}--ai codex{RESET} for better reliability and context handling.\n",
    prompt.len()
);
                let prompt_file = tempfile::NamedTempFile::new()?;
                std::fs::write(prompt_file.path(), prompt)?;

                let small_prompt = format!(
                    "Read and follow the complete PR review prompt in this file:\n\n{}\n\nTreat that file as the full instructions and source of truth.",
                    prompt_file.path().display()
                );

                run_command(Path::new("."), "copilot", &["-p", &small_prompt])
            } else {
                // For smaller prompts, we can pass directly through the CLI argument as intended.
                run_command(Path::new("."), "copilot", &["-p", prompt])
            }
        }

        AiTool::Codex => run_command_with_stdin(Path::new("."), "codex", &["review", "-"], prompt),
    }
}

/// Spawns a command and writes `input` to its stdin, capturing stdout.
fn run_command_with_stdin(
    repo_path: &Path,
    program: &str,
    args: &[&str],
    input: &str,
) -> Result<String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(repo_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn: {program} {}", args.join(" ")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .context("Failed to write prompt to stdin")?;
    }

    let output = child
        .wait_with_output()
        .with_context(|| format!("Failed to wait for: {program}"))?;

    if !output.status.success() {
        return Err(anyhow!(
            "Command failed: {} {}\n{}",
            program,
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn run_command(repo_path: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(repo_path)
        .output()
        .with_context(|| format!("Failed to execute command: {program} {}", args.join(" ")))?;

    if !output.status.success() {
        return Err(anyhow!(
            "Command failed: {} {}\n{}",
            program,
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn repo_name(repo_path: &Path) -> String {
    repo_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown-repository")
        .to_string()
}

pub fn reports_archive_dir() -> Result<PathBuf> {
    let home = env::var("HOME").context("Could not resolve HOME directory")?;

    Ok(PathBuf::from(home).join(".pr-review").join("reports"))
}

pub fn review_artifact_dir(review_name: &str) -> Result<PathBuf> {
    let dir = reports_archive_dir()?.join(review_name);

    fs::create_dir_all(&dir).with_context(|| {
        format!(
            "Failed to create review artifact directory: {}",
            dir.display()
        )
    })?;

    Ok(dir)
}

pub fn existing_review_artifact_dir(review_name: &str) -> Result<PathBuf> {
    let dir = reports_archive_dir()?.join(review_name);

    if !dir.exists() {
        return Err(anyhow!(
            "Review session not found: {}\nExpected directory: {}",
            review_name,
            dir.display()
        ));
    }

    if !dir.is_dir() {
        return Err(anyhow!(
            "Review session path exists but is not a directory: {}",
            dir.display()
        ));
    }

    Ok(dir)
}

pub fn list_review_sessions() -> Result<Vec<PathBuf>> {
    let reports_dir = reports_archive_dir()?;

    if !reports_dir.exists() {
        return Ok(vec![]);
    }

    let mut sessions = fs::read_dir(&reports_dir)
        .with_context(|| {
            format!(
                "Failed to read reports directory: {}",
                reports_dir.display()
            )
        })?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| path.join("conversation.md").exists())
        .collect::<Vec<_>>();

    sessions.sort();

    Ok(sessions)
}
