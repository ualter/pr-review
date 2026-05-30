use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "pr-review")]
#[command(about = "AI-assisted PR/commit review CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Review an AWS CodeCommit Pull Request
    Pr {
        pr_id: String,

        #[command(flatten)]
        common: CommonArgs,
    },

    /// Review a single Git commit by SHA
    Commit {
        sha: String,

        #[command(flatten)]
        common: CommonArgs,
    },
}

#[derive(Args, Clone)]
pub struct CommonArgs {
    /// Git remote name
    #[arg(long, default_value = "origin")]
    pub remote: String,

    /// Path to the repository
    #[arg(long, default_value = ".")]
    pub repo_path: PathBuf,

    /// AI tool to run after generating the prompt (omit to only generate artifacts)
    #[arg(long, value_name = "TOOL")]
    pub ai: Option<AiTool>,
}

#[derive(Clone, ValueEnum, Debug)]
pub enum AiTool {
    /// GitHub Copilot CLI (`copilot -p`)
    Copilot,
    /// OpenAI Codex CLI (`codex`)
    Codex,
}

impl AiTool {
    pub fn display_name(&self) -> &'static str {
        match self {
            AiTool::Copilot => "Copilot",
            AiTool::Codex => "Codex",
        }
    }

    pub fn manual_hint(&self, prompt_path: &std::path::Path) -> String {
        match self {
            AiTool::Copilot => format!(
                "copilot -p \"$(cat {})\"",
                prompt_path.display()
            ),
            AiTool::Codex => format!(
                "codex review - < {}",
                prompt_path.display()
            ),
        }
    }
}

pub struct ReviewInput {
    pub diff: String,
    pub metadata: String,
    pub prompt_scope: String,
    pub artifact_prefix: String,
    pub repository: String,
    pub source: String,
    pub target: String,
    pub review_ref: String,
}
