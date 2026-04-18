use anyhow::Result;

use crate::{
    history_store::{HistoryEntry, RecentEntries},
    ui,
};

use super::format::{
    compact_summary, file_count_label, file_preview, format_timestamp, kind_badge, repo_label,
    section_label, uses_markdown_rendering,
};

pub(crate) fn render_history(
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

pub(crate) fn render_entry_list(title: &str, entries: &[HistoryEntry], verbose: bool) {
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

pub(crate) fn render_compact_entry(entry: &HistoryEntry) {
    ui::headline(compact_summary(entry));
    ui::metadata_row(&[
        kind_badge(&entry.kind).to_owned(),
        format_timestamp(&entry.timestamp),
        format!("{}/{}", entry.provider, entry.model),
        repo_label(&entry.repo_path),
    ]);

    if let Some(preview) = file_preview(&entry.files) {
        ui::secondary(format!(
            "{preview} ({})",
            file_count_label(entry.files.len())
        ));
    }
}

pub(crate) fn render_verbose_entry(entry: &HistoryEntry) {
    ui::headline(compact_summary(entry));
    ui::metadata_row(&[
        kind_badge(&entry.kind).to_owned(),
        format_timestamp(&entry.timestamp),
        format!("{}/{}", entry.provider, entry.model),
    ]);
    ui::secondary(format!("repo: {}", entry.repo_path));

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
    ui::primary_card(title, &entry.message);
}
