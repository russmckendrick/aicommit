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

Pass Git commit flags through:

```sh
aic --no-verify
```

Use a message template:

```sh
aic "issue-123: $msg"
```

The placeholder defaults to `$msg` and can be changed with `AIC_MESSAGE_TEMPLATE_PLACEHOLDER`.
