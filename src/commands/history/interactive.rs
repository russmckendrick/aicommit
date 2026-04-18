use std::{
    fmt,
    io::{IsTerminal, stdin, stdout},
};

use anyhow::Result;

use crate::{
    history_store::{HistoryEntry, RecentEntries},
    ui,
};

use super::format::{
    compact_summary, file_count_label, format_timestamp, kind_badge, repo_label, selection_label,
    uses_markdown_rendering,
};

#[derive(Copy, Clone)]
enum BrowserView {
    Primary,
    Hidden,
}

#[derive(Clone)]
struct MenuOption {
    action: MenuAction,
    label: String,
}

impl MenuOption {
    fn new(action: MenuAction, label: impl Into<String>) -> Self {
        Self {
            action,
            label: label.into(),
        }
    }
}

impl fmt::Display for MenuOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label)
    }
}

#[derive(Clone)]
enum DetailAction {
    BackToList,
    Exit,
}

impl fmt::Display for DetailAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DetailAction::BackToList => write!(f, "Back to list"),
            DetailAction::Exit => write!(f, "Exit"),
        }
    }
}

#[derive(Clone)]
enum MenuAction {
    Entry(usize),
    ShowHidden,
    BackToMain,
    Exit,
}

pub(crate) fn run_interactive(result: &RecentEntries, include_all: bool) -> Result<()> {
    if result.primary_entries.is_empty() && (!include_all || result.hidden_entries.is_empty()) {
        if result.hidden_count > 0 {
            ui::info(format!(
                "no history entries found ({} hidden test/temp entries, use --all to show)",
                result.hidden_count
            ));
        } else {
            ui::info("no history entries found");
        }
        return Ok(());
    }

    let mut view = if result.primary_entries.is_empty() && include_all {
        BrowserView::Hidden
    } else {
        BrowserView::Primary
    };

    loop {
        let entries = entries_for_view(result, view);
        let prompt = match view {
            BrowserView::Primary => "History entries",
            BrowserView::Hidden => "Hidden test/temp entries",
        };

        let selection = match ui::select(prompt, menu_options(result, view, include_all)) {
            Ok(selection) => selection,
            Err(error) if ui::is_prompt_cancelled(&error) => return Ok(()),
            Err(error) => return Err(error),
        };

        match selection {
            MenuOption {
                action: MenuAction::Entry(index),
                ..
            } => {
                if let Some(entry) = entries.get(index) {
                    let should_exit = show_entry_detail(entry)?;
                    if should_exit {
                        return Ok(());
                    }
                }
            }
            MenuOption {
                action: MenuAction::ShowHidden,
                ..
            } => view = BrowserView::Hidden,
            MenuOption {
                action: MenuAction::BackToMain,
                ..
            } => view = BrowserView::Primary,
            MenuOption {
                action: MenuAction::Exit,
                ..
            } => return Ok(()),
        }
    }
}

pub(crate) fn should_use_interactive(force_interactive: bool, force_non_interactive: bool) -> bool {
    should_use_interactive_for_terminals(
        force_interactive,
        force_non_interactive,
        stdin().is_terminal(),
        stdout().is_terminal(),
    )
}

fn show_entry_detail(entry: &HistoryEntry) -> Result<bool> {
    ui::blank_line();
    ui::section(format!(
        "{} {}",
        kind_badge(&entry.kind),
        compact_summary(entry)
    ));
    ui::metadata_row(&[
        format_timestamp(&entry.timestamp),
        entry.provider.clone(),
        entry.model.clone(),
        repo_label(&entry.repo_path),
    ]);
    ui::secondary(format!("repo: {}", entry.repo_path));
    ui::secondary(format!("files: {}", file_count_label(entry.files.len())));

    if !entry.files.is_empty() {
        ui::file_metadata(&entry.files);
        for line in ui::summarize_files(&entry.files, 4, 3) {
            ui::bullet(line);
        }
    }

    ui::blank_line();
    let title = if uses_markdown_rendering(&entry.kind) {
        "Stored markdown"
    } else {
        "Stored output"
    };
    if uses_markdown_rendering(&entry.kind) {
        ui::markdown_card(title, &entry.message);
    } else {
        ui::primary_card(title, &entry.message);
    }

    ui::blank_line();
    let action = match ui::select(
        "History entry",
        vec![DetailAction::BackToList, DetailAction::Exit],
    ) {
        Ok(action) => action,
        Err(error) if ui::is_prompt_cancelled(&error) => return Ok(true),
        Err(error) => return Err(error),
    };

    Ok(matches!(action, DetailAction::Exit))
}

fn should_use_interactive_for_terminals(
    force_interactive: bool,
    force_non_interactive: bool,
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
) -> bool {
    if force_non_interactive {
        return false;
    }

    if force_interactive {
        return true;
    }

    stdin_is_terminal && stdout_is_terminal
}

fn entries_for_view(result: &RecentEntries, view: BrowserView) -> &[HistoryEntry] {
    match view {
        BrowserView::Primary => &result.primary_entries,
        BrowserView::Hidden => &result.hidden_entries,
    }
}

fn menu_options(result: &RecentEntries, view: BrowserView, include_all: bool) -> Vec<MenuOption> {
    let mut options = entries_for_view(result, view)
        .iter()
        .enumerate()
        .map(|(index, entry)| MenuOption::new(MenuAction::Entry(index), selection_label(entry)))
        .collect::<Vec<_>>();

    match view {
        BrowserView::Primary => {
            if include_all && result.hidden_count > 0 {
                options.push(MenuOption::new(
                    MenuAction::ShowHidden,
                    format!("Show hidden test/temp entries ({})", result.hidden_count),
                ));
            }
        }
        BrowserView::Hidden => {
            if result.primary_count > 0 {
                options.push(MenuOption::new(
                    MenuAction::BackToMain,
                    "Back to main history",
                ));
            }
        }
    }

    options.push(MenuOption::new(MenuAction::Exit, "Exit"));
    options
}

#[cfg(test)]
mod tests {
    use crate::history_store::{HistoryEntry, RecentEntries};

    use super::*;

    fn entry(kind: &str, repo_path: &str, message: &str) -> HistoryEntry {
        HistoryEntry {
            timestamp: "2026-04-09T18:57:00Z".to_string(),
            kind: kind.to_string(),
            message: message.to_string(),
            repo_path: repo_path.to_string(),
            files: vec!["src/history.rs".to_string(), "src/cli.rs".to_string()],
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
        }
    }

    #[test]
    fn menu_options_include_hidden_bucket_link() {
        let result = RecentEntries {
            primary_entries: vec![entry(
                "commit",
                "/Users/example/Code/aicommit",
                "feat: keep main",
            )],
            hidden_entries: vec![entry("review", "/Users/example/.tmp123", "P1: hidden test")],
            primary_count: 1,
            hidden_count: 1,
        };

        let options = menu_options(&result, BrowserView::Primary, true);
        assert_eq!(options.len(), 3);
        assert!(matches!(options[0].action, MenuAction::Entry(_)));
        assert!(matches!(options[1].action, MenuAction::ShowHidden));
        assert!(matches!(options[2].action, MenuAction::Exit));
    }

    #[test]
    fn interactive_is_default_on_terminals() {
        assert!(should_use_interactive_for_terminals(
            false, false, true, true
        ));
        assert!(!should_use_interactive_for_terminals(
            false, false, true, false
        ));
    }

    #[test]
    fn non_interactive_flag_overrides_terminal_default() {
        assert!(!should_use_interactive_for_terminals(
            false, true, true, true
        ));
    }
}
