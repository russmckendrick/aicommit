# Terminal UI

`aic` opens a full-screen terminal UI for the commit workflow whenever it runs in an interactive terminal.

- `aic --fast` skips it for one run and uses the quick prompt flow.
- `AIC_TUI=false` makes the prompt flow the default; `aic --tui` then opens the UI on demand.

The whole workflow happens inside it: stage, write the message, commit, and push. When you quit, `aic` leaves a short summary of the commits it made (hash, branch, where they were pushed, and line counts) in your normal terminal scrollback.

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
- **Header:** the branch, its sync state with the upstream (`↑2` ahead, `↓1 behind origin/main`, or diverged; checked in the background when `AIC_GITPUSH` is on), and the provider with its icon and model.
- **Message (bottom):** opens already writing a message when something is staged. If you start with nothing staged, the message starts writing itself a moment after you stage something. With a streaming provider the message appears as it is written. The counter shows the subject length against 72 characters. If you change what is staged afterwards, the panel warns that the message is stale.

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
| `c` | Commit (then push from the result screen) |
| `t` | Switch to the next theme for this session |
| `,` | Settings |
| `ctrl-k` / `:` | Command palette (type to filter, `enter` to run) |
| `?` | Show all shortcuts |
| `q` / `ctrl-c` | Quit without committing (staging changes are kept) |

The mouse works too: click a row to select it, click it again to stage or unstage it, and scroll the tree or the diff.

## Result screen

Committing (`c`) runs in the background with a progress panel, then shows the result over the commit screen:

```text
╭  Committed ─────────────────────────────────────────────────────────╮
│  a7d1032    main   +256 −102   4 files                             │
│   ✨ feat(explore): make release metadata complete                    │
│   - Replace round stickers with compact price-gun labels             │
│ ──────────────────────────────────────────────────────────────────── │
│  Push main to [ GitHub] origin?                                    │
╰──────────────────────────────────────────────────────────────────────╯
 enter push  s skip  q quit
```

| Key | Action |
|-----|--------|
| `enter` / `p` | Push the branch (with several remotes, pick one with `↑` `↓` first) |
| `s` / `esc` | Skip pushing |
| `r` | After a rejected push: `git pull --rebase`, then push again |
| `enter` | Once pushed or skipped: back to the commit screen if anything is still changed, otherwise close |
| `q` | Quit |

If the remote has commits your branch doesn't, the push is rejected and the screen says so. Press `r` to rebase onto them and push again; the commit hashes shown are updated to the rebased commits. When other uncommitted changes make a rebase unsafe, the screen tells you to commit or stash them and run `git pull --rebase` yourself. Pushing is not offered when `AIC_GITPUSH=false` or the repository has no remotes.

Being behind the upstream no longer stops `aic` from starting in the terminal UI; the header shows it, and the push step handles it. The prompt flow (`--fast`) still checks before committing.

## Icons

With `AIC_ICONS=auto` (the default) the terminal UI uses Nerd Font icons whenever `AIC_REMOTE_ICON_STYLE` does, which is also its default: file-type icons in the tree, git icons for branches and commits, provider logos in the header (OpenAI, Claude, GitHub Copilot, Azure, and Apple; Ollama and Groq, which have no Nerd Font logo, get a generic icon in their brand colour), and the Git host icon (GitHub, GitLab, and others) in push prompts. The OpenAI and Claude logos need Nerd Fonts 3.5 or later; older Nerd Fonts show a placeholder box for them. Set `AIC_ICONS=unicode` if your terminal font is not a Nerd Font, or `AIC_ICONS=nerd` to force them.

## Settings screen

Press `,` (or choose **Settings** in the command palette) to change your configuration without leaving the UI. Changes are saved to `~/.aicommit` as soon as you make them, using the same validation as `aic config set`.

| Group | Settings |
|-------|----------|
| AI provider | Provider, model, API key, API URL |
| Messages | GitMoji prefix, body bullets, one line only, omit scope, match repo style, language |
| Workflow | Offer to push, open this UI by default |
| Appearance | Theme, icons |

| Key | Action |
|-----|--------|
| `↑` `↓` / `j` `k` | Select a setting |
| `enter` / `space` | Toggle an on/off setting, or edit a text setting |
| `←` `→` / `h` `l` | Choose a provider, model, theme, or icon set |
| `enter` on Model | Type any model ID |
| `⌫` | Clear a text setting back to its default |
| `esc` / `,` | Back to the commit |

Changing the provider resets the model to that provider's default. The model list combines `aic`'s suggestions with chat models from `aic models --refresh`. API keys are masked. If an `AIC_*` environment variable is set for a key, the screen says so, because the environment wins over the config file.

### First run

If the configured provider needs an API key and none is set, `aic` opens straight onto the settings screen with a short welcome, the API key field selected. Add a key (or switch to a local provider such as `ollama` or `claude-code`), press `esc`, and `aic` starts writing your message. `q` quits.

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

See [Icons](#icons) for Nerd Font icons.

## Syntax highlighting

Diffs are highlighted with a small built-in highlighter: keywords, strings, comments, type names, and function calls for Rust, C-like languages (JavaScript, TypeScript, Go, C, Java, Swift, and others), Python, Ruby, shell, and TOML/YAML. Other files are shown with diff colours only.
