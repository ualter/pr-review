use anyhow::Result;
use std::{fs, path::Path, process::Command};

use crate::artifacts::reports_archive_dir;
use crate::ui::{BLUE_BOLD, GREEN_BOLD, RED_BOLD, RESET, YELLOW_BOLD};

pub fn run_doctor(repo_path: &Path) -> Result<()> {
    let line = "-----------------------------------------------------------------";
    println!("\n{GREEN_BOLD}🩺 pr-review doctor{RESET}");
    println!("{BLUE_BOLD}{line}{RESET}");

    let mut failed = false;

    check_command("git", &["--version"], &mut failed);
    check_command("aws", &["--version"], &mut failed);
    check_command("copilot", &["--version"], &mut failed);
    check_command("codex", &["--version"], &mut failed);

    check_git_repo(repo_path, &mut failed);
    check_reports_dir(&mut failed)?;

    println!("{BLUE_BOLD}{line}{RESET}");

    if failed {
        println!(
            "{YELLOW_BOLD}⚠ Some checks failed. Fix the issues above before running a review.{RESET}"
        );
    } else {
        println!("{GREEN_BOLD}✨ Environment looks ready.{RESET}");
    }

    Ok(())
}

fn check_command(program: &str, args: &[&str], failed: &mut bool) {
    match Command::new(program).args(args).output() {
        Ok(output) if output.status.success() => {
            println!("{GREEN_BOLD}✓{RESET} {program} found");
        }
        _ => {
            println!("{RED_BOLD}✗{RESET} {program} not found or not working");
            *failed = true;
        }
    }
}

fn check_git_repo(repo_path: &Path, failed: &mut bool) {
    let result = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(repo_path)
        .output();

    match result {
        Ok(output) if output.status.success() => {
            println!(
                "{GREEN_BOLD}✓{RESET} git repository detected: {}",
                repo_path.display()
            );
        }
        _ => {
            println!(
                "{RED_BOLD}✗{RESET} not a git repository: {}",
                repo_path.display()
            );
            *failed = true;
        }
    }
}

fn check_reports_dir(failed: &mut bool) -> Result<()> {
    let dir = reports_archive_dir()?;

    match fs::create_dir_all(&dir) {
        Ok(_) => println!(
            "{GREEN_BOLD}✓{RESET} reports directory writable: {}",
            dir.display()
        ),
        Err(err) => {
            println!(
                "{RED_BOLD}✗{RESET} reports directory not writable: {} ({err})",
                dir.display()
            );
            *failed = true;
        }
    }

    Ok(())
}
