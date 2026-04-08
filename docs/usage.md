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

## Branch Name Context

`aic` automatically detects ticket or issue references in the current branch name and feeds them into the prompt. For example, on a branch named `feature/PROJ-123-add-auth`, the generated message may reference `PROJ-123`. Both JIRA-style (`PROJ-123`) and GitHub-style (`#456`) patterns are recognised.

## Shell Completions

Generate shell completions:

```sh
aic completions bash
aic completions zsh
aic completions fish
```
