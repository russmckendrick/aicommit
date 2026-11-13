# aicommit Unreleased

## Added

- `.aicignore` is now read alongside `.aicommitignore` to exclude files from AI diff input. The two names are interchangeable; if both exist their patterns are combined, with `.aicignore` read second. See [Configuration](../configuration.md).

## Fixed

- A repository with a `.aicommitignore` file no longer fails every command with `failed to read .aicommitignore`. The file was being read correctly but a successful read was treated as the error.
