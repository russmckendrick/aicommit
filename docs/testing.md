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
