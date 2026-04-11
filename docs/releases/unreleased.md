# aicommit Unreleased

## Changed

- Made `aic --yes` fully non-interactive for commits by auto-staging all changed files, auto-pushing when exactly one remote is configured, and failing clearly instead of guessing when multiple remotes exist.
- Changed the default answer for the interactive single-remote push prompt from `No` to `Yes`.
- Added first-class `anthropic` provider support using Anthropic's Messages API, including setup guidance, model listing, and provider overrides.
- Added first-class `groq` provider support as a named OpenAI-compatible preset with Groq defaults, model listing, and provider overrides.
- Updated provider, configuration, installation, architecture, and roadmap docs to cover the new Anthropic and Groq paths.
- Added a dedicated GitHub Actions workflow at `.github/workflows/update-winget.yml`
  to submit Windows package updates to the WinGet community repository after a
  GitHub release is published.
- Made the WinGet workflow detect when the initial package submission has not
  merged yet, then exit with a clear rerun message instead of failing with a
  less helpful `wingetcreate update` error.
- Documented WinGet as the recommended Windows installation path, with GitHub
  Releases kept as the direct-download fallback while package updates propagate
  through WinGet review.
