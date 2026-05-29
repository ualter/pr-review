use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde_json::Value;
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const GREEN: &str = "\x1b[32m";
const GREEN_BOLD: &str = "\x1b[1;32m";
const BLACK_BOLD: &str = "\x1b[1;30m";
const BLUE_BOLD: &str = "\x1b[1;34m";
const YELLOW: &str = "\x1b[33m";
const YELLOW_BOLD: &str = "\x1b[1;33m";
const RESET: &str = "\x1b[0m";

const TESTING: bool = false;

#[derive(Parser)]
#[command(name = "pr-review")]
#[command(about = "AI-assisted PR/commit review CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
struct CommonArgs {
    /// Git remote name
    #[arg(long, default_value = "origin")]
    remote: String,

    /// Path to the repository
    #[arg(long, default_value = ".")]
    repo_path: PathBuf,

    /// Execute Copilot after generating prompt
    #[arg(long)]
    run_copilot: bool,
}

struct ReviewInput {
    diff: String,
    metadata: String,
    prompt_scope: String,
    artifact_prefix: String,

    repository: String,
    source: String,
    target: String,
    review_ref: String,
}

fn main() -> Result<()> {
    let start = Instant::now();
    let cli = Cli::parse();

    // PR review     = diff between target branch and source branch
    // Commit review = diff introduced by one single commit (its parent vs itself)
    let (input, common) = match cli.command {
        Commands::Pr { pr_id, common } => {
            let input = review_pr(&pr_id, &common)?;
            (input, common)
        }
        Commands::Commit { sha, common } => {
            let input = review_commit(&sha, &common)?;
            (input, common)
        }
    };

    let prompt = build_prompt(&input);

    let artifacts_dir = PathBuf::from("review-artifacts");
    fs::create_dir_all(&artifacts_dir)?;

    let tmp_dir = std::env::temp_dir();

    let diff_path = tmp_dir.join(format!("{}-diff.patch", input.artifact_prefix));
    let prompt_path = tmp_dir.join(format!("{}-copilot-prompt.txt", input.artifact_prefix));
    let report_path = PathBuf::from(format!("{}-review.md", input.artifact_prefix));

    print_header(
        &input.repository,
        &input.source,
        &input.target,
        &input.review_ref,
    );

    fs::write(&diff_path, &input.diff)?;
    fs::write(&prompt_path, &prompt)?;

    print_artifacts(&diff_path, &prompt_path);

    if common.run_copilot {
        println!("{YELLOW}Sending prompt to Copilot...{RESET}");

        let copilot_start = Instant::now();

        let (stop, handle) = start_spinner("Copilot is reviewing...");
        let prompt_arg = fs::read_to_string(&prompt_path)?;
        if TESTING {
            // Replace prompt_arg with a static string for testing
            let prompt_arg = String::from("1+1 is");
            let result = run_command(Path::new("."), "copilot", &["-p", &prompt_arg]);
            stop.store(true, Ordering::Relaxed);
            handle.join().ok();
            let review = result?;
            fs::write(&report_path, review)?;
            print_report(&report_path, &report_path, copilot_start.elapsed());
            println!(
                "{GREEN}Done in {:.2}s{RESET}",
                start.elapsed().as_secs_f64()
            );
            return Ok(());
        }
        let result = run_command(Path::new("."), "copilot", &["-p", &prompt_arg]);

        stop.store(true, Ordering::Relaxed);
        handle.join().ok();

        let review = result?;
        fs::write(&report_path, review)?;

        let archived_report_path = archive_report(&report_path)?;

        print_report(&report_path, &archived_report_path, copilot_start.elapsed());
    }

    println!(
        "{GREEN}Done in {:.2}s{RESET}",
        start.elapsed().as_secs_f64()
    );

    Ok(())
}

fn review_pr(pr_id: &str, common: &CommonArgs) -> Result<ReviewInput> {
    println!("\n{LINE}");
    println!("{YELLOW}Validating PR ID {BLUE_BOLD}{pr_id}...{RESET}");
    println!("{YELLOW}Fetching CodeCommit PR metadata...{RESET}");

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
    let destination_branch = clean_branch_name(destination);

    println!("{YELLOW}Fetching branches...{RESET}");

    run_command(
        &common.repo_path,
        "git",
        &["fetch", &common.remote, &source_branch],
    )?;

    run_command(
        &common.repo_path,
        "git",
        &["fetch", &common.remote, &destination_branch],
    )?;

    let diff = run_command(
        &common.repo_path,
        "git",
        &[
            "diff",
            &format!("{}/{}", common.remote, destination_branch),
            &format!("{}/{}", common.remote, source_branch),
            "--",
        ],
    )?;

    ensure_non_empty_diff(&diff)?;

    let repository = target["repositoryName"]
        .as_str()
        .unwrap_or("unknown-repository")
        .to_string();

    Ok(ReviewInput {
        diff,
        metadata: format!(
            "Review target: CodeCommit PR #{pr_id}\nRepository: {repository}\nSource branch: {source_branch}\nDestination branch: {destination_branch}"
        ),
        prompt_scope: "Review ONLY the changes contained in this PR diff file. Treat this diff as the source of truth.".to_string(),
        artifact_prefix: format!("codecommit-pr-{pr_id}"),
        repository,
        source: source_branch.clone(),
        target: destination_branch.clone(),
        review_ref: format!("review/pr-{pr_id}"),
    })
}

