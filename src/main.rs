mod artifacts;
mod cli;
mod review;
mod session;
mod ui;

use anyhow::{Context, Result};
use clap::Parser;
use std::{fs, path::PathBuf, sync::atomic::Ordering, time::Instant};

use artifacts::{
    existing_review_artifact_dir, review_artifact_dir, run_ai_tool, write_review_meta,
};
use cli::{Cli, Commands, CommonArgs, ReviewInput};
use review::{build_prompt, review_commit, review_pr};
use session::resume_interactive_session;
use ui::{print_artifacts, print_header, print_report, start_spinner, GREEN, RESET, YELLOW};

const TESTING: bool = false;

fn main() -> Result<()> {
    let start = Instant::now();
    let cli = Cli::parse();

    match cli.command {
        Commands::Session { review_name, ai } => {
            let artifact_dir = existing_review_artifact_dir(&review_name)?;

            resume_interactive_session(&artifact_dir, &ai)?;

            println!(
                "{GREEN}Done in {:.2}s{RESET}",
                start.elapsed().as_secs_f64()
            );

            Ok(())
        }

        Commands::Pr { pr_id, common } => {
            let input = review_pr(&pr_id, &common)?;

            run_review_flow(input, common, start)
        }

        Commands::Commit { sha, common } => {
            let input = review_commit(&sha, &common)?;

            run_review_flow(input, common, start)
        }
    }
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

    write_review_meta(&artifact_dir, &input, &common.ai)?;

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

        let spinner_msg: &'static str =
            Box::leak(format!("{} is reviewing...", tool.display_name()).into_boxed_str());

        let (stop, handle) = start_spinner(spinner_msg);

        let result = run_ai_tool(tool, &prompt_arg);

        stop.store(true, Ordering::Relaxed);
        handle.join().ok();

        let review = result?;

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

        if common.interactive {
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
    artifact_dir: &PathBuf,
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
