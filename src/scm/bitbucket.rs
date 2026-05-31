use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::env;

use crate::artifacts::run_command;
use crate::cli::CommonArgs;
use crate::config::user_config;
use crate::scm::{PrContext, ScmKind, ScmProvider};
use crate::ui::{BLACK_BOLD, BLUE, GREEN_BOLD, RED_BOLD, RESET, YELLOW_BOLD};

pub struct BitbucketProvider;

impl ScmProvider for BitbucketProvider {
    fn resolve_pr_context(&self, pr_id: &str, common: &CommonArgs) -> Result<PrContext> {
        let bb_url = resolve_setting(
            common.bb_url.as_deref(),
            "BB_URL",
            user_config().bitbucket_url.as_deref(),
            "bitbucket.url",
        )?;
        let bb_token = required_env("BB_TOKEN")?;
        let bb_project = resolve_setting(
            common.bb_project.as_deref(),
            "BB_PROJECT",
            user_config().bitbucket_project.as_deref(),
            "bitbucket.project",
        )?;
        let bb_repo = resolve_setting(
            common.bb_repo.as_deref(),
            "BB_REPO",
            user_config().bitbucket_repo.as_deref(),
            "bitbucket.repo",
        )?;
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
                "{RED_BOLD}Failed to fetch Bitbucket PR metadata{RESET} for PR `{}` from `{}`.\n\n\
{YELLOW_BOLD}Make sure these settings are configured in `~/.pr-review/config.toml`:{RESET}\n\
- `[bitbucket].url`\n\
- `[bitbucket].project`\n\
- `[bitbucket].repo`\n\
{GREEN_BOLD}Or override them for this run with:{RESET}\n\
- `--bb-url`\n\
- `--bb-project`\n\
- `--bb-repo`\n\
{GREEN_BOLD}And make sure this environment variable is set:{RESET}\n\
- `BB_TOKEN`",
                pr_id, endpoint
            )
        })?;

        let parsed: Value = serde_json::from_str(&json)?;

        let source_branch = pr_branch_name(&parsed, true)
            .ok_or_else(|| anyhow!("Missing Bitbucket source branch in PR metadata"))?;
        let target_branch = pr_branch_name(&parsed, false)
            .ok_or_else(|| anyhow!("Missing Bitbucket destination branch in PR metadata"))?;
        let repository =
            pr_repository_name(&parsed).unwrap_or_else(|| "unknown-repository".to_string());

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

    fn resolve_pr_diff(
        &self,
        pr_id: &str,
        common: &CommonArgs,
        _context: &PrContext,
    ) -> Result<String> {
        let bb_url = resolve_setting(
            common.bb_url.as_deref(),
            "BB_URL",
            user_config().bitbucket_url.as_deref(),
            "bitbucket.url",
        )?;
        let bb_token = required_env("BB_TOKEN")?;
        let bb_project = resolve_setting(
            common.bb_project.as_deref(),
            "BB_PROJECT",
            user_config().bitbucket_project.as_deref(),
            "bitbucket.project",
        )?;
        let bb_repo = resolve_setting(
            common.bb_repo.as_deref(),
            "BB_REPO",
            user_config().bitbucket_repo.as_deref(),
            "bitbucket.repo",
        )?;
        let endpoint = format!(
            "{}/rest/api/1.0/projects/{}/repos/{}/pull-requests/{}/diff",
            bb_url.trim_end_matches('/'),
            bb_project,
            bb_repo,
            pr_id
        );

        run_command(
            &common.repo_path,
            "curl",
            &[
                "-s",
                "-H",
                &format!("Authorization: Bearer {bb_token}"),
                "-H",
                "Accept: text/plain",
                &endpoint,
            ],
        )
        .with_context(|| {
            format!(
                "{RED_BOLD}Failed to fetch Bitbucket PR diff{RESET} for PR `{}` from `{}`.\n\n\
{YELLOW_BOLD}Make sure these settings are configured in `~/.pr-review/config.toml`:{RESET}\n\
- `[bitbucket].url`\n\
- `[bitbucket].project`\n\
- `[bitbucket].repo`\n\
{GREEN_BOLD}Or override them for this run with:{RESET}\n\
- `--bb-url`\n\
- `--bb-project`\n\
- `--bb-repo`\n\
{GREEN_BOLD}And make sure this environment variable is set:{RESET}\n\
- `BB_TOKEN`",
                pr_id, endpoint
            )
        })
    }
}

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| {
        format!("{RED_BOLD}Required Bitbucket environment variable {BLACK_BOLD}`{name}`{RED_BOLD} is not set{RESET}")
    })
}

fn resolve_setting(
    cli_value: Option<&str>,
    env_name: &str,
    config_value: Option<&str>,
    config_name: &str,
) -> Result<String> {
    if let Some(value) = non_empty(cli_value) {
        return Ok(value.to_string());
    }

    if let Some(value) = env::var(env_name)
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(value);
    }

    if let Some(value) = non_empty(config_value) {
        return Ok(value.to_string());
    }

    anyhow::bail!(
        "{RED_BOLD}Required Bitbucket setting is missing.{RESET}\n\
Set one of:\n\
{BLUE}- CLI override: {BLACK_BOLD}`--{}`{RESET}\n\
{BLUE}- environment variable: {BLACK_BOLD}`{}`{RESET}\n\
{BLUE}- config key in {BLACK_BOLD}`~/.pr-review/config.toml`{RESET}: {BLACK_BOLD}`{}`{RESET}",
        cli_flag_name(env_name),
        env_name,
        config_name,
    )
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

fn cli_flag_name(env_name: &str) -> &'static str {
    match env_name {
        "BB_URL" => "bb-url",
        "BB_PROJECT" => "bb-project",
        "BB_REPO" => "bb-repo",
        _ => "unknown",
    }
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
