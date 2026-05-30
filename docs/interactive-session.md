### Interactive Session Layer

```text
initial review:
  full prompt + full diff -> AI -> review.md

interactive chat:
  small prompt + selected context -> AI -> conversation.md

resume session:
  existing session state -> AI -> continue conversation
```

Main pieces:

1. **`session.rs`**

   * Creates/uses the review artifact directory.
   * Creates and maintains:

     * `conversation.md`
     * `conversation-summary.md`
     * `review-summary.md`
     * `diff-by-file/`
     * provider-specific session state files
   * Splits the full diff into one patch file per changed file.
   * Starts an interactive terminal loop:

     ```bash
     pr-review>
     ```

   * Saves every user/AI exchange to `conversation.md`.
   * Supports resuming previous interactive sessions without regenerating the review.

2. **Smart context selection**

   * Interactive mode does **not** resend the full diff every turn.
   * It dynamically builds a compact context using:

     ```text
     review-summary.md
     conversation-summary.md
     relevant diff file(s)
     current user question
     ```

   * Relevant diff files are selected heuristically based on keyword matching between the user question and changed files.
   * Full diff is only injected when explicitly requested:

     ```text
     /full
     ```

3. **Review summary**

   * After the initial AI review completes, the same AI provider generates a compressed version of the review:

     ```text
     review-summary.md
     ```

   * Interactive conversations use this summary instead of the full review report to reduce token usage and improve focus.

4. **Conversation summary**

   * After each interactive exchange, the tool updates:

     ```text
     conversation-summary.md
     ```

   * Future turns use the compact summary instead of replaying the entire conversation history.

5. **Provider-compatible**

   * Interactive mode uses the same provider abstraction already used for reviews:

     ```rust
     run_ai_tool(tool, &prompt)
     ```

   * Compatible with:

     ```text
     --ai copilot
     --ai codex
     ```

6. **Persistent session model**

   * The interactive session state is persisted under:

     ```text
     ~/.pr-review/reports/<review-name>/
     ```

   * Example:

     ```text
     ~/.pr-review/reports/codecommit-pr-4663/
     ```

   * Stored artifacts include:

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

7. **Resume existing sessions**

   * Existing interactive sessions can be resumed without:

     * fetching branches
     * regenerating diffs
     * rebuilding prompts
     * rerunning the AI review

   * The tool simply reloads the saved interactive session state and continues the conversation.

   * Command:

     ```bash
     pr-review session <review-name> --ai codex
     ```

   * Example:

     ```bash
     pr-review session codecommit-pr-4663 --ai codex
     ```

8. **Execution flow**

   ```mermaid
   sequenceDiagram
       autonumber

       actor User
       participant CLI as pr-review CLI
       participant Git as Git / CodeCommit
       participant FS as ~/.pr-review/reports
       participant AI as AI Tool
       participant Chat as Interactive Session

       alt New PR / Commit review
           User->>CLI: pr-review pr 4663 --ai codex --interactive
           CLI->>Git: Fetch PR/commit metadata
           CLI->>Git: Generate diff
           CLI->>CLI: Build review prompt
           CLI->>FS: Save diff.patch, prompt.txt, meta.json
           CLI->>AI: Run initial AI review
           AI-->>CLI: Review report
           CLI->>FS: Save review.md
           CLI->>CLI: prepare_session_artifacts()
           CLI->>Chat: run_interactive_session()
           Chat->>FS: Save chat/session state
       else Resume existing session
           User->>CLI: pr-review session codecommit-pr-4663 --ai codex
           CLI->>FS: Locate existing artifact directory
           CLI->>FS: Load existing session state
           CLI->>Chat: resume_interactive_session()
           Chat->>AI: Continue previous conversation
           Chat->>FS: Save updated chat/session state
       end
   ```

Expected usage:

```bash
pr-review pr 4663 \
  --repo-path ~/developer/repos/datahub-code/datahub-backend \
  --ai codex \
  --interactive
```

Resume later:

```bash
pr-review session codecommit-pr-4663 --ai codex
```

Commit review:

```bash
pr-review commit abc123 \
  --repo-path ~/developer/repos/datahub-code/datahub-backend \
  --ai codex \
  --interactive
```

Resume commit session:

```bash
pr-review session commit-abc123 --ai codex
```

In short:

`pr-review` evolved from a static AI review generator into a persistent AI-assisted engineering review environment, where the tool itself owns the review memory, summaries, diff segmentation, and interactive session continuity instead of depending on Copilot/Codex native session persistence. 