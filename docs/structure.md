```bash
cli.rs        -> arguments/subcommands
review.rs     -> PR/commit diff + prompt creation
artifacts.rs  -> filesystem + AI execution
ui.rs         -> terminal UX
session.rs    -> interactive continuation
```

```
initial review:
  full prompt + full diff -> AI -> review.md

interactive chat:
  small prompt + selected context -> AI -> conversation.md
```