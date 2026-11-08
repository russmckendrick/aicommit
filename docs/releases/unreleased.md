# aicommit Unreleased

## Terminal UI

- New `aic --tui` opens a full-screen terminal UI built with `ratatui`. Set `AIC_TUI=true` to open it by default. See [Terminal UI](../tui.md).
- Commit screen: a staged/unstaged tree where `space` stages or unstages individual hunks, a syntax-highlighted diff with word-level changes, and a message panel that starts writing as soon as the TUI opens, streaming where the provider supports it.
- Split screen: proposed commits appear as columns; move hunks between them with `←`/`→` or by dragging with the mouse, add or merge commits, and messages for changed commits are rewritten automatically.
- Command palette (`ctrl-k`), help overlay (`?`), live theme switching (`t`), subject-length counter, stale-message warning, mouse support, and a layout that adapts to the terminal width.
- New config keys: `AIC_TUI`, `AIC_THEME` (`auto` or any `aic map` theme), and `AIC_ICONS` (`unicode` or `nerd`).
- The TUI never commits by itself: the normal flow creates the commit, records history, and offers to push.

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

## Repository style matching

- Generated commit messages now follow the habits of the repository they land in. `aic` reads the last 50 non-merge commit subjects and tells the model which scopes are already in use, how descriptions are cased, whether subjects end with a full stop, and how long subjects usually are.
- Needs at least 8 commits of history, and never overrides `AIC_EMOJI` or the commit convention. Turn it off with `AIC_REPO_STYLE=false`. Custom prompt files opt in with the new `{{repo_style}}` placeholder.

## Updated default models

| Provider | Previous default | New default |
|----------|------------------|-------------|
| `openai`, `azure-openai` | `gpt-5.4-mini` | `gpt-6-luna` |
| `anthropic` | `claude-sonnet-4-20250514` | `claude-sonnet-5-5` |
| `groq` | `llama-3.1-8b-instant` (retired by Groq) | `openai/gpt-oss-20b` |
| `ollama` | `llama3.2` | `gpt-oss:20b` |

- `aic setup` and `aic models` suggest current models: GPT-6 Luna, Sol, and Astra; Claude Sonnet, Haiku, and Opus 5.5 and Fable 5.1; GPT-OSS on Groq and Ollama.
- Existing configs keep the model they have. To move to the new default, run `aic setup` or `aic config set AIC_MODEL=<model>`.
- `--provider` still swaps in the new provider's default when your configured model is any current or earlier default.
- GPT-6 models get the same request parameters as GPT-5 (`max_completion_tokens`, no sampling parameters).
