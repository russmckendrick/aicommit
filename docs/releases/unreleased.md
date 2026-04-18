# aicommit Unreleased

## Release automation

- Convert the tap and WinGet updater workflows into reusable workflows that can still be run manually, so `ci.yml` can call them after a release without duplicating the update logic.
- Keep the WinGet submission on the maintained `winget-releaser` GitHub Action while preserving tag-based manual reruns for existing releases.
