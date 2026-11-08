# aicommit Unreleased

## Streaming

- `openai`, `azure-openai`, `groq`, `ollama`, custom OpenAI-compatible endpoints, and `anthropic` now stream the final commit-message request. While it is written, the status spinner previews the subject line instead of a rotating status message.
- Chunk summaries for large diffs stay non-streaming; only the request that produces the message you see streams.
- Command-backed providers (`claude-code`, `codex`, `copilot`, `apple`) are unchanged and return the whole message at once.

## Machine-readable output

- New `--json` and `--quiet` flags on `aic`, `aic review`, and `aic pr` print only the result to stdout, for scripts, CI, and coding agents. See [Automation](../automation.md).
- In either mode `aic` never prompts and only commits when `--yes` is also passed. `--json --yes` reports the full commit hash.

## Hunk-level split commits

- The split flow now groups **hunks**, not files, so one file can be split across several commits. The model sees each hunk tagged with an ID (`src/ui.rs#2`) and returns hunk IDs; replies in the old file-based format still work.
- Split commits are created by snapshotting the index, resetting it to `HEAD`, and applying each group's patch with `git apply --cached`. The working tree is never touched.
- The split flow is no longer blocked when a staged file also has unstaged changes.
- If a split commit fails part-way, the remaining changes are restored to the index instead of being left unstaged.
- Files hidden by `.aicommitignore` now stay staged after a split, instead of being unstaged.
- A single file with two or more staged hunks can now be split.
