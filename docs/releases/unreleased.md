# aicommit Unreleased

## Providers

- Add `copilot` as a supported local CLI provider alongside `claude-code` and `codex`, so commit generation, review, PR drafting, log rewriting, setup, and model listing can all reuse an installed GitHub Copilot CLI without configuring `AIC_API_KEY`.

## Commit workflow

- Fall back to staged Git change metadata when every staged file is filtered from the readable diff, so `aic` can still draft commit messages for binary-only or asset-only changes without inspecting file contents.
- Add a push-aware upstream sync guard before commit creation, so `aic` now stops early when the tracked branch is behind or diverged instead of creating a new local commit and failing later at push time.
- Add AI-assisted Git recovery guidance for sync failures, rejected pushes, and rebase problems, while keeping Git itself as the source of truth for fetch, rebase, and conflict detection.

## Release automation

- Convert the tap and WinGet updater workflows into reusable workflows that can still be run manually, so `ci.yml` can call them after a release without duplicating the update logic.
- Keep the WinGet submission on the maintained `winget-releaser` GitHub Action while preserving tag-based manual reruns for existing releases.
- Validate the WinGet secret up front so the workflow now tells you when `WINGET_CREATE_GITHUB_TOKEN` is not a classic `public_repo` PAT, instead of failing later with an opaque branch-creation permission error.
- Validate the expected `winget-pkgs` fork before submission so the workflow now shows which GitHub user the PAT belongs to and whether that token can actually write to the fork used for PR creation.
- Publish the resolved WinGet pull request URL into the workflow summary and expose it as a reusable-workflow output, so both the manual updater run and the main release overview link straight to the submission PR.
