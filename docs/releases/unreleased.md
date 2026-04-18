# aicommit Unreleased

## Interactive CLI polish

- Refresh the interactive `aic` flows with a transcript-style presentation layer: compact file summaries, bordered commit and draft previews, clearer action labels, and more polished commit/push status output.
- Align `commit`, `review`, `pr`, `history`, `setup`, `models`, `config`, and hook-related terminal output around the same shared UI helpers.
- Add an interactive staged-file preflight so `aic` can unstage selected files before generating a commit message, while keeping `aic review` read-only once files are staged.
- Refresh `aic review` so it renders the review as terminal markdown and ends with a short completion summary instead of dropping straight back to the shell.
- Make staged-file unstaging work in repositories before the first commit, smooth the empty-selection path in the staged preflight, and clarify that staging menus only appear in interactive terminals.

## Release automation

- Move WinGet submission into the main tag release pipeline so the Windows package update runs automatically after GitHub Releases are published, matching the existing Homebrew tap flow.
- Keep a manual WinGet recovery workflow for rerunning a specific release tag when the package repository or external validation needs a second pass.
