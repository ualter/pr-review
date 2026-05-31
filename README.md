# pr-review

AI-assisted PR and commit review CLI written in Rust.

`pr-review` generates a high-signal review from a PR diff or a commit patch, stores the artifacts under `~/.pr-review/reports`, and can immediately continue in an interactive follow-up session with the selected AI.

The tool is opinionated. It is meant to behave more like a senior engineer reviewer than a generic summarizer: focus on bugs, regressions, architecture violations, security risk, deployment risk, and missing tests.

## What It Supports

- PR review from `codecommit` or `bitbucket`
- Single-commit review from local Git
- `copilot`, `copilot-sdk`, and `codex` AI providers
- Streaming AI output while the review is being generated
- Automatic archive of prompts, diffs, reports, and session state
- Interactive follow-up chat after the initial review
- Resume of saved sessions without regenerating the original review
- User config in `~/.pr-review/config.toml`
- Project-specific prompt profiles with repo-local and user-level overrides

## Requirements

- Rust / Cargo
- Git
- `copilot` CLI if you want to use `--ai copilot`
- GitHub Copilot CLI if you want to use `--ai copilot-sdk` (the SDK talks to the installed CLI runtime)
- `codex` CLI if you want to use `--ai codex`
- AWS CLI and a working AWS session for CodeCommit PR review
- `curl` for Bitbucket PR review
- Bitbucket token available as `BB_TOKEN` for Bitbucket PR review

## Build

```bash
cargo build --release
```

Binary:

```bash
target/release/pr-review
```

Optional install:

```bash
cargo install --path .
```

## Quick Start

Initialize the user config once:

```bash
pr-review config init
```

Review a CodeCommit PR and enter chat automatically:

```bash
pr-review pr 4669 \
  --repo-path ~/repos/backend \
  --scm codecommit \
  --ai codex
```

Review a Bitbucket PR:

```bash
export BB_TOKEN=...

pr-review pr 123 \
  --repo-path ~/repos/backend \
  --scm bitbucket \
  --ai copilot
```

Review a single commit:

```bash
pr-review commit 8f31c2a \
  --repo-path ~/repos/backend \
  --ai codex
```

If you want the tool to stop after writing the review instead of entering chat:

```bash
pr-review pr 4669 \
  --repo-path ~/repos/backend \
  --ai codex \
  --no-interactive
```

## Commands

### Review a PR

```bash
pr-review pr <PR_ID> [OPTIONS]
```

Important options:

- `--scm <codecommit|bitbucket>`: override the SCM provider for PR resolution
- `--repo-path <PATH>`: repository path used for local Git operations and artifact context
- `--remote <NAME>`: Git remote for CodeCommit/local Git fetches, default `origin`
- `--ai <copilot|copilot-sdk|codex>`: run the AI automatically after generating artifacts
- `--no-interactive`: do not enter the interactive session after the review
- `--bb-url <URL>`: override Bitbucket URL for this run
- `--bb-project <KEY>`: override Bitbucket project for this run
- `--bb-repo <SLUG>`: override Bitbucket repo for this run

Examples:

```bash
pr-review pr 4669 --repo-path ~/repos/backend --scm codecommit --ai codex
pr-review pr 123 --repo-path ~/repos/backend --scm bitbucket --ai copilot
pr-review pr 123 --repo-path ~/repos/backend --scm bitbucket --bb-project PLATFORM --bb-repo api --ai codex
pr-review pr 4669 --repo-path ~/repos/backend
```

### Review a Commit

```bash
pr-review commit <SHA> [OPTIONS]
```

Important options:

- `--repo-path <PATH>`
- `--remote <NAME>`
- `--ai <copilot|copilot-sdk|codex>`
- `--no-interactive`

Examples:

```bash
pr-review commit 8f31c2a --repo-path ~/repos/backend --ai codex
pr-review commit 8f31c2a --repo-path ~/repos/backend --ai copilot --no-interactive
pr-review commit 8f31c2a --repo-path ~/repos/backend
```

### Session Commands

List resumable sessions:

```bash
pr-review session list
```

Resume a session by name:

```bash
pr-review session resume codecommit-pr-4669 --ai codex
```

Resume using the configured default AI:

```bash
pr-review session resume codecommit-pr-4669
```

Resume with the interactive picker:

```bash
pr-review session resume
```

### Config and Environment Checks

Initialize or update the config file:

```bash
pr-review config init
```

Check local dependencies and directories:

```bash
pr-review doctor
```

Show the startup banner:

```bash
pr-review banner
```

### Prompt Commands

