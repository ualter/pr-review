## v0.1.3

  - add prompt budget reporting before AI review execution
  - show the largest diff contributors, prompt size, and estimated token usage
  - require confirmation before continuing with high-cost prompts
  - improve the prompt budget panel styling and alignment in the terminal UI
  - fix AI model config loading so Copilot, Copilot SDK, and Codex honor configured models correctly
  - default Copilot and Copilot SDK to `gpt-5.4`

## v0.1.2

  - add configurable model selection for Copilot, Copilot SDK, and Codex via config and `--model`
  - surface the active AI tool and model in interactive prompts, spinners, and session labels
  - persist the resolved AI model in review session metadata
  - add first-pass streaming markdown formatting for live AI output in review and interactive flows
  - improve spinner/status line cleanup for wide glyphs, ANSI styling, and elapsed time rendering
  - remove `banner_lab` from release targets so only `pr-review` is published

## v0.1.1

  - fix CodeCommit PR diff generation to use proper PR-style comparison
  - fix CodeCommit ref refresh so fetched refs match the refs used for diffing
  - clear stale `diff-by-file/` artifacts on fresh review runs
