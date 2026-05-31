use dialoguer::{
    console::{Style, Term},
    theme::ColorfulTheme,
};

use crate::cli::AiTool;
use crate::config::{user_config, PromptStyle};
use anyhow::Result;
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
pub const BLUE: &str = "\x1b[34m";
pub const BLUE_BOLD: &str = "\x1b[1;34m";
pub const YELLOW: &str = "\x1b[33m";
pub const YELLOW_BOLD: &str = "\x1b[1;33m";
pub const RESET: &str = "\x1b[0m";
pub const RED_BOLD: &str = "\x1b[1;31m";
pub const FG_WHITE_BOLD: &str = "\x1b[1;97m";
pub const FG_DARK_BG: &str = "\x1b[38;5;236m";
pub const BG_DARK: &str = "\x1b[48;5;236m";
pub const FG_GREEN_BG: &str = "\x1b[38;5;34m";
pub const BG_GREEN: &str = "\x1b[48;5;34m";
pub const BG_BLACK: &str = "\x1b[48;5;0m";

pub const LINE: &str =
    "----------------------------------------------------------------------------------------------------------------";

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

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

pub fn start_spinner(icon: &str, message: impl Into<String>) -> SpinnerHandle {
    let icon = icon.to_string();
    let message = message.into();

    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);

    let handle = thread::spawn(move || {
        let chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let mut i = 0;

        while !stop_thread.load(Ordering::Relaxed) {
            print!(
                "\r{} {}{} {}{}",
                icon,
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

pub fn render_interactive_prompt(tool: &AiTool) -> String {
    let tool_name = tool.display_name().to_lowercase();

    match user_config().prompt_style {
        PromptStyle::Simple => format!(
            "{BLUE_BOLD}{} pr-review>{RESET} ",
            tool.status_icon(),
        ),
        PromptStyle::Fancy => format!(
            "{FG_DARK_BG}░▒▓{BG_DARK}{FG_WHITE_BOLD}{} {} {BG_GREEN}{FG_DARK_BG}{FG_WHITE_BOLD} pr-review {BG_BLACK}{FG_GREEN_BG}{RESET} ",
            tool.status_icon(),
            tool_name,
        ),
    }
}

pub fn print_interactive_help(tool: &AiTool) {
    let help_rows = [
        ("❓ /help", "Show available commands and shortcuts"),
        ("📋 /summary", "Show the AI-generated conversation summary"),
        (
            "📋 /summary-print",
            "Print the AI-generated conversation summary",
        ),
        ("🧠 /review", "Show the full PR/commit review results"),
        (
            "🧠 /review-print",
            "Print the full PR/commit review results",
        ),
        ("🧠 /review-summary", "Show the saved review summary"),
        ("🧠 /review-summary-print", "Print the saved review summary"),
        (
            "💬 /last",
            "Show the latest interactive conversation or `/last N` exchanges",
        ),
        (
            "💬 /last-print",
            "Print the latest interactive conversation or `/last-print N` exchanges",
        ),
        ("🔄 /full", "Restart the review session from scratch"),
        ("🚪 /exit", "Save the session and exit"),
    ];

    println!("{LINE}");
    println!();
    println!("{GREEN_BOLD}Available Commands:{RESET}");
    let command_width = help_rows
        .iter()
        .map(|(command, _)| command.chars().count())
        .max()
        .unwrap_or(0);
    for (command, description) in help_rows {
        let padding = " ".repeat(command_width.saturating_sub(command.chars().count()));
        println!("  {YELLOW}{command}{RESET}{padding}  {BLACK_BOLD}{description}{RESET}");
    }
    println!();

    println!("{BLACK_BOLD}Anything else will be sent directly to the AI assistant {BLACK_BOLD}({}).{RESET}", tool.display_name());
    println!("{LINE}");
}

#[allow(dead_code)]
pub enum BannerType {
    Advanced,
    Banner01,
    Banner02,
}

pub fn print_startup_banner(which_banner: Option<BannerType>) {
    print_banner(true, which_banner);
}

pub fn print_version_banner() {
    print_banner(false, Some(BannerType::Banner01));
}

#[allow(dead_code)]
fn print_banner(clear_screen: bool, which_banner: Option<BannerType>) {
    if clear_screen {
        // Clear screen (and scrollback where supported) before starting the animation.
        print!("\x1b[2J\x1b[H\x1b[3J");
    }
    match which_banner.unwrap_or(BannerType::Advanced) {
        BannerType::Advanced => print_advanced_banner_animation(),
        BannerType::Banner01 => print_banner_01(),
        BannerType::Banner02 => print_banner_02(),
    }
}

fn print_banner_01() {
    let frames = [
        include_str!("../assets/banner_01/frame_01.txt"),
        include_str!("../assets/banner_01/frame_02.txt"),
        include_str!("../assets/banner_01/frame_03.txt"),
        include_str!("../assets/banner_01/frame_04.txt"),
        include_str!("../assets/banner_01/frame_05.txt"),
        include_str!("../assets/banner_01/frame_06.txt"),
        include_str!("../assets/banner_01/frame_07.txt"),
        include_str!("../assets/banner_01/frame_08.txt"),
        include_str!("../assets/banner_01/frame_09.txt"),
        include_str!("../assets/banner_01/frame_10.txt"),
        include_str!("../assets/banner_01/frame_11.txt"),
        include_str!("../assets/banner_01/frame_12.txt"),
        include_str!("../assets/banner_01/frame_13.txt"),
        include_str!("../assets/banner_01/frame_14.txt"),
        include_str!("../assets/banner_01/frame_15.txt"),
        include_str!("../assets/banner_01/frame_16.txt"),
        include_str!("../assets/banner_01/frame_17.txt"),
    ];

    println!();
    println!("{BLUE_BOLD}{LINE}{RESET}");

    print!("\x1b[?25l");
    let _ = std::io::stdout().flush();

    for frame in frames {
        print!("\r\x1b[2K{GREEN_BOLD}{frame}{RESET}");
        let _ = std::io::stdout().flush();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    print!("\x1b[?25h");
    let _ = std::io::stdout().flush();

    println!();
    println!("  {BLACK_BOLD} AI-Assisted Engineering Review CLI{RESET}");
    print_version_reveal();
    println!("{BLUE_BOLD}{LINE}{RESET}");
}

#[allow(dead_code)]
fn print_banner_02() {
    let logo = ["👉 PR-REVIEW"];

    let max_width = logo
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    print!("\x1b[2J\x1b[H");
    print!("\x1b[?25l");
    let _ = io::stdout().flush();

    for reveal in (0..=max_width).step_by(3) {
        print!("\x1b[H");
        print!("\n\n");

        for line in logo {
            let line_len = line.chars().count();
            let visible_len = reveal.min(line_len);
            let trailing_spaces = max_width.saturating_sub(visible_len);
            let mut rendered = String::new();

            for (i, ch) in line.chars().take(visible_len).enumerate() {
                let is_edge = i + 4 >= visible_len;
                rendered.push_str(if is_edge { FG_WHITE_BOLD } else { GREEN_BOLD });
                rendered.push(ch);
            }

            rendered.push_str(RESET);
            rendered.push_str(&" ".repeat(trailing_spaces));
            rendered.push('\n');
            print!("{rendered}");
        }

        let _ = io::stdout().flush();
        thread::sleep(Duration::from_millis(120));
    }

    let glow_window = 4usize;
    let glow_step = 1usize;
    let glow_frames = max_width + glow_window;

    for head in (0..=glow_frames).step_by(glow_step) {
        let highlight_start = head.saturating_sub(glow_window);
        let highlight_end = head;

        print!("\x1b[H");
        print!("\n\n");

        for line in logo {
            let line_len = line.chars().count();
            let mut rendered = String::new();

            for (i, ch) in line.chars().enumerate() {
                let color = if (highlight_start..highlight_end).contains(&i) {
                    YELLOW_BOLD
                } else {
                    GREEN_BOLD
                };
                rendered.push_str(color);
                rendered.push(ch);
            }

            rendered.push_str(RESET);
            rendered.push_str(&" ".repeat(max_width.saturating_sub(line_len)));
            rendered.push('\n');
            print!("{rendered}");
        }

        let _ = io::stdout().flush();
        thread::sleep(Duration::from_millis(50));
    }

    print!("\x1b[?25h");
    println!("{RESET}");
    let _ = io::stdout().flush();
}

fn print_advanced_banner_animation() {
    const LOGO: [&str; 7] = [
        r"██████╗ ██████╗       ██████╗ ███████╗██╗   ██╗██╗███████╗██╗    ██╗",
        r"██╔══██╗██╔══██╗      ██╔══██╗██╔════╝██║   ██║██║██╔════╝██║    ██║",
        r"██████╔╝██████╔╝█████╗██████╔╝█████╗  ██║   ██║██║█████╗  ██║ █╗ ██║",
        r"██╔═══╝ ██╔══██╗╚════╝██╔══██╗██╔══╝  ╚██╗ ██╔╝██║██╔══╝  ██║███╗██║",
        r"██║     ██║  ██║      ██║  ██║███████╗ ╚████╔╝ ██║███████╗╚███╔███╔╝",
        r"╚═╝     ╚═╝  ╚═╝      ╚═╝  ╚═╝╚══════╝  ╚═══╝  ╚═╝╚══════╝ ╚══╝╚══╝",
        r"                           pr-review",
    ];

    let max_width = LOGO
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    print!("\x1b[?25l");
    let _ = std::io::stdout().flush();

    for reveal in (0..=max_width).step_by(3) {
        print!("\x1b[H");
        print!("\n\n");
        for line in LOGO {
            let line_len = line.chars().count();
            let visible_len = reveal.min(line_len);
            let trailing_spaces = max_width.saturating_sub(visible_len);
            let mut rendered = String::new();

            for (i, ch) in line.chars().take(visible_len).enumerate() {
                let is_edge = i + 4 >= visible_len;
                rendered.push_str(if is_edge { FG_WHITE_BOLD } else { GREEN_BOLD });
                rendered.push(ch);
            }

            rendered.push_str(RESET);
            rendered.push_str(&" ".repeat(trailing_spaces));
            rendered.push('\n');
            print!("{rendered}");
        }
        let _ = std::io::stdout().flush();
        std::thread::sleep(std::time::Duration::from_millis(35));
    }

    let glow_window = 14usize;
    let glow_step = 2usize;
    let glow_frames = max_width + glow_window;

    for head in (0..=glow_frames).step_by(glow_step) {
        let highlight_start = head.saturating_sub(glow_window);
        let highlight_end = head;
        print!("\x1b[H");
        print!("\n\n");
        for line in LOGO {
            let line_len = line.chars().count();
            let mut rendered = String::new();
            for (i, ch) in line.chars().enumerate() {
                let color = if (highlight_start..highlight_end).contains(&i) {
                    YELLOW_BOLD
                } else {
                    GREEN_BOLD
                };
                rendered.push_str(color);
                rendered.push(ch);
            }
            rendered.push_str(RESET);
            rendered.push_str(&" ".repeat(max_width.saturating_sub(line_len)));
            rendered.push('\n');
            print!("{rendered}");
        }
        let _ = std::io::stdout().flush();
        std::thread::sleep(std::time::Duration::from_millis(22));
    }

    print!("\x1b[?25h");
    let _ = std::io::stdout().flush();

    println!();
    println!("  {BLACK_BOLD} AI-Assisted Engineering Review CLI{RESET}");
    print_version_reveal();
}

fn print_version_reveal() {
    let prefix = format!("   {BLUE_BOLD}📌 Version {RESET}");
    let version = format!("v{APP_VERSION}");

    println!();

    for i in 1..=version.chars().count() {
        let partial: String = version.chars().take(i).collect();
        print!("\r{prefix}{GREEN_BOLD}{partial}{RESET}");
        let _ = std::io::stdout().flush();
        std::thread::sleep(std::time::Duration::from_millis(80));
    }

    println!();
}

pub fn print_sessions(sessions: &[std::path::PathBuf]) {
    println!("{LINE}");
    println!("{YELLOW_BOLD}Existing review sessions{RESET}");
    println!("{LINE}");

    if sessions.is_empty() {
        println!("{BLACK_BOLD}No resumable sessions found.{RESET}");
        println!("{LINE}");
        return;
    }

    for session in sessions {
        if let Some(name) = session.file_name().and_then(|n| n.to_str()) {
            println!("  {GREEN_BOLD}{name}{RESET}");
            println!("    {BLACK_BOLD}Resume:{RESET} pr-review session resume {name} --ai codex");
        }
    }

    println!("{LINE}");
}

pub fn pick_session(sessions: Vec<String>) -> Result<String> {
    let theme = ColorfulTheme {
        values_style: Style::new().cyan(),
        active_item_style: Style::new().yellow().bold(),
        inactive_item_style: Style::new().blue().bold(),
        // active_item_prefix: Style::new().apply_to("❯".to_string()),
        active_item_prefix: Style::new().apply_to("  🟢".to_string()),
        inactive_item_prefix: Style::new().apply_to("  ⚫".to_string()),
        // checked_item_prefix: Style::new().apply_to("✓".to_string()),
        // unchecked_item_prefix: Style::new().apply_to(" ".to_string()),
        // picked_item_style: Style::new().green().bold(),
        prompt_style: Style::new().blue().bold(),
        prompt_prefix: Style::new().yellow().bold().apply_to("".to_owned()),
        prompt_suffix: Style::new().yellow().bold().apply_to("".to_owned()),
        ..ColorfulTheme::default()
    };

    let term = Term::stdout();

    term.clear_to_end_of_screen()?;

    let line = "-----------------------------------------------------------";
    //
    let index = dialoguer::Select::with_theme(&theme)
        // let index = dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .items(&sessions)
        .default(0)
        .with_prompt(format!(
            "{line}\n{YELLOW_BOLD}  🧮 👉 Select a review session to resume {BLACK_BOLD}(<ESC> to cancel){YELLOW_BOLD}\n{BLUE_BOLD} {line}{RESET}"
        ))
        .report(false)
        .interact_opt()?;

    match index {
        Some(i) => Ok(sessions[i].clone()),
        None => {
            term.clear_to_end_of_screen()?;
            Ok("".to_string())
        }
    }
}
