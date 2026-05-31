# Interactive Sessions

`pr-review` enters interactive mode automatically after a PR or commit review when `--ai` is provided. Use `--no-interactive` only if you want the review to finish and return directly to the shell.

The session layer is persistent. It stores the initial review, compressed summaries, conversation history, and per-file diff chunks so you can resume later without rerunning the original review.

## Prompt shape

Initial review:

```text
full review prompt + full diff -> AI -> review.md
```

Follow-up chat:

```text
review-summary.md
conversation-summary.md
selected diff chunk(s)
current user question
    -> AI -> append to conversation.md
```

Resume:

```text
saved artifact directory -> load session state -> continue conversation
```

`/full` is the only interactive command that forces the complete diff back into the prompt.

## Stored artifacts

Each interactive session lives under:

```text
~/.pr-review/reports/<review-name>/
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

`diff-by-file/` is created from the original diff so follow-up questions can load only the most relevant changed file patches instead of resending the whole diff every turn.

## Session commands

Inside the interactive session:

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

Notes:

- `/last` opens the conversation at the end.
- `/last N` shows only the last `N` request/response exchanges.
- `/exit` saves the conversation and refreshes `conversation-summary.md`.

## Conversation summary behavior

The conversation summary is no longer refreshed after every question. It is updated on `/exit` when the session changed. That reduces token usage for longer interactive sessions.

## Resume behavior

Resume commands:

```bash
pr-review session list
pr-review session
pr-review session <review-name> --ai codex
pr-review session <review-name>
pr-review session resume <review-name> --ai codex
pr-review session resume <review-name>
pr-review session resume
```

Behavior:

- `session list` shows resumable sessions from `~/.pr-review/reports`
- `session` without a subcommand opens the picker
- `session <review-name>` resumes that session directly
- `session resume <review-name>` continues that session
- `session resume` without a name opens the picker
- if `--ai` is omitted on resume, `default_ai` from config is used

Resuming a session does not:

- refetch the PR
- regenerate the diff
- rebuild the original review prompt
- rerun the initial review

It simply reloads the saved state and continues the conversation.

## Session fidelity

Current sessions persist full review metadata in `meta.json`, including review kind, artifact prefix, repo path, repository/source/target fields, and PR or commit identity. Resume uses that saved metadata rather than reconstructing placeholder values.

Legacy sessions without the newer metadata shape still load. A warning is shown only when the missing fields make the inferred identity incomplete.

## Example flow

Run a review and enter chat:

```bash
pr-review pr 4669 \
  --repo-path ~/repos/backend \
  --scm codecommit \
  --ai codex
```

Leave the session:

```text
/exit
```

Resume later:

```bash
pr-review session resume codecommit-pr-4669 --ai codex
```

Resume with the configured default AI:

```bash
pr-review session resume codecommit-pr-4669
```
