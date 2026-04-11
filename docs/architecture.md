# Architecture

The Rust crate is organized around small modules:

```text
src/cli.rs              CLI parser and dispatch
src/cli_help.toml       Bundled help text and config-key descriptions
src/commands/           User-facing command flows
src/config.rs           Defaults, global config, and environment overrides
src/git.rs              Git command wrapper and repository helpers
src/prompt.rs           Prompt-template interpolation and response cleanup
src/token.rs            Token counting and diff splitting
src/generator.rs        Prompt, chunking, and AI engine orchestration
src/history.rs          Commit and review history persistence
src/ai/                 Provider trait and provider implementations
```

The `aic` binary calls the shared library entrypoint.

```mermaid
flowchart LR
    Bin["aic binary"] --> Cli["src/cli.rs"]
    Cli --> Commands["src/commands"]
    Commands --> Config["src/config.rs"]
    Commands --> Git["src/git.rs"]
    Commands --> Generator["src/generator.rs"]
    Generator --> Prompt["src/prompt.rs"]
    Generator --> Token["src/token.rs"]
    Generator --> Ai["src/ai"]
    Ai --> Provider["HTTP or local CLI provider"]
```

Provider implementations use an `AiEngine` trait that accepts normalized chat messages and returns a commit message string. This keeps the commit flow independent of transport details such as HTTP payloads or local subprocess execution.

Current provider families:

- OpenAI-compatible HTTP engines for `openai`, `azure-openai`, and `groq`
- Anthropic Messages API engine for `anthropic`
- Command-backed engines for `claude-code` and `codex`

Git behavior is isolated behind `src/git.rs` so commit, push, hooks, staged-file discovery, and ignore-file filtering are testable without mixing Git process logic into UI commands.

Prompt templates live in `prompts/`:

- `commit-system.md` — system prompt for commit message generation. Supports scope hints derived from staged file paths.
- `review-system.md` — system prompt for `aic review` diff analysis.

Use `AIC_PROMPT_FILE` to point at a custom commit prompt template.

## Commit Generation Flow

```mermaid
sequenceDiagram
    participant User
    participant CLI as aic
    participant Git
    participant Generator
    participant Provider
    User->>CLI: Run aic
    CLI->>Git: Read staged files and diff
    CLI->>Generator: Send diff and config
    Generator->>Generator: Split large diffs into chunks
    Generator->>Provider: Generate chunk summaries if needed
    Generator->>Provider: Generate final commit message
    Provider-->>Generator: Commit message
    Generator-->>CLI: Formatted message
    CLI-->>User: Confirm, regenerate, or abort
    User->>CLI: Accept message
    CLI->>Git: git commit
```
