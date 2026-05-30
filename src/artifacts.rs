use crate::ui;
use crate::ui::{BLACK_BOLD, BLUE_BOLD, RESET, YELLOW, YELLOW_BOLD};
use anyhow::{anyhow, Context, Result};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
};

use crate::cli::{AiTool, ReviewInput};

const REVIEW_META_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewMeta {
    pub schema_version: u32,
    pub timestamp: String,
    pub tool: String,
    pub review_kind: String,
    pub repo_path: String,
    pub remote: String,
    pub artifact_prefix: String,
    pub repository: String,
    pub source: String,
    pub target: String,
    pub review_ref: String,
    pub metadata: String,
    pub pr_id: Option<String>,
    pub sha: Option<String>,
}

#[derive(Debug)]
pub struct LoadedReviewMeta {
    pub meta: ReviewMeta,
    pub warning: Option<String>,
}

pub struct AiRunResult {
    pub output: String,
    pub streamed: bool,
}

pub fn write_review_meta(
    artifact_dir: &Path,
    input: &ReviewInput,
    repo_path: &Path,
    tool: &Option<AiTool>,
) -> Result<()> {
    let meta = ReviewMeta {
        schema_version: REVIEW_META_SCHEMA_VERSION,
        timestamp: Local::now().to_rfc3339(),
        tool: tool.as_ref().map(|t| t.display_name()).unwrap_or("none").to_string(),
        review_kind: input.review_kind.clone(),
        repo_path: repo_path.display().to_string(),
        remote: input.remote.clone(),
        artifact_prefix: input.artifact_prefix.clone(),
        repository: input.repository.clone(),
        source: input.source.clone(),
        target: input.target.clone(),
        review_ref: input.review_ref.clone(),
        metadata: input.metadata.clone(),
        pr_id: input.pr_id.clone(),
        sha: input.sha.clone(),
    };

    let meta_path = artifact_dir.join("meta.json");
    fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)
        .with_context(|| format!("Failed to write meta file: {}", meta_path.display()))
}

