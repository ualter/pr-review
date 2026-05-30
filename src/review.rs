use anyhow::{anyhow, Context, Result};

use crate::artifacts::{repo_name, run_command};
use crate::cli::{CommonArgs, ReviewInput};
use crate::scm::codecommit::CodeCommitProvider;
use crate::scm::{current_scm_kind, ScmKind, ScmProvider};
use crate::ui::{
    BLACK_BOLD, BLUE, BLUE_BOLD, GREEN_BOLD, LINE, RED_BOLD, RESET, YELLOW, YELLOW_BOLD,
};

pub fn review_pr(pr_id: &str, common: &CommonArgs) -> Result<ReviewInput> {
    println!("\n{LINE}");
    println!("{YELLOW}Validating PR ID {BLUE_BOLD}{pr_id}...{RESET}");
    println!("{YELLOW}Fetching CodeCommit PR metadata...{RESET}");
    let provider = resolve_scm_provider();
    let context = provider.resolve_pr_context(pr_id, common)?;

    println!("{YELLOW}Fetching branches...{RESET}");

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
- `--remote` is not the CodeCommit remote for this PR\n\
- the source branch was deleted after the PR was opened\n\n\
{GREEN_BOLD}Try checking:{BLUE}\n\
- the repository path\n\
- `git remote -v`\n\
- whether the branch exists on that remote{BLACK_BOLD}",
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
- `--remote` is not the CodeCommit remote for this PR\n\
- the destination branch no longer exists on that remote\n\n\
{GREEN_BOLD}Try checking:{BLUE}\n\
- the repository path\n\
- `git remote -v`\n\
- whether the branch exists on that remote{BLACK_BOLD}",
            context.target_branch,
            common.remote,
            common.repo_path.display()
        )
    })?;

    let diff = run_command(
        &common.repo_path,
        "git",
        &[
            "diff",
            &format!("{}/{}", common.remote, context.target_branch),
            &format!("{}/{}", common.remote, context.source_branch),
            "--",
        ],
    )?;

    ensure_non_empty_diff(&diff)?;

    Ok(ReviewInput {
        diff,
        metadata: context.metadata,
        prompt_scope: "Review ONLY the changes contained in this PR diff file. Treat this diff as the source of truth.".to_string(),
        artifact_prefix: format!("codecommit-pr-{pr_id}"),
        review_kind: "pr".to_string(),
        repository: context.repository,
        source: context.source_branch,
        target: context.target_branch,
        review_ref: context.review_ref,
        remote: common.remote.clone(),
        pr_id: Some(pr_id.to_string()),
        sha: None,
    })
}

pub fn review_commit(sha: &str, common: &CommonArgs) -> Result<ReviewInput> {
    println!("\n{LINE}");
    println!("{YELLOW}Validating commit {sha}...{RESET}");

    // The ^{commit} syntax ensures we are checking for a commit object,
    // not just any git object (like a blob or tree) that might have the same SHA prefix
    run_command(
        &common.repo_path,
        "git",
        &["cat-file", "-e", &format!("{sha}^{{commit}}")],
    )
    .with_context(|| format!("Commit not found or invalid: {sha}"))?;

    // The --format string produces all relevant metadata fields for the AI prompt
    let metadata = run_command(
        &common.repo_path,
        "git",
        &[
            "show",
            "--no-patch",
            "--format=commit: %H%nshort: %h%nauthor: %an <%ae>%ndate: %ad%nsubject: %s%nbody:%n%b",
            sha,
        ],
    )?;

    // Reviews only the changes introduced by that single commit (parent vs itself)
    let diff = run_command(
        &common.repo_path,
        "git",
        &[
            "show",
            "--format=",
            "--patch",
            "--find-renames",
            "--find-copies",
            sha,
            "--",
        ],
    )?;

    ensure_non_empty_diff(&diff)?;

    Ok(ReviewInput {
        diff,
        metadata,
        prompt_scope: "Review ONLY the changes introduced by this commit. Treat this commit diff as the source of truth.".to_string(),
        artifact_prefix: format!("commit-{sha}"),
        review_kind: "commit".to_string(),
        repository: repo_name(&common.repo_path),
        source: sha.to_string(),
        target: "single commit".to_string(),
        review_ref: format!("review/commit-{sha}"),
        remote: common.remote.clone(),
        pr_id: None,
        sha: Some(sha.to_string()),
    })
}

pub fn build_prompt(input: &ReviewInput) -> String {
    format!(
        r#"{scope}

Do not review unrelated existing code unless:
- the new changes introduce risk into that area
- the modified code depends on fragile existing behavior
- there is an obvious regression/security concern directly connected to the diff

Assume unchanged code is out of scope unless required for understanding impact.

Review metadata:
{metadata}

Architecture rules:
FrontEnd -> GraphQL API resolvers/mutations -> Service -> Repository -> DB via SQLAlchemy models

Check that:
- resolvers/mutations call the Service layer only
- resolvers/mutations do NOT call Repository classes directly
- resolvers/mutations do NOT access SQLAlchemy models, DB sessions, or raw queries directly
- Service layer owns business logic and orchestration
- Repository layer owns persistence and DB access
- dependencies flow downward only
- no architectural layer is skipped
- transaction/session handling remains consistent with existing patterns
- If a blocking architectural issue is marked with `# ARCH-EXCEPTION: approved:<ticket>, since:<date>, reason:<text>`, do NOT flag it as a blocking issue. Only warn if the marker is malformed or missing required fields.

Flag any layering violation, dependency inversion, or bypassed abstraction.

Focus on:
- bugs and regressions
- security issues
- AWS/IAM/CDK/infrastructure risks
- missing or weak tests
- backward compatibility
- maintainability
- unclear, fragile, or overly complex design
- risky deployment, migration, or rollback concerns
- transaction/data consistency risks
- authorization/authentication mistakes
- concurrency, async, caching, or state-management risks
- performance regressions caused by the change

Do not rewrite the code yet.
Prefer fewer high-confidence findings over many speculative comments.
Call out only actionable findings.

For each finding include:
- severity: blocking or non-blocking
- exact file/function/class/resource affected
- why it matters
- realistic failure scenario or operational impact
- suggested fix direction without rewriting the implementation

Give me:
1. blocking issues
2. non-blocking suggestions
3. tests I should run or add
4. exact files/functions/resources to inspect
5. risky deployment or migration concerns
6. architecture/layering violations
7. anything that looks safe and does not require changes

Diff:
```diff
{diff}"#,
        scope = input.prompt_scope,
        metadata = input.metadata,
        diff = input.diff,
    )
}

fn resolve_scm_provider() -> Box<dyn ScmProvider> {
    match current_scm_kind() {
        ScmKind::CodeCommit => Box::new(CodeCommitProvider),
    }
}

fn ensure_non_empty_diff(diff: &str) -> Result<()> {
    if diff.trim().is_empty() {
        return Err(anyhow!("Generated diff is empty. Nothing to review."));
    }

    Ok(())
}
