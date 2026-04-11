use std::{
    fmt,
    io::{IsTerminal, stdin, stdout},
    path::Path,
};

use anyhow::Result;
use chrono::{DateTime, Local};

use crate::{history, history::HistoryEntry, history::RecentEntries, ui};

pub fn run(
    count: usize,
    kind: Option<String>,
    include_all: bool,
    verbose: bool,
    interactive: bool,
    non_interactive: bool,
) -> Result<()> {
    let result = history::recent_entries(count, kind.as_deref())?;
    let interactive = should_use_interactive(interactive, non_interactive);

    if interactive {
        return run_interactive(&result, include_all);
    }

    render_history(&result, kind.as_deref(), include_all, verbose)
}

fn render_history(
    result: &RecentEntries,
    kind: Option<&str>,
    include_all: bool,
    verbose: bool,
) -> Result<()> {
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

    ui::section(section_label(
        kind,
        result.primary_count,
        result.hidden_count,
    ));

    if !result.primary_entries.is_empty() {
        render_entry_list("Recent entries", &result.primary_entries, verbose);
    }

    if include_all && !result.hidden_entries.is_empty() {
        if !result.primary_entries.is_empty() {
            ui::blank_line();
        }
        render_entry_list(
            &format!("Hidden test/temp entries ({})", result.hidden_count),
            &result.hidden_entries,
            verbose,
        );
    }

    Ok(())
}

fn render_entry_list(title: &str, entries: &[HistoryEntry], verbose: bool) {
    ui::section(title);

    for entry in entries {
        ui::blank_line();
        if verbose {
            render_verbose_entry(entry);
        } else {
            render_compact_entry(entry);
        }
    }
}

fn run_interactive(result: &RecentEntries, include_all: bool) -> Result<()> {
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

fn show_entry_detail(entry: &HistoryEntry) -> Result<bool> {
    ui::blank_line();
    ui::section(format!(
        "{} {}",
        kind_badge(&entry.kind),
        compact_summary(entry)
    ));
    ui::secondary(format!(
        "{} | {} | {} | {}",
        format_timestamp(&entry.timestamp),
        entry.provider,
        entry.model,
        repo_label(&entry.repo_path)
    ));
    ui::secondary(format!("repo: {}", entry.repo_path));
    ui::secondary(format!("files: {}", file_count_label(entry.files.len())));

    if !entry.files.is_empty() {
        for file in &entry.files {
            ui::secondary(format!("  {file}"));
        }
    }

    ui::blank_line();
    if uses_markdown_rendering(&entry.kind) {
        ui::markdown(&entry.message);
    } else {
        ui::commit_message(&entry.message);
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

fn should_use_interactive(force_interactive: bool, force_non_interactive: bool) -> bool {
    should_use_interactive_for_terminals(
        force_interactive,
        force_non_interactive,
        stdin().is_terminal(),
        stdout().is_terminal(),
    )
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

fn render_compact_entry(entry: &HistoryEntry) {
    ui::headline(compact_summary(entry));
    ui::secondary(format!(
        "{} | {} | {}/{} | {}",
        kind_badge(&entry.kind),
        format_timestamp(&entry.timestamp),
        entry.provider,
        entry.model,
        repo_label(&entry.repo_path)
    ));

    if let Some(preview) = file_preview(&entry.files) {
        ui::secondary(format!(
            "{preview} ({})",
            file_count_label(entry.files.len())
        ));
    }
}

fn render_verbose_entry(entry: &HistoryEntry) {
    ui::headline(compact_summary(entry));
    ui::secondary(format!(
        "{} | {} | {}/{}",
        kind_badge(&entry.kind),
        format_timestamp(&entry.timestamp),
        entry.provider,
        entry.model
    ));
    ui::secondary(format!("repo: {}", entry.repo_path));

    if !entry.files.is_empty() {
        ui::secondary("files:");
        for file in &entry.files {
            ui::secondary(format!("  {file}"));
        }
    }

    ui::blank_line();
    if uses_markdown_rendering(&entry.kind) {
        ui::markdown(&entry.message);
    } else {
        ui::commit_message(&entry.message);
    }
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

fn section_label(kind: Option<&str>, primary_count: usize, hidden_count: usize) -> String {
    let prefix = match kind {
        Some(kind) => format!("Recent {kind} entries"),
        None => "Recent history entries".to_string(),
    };
    format!("{prefix} ({primary_count} main, {hidden_count} hidden)")
}

fn selection_label(entry: &HistoryEntry) -> String {
    truncate(
        &format!(
            "{} | {} | {} | {}",
            kind_badge(&entry.kind),
            format_timestamp(&entry.timestamp),
            repo_label(&entry.repo_path),
            compact_summary(entry)
        ),
        110,
    )
}

fn compact_summary(entry: &HistoryEntry) -> String {
    if entry.kind == "review" {
        review_excerpt(&entry.message)
    } else {
        entry
            .message
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string()
    }
}

fn uses_markdown_rendering(kind: &str) -> bool {
    matches!(kind, "review" | "pr")
}

fn review_excerpt(message: &str) -> String {
    let cleaned = message
        .lines()
        .map(clean_markdown_line)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    truncate(&cleaned, 120)
}

fn clean_markdown_line(line: &str) -> String {
    let mut cleaned = line.trim().to_string();
    while let Some(stripped) = cleaned.strip_prefix(['#', '-', '*', '>', '`', ' ']) {
        cleaned = stripped.to_string();
    }

    collapse_whitespace(
        &cleaned
            .replace('`', "")
            .replace("**", "")
            .replace("__", "")
            .replace('*', ""),
    )
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn repo_label(repo_path: &str) -> String {
    Path::new(repo_path)
        .file_name()
        .and_then(|name| {
            let value = name.to_string_lossy().trim().to_string();
            (!value.is_empty()).then_some(value)
        })
        .unwrap_or_else(|| repo_path.to_string())
}

fn file_preview(files: &[String]) -> Option<String> {
    match files {
        [] => None,
        [only] => Some(only.clone()),
        [first, second] => Some(format!("{first}, {second}")),
        [first, second, rest @ ..] => Some(format!("{first}, {second} +{} more", rest.len())),
    }
}

fn file_count_label(count: usize) -> String {
    match count {
        1 => "1 file".to_string(),
        value => format!("{value} files"),
    }
}

fn kind_badge(kind: &str) -> &'static str {
    match kind {
        "commit" => "commit",
        "review" => "review",
        _ => "entry",
    }
}

fn format_timestamp(raw: &str) -> String {
    let parsed = match DateTime::parse_from_rfc3339(raw) {
        Ok(parsed) => parsed.with_timezone(&Local),
        Err(_) => return raw.to_string(),
    };

    let today = Local::now().date_naive();
    let date = parsed.date_naive();

    if date == today {
        format!("Today {}", parsed.format("%H:%M"))
    } else if Some(date) == today.pred_opt() {
        format!("Yesterday {}", parsed.format("%H:%M"))
    } else {
        parsed.format("%Y-%m-%d %H:%M").to_string()
    }
}

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

#[cfg(test)]
mod tests {
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
    fn selection_label_uses_repo_and_subject() {
        let label = selection_label(&entry(
            "commit",
            "/Users/example/Code/aicommit",
            "feat(history): improve timeline\n\n- details",
        ));

        assert!(label.contains("commit"));
        assert!(label.contains("aicommit"));
        assert!(label.contains("feat(history): improve timeline"));
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
