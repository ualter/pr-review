use anyhow::{Context, Result, anyhow};

use crate::artifacts::{repo_name, run_command};
use crate::cli::{CommonArgs, ReviewInput};
use crate::prompt_profile::PromptProfile;
use crate::scm::bitbucket::BitbucketProvider;
use crate::scm::codecommit::CodeCommitProvider;
use crate::scm::{ScmKind, ScmProvider};
use crate::ui::{self, BLUE_BOLD, LINE, RESET, YELLOW};

pub fn review_pr(pr_id: &str, scm_kind: ScmKind, common: &CommonArgs) -> Result<ReviewInput> {
    ui::print_header();
    println!("\n{YELLOW}Validating PR ID {BLUE_BOLD}{pr_id}...{RESET}");
    println!("{BLUE_BOLD}{LINE}{RESET}");
    println!(
        "{YELLOW}Fetching {} PR metadata...{RESET}",
        scm_kind.display_name()
    );
    let provider = resolve_scm_provider(scm_kind);
    let context = provider.resolve_pr_context(pr_id, common)?;

    println!("{YELLOW}Resolving PR diff...{RESET}");

    let diff = provider.resolve_pr_diff(pr_id, common, &context)?;

    ensure_non_empty_diff(&diff)?;

    Ok(ReviewInput {
        diff,
        metadata: context.metadata,
        prompt_scope: "Review ONLY the changes contained in this PR diff file. Treat this diff as the source of truth.".to_string(),
        artifact_prefix: scm_kind.artifact_prefix(pr_id),
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
    ui::print_header();
    println!("\n{YELLOW}Validating commit {BLUE_BOLD}{sha}...{RESET}");
    println!("{BLUE_BOLD}{LINE}{RESET}");

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

pub fn build_prompt(input: &ReviewInput, profile: &PromptProfile) -> String {
    let out_of_scope = if profile.out_of_scope.is_empty() {
        String::new()
    } else {
        format!(
            "Do not review unrelated existing code unless:\n{}\n",
            profile
                .out_of_scope
                .iter()
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    let unchanged_code_guidance = profile
        .unchanged_code_guidance
        .as_deref()
        .unwrap_or_default();

    let architecture_section =
        if profile.architecture_summary.is_some() || !profile.architecture_rules.is_empty() {
            format!(
                "Architecture rules:\n{}\n\nCheck that:\n{}\n",
                profile
                    .architecture_summary
                    .as_deref()
                    .unwrap_or("No architecture summary configured."),
                profile
                    .architecture_rules
                    .iter()
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        } else {
            String::new()
        };

    let review_focus = profile
        .review_focus
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n");

    let extra_instructions = profile.extra_instructions.as_deref().unwrap_or_default();

    format!(
        r#"{scope}

{out_of_scope}

{unchanged_code_guidance}

Review metadata:
{metadata}

{architecture_section}

Focus on:
{review_focus}

{extra_instructions}

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
        out_of_scope = out_of_scope.trim_end(),
        unchanged_code_guidance = unchanged_code_guidance,
        metadata = input.metadata,
        architecture_section = architecture_section.trim_end(),
        review_focus = review_focus,
        extra_instructions = extra_instructions,
        diff = input.diff,
    )
}

fn resolve_scm_provider(kind: ScmKind) -> Box<dyn ScmProvider> {
    match kind {
        ScmKind::CodeCommit => Box::new(CodeCommitProvider),
        ScmKind::Bitbucket => Box::new(BitbucketProvider),
    }
}

fn ensure_non_empty_diff(diff: &str) -> Result<()> {
    if diff.trim().is_empty() {
        return Err(anyhow!("Generated diff is empty. Nothing to review."));
    }

    Ok(())
}
