use std::path::Path;

use chrono::{DateTime, Local};

use crate::history_store::HistoryEntry;

pub(crate) fn section_label(
    kind: Option<&str>,
    primary_count: usize,
    hidden_count: usize,
) -> String {
    let prefix = match kind {
        Some(kind) => format!("Recent {kind} entries"),
        None => "Recent history entries".to_string(),
    };
    format!("{prefix} ({primary_count} main, {hidden_count} hidden)")
}

pub(crate) fn selection_label(entry: &HistoryEntry) -> String {
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

pub(crate) fn compact_summary(entry: &HistoryEntry) -> String {
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

pub(crate) fn uses_markdown_rendering(kind: &str) -> bool {
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

pub(crate) fn repo_label(repo_path: &str) -> String {
    Path::new(repo_path)
        .file_name()
        .and_then(|name| {
            let value = name.to_string_lossy().trim().to_string();
            (!value.is_empty()).then_some(value)
        })
        .unwrap_or_else(|| repo_path.to_string())
}

pub(crate) fn file_preview(files: &[String]) -> Option<String> {
    match files {
        [] => None,
        [only] => Some(only.clone()),
        [first, second] => Some(format!("{first}, {second}")),
        [first, second, rest @ ..] => Some(format!("{first}, {second} +{} more", rest.len())),
    }
}

pub(crate) fn file_count_label(count: usize) -> String {
    match count {
        1 => "1 file".to_string(),
        value => format!("{value} files"),
    }
}

pub(crate) fn kind_badge(kind: &str) -> &'static str {
    match kind {
        "commit" => "commit",
        "review" => "review",
        _ => "entry",
    }
}

pub(crate) fn format_timestamp(raw: &str) -> String {
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
