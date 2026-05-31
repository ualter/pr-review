mod artifacts;
mod cli;
mod config;
mod doctor;
mod markdown_viewer;
mod review;
mod scm;
mod session;
mod ui;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use artifacts::{
    existing_review_artifact_dir, review_artifact_dir, run_ai_tool_streaming, write_review_meta,
};
use cli::{Cli, Commands, CommonArgs, ConfigCommand, ReviewInput};
use config::{init_user_config, load_user_config, set_user_config, ConfigInitStatus};
use review::{build_prompt, review_commit, review_pr};
use scm::ScmKind;
use session::resume_interactive_session;
use ui::{
    print_artifacts, print_header, print_report, start_spinner, GREEN, GREEN_BOLD, RESET, YELLOW,
};

use crate::{
    artifacts::{list_review_sessions, select_review_session},
    cli::SessionCommand,
    ui::{print_sessions, print_startup_banner},
};

const TESTING: bool = false;

fn main() -> Result<()> {
    let start = Instant::now();
    let cli = Cli::parse();

    if cli.version {
        ui::print_startup_banner();
        return Ok(());
    }

    let config = load_user_config()?;
    set_user_config(config.clone());

    match cli.command {
        Some(Commands::Banner) => {
            ui::print_startup_banner();
            Ok(())
        }

        Some(Commands::Config { command }) => match command {
            ConfigCommand::Init => {
                let (path, status) = init_user_config()?;

                match status {
                    ConfigInitStatus::Created => println!(
                        "{GREEN_BOLD}Created config file:{RESET} {}",
                        path.display()
                    ),
                    ConfigInitStatus::Updated => println!(
                        "{YELLOW}Found existing config and added missing parameters:{RESET} {}",
                        path.display()
                    ),
                    ConfigInitStatus::Unchanged => println!(
                        "{YELLOW}Config file already up to date:{RESET} {}",
                        path.display()
                    ),
                }

                Ok(())
            }
        },

        Some(Commands::Doctor) => {
            doctor::run_doctor(std::path::Path::new("."))?;
            Ok(())
        }

        Some(Commands::Session { command }) => match command {
            SessionCommand::List => {
                let sessions = list_review_sessions()?;
                let session_paths: Vec<std::path::PathBuf> =
                    sessions.into_iter().map(std::path::PathBuf::from).collect();
                print_sessions(&session_paths);
                Ok(())
            }

            SessionCommand::Resume { review_name, ai } => {
                let review_name = match review_name {
                    Some(name) => name,
                    None => {
                        print_startup_banner();
                        select_review_session()?
                    }
                };

                if review_name.is_empty() {
                    println!("\n👋 {GREEN_BOLD}Session selection cancelled.{RESET}");
                    return Ok(());
                }

                let ai = ai.or_else(|| config.default_ai.clone()).ok_or_else(|| {
                    anyhow::anyhow!(
                        "No AI tool specified. Use `--ai <tool>` or set `default_ai = \"codex\"` in ~/.pr-review/config.toml."
                    )
                })?;

                let artifact_dir = existing_review_artifact_dir(&review_name)?;
                resume_interactive_session(&artifact_dir, &ai)?;
                Ok(())
            }
        },

        Some(Commands::Pr { pr_id, scm, common }) => {
            let common = apply_default_ai(common, &config);
            let scm = scm
                .or(config.default_scm)
                .unwrap_or(ScmKind::CodeCommit);
            let input = review_pr(&pr_id, scm, &common)?;

            run_review_flow(input, common, start)
        }

        Some(Commands::Commit { sha, common }) => {
            let common = apply_default_ai(common, &config);
            let input = review_commit(&sha, &common)?;

            run_review_flow(input, common, start)
        }

        None => {
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}

fn apply_default_ai(mut common: CommonArgs, config: &config::AppConfig) -> CommonArgs {
    if common.ai.is_none() {
        common.ai = config.default_ai.clone();
    }

    common
}

fn run_review_flow(input: ReviewInput, common: CommonArgs, start: Instant) -> Result<()> {
    let prompt = build_prompt(&input);
    let tmp_dir = std::env::temp_dir();
    let diff_path = tmp_dir.join(format!("{}-diff.patch", input.artifact_prefix));
    let prompt_path = tmp_dir.join(format!("{}-prompt.txt", input.artifact_prefix));
    let report_path = PathBuf::from(format!("{}-review.md", input.artifact_prefix));
    let artifact_dir = review_artifact_dir(&input.artifact_prefix)?;

    print_header(
        &input.repository,
        &input.source,
        &input.target,
        &input.review_ref,
    );

    let (archived_diff_path, archived_prompt_path) =
        store_review_artifacts(&input, &prompt, &diff_path, &prompt_path, &artifact_dir)?;

    write_review_meta(&artifact_dir, &input, &common.repo_path, &common.ai)?;

    print_artifacts(
        &diff_path,
        &prompt_path,
        &artifact_dir,
        &archived_diff_path,
        &archived_prompt_path,
        &common.ai,
    );

    if let Some(tool) = &common.ai {
        println!(
            "{YELLOW}Sending prompt to {}...{RESET}",
            tool.display_name()
        );

        let ai_start = Instant::now();

        let prompt_arg = fs::read_to_string(&prompt_path)?;
        let prompt_arg = if TESTING {
            "1+1 is".to_string()
        } else {
            prompt_arg
        };

        let mut spinner_handler = Some(start_spinner(
            tool.status_icon(),
            format!("{} is reviewing...", tool.display_name()),
        ));
        let mut streamed_anything = false;

        let review = run_ai_tool_streaming(tool, &prompt_arg, |chunk| {
            if let Some(active_spinner) = spinner_handler.take() {
                active_spinner.stop();
                println!();
            }

            streamed_anything = true;
            print!("{chunk}");
            let _ = io::stdout().flush();
        })?
        .output;

        if let Some(active_spinner) = spinner_handler.take() {
            active_spinner.stop();
        } else if streamed_anything {
            println!();
        }

        fs::write(&report_path, &review)
            .with_context(|| format!("Failed to write report file: {}", report_path.display()))?;

        let archived_report_path = artifact_dir.join("review.md");

        fs::copy(&report_path, &archived_report_path).with_context(|| {
            format!(
                "Failed to archive report from {} to {}",
                report_path.display(),
                archived_report_path.display()
            )
        })?;

        print_report(
            &report_path,
            &archived_report_path,
            ai_start.elapsed(),
            tool,
        );

        session::prepare_session_artifacts(&artifact_dir, &input, &review, tool)?;

        if !common.no_interactive {
            session::run_interactive_session(&input, &artifact_dir, tool)?;
        }
    }

    println!(
        "{GREEN}Done in {:.2}s{RESET}",
        start.elapsed().as_secs_f64()
    );

    Ok(())
}

fn store_review_artifacts(
    input: &cli::ReviewInput,
    prompt: &str,
    diff_path: &PathBuf,
    prompt_path: &PathBuf,
    artifact_dir: &Path,
) -> Result<(PathBuf, PathBuf), anyhow::Error> {
    fs::write(diff_path, &input.diff)
        .with_context(|| format!("Failed to write diff file: {}", diff_path.display()))?;

    fs::write(prompt_path, prompt)
        .with_context(|| format!("Failed to write prompt file: {}", prompt_path.display()))?;

    let archived_diff_path = artifact_dir.join("diff.patch");
    let archived_prompt_path = artifact_dir.join("prompt.txt");

    fs::copy(diff_path, &archived_diff_path).with_context(|| {
        format!(
            "Failed to archive diff from {} to {}",
            diff_path.display(),
            archived_diff_path.display()
        )
    })?;

    fs::copy(prompt_path, &archived_prompt_path).with_context(|| {
        format!(
            "Failed to archive prompt from {} to {}",
            prompt_path.display(),
            archived_prompt_path.display()
        )
    })?;

    Ok((archived_diff_path, archived_prompt_path))
}
