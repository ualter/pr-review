# pr-review

AI-assisted Pull Request and Commit review CLI written in Rust.

`pr-review` automates high-signal engineering reviews using AI tools such as GitHub Copilot CLI (and later Codex / additional providers).

The goal is not to generate generic summaries.

The goal is to behave more like a senior engineer reviewer:

* architecture-aware
* infrastructure-aware
* security-aware
* operational-risk-aware
* focused on actionable findings only

---

# Current Features

* AWS CodeCommit PR review
* Single commit review by SHA
* Automatic git diff generation
* AI prompt generation
* Copilot CLI integration
* Colored terminal UX
* Spinner/progress display
* Artifact generation
* Review report generation
* Repository path support (`--repo-path`)
* Architecture/layering validation rules

---

# Installation

## Requirements

* Rust
* Cargo
* Git
* AWS CLI
* GitHub Copilot CLI
* authenticated AWS session
* authenticated Copilot CLI

---

## Build

```bash
cargo build --release
```

Binary:

```bash
target/release/pr-review
```

Optional global install:

```bash
cargo install --path .
```

---

# CLI Usage

The CLI currently supports two review modes:

```bash
pr-review pr <PR_ID>
pr-review commit <SHA>
```

---

# Pull Request Review

## Example

```bash
pr-review pr 4663 \
  --repo-path ~/developer/repos/datahub-code/datahub-backend \
  --run-copilot
```

---

## What Happens Internally

The tool:

1. Retrieves PR metadata from AWS CodeCommit
2. Extracts source and target branches
3. Fetches both branches
4. Generates the git diff between them
5. Builds a structured AI review prompt
6. Writes artifacts to disk
7. Optionally sends the prompt to Copilot
8. Saves the AI review report

---

## PR Diff Logic

PR review compares:

```text
target branch
VS
source branch
```

Equivalent conceptually to:

```bash
git diff target_branch source_branch
```

This represents the TOTAL net change introduced by the branch.

---

# Commit Review

## Example

```bash
pr-review commit 8f31c2a \
  --repo-path ~/developer/repos/datahub-code/datahub-backend \
  --run-copilot
```

---

## What Happens Internally

The tool:

1. Validates the commit exists
2. Extracts commit metadata
3. Generates the patch introduced by the commit
4. Builds a structured AI review prompt
5. Writes artifacts to disk
6. Optionally sends the prompt to Copilot
7. Saves the AI review report

---

## Commit Diff Logic

Commit review compares:

```text
parent commit
VS
target commit
```

Equivalent conceptually to:

```bash
git show <SHA>
```

or:

```bash
git diff <SHA>^ <SHA>
```

Meaning:

> "What exactly changed when the parent commit became this commit?"

---

# Artifact Generation

The tool generates several artifacts.

## Temporary Files

Written into `/tmp`:

```text
/tmp/codecommit-pr-4663-diff.patch
/tmp/codecommit-pr-4663-copilot-prompt.txt
```

or:

```text
/tmp/commit-8f31c2a-diff.patch
/tmp/commit-8f31c2a-copilot-prompt.txt
```

---

## Final Review Report

Written into the current directory:

```text
codecommit-pr-4663-review.md
```

or:

```text
commit-8f31c2a-review.md
```

---

# Terminal Output

Example:

```text
-------------------------------------------------------------------------------
Repository: datahub-backend-dev
Source:     DLTA-18480-squad-stack-and-CDK
Target:     Development-v5.4
Review:     review/pr-4663
-------------------------------------------------------------------------------
Diff written to:   /tmp/codecommit-pr-4663-diff.patch
Prompt written to: /tmp/codecommit-pr-4663-copilot-prompt.txt
-------------------------------------------------------------------------------

Run Copilot manually with:
  copilot -p "$(cat /tmp/codecommit-pr-4663-copilot-prompt.txt)"

Sending prompt to Copilot...

Copilot review completed in 145.2s
-------------------------------------------------------------------------------

Review report written to: codecommit-pr-4663-review.md
-------------------------------------------------------------------------------
```

---

# AI Review Philosophy

The tool intentionally avoids noisy low-value AI comments.

The review prompt enforces focus on:

* bugs
* regressions
* security issues
* AWS/CDK/IAM risks
* maintainability concerns
* rollback/deployment risks
* transaction consistency
* architectural violations
* dependency direction
* authorization/authentication mistakes
* performance regressions

The tool explicitly avoids:

* formatting comments
* generic style nitpicks
* speculative findings
* low-confidence suggestions

---

# Architecture Rules

The current review rules assume this architecture:

```text
FrontEnd
  -> GraphQL API resolvers/mutations
  -> Service layer
  -> Repository layer
  -> DB via SQLAlchemy models
```

The AI validates:

* resolvers call Service layer only
* resolvers do not access repositories directly
* resolvers do not access SQLAlchemy directly
* Service owns orchestration/business logic
* Repository owns persistence
* dependency flow remains downward
* no architectural layer skipping

---

# Current Internal Flow

```text
CLI
  -> review_pr() / review_commit()
  -> generate diff
  -> build_prompt()
  -> write artifacts
  -> optional Copilot execution
  -> save report
```

---

# Current Commands

## Review a PR

```bash
pr-review pr <PR_ID> [OPTIONS]
```

## Review a commit

```bash
pr-review commit <SHA> [OPTIONS]
```

---

# Options

```text
--remote <name>       Git remote name (default: origin)

--repo-path <path>    Path to the repository

--run-copilot         Execute Copilot automatically
```

---

# Example Manual Copilot Execution

If `--run-copilot` is omitted:

```bash
copilot -p "$(cat /tmp/codecommit-pr-4663-copilot-prompt.txt)"
```

---

# Important Notes

## Repository Path

The tool does NOT require execution from inside the repository.

Example:

```bash
pr-review pr 4663 \
  --repo-path ~/repos/datahub-backend
```

Internally all git commands run using:

```rust
.current_dir(repo_path)
```

---

## Commit Review Limitations

Current commit review works best for:

* normal commits
* squash commits
* feature commits

Merge commits may produce more complex diffs because Git commits can have multiple parents.

---

# Future Vision

Planned future features:

* GitHub PR support
* GitLab support
* Codex provider support
* multiple AI providers
* streaming output
* automatic PR comments
* review caching
* diff chunking
* repo-specific templates
* semantic indexing/RAG
* SARIF export
* CI/CD integration
* security review profiles
* performance review profiles
* infrastructure review profiles

---

# Philosophy

The objective is NOT:

```text
"Use AI to summarize a PR"
```

The objective is:

```text
"Use AI to augment senior engineering review workflows"
```