Create user-level prompt templates for one SCM and repo:

```bash
pr-review prompt init --scm codecommit --repo my-repo
```

This creates:

```text
~/.pr-review/prompts/codecommit/default.toml
~/.pr-review/prompts/codecommit/my-repo.toml
```

Show the final built prompt after profile resolution:

```bash
pr-review prompt show
pr-review prompt show --scm codecommit --repo my-repo
```

Optional:

- no arguments: show the built-in default prompt
- `--scm` + `--repo`: show the resolved prompt for that target
- `--repo-path <PATH>`: include a repo-local `.pr-review/prompt.toml` in the resolution chain if present

## Review Flow

### PR review

The tool:

1. resolves PR metadata from the configured SCM provider
2. resolves the PR diff
3. builds the review prompt
4. writes artifacts to disk
5. runs the AI if `--ai` is provided
6. writes `review.md`
7. prepares the interactive session artifacts
8. enters the interactive session unless `--no-interactive` was passed

### Commit review

The tool:

1. validates the commit exists locally
2. reads commit metadata from Git
3. generates the commit patch with Git
4. builds the review prompt
5. writes artifacts to disk
6. runs the AI if `--ai` is provided
7. writes `review.md`
8. prepares the interactive session artifacts
9. enters the interactive session unless `--no-interactive` was passed

If `--ai` is omitted, `pr-review` still generates and archives the diff, prompt, and metadata, but it does not run the AI and does not start an interactive session.

## AI Runtime Notes

- `copilot` and `codex` use the CLI backend.
- `copilot-sdk` uses the GitHub Copilot SDK session API and still requires the installed Copilot CLI runtime.
- AI output streams progressively while a review or interactive answer is being generated.
- The progress spinner shows:
  - a live status suffix during processing
  - elapsed time in `HH:MM:SS`
  - the cursor is hidden while processing and restored when processing finishes or when the process is interrupted with `CTRL+C`

Examples:

```bash
pr-review pr 4669 --repo-path ~/repos/backend --scm codecommit --ai copilot-sdk
pr-review commit 8f31c2a --repo-path ~/repos/backend --ai copilot-sdk
```

## SCM Behavior

### CodeCommit

For CodeCommit PRs:

- PR metadata comes from CodeCommit
- PR diff is still resolved with local Git using the configured `--remote`
- the local repository must match the target repository

If the source or destination branch fetch fails, `pr-review` prints a friendlier error that points to likely causes such as wrong `--repo-path`, wrong `--remote`, or a deleted branch.

### Bitbucket

For Bitbucket PRs:

- PR metadata comes from Bitbucket REST
- PR diff also comes from Bitbucket REST
- PR review does not need Git remote credentials
- `BB_TOKEN` must be set in the shell environment

Bitbucket settings resolve with this precedence:

1. CLI override
2. environment variable
3. `~/.pr-review/config.toml`

Supported settings:

- URL:
  - CLI: `--bb-url`
  - env: `BB_URL`
  - config: `[bitbucket].url`
- project:
  - CLI: `--bb-project`
  - env: `BB_PROJECT`
  - config: `[bitbucket].project`
- repo:
  - CLI: `--bb-repo`
  - env: `BB_REPO`
  - config: `[bitbucket].repo`
- token:
  - env only: `BB_TOKEN`

Example:

```bash
export BB_TOKEN=...

pr-review pr 123 \
  --repo-path ~/repos/backend \
  --scm bitbucket \
  --bb-project PLATFORM \
  --bb-repo api \
  --ai codex
```

## Configuration

Config file path:

```text
~/.pr-review/config.toml
```

Run:

```bash
pr-review config init
```

Behavior:

- creates the file if it does not exist
- if the file exists, adds missing supported keys
- leaves existing values untouched

Current config shape:

```toml
# pr-review user configuration

[ai]
# valid values: "copilot" or "codex"
default_ai = "codex"
# valid values: "fancy" or "simple"
prompt_style = "fancy"
copilot_icon = "🧑‍✈️"
codex_icon = "🤖"

[scm]
# valid values: "codecommit" or "bitbucket"
default = "codecommit"

[bitbucket]
url = "https://bitbucket.example.com"
project = "MYPROJ"
repo = "my-repo"
```

### Config keys

#### `[ai]`

- `default_ai`: default AI used when `--ai` is omitted
- `prompt_style`: `fancy` or `simple`
- `copilot_icon`: UI icon for Copilot
- `codex_icon`: UI icon for Codex

#### `[scm]`

- `default`: default SCM used when `pr-review pr ...` is run without `--scm`

