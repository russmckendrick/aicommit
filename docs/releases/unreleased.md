# aicommit Unreleased

## Changed

- Added a dedicated GitHub Actions workflow at `.github/workflows/update-winget.yml`
  to submit Windows package updates to the WinGet community repository after a
  GitHub release is published.
- Made the WinGet workflow detect when the initial package submission has not
  merged yet, then exit with a clear rerun message instead of failing with a
  less helpful `wingetcreate update` error.
- Documented WinGet as the recommended Windows installation path, with GitHub
  Releases kept as the direct-download fallback while package updates propagate
  through WinGet review.
