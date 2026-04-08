use anyhow::Result;

use crate::{history, ui};

pub fn run(count: usize, kind: Option<String>) -> Result<()> {
    let entries = history::recent_entries(count, kind.as_deref())?;
    if entries.is_empty() {
        ui::info("no history entries found");
        return Ok(());
    }

    let label = match kind.as_deref() {
        Some(k) => format!("Recent {k} entries ({})", entries.len()),
        None => format!("Recent history entries ({})", entries.len()),
    };
    ui::section(label);

    for entry in &entries {
        ui::blank_line();
        ui::secondary(format!(
            "  [{}]  {}  {}/{}  {} file(s)",
            entry.kind,
            entry.timestamp,
            entry.provider,
            entry.model,
            entry.files.len()
        ));
        if entry.kind == "review" {
            ui::markdown(&entry.message);
        } else {
            ui::commit_message(&entry.message);
        }
        ui::secondary(format!("  {}", entry.repo_path));
    }

    Ok(())
}
