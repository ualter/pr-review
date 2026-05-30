pub mod codecommit;

use anyhow::Result;

use crate::cli::CommonArgs;

#[derive(Debug, Clone)]
pub struct PrContext {
    pub repository: String,
    pub source_branch: String,
    pub target_branch: String,
    pub review_ref: String,
    pub metadata: String,
}

#[derive(Debug, Clone, Copy)]
pub enum ScmKind {
    CodeCommit,
}

pub trait ScmProvider {
    fn resolve_pr_context(&self, pr_id: &str, common: &CommonArgs) -> Result<PrContext>;
}

pub fn current_scm_kind() -> ScmKind {
    ScmKind::CodeCommit
}
