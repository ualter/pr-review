use crate::cli::AiTool;
use std::{
    io::{self, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub const GREEN: &str = "\x1b[32m";
pub const GREEN_BOLD: &str = "\x1b[1;32m";
pub const BLACK_BOLD: &str = "\x1b[1;30m";
pub const BLUE_BOLD: &str = "\x1b[1;34m";
pub const YELLOW: &str = "\x1b[33m";
pub const YELLOW_BOLD: &str = "\x1b[1;33m";
pub const RESET: &str = "\x1b[0m";
pub const RED_BOLD: &str = "\x1b[1;31m";

pub const LINE: &str =
    "----------------------------------------------------------------------------------------------------------------";

pub fn print_header(repository: &str, source: &str, target: &str, review_branch: &str) {
    println!("{LINE}");
    println!("{}Repository: {}{}", GREEN_BOLD, repository, RESET);
    println!("{}Source:     {}{}", BLUE_BOLD, source, RESET);
    println!("{}Target:     {}{}", BLUE_BOLD, target, RESET);
    println!("{}Review:     {}{}", YELLOW_BOLD, review_branch, RESET);
    println!("{LINE}");
}

pub struct SpinnerHandle {
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl SpinnerHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);

        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub fn print_artifacts(
    diff_path: &Path,
    prompt_path: &Path,
    artifact_dir: &Path,
    archived_diff_path: &Path,
    archived_prompt_path: &Path,
    ai: &Option<AiTool>,
) {
    println!(
        "{}Diff written to:   {}{}",
        GREEN_BOLD,
        diff_path.display(),
        RESET
    );
    println!(
        "{}Prompt written to: {}{}",
        BLUE_BOLD,
        prompt_path.display(),
        RESET
    );
    println!("{LINE}");
    println!(
        "{}Artifacts archived to: {}{}",
        GREEN_BOLD,
        artifact_dir.display(),
        RESET
    );
    println!(
        "{}Archived diff:     {}{}",
        GREEN_BOLD,
        archived_diff_path.display(),
        RESET
    );
    println!(
        "{}Archived prompt:   {}{}",
        BLUE_BOLD,
        archived_prompt_path.display(),
        RESET
    );
    println!("{LINE}");
    if ai.is_none() {
        println!("{BLACK_BOLD}Run manually with one of:{RESET}");
        for tool in &[AiTool::Copilot, AiTool::Codex] {
            println!(
                "  {BLACK_BOLD}[{}]  {}{RESET}",
                tool.display_name(),
                tool.manual_hint(prompt_path)
            );
        }
        println!("{LINE}");
    }
}

pub fn print_report(report_path: &Path, archived_path: &Path, elapsed: Duration, tool: &AiTool) {
    println!(
        "{}{} review completed in {:.1}s{}",
        GREEN_BOLD,
        tool.display_name(),
        elapsed.as_secs_f64(),
        RESET
    );
    println!("{LINE}");
    println!(
        "{}Review report written to: {}{}{}",
        GREEN_BOLD,
        BLUE_BOLD,
        report_path.display(),
        RESET
    );
    println!(
        "{}Archived report written to: {}{}{}",
        GREEN_BOLD,
        BLUE_BOLD,
        archived_path.display(),
        RESET
    );
    println!("{LINE}");
}

pub fn start_spinner(message: impl Into<String>) -> SpinnerHandle {
    let message = message.into();

    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);

    let handle = thread::spawn(move || {
        let chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let mut i = 0;

        while !stop_thread.load(Ordering::Relaxed) {
            print!(
                "\r🤖 {}{} {}{}",
                YELLOW,
                message,
                chars[i % chars.len()],
                RESET
            );
            let _ = io::stdout().flush();

            i += 1;
            thread::sleep(Duration::from_millis(100));
        }

        print!("\r{}\r", " ".repeat(120));
        let _ = io::stdout().flush();
    });

    SpinnerHandle {
        stop,
        handle: Some(handle),
    }
}

pub fn print_interactive_help(tool: &AiTool) {
    println!("{LINE}");
    println!(
        "{YELLOW_BOLD}Interactive session - {BLUE_BOLD}{}{RESET}",
        tool.display_name()
    );
    println!();
    println!("{GREEN_BOLD}Commands:{RESET}");
    println!("  {YELLOW}/help{RESET}   {BLACK_BOLD}Show this help{RESET}");
    println!("  {YELLOW}/exit{RESET}   {BLACK_BOLD}Exit interactive session{RESET}");
    println!();
    println!(
        "Anything else is sent to {BLUE_BOLD}{}{RESET}.",
        tool.display_name()
    );
    println!("{LINE}");
}
