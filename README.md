# aicommit

A fast, opinionated CLI that generates Git commit messages and reviews staged diffs using AI. Written in Rust, installed as `aic`.

## Quick start

```sh
# Install via Homebrew (macOS)
brew install russmckendrick/tap/aicommit

# Or download a binary from GitHub Releases
# https://github.com/russmckendrick/aicommit/releases

# Run setup to configure your provider and API key
aic setup
```

## Usage

Stage your changes and run `aic` — it generates a conventional commit message, shows it for confirmation, and commits on approval:

```sh
git add -p
aic
```

If nothing is staged, `aic` offers to stage files for you.

### Flags

| Flag | Description |
|------|-------------|
| `-c`, `--context` | Add context for the AI (e.g., `-c "closes #42"`) |
| `-y`, `--yes` | Skip the confirmation prompt |
| `-d`, `--dry-run` | Print the message without committing |
| `--amend` | Regenerate and amend the last commit |
| `--fgm` | Use the full GitMoji specification |

### Review staged changes

Get AI-powered feedback on your staged diff before committing — findings are grouped by severity and rendered in the terminal:

```sh
aic review
aic review -c "focus on security"
```

### Rewrite commit messages

Clean up the last N commit messages on your branch before opening a PR:

```sh
aic log
aic log -n 3
```

### Browse history

View recent AI-generated commit messages and reviews:

```sh
aic history
aic history --kind review
aic history --all
aic history --verbose
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
- **Scope hints** — detects likely scopes from changed file paths
- **Branch context** — extracts ticket/issue references from branch names
- **Diff review** — AI-powered code review with severity grouping
- **Message rewriting** — clean up commit history with `aic log`
- **History** — browse past generated messages and reviews
- **Large diff handling** — automatic chunking and synthesis
- **Custom prompts** — swap the system prompt without recompiling
- **OpenAI and Azure OpenAI** providers, plus any OpenAI-compatible endpoint

## Configuration

Set values globally with `aic config set` or per-invocation with environment variables:

```sh
aic config set AIC_MODEL=gpt-5.4-mini AIC_EMOJI=true
AIC_LANGUAGE=french aic
```

See [docs/configuration.md](docs/configuration.md) for the full key reference.

## Documentation

Detailed docs live in [`docs/`](docs/):

- [Installation](docs/installation.md) — Homebrew, GitHub Releases, from source
- [Usage](docs/usage.md) — commit workflow, review, flags, hooks
- [Configuration](docs/configuration.md) — keys, prompt templates, ignore files
- [Providers](docs/providers.md) — OpenAI, Azure OpenAI, custom endpoints
- [Architecture](docs/architecture.md) — module layout and data flow
- [Roadmap](docs/roadmap.md) — planned and completed features

## License

MIT
