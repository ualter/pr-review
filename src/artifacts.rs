use anyhow::{anyhow, Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn run_command(repo_path: &Path, program: &str, args: &[&str]) -> Result<String> {
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

pub fn repo_name(repo_path: &Path) -> String {
    repo_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown-repository")
        .to_string()
}

pub fn reports_archive_dir() -> Result<PathBuf> {
    let home = env::var("HOME").context("Could not resolve HOME directory")?;

    Ok(PathBuf::from(home).join(".pr-review").join("reports"))
}

pub fn review_artifact_dir(review_name: &str) -> Result<PathBuf> {
    let dir = reports_archive_dir()?.join(review_name);

    fs::create_dir_all(&dir).with_context(|| {
        format!(
            "Failed to create review artifact directory: {}",
            dir.display()
        )
    })?;

    Ok(dir)
}
