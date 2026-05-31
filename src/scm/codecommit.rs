use anyhow::{anyhow, Context, Result};
use serde_json::Value;

use crate::artifacts::run_command;
use crate::cli::CommonArgs;
use crate::scm::{PrContext, ScmProvider};
use crate::ui::{BLUE, GREEN_BOLD, RED_BOLD, RESET, YELLOW_BOLD};

pub struct CodeCommitProvider;

impl ScmProvider for CodeCommitProvider {
    fn resolve_pr_context(&self, pr_id: &str, common: &CommonArgs) -> Result<PrContext> {
        let json = run_command(
            &common.repo_path,
            "aws",
            &["codecommit", "get-pull-request", "--pull-request-id", pr_id],
        )
        .map_err(|err| rewrite_codecommit_auth_error(err, pr_id))?;

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

    fn resolve_pr_diff(
        &self,
        _pr_id: &str,
        common: &CommonArgs,
        context: &PrContext,
    ) -> Result<String> {
        run_command(
            &common.repo_path,
            "git",
            &["fetch", &common.remote, &context.source_branch],
        )
        .with_context(|| {
            format!(
                "{RED_BOLD}Failed to fetch source branch{RESET} `{}` {RED_BOLD}from remote{RESET} `{}` {RED_BOLD}in{RESET} `{}`.\n\n\
{YELLOW_BOLD}This usually means:{BLUE}\n\
- `--repo-path` points to the wrong repository\n\
- `--remote` is not the configured remote for this PR\n\
- the source branch was deleted after the PR was opened\n\n\
{GREEN_BOLD}Try checking:{BLUE}\n\
- the repository path\n\
- `git remote -v`\n\
- whether the branch exists on that remote",
                context.source_branch,
                common.remote,
                common.repo_path.display()
            )
        })?;

        run_command(
            &common.repo_path,
            "git",
            &["fetch", &common.remote, &context.target_branch],
        )
        .with_context(|| {
            format!(
                "{RED_BOLD}Failed to fetch destination branch{RESET} `{}` {RED_BOLD}from remote{RESET} `{}` {RED_BOLD}in{RESET} `{}`.\n\n\
{YELLOW_BOLD}This usually means:{BLUE}\n\
- `--repo-path` points to the wrong repository\n\
- `--remote` is not the configured remote for this PR\n\
- the destination branch no longer exists on that remote\n\n\
{GREEN_BOLD}Try checking:{BLUE}\n\
- the repository path\n\
- `git remote -v`\n\
- whether the branch exists on that remote",
                context.target_branch,
                common.remote,
                common.repo_path.display()
            )
        })?;

        run_command(
            &common.repo_path,
            "git",
            &[
                "diff",
                &format!("{}/{}", common.remote, context.target_branch),
                &format!("{}/{}", common.remote, context.source_branch),
                "--",
            ],
        )
    }
}

fn rewrite_codecommit_auth_error(err: anyhow::Error, pr_id: &str) -> anyhow::Error {
    let raw = err.to_string();

    if raw.contains("ExpiredTokenException") {
        return anyhow!(
            "{RED_BOLD}AWS session expired while fetching CodeCommit PR metadata{RESET} for PR `{}`.\n\n\
{YELLOW_BOLD}What happened:{BLUE}\n\
- your AWS CLI credentials/session token is no longer valid\n\
- `aws codecommit get-pull-request` could not authenticate\n\n\
{GREEN_BOLD}What to do:{BLUE}\n\
- refresh or re-login your AWS session\n\
- verify it with `aws sts get-caller-identity`\n\
- then rerun `pr-review`\n\n\
{YELLOW_BOLD}Original AWS error:{RESET}\n{}",
            pr_id,
            raw
        );
    }

    if raw.contains("InvalidClientTokenId") || raw.contains("UnrecognizedClientException") {
        return anyhow!(
            "{RED_BOLD}AWS credentials are invalid while fetching CodeCommit PR metadata{RESET} for PR `{}`.\n\n\
{YELLOW_BOLD}What happened:{BLUE}\n\
- the AWS CLI credentials currently in use were rejected\n\
- `aws codecommit get-pull-request` could not authenticate this request\n\n\
{GREEN_BOLD}What to do:{BLUE}\n\
- refresh or re-login your AWS session\n\
- check the active AWS profile and environment variables\n\
- verify it with `aws sts get-caller-identity`\n\
- then rerun `pr-review`\n\n\
{YELLOW_BOLD}Original AWS error:{RESET}\n{}",
            pr_id,
            raw
        );
    }

    err
}

fn clean_branch_name(reference: &str) -> String {
    reference
        .strip_prefix("refs/heads/")
        .unwrap_or(reference)
        .to_string()
}
