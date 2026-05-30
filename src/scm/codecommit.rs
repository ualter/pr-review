use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::artifacts::run_command;
use crate::cli::CommonArgs;
use crate::scm::{PrContext, ScmProvider};

pub struct CodeCommitProvider;

impl ScmProvider for CodeCommitProvider {
    fn resolve_pr_context(&self, pr_id: &str, common: &CommonArgs) -> Result<PrContext> {
        let json = run_command(
            &common.repo_path,
            "aws",
            &["codecommit", "get-pull-request", "--pull-request-id", pr_id],
        )?;

        let parsed: Value = serde_json::from_str(&json)?;
        let pr = &parsed["pullRequest"];

        let target = pr["pullRequestTargets"]
            .as_array()
            .and_then(|items| items.first())
            .ok_or_else(|| anyhow!("No pullRequestTargets found"))?;

        let source = target["sourceReference"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing sourceReference"))?;

        let destination = target["destinationReference"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing destinationReference"))?;

        let source_branch = clean_branch_name(source);
        let target_branch = clean_branch_name(destination);
        let repository = target["repositoryName"]
            .as_str()
            .unwrap_or("unknown-repository")
            .to_string();

        Ok(PrContext {
            metadata: format!(
                "Review target: CodeCommit PR #{pr_id}\nRepository: {repository}\nSource branch: {source_branch}\nDestination branch: {target_branch}"
            ),
            review_ref: format!("review/pr-{pr_id}"),
            repository,
            source_branch,
            target_branch,
        })
    }
}

fn clean_branch_name(reference: &str) -> String {
    reference
        .strip_prefix("refs/heads/")
        .unwrap_or(reference)
        .to_string()
}
