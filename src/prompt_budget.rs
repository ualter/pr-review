use crate::{
    cli::ReviewInput,
    ui::{
        BLACK_BOLD, BLUE, BLUE_BOLD, FG_WHITE_BOLD, GREEN_BOLD, LINE, RED_BOLD, RESET,
        YELLOW_BOLD,
    },
};
use dialoguer::console::measure_text_width;
use std::io::{self, Write};

const BG_BUDGET_DARK: &str = "\x1b[48;5;234m";
const FG_BUDGET_TITLE: &str = GREEN_BOLD;
const FG_BUDGET_LABEL: &str = "\x1b[38;5;252m";
const FG_BUDGET_VALUE: &str = BLUE_BOLD;
const FG_BUDGET_DIVIDER: &str = BLUE;
const FG_BUDGET_FILE: &str = BLACK_BOLD;

#[derive(Debug, Clone)]
pub struct PromptBudgetReport {
    pub rules_profile_tokens: usize,
    pub metadata_tokens: usize,
    pub diff_tokens: usize,
    pub total_tokens: usize,
    pub prompt_bytes: usize,
    pub risk_level: PromptRiskLevel,
    pub largest_files: Vec<FileBudgetEntry>,
}

#[derive(Debug, Clone)]
pub struct FileBudgetEntry {
    pub path: String,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum PromptRiskLevel {
    Low,
    Medium,
    High,
}

pub enum PromptBudgetDecision {
    Proceed,
    Cancelled,
}

pub fn analyze_prompt_budget(input: &ReviewInput, prompt: &str) -> PromptBudgetReport {
    let total_tokens = estimate_tokens(prompt);
    let metadata_tokens = estimate_tokens(&input.metadata);
    let diff_tokens = estimate_tokens(&input.diff);
    let rules_profile_tokens = total_tokens.saturating_sub(metadata_tokens + diff_tokens);
    let prompt_bytes = prompt.len();
    let risk_level = PromptRiskLevel::from_total_tokens(total_tokens);
    let largest_files = analyze_largest_files(&input.diff);

    PromptBudgetReport {
        rules_profile_tokens,
        metadata_tokens,
        diff_tokens,
        total_tokens,
        prompt_bytes,
        risk_level,
        largest_files,
    }
}

pub fn print_prompt_budget(report: &PromptBudgetReport) {
    let rules = format_tokens(report.rules_profile_tokens);
    let metadata = format_tokens(report.metadata_tokens);
    let diff = format_tokens(report.diff_tokens);
    let total = format_tokens(report.total_tokens);
    let prompt_size = format_approx_bytes(report.prompt_bytes as u64);
    let value_width = [
        rules.len(),
        metadata.len(),
        diff.len(),
        total.len(),
        prompt_size.len(),
        report.risk_level.label().len(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);

    print_budget_line(&format!("{FG_BUDGET_TITLE} Prompt Budget {FG_WHITE_BOLD}"));
    print_budget_line(&format!("{FG_BUDGET_DIVIDER} ------------- {FG_WHITE_BOLD}"));
    print_budget_line(&format_budget_row("Rules/Profile:", &rules, value_width));
    print_budget_line(&format_budget_row("Metadata:", &metadata, value_width));
    print_budget_line(&format_budget_row("Diff:", &diff, value_width));
    print_budget_line(&format!(
        "{FG_BUDGET_DIVIDER} -------------------------------- {FG_WHITE_BOLD}"
    ));
    print_budget_line(&format_budget_row("Total:", &total, value_width));
    print_budget_line(" ");
    print_budget_line(&format_budget_row("Prompt Size:", &prompt_size, value_width));
    print_budget_line(&format!(
        " {FG_BUDGET_LABEL}{:<14}{FG_WHITE_BOLD} {}{:>width$}{} ",
        "Risk Level:",
        report.risk_level.color(),
        report.risk_level.label(),
        FG_WHITE_BOLD,
        width = value_width
    ));

    if !report.largest_files.is_empty() {
        print_budget_line(" ");
        print_budget_line(&format!("{FG_BUDGET_TITLE} Largest Files: {FG_WHITE_BOLD}"));
        for (idx, file) in report.largest_files.iter().enumerate() {
            print_budget_line(&format!(
                " {FG_BUDGET_FILE}{}. {:<30}{FG_BUDGET_VALUE} {:>width$}{FG_WHITE_BOLD} ",
                idx + 1,
                truncate_path(&file.path, 30),
                format_tokens(file.estimated_tokens),
                width = value_width
            ));
        }
    }
    println!("{BLUE_BOLD}{LINE}{RESET}");
}

fn format_budget_row(label: &str, value: &str, value_width: usize) -> String {
    format!(
        " {FG_BUDGET_LABEL}{:<14}{FG_BUDGET_VALUE} {:>width$}{FG_WHITE_BOLD} ",
        label,
        value,
        width = value_width
    )
}

fn print_budget_line(content: &str) {
    let panel_width = LINE.chars().count();
    let visible_width = measure_text_width(content);
    let padding = " ".repeat(panel_width.saturating_sub(visible_width));
    println!("{BG_BUDGET_DARK}{FG_WHITE_BOLD}{content}{padding}{RESET}");
}

impl PromptRiskLevel {
    fn from_total_tokens(tokens: usize) -> Self {
        match tokens {
            0..=15_999 => Self::Low,
            16_000..=39_999 => Self::Medium,
            _ => Self::High,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
        }
    }

    fn color(&self) -> &'static str {
        match self {
            Self::Low => GREEN_BOLD,
            Self::Medium => YELLOW_BOLD,
            Self::High => RED_BOLD,
        }
    }

    pub fn is_high(&self) -> bool {
        matches!(self, Self::High)
    }
}

fn analyze_largest_files(diff: &str) -> Vec<FileBudgetEntry> {
    let mut current_path: Option<String> = None;
    let mut current_lines: Vec<&str> = Vec::new();
    let mut files = Vec::new();

    for line in diff.lines() {
        if let Some(path) = parse_diff_header(line) {
            if let Some(previous_path) = current_path.take() {
                files.push(FileBudgetEntry {
                    estimated_tokens: estimate_tokens(&current_lines.join("\n")),
                    path: previous_path,
                });
                current_lines.clear();
            }
            current_path = Some(path);
        }

        if current_path.is_some() {
            current_lines.push(line);
        }
    }

    if let Some(path) = current_path {
        files.push(FileBudgetEntry {
            estimated_tokens: estimate_tokens(&current_lines.join("\n")),
            path,
        });
    }

    files.sort_by_key(|entry| std::cmp::Reverse(entry.estimated_tokens));
    files.truncate(3);
    files
}

fn parse_diff_header(line: &str) -> Option<String> {
    let mut parts = line.split_whitespace();
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("diff"), Some("--git"), Some(_a_path), Some(b_path)) => {
            Some(b_path.trim_start_matches("b/").to_string())
        }
        _ => None,
    }
}

fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

fn format_tokens(tokens: usize) -> String {
    if tokens >= 1_000 {
        format!("~{:.1}k tokens", tokens as f64 / 1_000.0)
    } else {
        format!("~{} tokens", tokens)
    }
}

fn format_approx_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;

    let bytes_f64 = bytes as f64;
    if bytes_f64 >= MB {
        format!("~{:.1} MB", bytes_f64 / MB)
    } else if bytes_f64 >= KB {
        format!("~{:.1} KB", bytes_f64 / KB)
    } else {
        format!("~{} B", bytes)
    }
}

fn truncate_path(path: &str, max_width: usize) -> String {
    if path.chars().count() <= max_width {
        return path.to_string();
    }

    let tail_width = max_width.saturating_sub(1);
    let tail: String = path
        .chars()
        .rev()
        .take(tail_width)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

pub fn confirm_prompt_budget(report: &PromptBudgetReport) -> Result<PromptBudgetDecision, io::Error> {
    if !report.risk_level.is_high() {
        return Ok(PromptBudgetDecision::Proceed);
    }

    print!(
        "{}High-cost prompt detected. Proceed with AI review? [y/N]: {}",
        YELLOW_BOLD, RESET
    );
    io::stdout().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;

    let answer = answer.trim().to_ascii_lowercase();
    if answer == "y" || answer == "yes" {
        Ok(PromptBudgetDecision::Proceed)
    } else {
        Ok(PromptBudgetDecision::Cancelled)
    }
}