pub fn load_review_meta(artifact_dir: &Path) -> Result<LoadedReviewMeta> {
    let meta_path = artifact_dir.join("meta.json");
    let meta_raw = fs::read_to_string(&meta_path)
        .with_context(|| format!("Failed to read meta file: {}", meta_path.display()))?;

    let value: serde_json::Value = serde_json::from_str(&meta_raw)
        .with_context(|| format!("Failed to parse meta file: {}", meta_path.display()))?;

    if value.get("schema_version").is_some() {
        let meta: ReviewMeta = serde_json::from_value(value)
            .with_context(|| format!("Failed to decode meta file: {}", meta_path.display()))?;

        return Ok(LoadedReviewMeta {
            meta,
            warning: None,
        });
    }

    let repository = value
        .get("repository")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Legacy session metadata is missing required field `repository`"))?;
    let source = value
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Legacy session metadata is missing required field `source`"))?;
    let target = value
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Legacy session metadata is missing required field `target`"))?;
    let review_ref = value
        .get("review_ref")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Legacy session metadata is missing required field `review_ref`"))?;

    let review_name = artifact_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown-review")
        .to_string();

    let inferred_kind = if review_name.starts_with("commit-") {
        "commit"
    } else {
        "pr"
    };
    let inferred_pr_id = review_name
        .strip_prefix("codecommit-pr-")
        .map(ToString::to_string);
    let inferred_sha = review_name.strip_prefix("commit-").map(ToString::to_string);
    let inferred_metadata = match inferred_kind {
        "commit" => format!(
            "Review target: commit {}\nRepository: {}\nSource: {}\nTarget: {}",
            inferred_sha.as_deref().unwrap_or("unknown"),
            repository,
            source,
            target
        ),
        _ => format!(
            "Review target: CodeCommit PR #{}\nRepository: {}\nSource branch: {}\nDestination branch: {}",
            inferred_pr_id.as_deref().unwrap_or("unknown"),
            repository,
            source,
            target
        ),
    };
    let warning = match inferred_kind {
        "commit" if inferred_sha.is_none() => Some(
            "Using legacy session metadata; commit identity was inferred incompletely."
                .to_string(),
        ),
        "pr" if inferred_pr_id.is_none() => Some(
            "Using legacy session metadata; PR identity was inferred incompletely.".to_string(),
        ),
        _ => None,
    };

    Ok(LoadedReviewMeta {
        meta: ReviewMeta {
            schema_version: 1,
            timestamp: value
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            tool: value
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("none")
                .to_string(),
            review_kind: inferred_kind.to_string(),
            repo_path: ".".to_string(),
            remote: "origin".to_string(),
            artifact_prefix: review_name,
            repository: repository.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            review_ref: review_ref.to_string(),
            metadata: inferred_metadata,
            pr_id: inferred_pr_id,
            sha: inferred_sha,
        },
        warning,
    })
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

pub fn run_ai_tool_streaming<F>(tool: &AiTool, prompt: &str, on_chunk: F) -> Result<AiRunResult>
where
    F: FnMut(&str),
{
    match tool {
        AiTool::Copilot => {
            if prompt.len() > MAX_COPILOT_PROMPT_BYTES {
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

                Ok(AiRunResult {
                    output: run_command_streaming(
                        Path::new("."),
                        "copilot",
                        &["-p", &small_prompt],
                        on_chunk,
                    )?,
                    streamed: true,
                })
            } else {
                Ok(AiRunResult {
                    output: run_command_streaming(
                        Path::new("."),
                        "copilot",
                        &["-p", prompt],
                        on_chunk,
                    )?,
                    streamed: true,
                })
            }
        }
        AiTool::Codex => Ok(AiRunResult {
            output: run_command_with_stdin_streaming(
                Path::new("."),
                "codex",
                &["review", "-"],
                prompt,
                on_chunk,
            )?,
            streamed: true,
        }),
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

fn run_command_streaming<F>(
    repo_path: &Path,
    program: &str,
    args: &[&str],
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let mut child = Command::new(program)
        .args(args)
        .current_dir(repo_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn: {program} {}", args.join(" ")))?;

    let Some(mut stdout) = child.stdout.take() else {
        return Err(anyhow!("Failed to capture stdout for: {program} {}", args.join(" ")));
    };

    let stderr_handle = child.stderr.take().map(|mut stderr| {
        thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = stderr.read_to_end(&mut buffer);
            buffer
        })
    });

    let mut collected = String::new();
    let mut buffer = [0u8; 4096];
    loop {
        let bytes_read = stdout
            .read(&mut buffer)
            .with_context(|| format!("Failed to read stdout from: {program}"))?;
        if bytes_read == 0 {
            break;
        }

        let chunk = String::from_utf8_lossy(&buffer[..bytes_read]);
        collected.push_str(&chunk);
        on_chunk(&chunk);
    }

    let status = child
        .wait()
        .with_context(|| format!("Failed to wait for: {program}"))?;

    let stderr = match stderr_handle {
        Some(handle) => handle
            .join()
            .map_err(|_| anyhow!("Stderr reader thread panicked for: {program}"))?,
        None => Vec::new(),
    };

    if !status.success() {
        return Err(anyhow!(
            "Command failed: {} {}\n{}",
            program,
            args.join(" "),
            String::from_utf8_lossy(&stderr)
        ));
    }

    Ok(collected)
}

fn run_command_with_stdin_streaming<F>(
    repo_path: &Path,
    program: &str,
    args: &[&str],
    input: &str,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let mut child = Command::new(program)
        .args(args)
        .current_dir(repo_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn: {program} {}", args.join(" ")))?;

    let Some(mut stdin) = child.stdin.take() else {
        return Err(anyhow!("Failed to open stdin for: {program} {}", args.join(" ")));
    };

    let input_owned = input.to_string();
    let stdin_handle = thread::spawn(move || -> Result<()> {
        stdin
            .write_all(input_owned.as_bytes())
            .context("Failed to write prompt to stdin")?;
        Ok(())
    });

    let Some(mut stdout) = child.stdout.take() else {
        return Err(anyhow!("Failed to capture stdout for: {program} {}", args.join(" ")));
    };

    let stderr_handle = child.stderr.take().map(|mut stderr| {
        thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = stderr.read_to_end(&mut buffer);
            buffer
        })
    });

    let mut collected = String::new();
    let mut buffer = [0u8; 4096];
    loop {
        let bytes_read = stdout
            .read(&mut buffer)
            .with_context(|| format!("Failed to read stdout from: {program}"))?;
        if bytes_read == 0 {
            break;
        }

        let chunk = String::from_utf8_lossy(&buffer[..bytes_read]);
        collected.push_str(&chunk);
        on_chunk(&chunk);
    }

    stdin_handle
        .join()
        .map_err(|_| anyhow!("Stdin writer thread panicked for: {program}"))??;

    let status = child
        .wait()
        .with_context(|| format!("Failed to wait for: {program}"))?;

    let stderr = match stderr_handle {
        Some(handle) => handle
            .join()
            .map_err(|_| anyhow!("Stderr reader thread panicked for: {program}"))?,
        None => Vec::new(),
    };

    if !status.success() {
        return Err(anyhow!(
            "Command failed: {} {}\n{}",
            program,
            args.join(" "),
            String::from_utf8_lossy(&stderr)
        ));
    }

    Ok(collected)
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

pub fn list_review_sessions() -> Result<Vec<String>> {
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
        .filter(|path| path.join("conversation-summary.md").exists())
        .filter(|path| path.join("review-summary.md").exists())
        .filter(|path| path.join("diff.patch").exists())
        .filter(|path| path.join("meta.json").exists())
        .filter_map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
        })
        .collect::<Vec<_>>();

    sessions.sort();

    Ok(sessions)
}

