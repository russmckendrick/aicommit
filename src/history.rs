use std::{
    env, fs,
    path::{Path, PathBuf},
};

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

#[derive(Debug, Clone)]
pub struct RecentEntries {
    pub entries: Vec<HistoryEntry>,
    pub hidden_count: usize,
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

pub fn recent_entries(n: usize, kind: Option<&str>, include_all: bool) -> Result<RecentEntries> {
    let entries = load_entries()?;
    let filtered: Vec<_> = match kind {
        Some(k) => entries.into_iter().filter(|e| e.kind == k).collect(),
        None => entries,
    };
    let hidden_count = filtered.iter().filter(|entry| is_temp_entry(entry)).count();
    let visible: Vec<_> = if include_all {
        filtered
    } else {
        filtered
            .into_iter()
            .filter(|entry| !is_temp_entry(entry))
            .collect()
    };
    let start = visible.len().saturating_sub(n);
    Ok(RecentEntries {
        entries: visible[start..].iter().rev().cloned().collect(),
        hidden_count: if include_all { 0 } else { hidden_count },
    })
}

pub fn now_iso8601() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn is_temp_entry(entry: &HistoryEntry) -> bool {
    path_is_in_temp_dir(&entry.repo_path)
}

fn path_is_in_temp_dir(path: &str) -> bool {
    let candidate = Path::new(path);
    candidate.starts_with(env::temp_dir())
        || normalize_private_prefix(candidate)
            .starts_with(normalize_private_prefix(&env::temp_dir()))
}

fn normalize_private_prefix(path: &Path) -> PathBuf {
    let stripped = path.strip_prefix("/private").unwrap_or(path).to_path_buf();

    #[cfg(windows)]
    {
        PathBuf::from(stripped.to_string_lossy().to_lowercase())
    }

    #[cfg(not(windows))]
    {
        stripped
    }
}
