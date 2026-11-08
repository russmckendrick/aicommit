# aicommit

A fast, opinionated CLI that generates Git commit messages and reviews staged diffs using AI. Written in Rust, installed as `aic`.

## Quick start

```sh
# Install via Homebrew (macOS)
brew install russmckendrick/tap/aicommit

# Install via WinGet (Windows)
winget install --id RussMckendrick.Aicommit -e

# Or download a binary from GitHub Releases
# https://github.com/russmckendrick/aicommit/releases

# Run setup to configure your provider and credentials
aic setup
```

## Usage

Run `aic` in a repository with changes:

```sh
aic
```

In a terminal, `aic` opens a full-screen terminal UI. Stage individual hunks with `space`, watch the message stream in, edit it, split the staged set into several commits by moving hunks between columns, and change your provider, model, and API key in settings (`,`). With no API key configured, it opens on settings so you can add one. See [docs/tui.md](docs/tui.md).

### Quick prompt flow

```sh
git add -p
aic --fast
```

`--fast` (or `AIC_TUI=false`) skips the terminal UI: `aic` generates a message, shows it for confirmation, and commits on approval. If files are already staged it lets you continue, unstage selected files, or abort; if nothing is staged it lets you stage all changed files, choose files, or cancel. It can also offer to split the staged set into several commits.

Scripts, pipes, Git hooks, `--yes`, `--dry-run`, `--amend`, `--json`, and `--quiet` always use the prompt-free CLI path. With `--yes`, `aic` stages all changed files automatically.

### Flags

| Flag | Description |
|------|-------------|
| `-c`, `--context` | Add context for the AI (e.g., `-c "closes #42"`) |
| `-y`, `--yes` | Skip commit prompts and auto-stage/auto-push when unambiguous |
| `-d`, `--dry-run` | Print the message without committing |
| `--amend` | Regenerate and amend the last commit |
| `--fgm` | Use the full GitMoji specification |
| `-f`, `--fast` | Skip the terminal UI and use the quick prompt flow |
| `--tui` | Open the terminal UI even when `AIC_TUI=false` |
| `--json` / `-q`, `--quiet` | Print only the result (JSON or plain text) for scripts and agents |

### Review staged changes

Get AI-powered feedback on your staged diff before committing - findings are grouped by severity and rendered in the terminal:

```sh
aic review
aic review -c "focus on security"
```

`aic review` inspects the current staged set as-is. In an interactive terminal, if nothing is staged, it offers the same staging menu as `aic`, but it does not unstage files during review.

### Rewrite commit messages

Clean up the last N commit messages on your branch before opening a PR:

```sh
aic log
aic log -n 3
```

### Draft a pull request

Generate a PR title and Markdown description from the branch commits and diff against a base branch:

```sh
aic pr
aic pr --base origin/main
aic pr -c "highlight rollout risk"
aic pr --yes
```

### Browse history

View recent AI-generated commit messages, PR drafts, and reviews:

```sh
aic history                  # interactive by default in a terminal
aic history --non-interactive
aic history --all
aic history --verbose
aic history --kind review
```

### Visualize the codebase

Generate SVG visualizations of file structure, commit history, and activity:

```sh
aic map tree                 # file hierarchy treemap
aic map history              # zigzag commit timeline
aic map heat                 # file modification heatmap
aic map activity             # GitHub-style contribution grid
```

### More commands

```sh
aic setup                  # Interactive provider setup
aic config set KEY=value   # Set configuration
aic models --refresh       # List available models
aic hook set               # Install prepare-commit-msg hook
aic completions zsh        # Generate shell completions
```

## Features

- **Conventional commits** with optional GitMoji prefixes
- **Scope hints** - detects likely scopes from changed file paths
- **Branch context** - extracts ticket/issue references from branch names
- **Diff review** - AI-powered code review with severity grouping
- **Message rewriting** - clean up commit history with `aic log`
- **Pull request drafts** - generate a PR title and Markdown body with `aic pr`
- **History** - browse past generated messages, PR drafts, and reviews
- **Large diff handling** - automatic chunking and synthesis
- **Custom prompts** - swap the system prompt without recompiling
- **Terminal UI** - stage hunks, stream messages, and split commits in a full-screen UI (the default in a terminal)
- **Split commits** - turn one staged change set into multiple commits, hunk by hunk, when concerns are separate
- **Streaming** - see the message as it is written with OpenAI-compatible and Anthropic providers
- **Automation** - `--json` and `--quiet` output for scripts, CI, and coding agents
- **Visualization** - generate SVG treemaps, timelines, heatmaps, and activity graphs with `aic map`
- **Provider choice** - OpenAI, Azure OpenAI, Anthropic, Groq, Ollama, Claude Code, Codex, GitHub Copilot CLI, and custom OpenAI-compatible endpoints

## Configuration

Set values globally with `aic config set` or per-invocation with environment variables:

```sh
aic config set AIC_MODEL=gpt-6-luna AIC_EMOJI=true
AIC_LANGUAGE=french aic
```

See [docs/configuration.md](docs/configuration.md) for the full key reference.

## Documentation

Detailed docs live in [`docs/`](docs/):

- [Installation](docs/installation.md) - Homebrew, WinGet, GitHub Releases, from source
- [Usage](docs/usage.md) - commit workflow, review, flags, hooks
- [Configuration](docs/configuration.md) - keys, prompt templates, ignore files
- [Providers](docs/providers.md) - OpenAI, Azure OpenAI, Anthropic, Groq, Ollama, Claude Code, Codex, GitHub Copilot CLI, Apple Foundation Models, custom endpoints
- [Architecture](docs/architecture.md) - module layout and data flow
- [Roadmap](docs/roadmap.md) - planned and completed features

## License

MIT
