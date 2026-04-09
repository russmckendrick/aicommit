use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Local};

use crate::{history, history::HistoryEntry, ui};

pub fn run(count: usize, kind: Option<String>, include_all: bool, verbose: bool) -> Result<()> {
    let result = history::recent_entries(count, kind.as_deref(), include_all)?;
    if result.entries.is_empty() {
        if result.hidden_count > 0 {
            ui::info(format!(
                "no history entries found ({} hidden, use --all to show)",
                result.hidden_count
            ));
        } else {
            ui::info("no history entries found");
        }
        return Ok(());
    }

    ui::section(section_label(
        kind.as_deref(),
        result.entries.len(),
        result.hidden_count,
    ));

    for entry in &result.entries {
        ui::blank_line();
        if verbose {
            render_verbose_entry(entry);
        } else {
            render_compact_entry(entry);
        }
    }

    Ok(())
}

fn render_compact_entry(entry: &HistoryEntry) {
    ui::secondary(format!(
        "[{}]  {}  {}/{}  {}  {}",
        entry.kind,
        format_timestamp(&entry.timestamp),
        entry.provider,
        entry.model,
        repo_label(&entry.repo_path),
        file_count_label(entry.files.len())
    ));
    ui::headline(compact_summary(entry));

    if let Some(preview) = file_preview(&entry.files) {
        ui::secondary(format!("files: {preview}"));
    }
}

fn render_verbose_entry(entry: &HistoryEntry) {
    ui::secondary(format!(
        "[{}]  {}  {}/{}  {}",
        entry.kind,
        format_timestamp(&entry.timestamp),
        entry.provider,
        entry.model,
        file_count_label(entry.files.len())
    ));

    if entry.kind == "review" {
        ui::markdown(&entry.message);
    } else {
        ui::commit_message(&entry.message);
    }

    ui::secondary(format!("repo: {}", entry.repo_path));
    if !entry.files.is_empty() {
        ui::secondary("files:");
        for file in &entry.files {
            ui::secondary(format!("  {file}"));
        }
    }
}

fn section_label(kind: Option<&str>, shown_count: usize, hidden_count: usize) -> String {
    let prefix = match kind {
        Some(kind) => format!("Recent {kind} entries"),
        None => "Recent history entries".to_string(),
    };
    format!("{prefix} ({shown_count} shown, {hidden_count} hidden)")
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
