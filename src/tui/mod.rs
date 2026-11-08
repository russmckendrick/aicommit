//! Interactive terminal UI (`aic --tui`): stage hunks, stream a message,
//! edit it, and split the staged set into several commits.
//!
//! The TUI never commits by itself. It returns an [`Outcome`] and the
//! normal commit flow performs the commit, history, and push steps, so both
//! frontends share one code path for anything that changes history.

mod app;
mod editor;
mod highlight;
mod icons;
mod settings;
pub mod theme;
mod view;

use std::{
    io::{IsTerminal, Stdout, stdout},
    time::Duration,
};

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

pub use app::{SessionCommit, TuiInput};

/// Whether the TUI can run here: both stdin and stdout must be terminals.
pub fn available() -> bool {
    std::io::stdin().is_terminal() && stdout().is_terminal()
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            previous(info);
        }));
        let terminal = Terminal::new(CrosstermBackend::new(out))?;
        Ok(Self { terminal })
    }
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen);
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
        let _ = self.terminal.show_cursor();
    }
}

/// Run the commit TUI until the user quits, and return the commits made.
pub async fn run_commit(input: TuiInput) -> Result<Vec<SessionCommit>> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut app = app::App::new(input, tx)?;
    if !app.staged_files.is_empty() && !app.input.first_run {
        // Open straight into a streaming message: the first thing people see.
        app.start_generation();
    }

    let mut guard = TerminalGuard::enter()?;
    let mut dirty = true;
    loop {
        while let Ok(event) = rx.try_recv() {
            app.handle_app_event(event);
            dirty = true;
        }
        app.tick();
        if app.needs_clear {
            // Clear the screen and blank both buffers so the next draw
            // repaints every cell. `Terminal::clear` would also query the
            // cursor position, which some terminals never answer.
            execute!(
                guard.terminal.backend_mut(),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
            )?;
            guard.terminal.swap_buffers();
            app.needs_clear = false;
            dirty = true;
        }
        if dirty || app.animating() {
            guard.terminal.draw(|frame| view::draw(frame, &mut app))?;
            dirty = false;
        }
        if app.outcome.take().is_some() {
            return Ok(std::mem::take(&mut app.log));
        }

        let timeout = if app.animating() {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(120)
        };
        // Poll on a blocking thread so generation tasks keep running.
        let ready = tokio::task::block_in_place(|| event::poll(timeout))?;
        if ready {
            match tokio::task::block_in_place(event::read)? {
                Event::Key(key) => app.handle_key(key),
                Event::Mouse(mouse) => app.handle_mouse(mouse),
                Event::Resize(_, _) => app.needs_clear = true,
                _ => continue,
            }
            dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, process::Command};

    use ratatui::{Terminal, backend::TestBackend};
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    use super::{
        app::{App, Screen, TuiInput},
        view,
    };
    use crate::{config::Config, git::cwd_test_lock};

    fn git(cwd: &Path, args: &[&str]) {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(cwd)
                .status()
                .unwrap()
                .success()
        );
    }

    fn repo() -> TempDir {
        let temp = TempDir::new().unwrap();
        git(temp.path(), &["init", "-q", "-b", "main"]);
        git(temp.path(), &["config", "user.email", "t@example.com"]);
        git(temp.path(), &["config", "user.name", "T"]);
        let lines = (1..=40)
            .map(|n| format!("let value_{n} = {n};\n"))
            .collect::<String>();
        std::fs::write(temp.path().join("lib.rs"), &lines).unwrap();
        git(temp.path(), &["add", "."]);
        git(temp.path(), &["commit", "-q", "-m", "init"]);
        let changed = lines
            .replace("let value_2 = 2;", "let renamed_2 = 2;")
            .replace("let value_35 = 35;", "let value_35 = 350;");
        std::fs::write(temp.path().join("lib.rs"), changed).unwrap();
        git(temp.path(), &["add", "lib.rs"]);
        std::fs::write(temp.path().join("notes.md"), "# notes\n").unwrap();
        temp
    }

    fn render(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| view::draw(frame, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().to_owned())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn input() -> TuiInput {
        TuiInput {
            config: Config {
                ai_provider: "test".to_owned(),
                gitpush: false,
                theme: "github-dark".to_owned(),
                icons: "unicode".to_owned(),
                ..Config::default()
            },
            first_run: false,
            extra_args: Vec::new(),
            context: String::new(),
            full_gitmoji_spec: false,
        }
    }

    async fn drain(app: &mut App, rx: &mut mpsc::UnboundedReceiver<super::app::AppEvent>) {
        for _ in 0..200 {
            while let Ok(event) = rx.try_recv() {
                app.handle_app_event(event);
            }
            if !app.animating_work() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test(flavor = "multi_thread")]
    async fn commit_screen_renders_tree_diff_and_generated_message() {
        let _lock = cwd_test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let repo = repo();
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(repo.path()).unwrap();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut app = App::new(input(), tx).unwrap();
        app.start_generation();
        drain(&mut app, &mut rx).await;
        app.move_selection(1);
        let wide = render(&mut app, 120, 30);
        let narrow = render(&mut app, 80, 30);
        let tiny = render(&mut app, 50, 10);
        std::env::set_current_dir(previous).unwrap();

        println!("{wide}\n\n{narrow}\n\n{tiny}");
        assert!(wide.contains("◇ aic · ● commit · ⎇ main"));
        assert!(wide.contains("Staged · 1 file · 2 hunks"));
        assert!(wide.contains("● lib.rs"));
        assert!(wide.contains("✓ #1"));
        assert!(wide.contains("Unstaged · 1 file"));
        assert!(wide.contains("notes.md new"));
        assert!(wide.contains("renamed_2"));
        assert!(wide.contains("feat: add generated commit message"));
        assert!(wide.contains("space  stage"));
        assert!(narrow.contains("Staged · 1 file"));
        assert!(tiny.contains("Terminal too small"));
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test(flavor = "multi_thread")]
    async fn staging_toggles_hunks_and_split_screen_renders_groups() {
        let _lock = cwd_test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let repo = repo();
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(repo.path()).unwrap();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut app = App::new(input(), tx).unwrap();
        // Select hunk #2 and unstage it: lib.rs becomes partially staged.
        app.move_selection(2);
        app.run_action(super::app::Action::ToggleStage);
        let staged_after = app.staged.hunk_ids();
        let partial = render(&mut app, 120, 30);
        // Stage it back, then split.
        app.run_action(super::app::Action::StageAll);
        app.run_action(super::app::Action::Split);
        drain(&mut app, &mut rx).await;
        if let Some(split) = app.split.as_mut() {
            // Render after the reveal animation has finished.
            split.appeared -= std::time::Duration::from_secs(2);
        }
        let split = render(&mut app, 120, 30);
        let screen = app.screen;
        std::env::set_current_dir(previous).unwrap();

        println!("{partial}\n\n{split}");
        assert_eq!(staged_after, vec!["lib.rs#1"]);
        assert!(partial.contains("◐ lib.rs"));
        assert_eq!(screen, Screen::Split);
        assert!(split.contains("◇ aic · ⑂ split"));
        assert!(split.contains("3 hunks · 2 files → 2 commits"));
        assert!(split.contains("1 Primary change"));
        assert!(split.contains("2 Follow-up change"));
        assert!(split.contains("lib.rs #1"));
        assert!(split.contains("notes.md #1"));
        assert!(split.contains("same file"));
        assert!(split.contains("feat: add generated commit message"));
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test(flavor = "multi_thread")]
    async fn settings_screen_renders_and_saves_changes() {
        let _lock = cwd_test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let repo = repo();
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(repo.path()).unwrap();

        let (tx, _rx) = mpsc::unbounded_channel();
        let mut first_run_input = input();
        first_run_input.config.ai_provider = "openai".to_owned();
        first_run_input.first_run = true;
        let mut app = App::new(first_run_input, tx).unwrap();
        app.config_path = repo.path().join(".aicommit");
        if let Some(settings) = app.settings.as_mut() {
            settings.path = app.config_path.clone();
        }
        let first_run = render(&mut app, 120, 34);

        // Toggle GitMoji off (fields: provider, model, key, url, emoji).
        for _ in 0..2 {
            app.handle_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Down,
                crossterm::event::KeyModifiers::NONE,
            ));
        }
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        let saved = std::fs::read_to_string(repo.path().join(".aicommit")).unwrap_or_default();
        let after = render(&mut app, 120, 34);
        std::env::set_current_dir(previous).unwrap();

        println!("{first_run}\n\n{after}");
        assert_eq!(app.screen, Screen::Settings);
        assert!(first_run.contains("◇ aic · ⚙ settings"));
        assert!(first_run.contains("Welcome to aic."));
        assert!(first_run.contains("▌ API key"));
        assert!(first_run.contains("not set"));
        assert!(first_run.contains("‹ ") || first_run.contains("openai"));
        assert!(first_run.contains("GitMoji prefix"));
        assert!(first_run.contains("Open this UI by default"));
        assert!(saved.contains("AIC_EMOJI = false"), "{saved}");
        assert!(after.contains("○ off"));
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test(flavor = "multi_thread")]
    async fn committing_shows_the_result_screen_and_continues() {
        let _lock = cwd_test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let repo = repo();
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(repo.path()).unwrap();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut app = App::new(input(), tx).unwrap();
        app.start_generation();
        drain(&mut app, &mut rx).await;
        app.run_action(super::app::Action::Commit);
        let committing = render(&mut app, 120, 34);
        for _ in 0..200 {
            while let Ok(event) = rx.try_recv() {
                app.handle_app_event(event);
            }
            if app.committing.is_none() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        if let Some(done) = app.done.as_mut() {
            done.appeared -= std::time::Duration::from_secs(2);
        }
        let result = render(&mut app, 120, 34);
        let subject = Command::new("git")
            .args(["log", "--format=%s", "-1"])
            .current_dir(repo.path())
            .output()
            .unwrap();
        // notes.md is still untracked, so enter goes back to the commit screen.
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        let screen_after = app.screen;
        std::env::set_current_dir(previous).unwrap();

        println!("{committing}\n\n{result}");
        assert!(committing.contains("Committing…"));
        assert_eq!(
            String::from_utf8_lossy(&subject.stdout).trim(),
            "feat: add generated commit message"
        );
        assert_eq!(app.log.len(), 1);
        assert!(result.contains("✓ Committed"));
        assert!(result.contains("⎇ main"));
        assert!(result.contains("+2 −2"));
        assert!(result.contains("1 file"));
        assert!(result.contains("Pushing is off (AIC_GITPUSH=false)"));
        assert!(result.contains("next commit (1 file still changed)"));
        assert_eq!(screen_after, Screen::Commit);
    }
}
