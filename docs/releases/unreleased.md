# aicommit Unreleased

## Changed

- Made `aic --yes` fully non-interactive for commits by auto-staging all changed files, auto-pushing when exactly one remote is configured, and failing clearly instead of guessing when multiple remotes exist.
- Changed the default answer for the interactive single-remote push prompt from `No` to `Yes`.
- Added first-class `anthropic` provider support using Anthropic's Messages API, including setup guidance, model listing, and provider overrides.
- Added first-class `groq` provider support as a named OpenAI-compatible preset with Groq defaults, model listing, and provider overrides.
- Added first-class `ollama` provider support as a named local OpenAI-compatible preset with local defaults and model listing.
- Added interactive file-group diff splitting to the normal `aic` commit flow, including AI-suggested groups, manual regrouping, and sequential split commits.
- Updated provider, configuration, installation, usage, architecture, and roadmap docs to cover Ollama and interactive diff splitting.
- Added a dedicated GitHub Actions workflow at `.github/workflows/update-winget.yml`
  to submit Windows package updates to the WinGet community repository after a
  GitHub release is published.
- Made the WinGet workflow detect when the initial package submission has not
  merged yet, then exit with a clear rerun message instead of failing with a
  less helpful `wingetcreate update` error.
- Documented WinGet as the recommended Windows installation path, with GitHub
  Releases kept as the direct-download fallback while package updates propagate
  through WinGet review.
