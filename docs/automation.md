# Automation

`aic`, `aic review`, and `aic pr` accept `--json` and `--quiet`, so scripts, CI jobs, and coding agents (Claude Code, Codex, and others) can call them as tools.

In either mode:

- stdout carries only the result. Progress, cards, and spinners are suppressed, and warnings go to stderr.
- `aic` never prompts. If a step would need a prompt (for example nothing is staged and `--yes` was not passed), it exits non-zero with an error on stderr instead.
- the split-commit flow is skipped; one message is generated for the whole staged set.
- `aic` only commits when `--yes` is also passed, and never with `--dry-run`. Without `--yes`, nothing is staged, committed, or pushed.

`--json` and `--quiet` cannot be combined.

## `--quiet`

| Command | Prints |
|---------|--------|
| `aic --quiet` | The commit message |
| `aic review --quiet` | The review Markdown |
| `aic pr --quiet` | The PR title, a blank line, then the description |

```sh
git commit -m "$(aic --quiet)"
```

## `--json`

Every object has a `schema` field. Additive changes keep the number; breaking changes bump it. The current version is `1`.

### `aic --json`

```json
{
  "schema": 1,
  "kind": "commit",
  "message": "✨ feat(ai): stream completions\n\n- add complete_streaming",
  "subject": "✨ feat(ai): stream completions",
  "body": "- add complete_streaming",
  "files": ["src/ai/mod.rs"],
  "provider": "anthropic",
  "model": "claude-sonnet-5-5",
  "committed": false,
  "commit": null
}
```

With `--yes`, `aic` stages all changes when nothing is staged, commits (and pushes when `AIC_GITPUSH` allows it), and sets `committed` to `true` and `commit` to the full commit hash.

### `aic review --json`

```json
{
  "schema": 1,
  "kind": "review",
  "review": "## Findings\n...",
  "files": ["src/ai/mod.rs"],
  "provider": "anthropic",
  "model": "claude-sonnet-5-5"
}
```

### `aic pr --json`

```json
{
  "schema": 1,
  "kind": "pr",
  "title": "Add streaming completions",
  "body": "## Summary\n...",
  "base": "origin/main",
  "commits": 3,
  "files": ["src/ai/mod.rs", "src/ai/sse.rs"],
  "provider": "anthropic",
  "model": "claude-sonnet-5-5"
}
```

Reviews and PR drafts are still written to `aic history` in machine mode. Commit messages are recorded when a commit is actually made (`--yes`).
