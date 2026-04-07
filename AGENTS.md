# AGENTS.md

## Tooling

- Format: `cargo fmt --check`
- Compile: `cargo check`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`
- Tests: `cargo test`

## Non-Obvious Rules

- Keep public naming to `aicommit`, `aic`, and `AIC_*`; do not add legacy aliases or config names.
- Keep substantial documentation in `docs/`; keep `README.md` as a short entrypoint.
- The provider integration test uses a local mock HTTP server, so sandboxed runs may need permission to bind localhost.
- Tune default commit-message behavior in `prompts/commit-system.md`, not in Rust string literals.

## Project-Specific Patterns

- Add provider implementations behind the `AiEngine` trait.
- Keep Git process calls isolated in `src/git.rs`.
