# Usage

Generate a commit message from staged files:

```sh
git add <files>
aic
```

If no files are staged, `aic` can stage all changed files or let you choose files interactively.

Skip the confirmation prompt:

```sh
aic --yes
```

Add extra context for the generated message:

```sh
aic --context "include issue 123"
```

Use the full GitMoji prompt:

```sh
aic --fgm
```

Generate and display the message without committing:

```sh
aic --dry-run
```

Regenerate and amend the last commit message:

```sh
aic --amend
```

Pass Git commit flags through:

```sh
aic --no-verify
```

Use a message template:

```sh
aic "issue-123: $msg"
```

The placeholder defaults to `$msg` and can be changed with `AIC_MESSAGE_TEMPLATE_PLACEHOLDER`.

Tune the system prompt without recompiling by setting `AIC_PROMPT_FILE` to a custom prompt-template path.

## Diff Review

Get AI-powered feedback on staged changes before committing:

```sh
aic review
```

Optionally focus the review on a particular concern:

```sh
aic review --context "focus on security"
```

Findings are grouped by severity (Critical, Warning, Suggestion) and cover bugs, security, performance, correctness, and readability. Large diffs are automatically chunked and synthesized into a single review.

## Pull Request Drafts

Generate a pull request title and Markdown description from the current branch:

```sh
aic pr
aic pr --base origin/main
aic pr --context "call out migration risk"
aic pr --yes
```

`aic pr` compares `HEAD` against a base ref, uses the branch commits plus cumulative diff as context, and prints a copy-ready title and description. If you do not pass `--base`, `aic` tries `refs/remotes/origin/HEAD`, then `origin/main`, `origin/master`, `main`, and `master`.

## Commit History

Every generated commit message, PR draft, and review is saved to `~/.aicommit-history.json`. Browse recent entries:

```sh
aic history
aic history --non-interactive
aic history -n 5
aic history --all
aic history --verbose
```

When `aic history` is run in a terminal, it opens the interactive picker by default. Use `--non-interactive` for the plain text summary view.

Filter by kind:

```sh
aic history --kind commit
aic history --kind pr
aic history --kind review
```

## Rewrite Commit Messages

Clean up the last N commit messages on your branch using AI before opening a PR:

```sh
aic log
aic log -n 3
aic log -n 5 --yes
```

This generates new messages for each commit, shows a before/after comparison, and rewrites them via `git rebase` on confirmation.

Requirements:
- The working tree must be clean (no uncommitted changes).
- The range must not contain merge commits.
- This rewrites git history — do not use on commits that have been pushed to a shared branch.

## Scope Hints

When conventional commit scopes are enabled (the default), `aic` detects likely scopes from the staged file paths and suggests them to the AI. For example, changes to `src/ai/` hint at the scope `ai`, while `Cargo.toml` hints at `deps`. This improves scope consistency without requiring manual input. Disable scopes entirely with `AIC_OMIT_SCOPE=true`.

## Branch Name Context

`aic` automatically detects ticket or issue references in the current branch name and feeds them into the prompt. For example, on a branch named `feature/PROJ-123-add-auth`, the generated message may reference `PROJ-123`. Both JIRA-style (`PROJ-123`) and GitHub-style (`#456`) patterns are recognised.

## Shell Completions

Generate shell completions:

```sh
aic completions bash
aic completions zsh
aic completions fish
```