pub fn select_review_session() -> Result<String> {
    let sessions = list_review_sessions()?;

    if sessions.is_empty() {
        return Err(anyhow!(
            "No resumable sessions found in ~/.pr-review/reports"
        ));
    }

    ui::pick_session(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ReviewInput;

    fn sample_review_input() -> ReviewInput {
        ReviewInput {
            diff: "diff --git a/src/lib.rs b/src/lib.rs".to_string(),
            metadata: "Review target: CodeCommit PR #42\nRepository: datahub\nSource branch: feature\nDestination branch: main".to_string(),
            prompt_scope: "scope".to_string(),
            artifact_prefix: "codecommit-pr-42".to_string(),
            review_kind: "pr".to_string(),
            repository: "datahub".to_string(),
            source: "feature".to_string(),
            target: "main".to_string(),
            review_ref: "review/pr-42".to_string(),
            remote: "origin".to_string(),
            pr_id: Some("42".to_string()),
            sha: None,
        }
    }

    #[test]
    fn writes_and_loads_current_review_meta() {
        let temp_dir = tempfile::tempdir().unwrap();
        let input = sample_review_input();

        write_review_meta(temp_dir.path(), &input, Path::new("/tmp/repo"), &Some(AiTool::Codex))
            .unwrap();

        let loaded = load_review_meta(temp_dir.path()).unwrap();

        assert!(loaded.warning.is_none());
        assert_eq!(loaded.meta.schema_version, 2);
        assert_eq!(loaded.meta.review_kind, "pr");
        assert_eq!(loaded.meta.repo_path, "/tmp/repo");
        assert_eq!(loaded.meta.remote, "origin");
        assert_eq!(loaded.meta.pr_id.as_deref(), Some("42"));
        assert_eq!(loaded.meta.metadata, input.metadata);
    }

    #[test]
    fn loads_legacy_review_meta_without_warning_when_identity_is_clear() {
        let temp_dir = tempfile::tempdir().unwrap();
        let artifact_dir = temp_dir.path().join("commit-deadbeef");
        fs::create_dir_all(&artifact_dir).unwrap();

        fs::write(
            artifact_dir.join("meta.json"),
            r#"{
  "timestamp": "2026-05-28T10:00:00+00:00",
  "tool": "Codex",
  "repository": "internal-repo",
  "source": "deadbeef",
  "target": "single commit",
  "review_ref": "review/commit-deadbeef"
}"#,
        )
        .unwrap();

        let loaded = load_review_meta(&artifact_dir).unwrap();

        assert!(loaded.warning.is_none());
        assert_eq!(loaded.meta.schema_version, 1);
        assert_eq!(loaded.meta.review_kind, "commit");
        assert_eq!(loaded.meta.sha.as_deref(), Some("deadbeef"));
        assert_eq!(loaded.meta.remote, "origin");
        assert!(loaded.meta.metadata.contains("Review target: commit deadbeef"));
    }
}
