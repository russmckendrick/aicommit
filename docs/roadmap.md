# Roadmap

The v1 priority is a reliable Rust CLI for local commit generation through `aic`.

## High Impact

- **Additional AI providers** — Anthropic (Claude), Ollama / local models, Google Gemini, and a first-class "generic OpenAI-compatible" provider name for endpoints like Groq, Together, and Mistral.
- **`aic log` — rewrite past commit messages** — clean up the last N commit messages on a branch using AI before opening a PR. Subsumes the earlier GitHub Action idea for rewriting pushed commit messages.
- **`aic review` — AI-powered diff review** — get feedback on the staged diff (bugs, style, security) using a review-focused prompt and the existing provider infrastructure.
- **Shell completion generation** — use `clap_complete` to add an `aic completions <shell>` subcommand.

## Medium Impact, Low Effort

- **`--dry-run` flag** — generate and display the commit message without committing. Useful for testing prompt tweaks.
- **`--amend` flag** — regenerate and amend the last commit message.
- **Branch name as context** — automatically extract ticket/issue numbers from branch names (e.g. `feature/PROJ-123-add-auth`) and feed them into the prompt.
- **Conventional commit scope hints** — detect likely scopes from changed file paths to improve scope consistency.
- **Commit message history** — keep a local log of generated messages (`~/.aicommit-history.json`) and add `aic history` to browse them.

## Medium Impact, Moderate Effort

- **`aic pr` — PR description generation** — generate a pull request title and description from the branch's commits and cumulative diff against the base branch.
- **Interactive diff splitting** — when a diff touches multiple concerns, offer to split it into multiple commits with separate messages.
- **Config profiles** — named profiles (`~/.aicommit.d/work.toml`, `~/.aicommit.d/personal.toml`) selectable via `--profile` or auto-detected from the git remote URL.
- Commitlint-style config inference.

## Quality of Life

- **`aic init`** — generate a `.aicommitignore` with sensible defaults for the detected project type.
- **Cost estimation** — show estimated token usage and approximate cost before sending to the API.
- **Clipboard copy option** — add "Copy to clipboard" alongside Yes/No/Edit in the confirmation prompt.
- **`--quiet` / `--json` output mode** — output only the generated message to stdout for piping into other tools.
- Broader language prompt examples.
- Release packaging for common package managers.
