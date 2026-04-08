use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

const HISTORY_FILE: &str = ".aicommit-history.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: String,
    pub kind: String,
    pub message: String,
    pub repo_path: String,
    pub files: Vec<String>,
    pub provider: String,
    pub model: String,
}

pub fn history_path() -> Result<PathBuf> {
    let home = BaseDirs::new()
        .map(|base| base.home_dir().to_path_buf())
        .or_else(|| env::var_os("HOME").map(PathBuf::from))
        .context("could not determine home directory")?;
    Ok(home.join(HISTORY_FILE))
}

pub fn append_entry(entry: &HistoryEntry) -> Result<()> {
    let path = history_path()?;
    let mut entries = load_entries()?;
    entries.push(entry.clone());
    let json = serde_json::to_string_pretty(&entries)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn load_entries() -> Result<Vec<HistoryEntry>> {
    let path = history_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)?;
    let entries: Vec<HistoryEntry> = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse history from {}", path.display()))?;
    Ok(entries)
}

pub fn recent_entries(n: usize, kind: Option<&str>) -> Result<Vec<HistoryEntry>> {
    let entries = load_entries()?;
    let filtered: Vec<_> = match kind {
        Some(k) => entries.into_iter().filter(|e| e.kind == k).collect(),
        None => entries,
    };
    let start = filtered.len().saturating_sub(n);
    Ok(filtered[start..].iter().rev().cloned().collect())
}

pub fn now_iso8601() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
