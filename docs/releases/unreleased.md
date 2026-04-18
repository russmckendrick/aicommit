# aicommit Unreleased

## Release automation

- Sync the submitter fork of `microsoft/winget-pkgs` before running `wingetcreate update --submit`, so automated WinGet submissions do not fail just because the cached fork has drifted behind upstream.
- Let `wingetcreate` read `WINGET_CREATE_GITHUB_TOKEN` from the environment instead of passing the PAT on the command line, which avoids the token warning in the workflow logs.
