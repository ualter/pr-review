use clap::{Args, Parser, Subcommand, ValueEnum};
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use crate::config::user_config;
use crate::scm::ScmKind;

#[derive(Parser)]
#[command(name = "pr-review")]
#[command(about = "AI-assisted PR/commit review CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Args, Clone)]
pub struct SessionArgs {
    #[arg(long)]
    pub ai: AiTool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Display the startup banner
    Banner,

    /// Manage user configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },

    /// Check local environment and external CLI dependencies
    Doctor,

    /// Review an AWS CodeCommit Pull Request
    Pr {
        pr_id: String,

        /// SCM provider used to resolve PR metadata
        #[arg(long, value_name = "SCM")]
        scm: Option<ScmKind>,

        #[command(flatten)]
        common: CommonArgs,
    },

    /// Review a single Git commit by SHA
    Commit {
        sha: String,

        #[command(flatten)]
        common: CommonArgs,
    },

    /// Manage interactive review sessions
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Create ~/.pr-review/config.toml if it does not exist
    Init,
}

#[derive(Subcommand)]
pub enum SessionCommand {
    /// List existing review sessions
    List,

    /// Resume an existing interactive review session
    Resume {
        /// Existing review name. If omitted, an interactive picker is shown.
        review_name: Option<String>,

        /// AI tool used for the interactive session
        #[arg(long, value_name = "TOOL")]
        ai: Option<AiTool>,
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

    /// Return to the shell after the review instead of entering interactive chat
    #[arg(long)]
    pub no_interactive: bool,
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

    pub fn status_icon(&self) -> &str {
        match self {
            AiTool::Copilot => user_config()
                .copilot_icon
                .as_deref()
                .unwrap_or("🧑‍✈️"),
            AiTool::Codex => user_config()
                .codex_icon
                .as_deref()
                .unwrap_or("🤖"),
        }
    }

    pub fn manual_hint(&self, prompt_path: &std::path::Path) -> String {
        match self {
            AiTool::Copilot => format!("copilot -p \"$(cat {})\"", prompt_path.display()),
            AiTool::Codex => format!("codex review - < {}", prompt_path.display()),
        }
    }

    pub fn from_config_value(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "copilot" => Ok(AiTool::Copilot),
            "codex" => Ok(AiTool::Codex),
            other => Err(anyhow!(
                "Invalid AI tool `{other}` in config. Expected `copilot` or `codex`."
            )),
        }
    }
}

pub struct ReviewInput {
    pub diff: String,
    pub metadata: String,
    pub prompt_scope: String,
    pub artifact_prefix: String,
    pub review_kind: String,
    pub repository: String,
    pub source: String,
    pub target: String,
    pub review_ref: String,
    pub remote: String,
    pub pr_id: Option<String>,
    pub sha: Option<String>,
}