#### `[bitbucket]`

- `url`: default Bitbucket Server/Data Center base URL
- `project`: default project key
- `repo`: default repo slug

## Prompt Profiles

The review prompt is no longer a single global hardcoded policy only.

`pr-review` now resolves a project-specific prompt profile and merges it onto the built-in default review policy. If no prompt profile exists, behavior stays effectively the same as before.

### Resolution order

Prompt profiles are resolved in this order:

1. `<repo-path>/.pr-review/prompt.toml`
2. `~/.pr-review/prompts/<scm>/<repo>.toml`
3. `~/.pr-review/prompts/<scm>/default.toml`
4. built-in default profile

Examples:

```text
~/repos/backend/.pr-review/prompt.toml
~/.pr-review/prompts/codecommit/datahub-backend-dev.toml
~/.pr-review/prompts/bitbucket/PLATFORM__api.toml
~/.pr-review/prompts/bitbucket/default.toml
```

`PLATFORM/api` becomes `PLATFORM__api` for the user-level prompt filename.

### Supported fields

```toml
[architecture]
summary = "Frontend -> API -> Service -> Repository -> DB"
rules = [
  "API handlers call services only",
  "Services own orchestration",
  "Repositories own persistence"
]
unchanged_code_guidance = "Only inspect unchanged code when required for impact analysis."

[review]
focus = ["bugs", "security", "tests"]
out_of_scope = ["formatting", "style nits"]

[prompt]
extra_instructions = """
Prefer high-confidence findings only.
Call out risky migrations explicitly.
"""
```

### Behavior

- prompt profiles are additive overrides, not full replacements
- if a profile defines only one section, the rest still comes from the built-in default
- repo-local profiles override user-level profiles
- commit review uses repo-local prompt profiles first; if none exist, it falls back to the built-in default
- PR review can also use SCM-specific user-level prompt profiles

### Practical usage

Recommended user-level setup:

```text
~/.pr-review/prompts/<scm>/default.toml
~/.pr-review/prompts/<scm>/<repo>.toml
```

Generate both with:

```bash
pr-review prompt init --scm codecommit --repo my-repo
```

If the prompt policy belongs directly to the project, repo-local profiles are still supported manually:

```text
<repo>/.pr-review/prompt.toml
```

## Interactive Session

After a review completes with `--ai`, `pr-review` enters an interactive session by default.

The initial review uses the full review prompt and the full diff. Follow-up questions use a lighter prompt that combines:

- `review-summary.md`
- `conversation-summary.md`
- selected relevant diff chunks from `diff-by-file/`
- your current question

Only `/full` forces the whole diff back into the follow-up prompt.

The conversation summary is updated on `/exit`, not after every exchange.

### Interactive commands

- `/help`
- `/summary`
- `/summary-print`
- `/review`
- `/review-print`
- `/review-summary`
- `/review-summary-print`
- `/last`
- `/last N`
- `/last-print`
- `/last-print N`
- `/full`
- `/exit`

Normal text is sent directly to the AI as a follow-up question.

### Resume behavior

Resuming a session does not:

- re-fetch the PR
- regenerate the diff
- rebuild the original review
- rerun the initial full review

It reloads the saved session state and continues the conversation.

## Artifact Layout

Archived reviews live under:

```text
~/.pr-review/reports/<review-name>/
```

Examples:

```text
~/.pr-review/reports/codecommit-pr-4669/
~/.pr-review/reports/bitbucket-pr-123/
~/.pr-review/reports/commit-8f31c2a/
```

Typical contents:

```text
diff.patch
prompt.txt
review.md
review-summary.md
conversation.md
conversation-summary.md
diff-by-file/
meta.json
```

Temporary copies of the prompt and diff are also written under the system temp directory during review generation.

## AI Cost Shape

Cheapest actions:

- `pr-review session list`
- `pr-review session resume ...`
- all viewer commands like `/summary`, `/review`, `/last`

More expensive actions:

- a normal interactive follow-up question
- `/full`
- rerunning the initial PR or commit review from scratch

Practical pattern:

1. run the full review once
2. resume the saved session later
3. ask focused questions
4. use `/full` only when needed

## Notes

- Commit review is SCM-agnostic and local-Git-driven.
- PR review is SCM-aware.
- The review prompt is intentionally opinionated and currently assumes this application architecture:

```text
FrontEnd -> GraphQL API resolvers/mutations -> Service -> Repository -> DB via SQLAlchemy models
```

- The AI is instructed to focus on high-confidence findings, not style nitpicks or generic summaries.