fn review_commit(sha: &str, common: &CommonArgs) -> Result<ReviewInput> {
    println!("\n{LINE}");
    println!("{YELLOW}Validating commit {sha}...{RESET}");

    // Validate the commit exists and is a proper commit object
    // The ^{commit} syntax ensures we are checking for a commit object,
    // not just any git object (like a blob or tree) that might have the same SHA prefix
    run_command(
        &common.repo_path,
        "git",
        &["cat-file", "-e", &format!("{sha}^{{commit}}")],
    )
    .with_context(|| format!("Commit not found or invalid: {sha}"))?;

    // Get commit metadata in a structured format
    // The --format string is designed to be easily parseable and human-readable, containing all relevant metadata fields
    // This gets commit hash, short hash, author, date, subject and body. That metadata is later inserted into the AI prompt.
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

    // Generate the commit diff
    // The diff shows the changes introduced by this commit, which will be reviewed by the AI.
    // It reviews only the changes introduced by that commit, not a whole branch or PR. It is basically:
    //  --> "Show me the patch produced by this one commit."
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

    // Builds the review input
    // The repository name is derived from the repo path for display purposes in the prompt and report.
    Ok(ReviewInput {
        diff,
        metadata,
        prompt_scope: "Review ONLY the changes introduced by this commit. Treat this commit diff as the source of truth.".to_string(),
        artifact_prefix: format!("commit-{sha}"),
        repository: repo_name(&common.repo_path),
        source: sha.to_string(),
        target: "single commit".to_string(),
        review_ref: format!("review/commit-{sha}"),
    })
}

fn build_prompt(input: &ReviewInput) -> String {
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

fn run_command(repo_path: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(repo_path)
        .output()
        .with_context(|| format!("Failed to execute command: {program} {}", args.join(" ")))?;

    if !output.status.success() {
        return Err(anyhow!(
            "Command failed: {} {}\n{}",
            program,
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn ensure_non_empty_diff(diff: &str) -> Result<()> {
    if diff.trim().is_empty() {
        return Err(anyhow!("Generated diff is empty. Nothing to review."));
    }

    Ok(())
}

fn clean_branch_name(reference: &str) -> String {
    reference
        .strip_prefix("refs/heads/")
        .unwrap_or(reference)
        .to_string()
}

fn start_spinner(message: &'static str) -> (Arc<AtomicBool>, thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);

    let handle = thread::spawn(move || {
        let chars = ["|", "/", "-", "\\"];
        let mut i = 0;

        while !stop_thread.load(Ordering::Relaxed) {
            print!(
                "\r{}{} {}{}",
                YELLOW,
                message,
                chars[i % chars.len()],
                RESET
            );
            let _ = io::stdout().flush();
            i += 1;
            thread::sleep(Duration::from_millis(100));
        }

        print!("\r{}\r", " ".repeat(100));
        let _ = io::stdout().flush();
    });

    (stop, handle)
}

const LINE: &str =
    "----------------------------------------------------------------------------------------------------------------";

fn print_header(repository: &str, source: &str, target: &str, review_branch: &str) {
    println!("{LINE}");
    println!("{}Repository: {}{}", GREEN_BOLD, repository, RESET);
    println!("{}Source:     {}{}", BLUE_BOLD, source, RESET);
    println!("{}Target:     {}{}", BLUE_BOLD, target, RESET);
    println!("{}Review:     {}{}", YELLOW_BOLD, review_branch, RESET);
    println!("{LINE}");
}

fn print_artifacts(diff_path: &Path, prompt_path: &Path) {
    println!(
        "{}Diff written to:   {}{}",
        GREEN_BOLD,
        diff_path.display(),
        RESET
    );
    println!(
        "{}Prompt written to: {}{}",
        BLUE_BOLD,
        prompt_path.display(),
        RESET
    );
    println!("{LINE}");
    println!("{BLACK_BOLD}Run Copilot manually with:{RESET}");
    println!(
        "  {BLACK_BOLD}copilot -p \"$(cat {})\"{RESET}",
        prompt_path.display()
    );
}

fn print_report(report_path: &Path, archived_path: &Path, elapsed: Duration) {
    println!(
        "{}Copilot review completed in {:.1}s{}",
        GREEN_BOLD,
        elapsed.as_secs_f64(),
        RESET
    );
    println!("{LINE}");
    println!(
        "{}Review report written to: {}{}{}",
        GREEN_BOLD,
        BLUE_BOLD,
        report_path.display(),
        RESET
    );
    println!(
        "{}Archived report written to: {}{}{}",
        GREEN_BOLD,
        BLUE_BOLD,
        archived_path.display(),
        RESET
    );
    println!("{LINE}");
}

fn repo_name(repo_path: &Path) -> String {
    repo_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown-repository")
        .to_string()
}

fn reports_archive_dir() -> Result<PathBuf> {
    let home = env::var("HOME").context("Could not resolve HOME directory")?;

    Ok(PathBuf::from(home).join(".pr-review").join("reports"))
}

fn archive_report(report_path: &Path) -> Result<PathBuf> {
    let archive_dir = reports_archive_dir()?;

    fs::create_dir_all(&archive_dir).with_context(|| {
        format!(
            "Failed to create reports archive dir: {}",
            archive_dir.display()
        )
    })?;

    let file_name = report_path
        .file_name()
        .context("Report path has no file name")?;

    let archived_path = archive_dir.join(file_name);

    fs::copy(report_path, &archived_path).with_context(|| {
        format!(
            "Failed to copy report from {} to {}",
            report_path.display(),
            archived_path.display()
        )
    })?;

    Ok(archived_path)
}
