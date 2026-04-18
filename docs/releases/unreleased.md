# aicommit Unreleased

## Release automation

- Convert the tap and WinGet updater workflows into reusable workflows that can still be run manually, so `ci.yml` can call them after a release without duplicating the update logic.
- Keep the WinGet submission on the maintained `winget-releaser` GitHub Action while preserving tag-based manual reruns for existing releases.
- Validate the WinGet secret up front so the workflow now tells you when `WINGET_CREATE_GITHUB_TOKEN` is not a classic `public_repo` PAT, instead of failing later with an opaque branch-creation permission error.
