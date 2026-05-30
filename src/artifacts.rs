use anyhow::{anyhow, Context, Result};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::cli::AiTool;

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
        AiTool::Copilot => run_command(Path::new("."), "copilot", &["-p", prompt]),
        // `codex review -` reads the prompt from stdin, running fully non-interactively
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
