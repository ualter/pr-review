```text
cli.rs         -> CLI commands, flags, AI/scm option surfaces
config.rs      -> user config loading and `config init`
review.rs      -> review orchestration and prompt creation
scm/           -> provider-specific PR metadata/diff resolution
artifacts.rs   -> reports archive, metadata persistence, AI execution
session.rs     -> interactive chat, summaries, resume flow, diff chunking
ui.rs          -> terminal UX, spinner, prompts, viewers, help text
doctor.rs      -> local environment checks
```

```text
initial review:
  full prompt + full diff -> AI -> review.md

interactive chat:
  review-summary + conversation-summary + selected diff -> AI -> conversation.md
```
