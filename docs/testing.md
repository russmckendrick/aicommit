# Testing

Run formatting:

```sh
cargo fmt --check
```

Run compile checks:

```sh
cargo check
```

Run lint checks:

```sh
cargo clippy --all-targets --all-features -- -D warnings
```

Run tests:

```sh
cargo test
```

The test suite is designed to cover configuration precedence, prompt-template interpolation, token splitting, provider payload handling, Git repository flows, ignore-file behavior, and hook message insertion.

It also covers the streaming parsers (against `wiremock` server-sent-event responses), `--json`/`--quiet` output, hunk parsing, and hunk-level split commits in real temporary repositories, including partially staged files, renames, binary files, and rollback after a failed commit.

The terminal UI is tested by rendering frames to `ratatui`'s `TestBackend` against a temporary repository with the built-in `test` provider. To see the rendered frames:

```sh
cargo test tui::tests -- --nocapture
```
