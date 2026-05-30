use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::env;

use crate::artifacts::run_command;
use crate::cli::CommonArgs;
use crate::scm::{PrContext, ScmKind, ScmProvider};

pub struct BitbucketProvider;

impl ScmProvider for BitbucketProvider {
    fn resolve_pr_context(&self, pr_id: &str, common: &CommonArgs) -> Result<PrContext> {
        let bb_url = required_env("BB_URL")?;
        let bb_token = required_env("BB_TOKEN")?;
        let bb_project = required_env("BB_PROJECT")?;
        let bb_repo = required_env("BB_REPO")?;
        let endpoint = format!(
            "{}/rest/api/1.0/projects/{}/repos/{}/pull-requests/{}",
            bb_url.trim_end_matches('/'),
            bb_project,
            bb_repo,
            pr_id
        );

        let json = run_command(
            &common.repo_path,
            "curl",
            &[
                "-s",
                "-H",
                &format!("Authorization: Bearer {bb_token}"),
                &endpoint,
            ],
        )
        .with_context(|| {
            format!(
                "Failed to fetch Bitbucket PR metadata for PR `{}` from `{}`.\n\n\
Make sure these environment variables are set correctly:\n\
- `BB_URL`\n\
- `BB_TOKEN`\n\
- `BB_PROJECT`\n\
- `BB_REPO`",
                pr_id, endpoint
            )
        })?;

        let parsed: Value = serde_json::from_str(&json)?;

        let source_branch = pr_branch_name(&parsed, true)
            .ok_or_else(|| anyhow!("Missing Bitbucket source branch in PR metadata"))?;
        let target_branch = pr_branch_name(&parsed, false)
            .ok_or_else(|| anyhow!("Missing Bitbucket destination branch in PR metadata"))?;
        let repository = pr_repository_name(&parsed)
            .unwrap_or_else(|| "unknown-repository".to_string());

        Ok(PrContext {
            metadata: format!(
                "Review target: Bitbucket PR #{pr_id}\nRepository: {repository}\nSource branch: {source_branch}\nDestination branch: {target_branch}"
            ),
            review_ref: ScmKind::Bitbucket.review_ref(pr_id),
            repository,
            source_branch,
            target_branch,
        })
    }
}

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("Required Bitbucket environment variable `{name}` is not set"))
}

fn pr_branch_name(pr: &Value, source: bool) -> Option<String> {
    if source {
        pr.pointer("/source/branch/name")
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
            .or_else(|| {
                pr.pointer("/fromRef/displayId")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string)
            })
    } else {
        pr.pointer("/destination/branch/name")
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
            .or_else(|| {
                pr.pointer("/toRef/displayId")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string)
            })
    }
}

fn pr_repository_name(pr: &Value) -> Option<String> {
    pr.pointer("/destination/repository/full_name")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .or_else(|| {
            pr.pointer("/destination/repository/name")
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
        })
        .or_else(|| {
            let project = pr
                .pointer("/toRef/repository/project/key")
                .and_then(|v| v.as_str());
            let slug = pr
                .pointer("/toRef/repository/slug")
                .and_then(|v| v.as_str());

            match (project, slug) {
                (Some(project), Some(slug)) => Some(format!("{project}/{slug}")),
                (_, Some(slug)) => Some(slug.to_string()),
                _ => None,
            }
        })
        .or_else(|| {
            pr.pointer("/source/repository/full_name")
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
        })
}
