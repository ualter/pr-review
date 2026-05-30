### Interactive Session Layer

```text
initial review:
  full prompt + full diff -> AI -> review.md

interactive chat:
  small prompt + selected context -> AI -> conversation.md
```

Main pieces:

1. **`session.rs`**

   * Creates/uses the review artifact directory.
   * Creates:

     * `conversation.md`
     * `conversation-summary.md`
     * `review-summary.md`
     * `diff-by-file/`
   * Splits the full diff into one patch file per changed file.
   * Starts an interactive terminal loop:

     ```bash
     pr-review> why is this risky?
     ```
   * Saves every user/AI exchange to `conversation.md`.

2. **Smart context selection**

   * It does **not** send the full diff every time.
   * It sends:

     ```text
     review-summary.md
     conversation-summary.md
     relevant diff file if matched
     user question
     ```
   * Full diff is only sent when you type:

     ```text
     /full
     ```

3. **Review summary**

   * After the first AI review, it asks the same AI tool to compress the full review into `review-summary.md`.
   * Future chat turns use this summary instead of the full review.

4. **Conversation summary**

   * After each chat response, it updates `conversation-summary.md`.
   * Future turns use the compact summary instead of replaying the whole chat history.

5. **Provider-compatible**

   * It uses your existing:

     ```rust
     run_ai_tool(tool, &prompt)
     ```
   * So it works with both:

     ```text
     --ai copilot
     --ai codex
     ```

Expected usage:

```bash
pr-review pr 4663 \
  --repo-path ~/developer/repos/datahub-code/datahub-backend \
  --ai codex \
  --interactive
```

In short: we moved from a static review report to a persistent AI review session, where `pr-review` owns the memory instead of relying on Copilot/Codex session state.
