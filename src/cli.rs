use anyhow::{anyhow, Result};
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use std::{fmt, path::PathBuf};

use crate::config::user_config;
use crate::scm::ScmKind;

#[derive(Parser)]
#[command(name = "pr-review")]
#[command(about = "AI-assisted PR/commit review CLI")]
#[command(disable_version_flag = true)]
pub struct Cli {
    #[arg(long = "version", short = 'V', visible_short_alias = 'v', action = ArgAction::SetTrue)]
    pub version: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Args, Clone)]
pub struct SessionArgs {
    #[arg(long)]
    pub ai: AiTool,

    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,
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

    /// Manage prompt profile templates
    Prompt {
        #[command(subcommand)]
        command: PromptCommand,
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
        command: Option<SessionCommand>,

        /// Existing review name. If omitted, an interactive picker is shown.
        review_name: Option<String>,

        /// AI tool used for the interactive session
        #[arg(long, value_name = "TOOL")]
        ai: Option<AiTool>,

        /// Override the model for the selected AI tool
        #[arg(long, value_name = "MODEL")]
        model: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Create ~/.pr-review/config.toml if it does not exist
    Init,
}

#[derive(Subcommand)]
pub enum PromptCommand {
    /// Create user-level prompt profile templates under ~/.pr-review/prompts/<scm>/
    Init {
        /// SCM name for the prompt profile location
        #[arg(long, value_name = "SCM")]
        scm: ScmKind,

        /// Repository name for the repo-specific prompt profile
        #[arg(long, value_name = "REPO")]
        repo: String,
    },

    /// Show the built-in default prompt, or the resolved prompt for a given SCM/repository
    Show {
        /// SCM name for prompt resolution
        #[arg(long, value_name = "SCM")]
        scm: Option<ScmKind>,

        /// Repository name for prompt resolution
        #[arg(long, value_name = "REPO")]
        repo: Option<String>,

        /// Repo path used to include a repo-local .pr-review/prompt.toml if present
        #[arg(long, default_value = ".")]
        repo_path: PathBuf,
    },
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

        /// Override the model for the selected AI tool
        #[arg(long, value_name = "MODEL")]
        model: Option<String>,
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

    /// Override Bitbucket base URL for this run
    #[arg(long, value_name = "URL")]
    pub bb_url: Option<String>,

    /// Override Bitbucket project key for this run
    #[arg(long, value_name = "PROJECT")]
    pub bb_project: Option<String>,

    /// Override Bitbucket repository slug for this run
    #[arg(long, value_name = "REPO")]
    pub bb_repo: Option<String>,

    /// AI tool to run after generating the prompt (omit to only generate artifacts)
    #[arg(long, value_name = "TOOL")]
    pub ai: Option<AiTool>,

    /// Override the model for the selected AI tool
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,

    /// Return to the shell after the review instead of entering interactive chat
    #[arg(long)]
    pub no_interactive: bool,
}

#[derive(Clone, Copy, ValueEnum, Debug)]
pub enum AiTool {
    /// GitHub Copilot CLI (`copilot -p`)
    Copilot,
    /// GitHub Copilot SDK backend (feature-gated experimental path)
    #[cfg(feature = "copilot-sdk")]
    CopilotSdk,
    /// OpenAI Codex CLI (`codex`)
    Codex,
}

impl AiTool {
    pub fn display_name(&self) -> &'static str {
        match self {
            AiTool::Copilot => "Copilot",
            #[cfg(feature = "copilot-sdk")]
            AiTool::CopilotSdk => "Copilot SDK",
            AiTool::Codex => "Codex",
        }
    }

    pub fn status_icon(&self) -> &str {
        match self {
            AiTool::Copilot => user_config()
                .copilot_icon
                .as_deref()
                .unwrap_or("🧑‍✈️"),
            #[cfg(feature = "copilot-sdk")]
            AiTool::CopilotSdk => user_config()
                .copilot_icon
                .as_deref()
                .unwrap_or("🧑‍✈️"),
            AiTool::Codex => user_config()
                .codex_icon
                .as_deref()
                .unwrap_or("🤖"),
        }
    }

    pub fn shows_live_status_updates(&self) -> bool {
        match self {
            AiTool::Copilot => false,
            #[cfg(feature = "copilot-sdk")]
            AiTool::CopilotSdk => true,
            AiTool::Codex => false,
        }
    }

    pub fn manual_hint(&self, prompt_path: &std::path::Path) -> String {
        match self {
            AiTool::Copilot => format!("copilot -p \"$(cat {})\"", prompt_path.display()),
            #[cfg(feature = "copilot-sdk")]
            AiTool::CopilotSdk => format!(
                "This build includes the experimental Copilot SDK backend; no manual CLI equivalent is exposed for {}.",
                prompt_path.display()
            ),
            AiTool::Codex => format!("codex review - < {}", prompt_path.display()),
        }
    }

    pub fn from_config_value(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "copilot" => Ok(AiTool::Copilot),
            #[cfg(feature = "copilot-sdk")]
            "copilot-sdk" | "copilotsdk" => Ok(AiTool::CopilotSdk),
            "codex" => Ok(AiTool::Codex),
            other => Err(anyhow!(
                "Invalid AI tool `{other}` in config. Expected {}.",
                expected_ai_tool_values()
            )),
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            AiTool::Copilot => "gpt-5.4",
            #[cfg(feature = "copilot-sdk")]
            AiTool::CopilotSdk => "gpt-5.4",
            AiTool::Codex => "gpt-5-codex",
        }
    }

    pub fn configured_model(&self) -> Option<String> {
        let config = user_config();
        match self {
            AiTool::Copilot => config.copilot_model.clone(),
            #[cfg(feature = "copilot-sdk")]
            AiTool::CopilotSdk => config.copilot_sdk_model.clone(),
            AiTool::Codex => config.codex_model.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AiRuntime {
    pub tool: AiTool,
    pub model: String,
}

impl AiRuntime {
    pub fn resolve(tool: AiTool, cli_model: Option<&str>) -> Self {
        let model = cli_model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .or_else(|| tool.configured_model())
            .unwrap_or_else(|| tool.default_model().to_string());

        Self { tool, model }
    }

    pub fn display_name(&self) -> &'static str {
        self.tool.display_name()
    }

    pub fn status_icon(&self) -> &str {
        self.tool.status_icon()
    }

    pub fn shows_live_status_updates(&self) -> bool {
        self.tool.shows_live_status_updates()
    }

    pub fn manual_hint(&self, prompt_path: &std::path::Path) -> String {
        self.tool.manual_hint(prompt_path)
    }
}

impl fmt::Display for AiRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} 🧠 {}", self.display_name(), self.model)
    }
}

fn expected_ai_tool_values() -> &'static str {
    #[cfg(feature = "copilot-sdk")]
    {
        "`copilot`, `codex`, or `copilot-sdk`"
    }

    #[cfg(not(feature = "copilot-sdk"))]
    {
        "`copilot` or `codex`"
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
