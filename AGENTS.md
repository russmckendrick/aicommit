# AGENTS.md

## Tooling

- Format: `cargo fmt --check`
- Compile: `cargo check`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`
- Tests: `cargo test`

## Non-Obvious Rules

- Keep the public CLI command as `aic`; use `aicommit` only for project/package identity, release naming, and storage paths.
- Tune default commit-message behavior in `prompts/commit-system.md`, not in Rust string literals.
- The provider integration test uses a local mock HTTP server via `wiremock`, so sandboxed runs may need permission to bind localhost.
- CI intentionally ignores doc-only changes in `docs/**`, `README.md`, and `AGENTS.md`, so keep documentation drift in check manually.

## Docs

- Treat `docs/README.md` as the documentation index; update it when adding, removing, or renaming doc pages.
- Keep `README.md` as a short entrypoint and put substantial documentation in `docs/`.
- When behavior changes, keep the docs updated in the same change: commands, flags, config keys, prompts, providers, workflows, and testing instructions should stay in sync.
- Add or update release notes in `docs/releases/` for user-visible changes that affect shipped behavior or documented workflows.

## Project-Specific Patterns

- Add provider implementations behind the `AiEngine` trait.
- Keep Git process calls isolated in `src/git.rs`.
