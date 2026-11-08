# Terminal UI

`aic --tui` opens a full-screen terminal UI for the commit workflow. Set `AIC_TUI=true` to open it whenever you run plain `aic` in a terminal.

It never commits on its own. When you press commit, the TUI closes and the normal `aic` flow creates the commit, records history, and offers to push, exactly as the prompt-based flow does.

The TUI is not used when stdin or stdout is not a terminal, or with `--yes`, `--dry-run`, `--amend`, `--json`, or `--quiet`.

## Commit screen

```text
◇ aic · commit · ⎇ main · ticket #42          anthropic/claude-sonnet-5-5 · 3.1k tok
╭──────────────────────────────╮╭ src/ai/mod.rs ──────────────────────── hunk 1/2 ╮
│ Staged · 3 files · 5 hunks   ││ @@ -46,7 +46,15 @@                                │
│ ▌● src/ai/mod.rs             ││   #[async_trait]                                  │
│    ✓ #1 trait AiEngine +8 −1 ││ - async fn generate_commit_message(&self, …)      │
│ ◐ src/ai/anthropic.rs        ││ + async fn complete(&self, m: &[ChatMessage])     │
│ Unstaged · 1 file            ││   …                                               │
│ ○ Cargo.lock new             ││                                                   │
╰──────────────────────────────╯╰───────────────────────────────────────────────────╯
╭ Message ──────────────────────────────── ⠹ ━━━━━━━━━━━━━━━ Writing commit message 3s ╮
│ ✨ feat(ai): stream completions from anthropic and openai providers▌                  │
╰───────────────────────────────────────────────────────────────────────────── 63/72 ╯
 space stage  g generate  e edit  s split  c commit  t theme  ^k palette  ? help
```

- **Tree (left):** staged files with their hunks, then unstaged files and untracked files. `●` is fully staged, `◐` partly staged, `○` unstaged. Each hunk shows its function context (or first changed line) and its added/removed line counts.
- **Diff (right):** the selected file or hunk with syntax highlighting, add/remove tints, and the changed words inside a modified line highlighted.
- **Message (bottom):** opens already writing a message when something is staged. With a streaming provider the message appears as it is written. The counter shows the subject length against 72 characters. If you change what is staged afterwards, the panel warns that the message is stale.

On terminals at least 100 columns wide the tree and diff sit side by side; narrower terminals stack them. Below 60×16 the TUI asks for a bigger window.

### Keys

| Key | Action |
|-----|--------|
| `↑` `↓` / `j` `k` | Move through files and hunks |
| `J` `K` / `PgUp` `PgDn` | Scroll the diff |
| `space` | Stage or unstage the selected hunk, file, or untracked file |
| `a` / `u` | Stage everything / unstage everything |
| `g` (or `r`) | Generate the message again |
| `esc` | Cancel a generation in progress |
| `e` / `tab` | Edit the message; `esc` or `tab` to finish |
| `ctrl-s` | Commit while editing |
| `s` | Split the staged changes into several commits |
| `c` | Commit |
| `t` | Switch to the next theme |
| `ctrl-k` / `:` | Command palette (type to filter, `enter` to run) |
| `?` | Show all shortcuts |
| `q` / `ctrl-c` | Quit without committing (staging changes are kept) |

The mouse works too: click a row to select it, click it again to stage or unstage it, and scroll the tree or the diff.

## Split screen

Press `s` with at least two staged hunks. `aic` asks the model to group the hunks, then shows each proposed commit as a column with its own message:

```text
◇ aic · split · ⎇ main                                   7 hunks · 4 files → 3 commits
╭ 1 Streaming trait ─────────╮╭ 2 UI tidy-up ──────────────╮╭ 3 Docs ────────────────────╮
│ Engine API change          ││ Spinner copy only          ││ Provider table             │
│ ▌ src/ai/mod.rs #1   +8 −1 ││ │ src/ui.rs #1 same file …  ││ │ docs/providers.md #1 +6 … │
│ │ src/ai/mod.rs #2   +3 −0 ││ │ src/ui.rs #3 same file …  ││ │ src/ui.rs #2 same file …  │
│ ────────────────────────── ││ ────────────────────────── ││ ────────────────────────── │
│ ✨ feat(ai): add streaming ││ 💄 style(ui): shorten …    ││ 📝 docs(providers): note … │
╰────────────────────────────╯╰────────────────────────────╯╰────────────────────────────╯
╭ src/ai/mod.rs#1 ──────────────────────────────────────────────────────── in commit 1 ╮
│ @@ -46,7 +46,15 @@ …                                                                  │
╰──────────────────────────────────────────────────────────────────────────────────────╯
 ↑↓ select  tab column  ←→ move hunk  n new group  m merge  r regen message  e edit  enter commit all
```

`same file` marks a file whose hunks go to more than one commit.

| Key | Action |
|-----|--------|
| `↑` `↓` / `j` `k` | Select a hunk |
| `tab` `shift-tab` / `h` `l` | Move between commits |
| `←` `→` / `H` `L` | Move the selected hunk to the previous or next commit; moving right past the last commit creates a new one |
| drag | Drag a hunk to another column with the mouse |
| `n` | Add an empty commit (up to 6) |
| `m` | Merge the selected commit into its neighbour |
| `r` | Rewrite the selected commit's message |
| `e` | Edit the selected commit's message; `esc` saves |
| `enter` | Create all commits |
| `esc` | Go back to a single commit |

Messages for the commits you changed are rewritten automatically a moment after you stop moving hunks. Commits are created the same way as the prompt-based split flow: from an index snapshot, never touching the working tree (see [Usage](usage.md#interactive-diff-splitting)).

## Themes and colour

The TUI uses the same palettes as `aic map`. `AIC_THEME=auto` (the default) picks `github-dark` or `github-light` from the terminal background; set any theme name to pin one, or press `t` to cycle through them for the current session.

The TUI keeps your terminal's own background colour, and uses 24-bit colour when the terminal advertises it (`COLORTERM=truecolor`, iTerm2, WezTerm, Ghostty, VS Code). Otherwise it falls back to the nearest 256-colour values.

Set `AIC_ICONS=nerd` if your terminal font is a Nerd Font, for a Git branch glyph in the header.

## Syntax highlighting

Diffs are highlighted with a small built-in highlighter: keywords, strings, comments, type names, and function calls for Rust, C-like languages (JavaScript, TypeScript, Go, C, Java, Swift, and others), Python, Ruby, shell, and TOML/YAML. Other files are shown with diff colours only.
