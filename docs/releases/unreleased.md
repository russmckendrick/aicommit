# aicommit Unreleased

## Streaming

- `openai`, `azure-openai`, `groq`, `ollama`, custom OpenAI-compatible endpoints, and `anthropic` now stream the final commit-message request. While it is written, the status spinner previews the subject line instead of a rotating status message.
- Chunk summaries for large diffs stay non-streaming; only the request that produces the message you see streams.
- Command-backed providers (`claude-code`, `codex`, `copilot`, `apple`) are unchanged and return the whole message at once.

## Machine-readable output

- New `--json` and `--quiet` flags on `aic`, `aic review`, and `aic pr` print only the result to stdout, for scripts, CI, and coding agents. See [Automation](../automation.md).
- In either mode `aic` never prompts and only commits when `--yes` is also passed. `--json --yes` reports the full commit hash.
