use clap::{Args, Parser, Subcommand};
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

    /// Execute Copilot after generating prompt
    #[arg(long)]
    pub run_copilot: bool,
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
