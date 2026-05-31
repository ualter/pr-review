mod ai_backend;
mod artifacts;
mod cli;
mod config;
mod debug;
mod doctor;
mod markdown_viewer;
mod prompt_profile;
mod review;
mod scm;
mod session;
mod ui;

use ai_backend::{AiEvent, backend_for_tool};
use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use artifacts::{existing_review_artifact_dir, review_artifact_dir, write_review_meta};
use cli::{Cli, Commands, CommonArgs, ConfigCommand, PromptCommand, ReviewInput};
use config::{ConfigInitStatus, init_user_config, load_user_config, set_user_config};
use prompt_profile::{PromptInitStatus, init_user_prompt_profiles, resolve_prompt_profile};
use review::{build_prompt, review_commit, review_pr};
use scm::ScmKind;
use session::resume_interactive_session;
use ui::{
    GREEN, GREEN_BOLD, RESET, YELLOW, print_artifacts, print_report, print_review_flow_header,
    restore_cursor, start_spinner,
};

use crate::{
    artifacts::{list_review_sessions, select_review_session},
    cli::SessionCommand,
    debug::{TEST_PROMPT, TESTING},
    markdown_viewer::open_markdown_text,
    ui::{BannerType, print_sessions},
};

fn main() -> Result<()> {
    ctrlc::set_handler(|| {
        restore_cursor();
        println!();
        std::process::exit(130);
    })
    .context("Failed to install CTRL+C handler")?;

    let start = Instant::now();
    let cli = Cli::parse();

    if cli.version {
        ui::print_version_banner();
        return Ok(());
    }

    let config = load_user_config()?;
    set_user_config(config.clone());

    match cli.command {
        Some(Commands::Banner) => {
            ui::print_startup_banner(Some(BannerType::Advanced));
            Ok(())
        }

        Some(Commands::Config { command }) => match command {
            ConfigCommand::Init => {
                ui::print_header();
                let (path, status) = init_user_config()?;

                match status {
                    ConfigInitStatus::Created => {
                        println!("{GREEN_BOLD}Created config file:{RESET} {}", path.display())
                    }
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

        Some(Commands::Prompt { command }) => match command {
            PromptCommand::Init { scm, repo } => {
                ui::print_header();
                let result = init_user_prompt_profiles(scm, &repo)?;

                match result.default_status {
                    PromptInitStatus::Created => println!(
                        "{GREEN_BOLD}Created default prompt profile template:{RESET} {}",
                        result.default_path.display()
                    ),
                    PromptInitStatus::Unchanged => println!(
                        "{YELLOW}Default prompt profile already exists:{RESET} {}",
                        result.default_path.display()
                    ),
                }

                match result.repo_status {
                    PromptInitStatus::Created => println!(
                        "{GREEN_BOLD}Created repo prompt profile template:{RESET} {}",
                        result.repo_path.display()
                    ),
                    PromptInitStatus::Unchanged => println!(
                        "{YELLOW}Repo prompt profile already exists:{RESET} {}",
                        result.repo_path.display()
                    ),
                }

                Ok(())
            }

            PromptCommand::Show {
                scm,
                repo,
                repo_path,
            } => {
                ui::print_header();
                let (prompt_profile, input, scope_label, title) = match (scm, repo.as_deref()) {
                    (None, None) => {
                        let input = preview_review_input(ScmKind::CodeCommit, "example-repo");
                        let profile = prompt_profile::built_in_default_profile();
                        (
                            profile,
                            input,
                            "built-in default".to_string(),
                            "🧾 Default Prompt Preview".to_string(),
                        )
                    }
                    (Some(scm_kind), Some(repository)) => {
                        let profile =
                            resolve_prompt_profile(Some(scm_kind), repository, &repo_path)?;
                        let input = preview_review_input(scm_kind, repository);
                        (
                            profile,
                            input,
                            format!(
                                "resolved for {} / {}",
                                scm_kind.config_dir_name(),
                                repository
                            ),
                            "🧾 Prompt Preview".to_string(),
                        )
                    }
                    (Some(_), None) => {
                        anyhow::bail!(
                            "`pr-review prompt show --scm <scm>` also requires `--repo <name>`, or run `pr-review prompt show` with no arguments to inspect the built-in default prompt."
                        )
                    }
                    (None, Some(_)) => {
                        anyhow::bail!(
                            "`pr-review prompt show --repo <name>` also requires `--scm <scm>`, or run `pr-review prompt show` with no arguments to inspect the built-in default prompt."
                        )
                    }
                };

                let prompt = build_prompt(&input, &prompt_profile);
                let markdown = format!(
                    "# Prompt Preview\n\n- Scope: `{}`\n- SCM: `{}`\n- Repository: `{}`\n- Repo path: `{}`\n\n```text\n{}\n```",
                    scope_label,
                    scm.map(|kind| kind.config_dir_name()).unwrap_or("default"),
                    input.repository,
                    repo_path.display(),
                    prompt
                );
                open_markdown_text(&title, &markdown, false)
            }
        },

        Some(Commands::Doctor) => {
            ui::print_header();
            doctor::run_doctor(std::path::Path::new("."))?;
            Ok(())
        }

        Some(Commands::Session {
            command,
            review_name,
            ai,
        }) => match command {
            Some(SessionCommand::List) => {
                ui::print_header();
                let sessions = list_review_sessions()?;
                let session_paths: Vec<std::path::PathBuf> =
                    sessions.into_iter().map(std::path::PathBuf::from).collect();
                print_sessions(&session_paths);
                Ok(())
            }

            Some(SessionCommand::Resume { review_name, ai }) => {
                ui::print_header();
                let review_name = match review_name {
                    Some(name) => name,
                    None => {
                        // print_startup_banner(Some(BannerType::Banner02));
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

            None => {
                ui::print_header();
                let review_name = match review_name {
                    Some(name) => name,
                    None => select_review_session()?,
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
            let scm = scm.or(config.default_scm).unwrap_or(ScmKind::CodeCommit);
            let input = review_pr(&pr_id, scm, &common)?;

            run_review_flow(input, common, Some(scm), start)
        }

        Some(Commands::Commit { sha, common }) => {
            let common = apply_default_ai(common, &config);
            let input = review_commit(&sha, &common)?;

            run_review_flow(input, common, None, start)
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

fn run_review_flow(
    input: ReviewInput,
    common: CommonArgs,
    scm_kind: Option<ScmKind>,
    start: Instant,
) -> Result<()> {
    let prompt_profile = resolve_prompt_profile(scm_kind, &input.repository, &common.repo_path)?;
    let prompt = build_prompt(&input, &prompt_profile);
    let tmp_dir = std::env::temp_dir();
    let diff_path = tmp_dir.join(format!("{}-diff.patch", input.artifact_prefix));
    let prompt_path = tmp_dir.join(format!("{}-prompt.txt", input.artifact_prefix));
    let report_path = PathBuf::from(format!("{}-review.md", input.artifact_prefix));
    let artifact_dir = review_artifact_dir(&input.artifact_prefix)?;

    print_review_flow_header(
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
        print!(
            "\r{YELLOW}Sending prompt to {}...{RESET}",
            tool.display_name()
        );
        io::stdout().flush()?;

        let ai_start = Instant::now();

        let prompt_arg = fs::read_to_string(&prompt_path)?;
        let prompt_arg = if TESTING {
            TEST_PROMPT.to_string()
        } else {
            prompt_arg
        };

        let backend = backend_for_tool(tool);
        let spinner_label = format!("{} is reviewing:", tool.display_name());
        let mut spinner_handler = Some(start_spinner(tool.status_icon(), spinner_label.clone()));
        let mut streamed_anything = false;
        let review = backend.run_review(&prompt_arg, &mut |event| match event {
            AiEvent::TextDelta(chunk) => {
                if let Some(active_spinner) = spinner_handler.take() {
                    active_spinner.stop();
                    println!();
                }

                streamed_anything = true;
                print!("{chunk}");
                let _ = io::stdout().flush();
            }
            AiEvent::Status(message) => {
                if crate::debug::DEBUG {
                    if let Some(active_spinner) = spinner_handler.take() {
                        active_spinner.stop();
                        println!();
                    }
                    println!("{YELLOW}[debug]{RESET} {message}");
                    if !streamed_anything {
                        spinner_handler =
                            Some(start_spinner(tool.status_icon(), spinner_label.clone()));
                    }
                } else if tool.shows_live_status_updates()
                    && let Some(active_spinner) = spinner_handler.as_ref()
                {
                    active_spinner.set_status(message);
                }
            }
            AiEvent::Failed(message) => {
                if crate::debug::DEBUG {
                    if let Some(active_spinner) = spinner_handler.take() {
                        active_spinner.stop();
                        println!();
                    }
                    println!("{YELLOW}[debug]{RESET} failure: {message}");
                } else if tool.shows_live_status_updates()
                    && let Some(active_spinner) = spinner_handler.as_ref()
                {
                    active_spinner.set_status(format!("failure: {message}"));
                }
            }
            AiEvent::Started | AiEvent::Finished => {}
        })?;

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

fn preview_review_input(scm_kind: ScmKind, repository: &str) -> ReviewInput {
    let pr_id = "12345".to_string();

    ReviewInput {
        diff: r#"diff --git a/src/example.rs b/src/example.rs
index 1111111..2222222 100644
--- a/src/example.rs
+++ b/src/example.rs
@@ -1,4 +1,6 @@
 pub fn example() {
-    do_old_thing();
+    if is_enabled() {
+        do_new_thing();
+    }
 }
"#
        .to_string(),
        metadata: format!(
            "Review target: {} PR #{}\nRepository: {}\nSource branch: feature/example\nDestination branch: main",
            scm_kind.display_name(),
            pr_id,
            repository
        ),
        prompt_scope:
            "Review ONLY the changes contained in this PR diff file. Treat this diff as the source of truth."
                .to_string(),
        artifact_prefix: scm_kind.artifact_prefix(&pr_id),
        review_kind: "pr".to_string(),
        repository: repository.to_string(),
        source: "feature/example".to_string(),
        target: "main".to_string(),
        review_ref: scm_kind.review_ref(&pr_id),
        remote: "origin".to_string(),
        pr_id: Some(pr_id),
        sha: None,
    }
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
