pub mod bitbucket;
pub mod codecommit;

use anyhow::{anyhow, Result};
use clap::ValueEnum;

use crate::cli::CommonArgs;

#[derive(Debug, Clone)]
pub struct PrContext {
    pub repository: String,
    pub source_branch: String,
    pub target_branch: String,
    pub review_ref: String,
    pub metadata: String,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum ScmKind {
    #[value(name = "codecommit")]
    CodeCommit,

    #[value(name = "bitbucket")]
    Bitbucket,
}

pub trait ScmProvider {
    fn resolve_pr_context(&self, pr_id: &str, common: &CommonArgs) -> Result<PrContext>;
    fn resolve_pr_diff(
        &self,
        pr_id: &str,
        common: &CommonArgs,
        context: &PrContext,
    ) -> Result<String>;
}

impl ScmKind {
    pub fn display_name(&self) -> &'static str {
        match self {
            ScmKind::CodeCommit => "CodeCommit",
            ScmKind::Bitbucket => "Bitbucket",
        }
    }

    pub fn artifact_prefix(&self, pr_id: &str) -> String {
        match self {
            ScmKind::CodeCommit => format!("codecommit-pr-{pr_id}"),
            ScmKind::Bitbucket => format!("bitbucket-pr-{pr_id}"),
        }
    }

    pub fn review_ref(&self, pr_id: &str) -> String {
        match self {
            ScmKind::CodeCommit => format!("review/pr-{pr_id}"),
            ScmKind::Bitbucket => format!("review/bitbucket-pr-{pr_id}"),
        }
    }

    pub fn from_config_value(value: &str) -> Result<Self> {
        match value.trim().to_lowercase().as_str() {
            "codecommit" => Ok(ScmKind::CodeCommit),
            "bitbucket" => Ok(ScmKind::Bitbucket),
            other => Err(anyhow!(
                "Invalid SCM `{other}` in config. Expected `codecommit` or `bitbucket`."
            )),
        }
    }
}
