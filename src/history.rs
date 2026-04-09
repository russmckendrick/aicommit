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
    pub primary_entries: Vec<HistoryEntry>,
    pub hidden_entries: Vec<HistoryEntry>,
    pub primary_count: usize,
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

pub fn recent_entries(n: usize, kind: Option<&str>) -> Result<RecentEntries> {
    let entries = load_entries()?;
    let filtered: Vec<_> = match kind {
        Some(k) => entries.into_iter().filter(|e| e.kind == k).collect(),
        None => entries,
    };

    let (primary, hidden): (Vec<_>, Vec<_>) = filtered
        .into_iter()
        .partition(|entry| !is_hidden_entry(entry));

    Ok(RecentEntries {
        primary_count: primary.len(),
        hidden_count: hidden.len(),
        primary_entries: recent_slice(primary, n),
        hidden_entries: recent_slice(hidden, n),
    })
}

pub fn now_iso8601() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn recent_slice(entries: Vec<HistoryEntry>, n: usize) -> Vec<HistoryEntry> {
    let start = entries.len().saturating_sub(n);
    entries[start..].iter().rev().cloned().collect()
}

fn is_hidden_entry(entry: &HistoryEntry) -> bool {
    entry.provider == "test"
        || path_is_in_temp_dir(&entry.repo_path)
        || repo_basename(&entry.repo_path)
            .map(|name| name.starts_with(".tmp"))
            .unwrap_or(false)
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

fn repo_basename(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().trim().to_string())
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(repo_path: &str, provider: &str) -> HistoryEntry {
        HistoryEntry {
            timestamp: "2026-04-09T18:57:00Z".to_string(),
            kind: "commit".to_string(),
            message: "feat: test".to_string(),
            repo_path: repo_path.to_string(),
            files: vec!["src.txt".to_string()],
            provider: provider.to_string(),
            model: "default".to_string(),
        }
    }

    #[test]
    fn hides_temp_dir_paths() {
        let path = env::temp_dir().join(".tmpaic-history");
        assert!(is_hidden_entry(&entry(
            path.to_string_lossy().as_ref(),
            "openai"
        )));
    }

    #[test]
    fn hides_tmp_basename_even_outside_temp_dir() {
        assert!(is_hidden_entry(&entry(
            "/Users/example/.tmpabc123",
            "openai"
        )));
    }

    #[test]
    fn hides_test_provider_even_without_temp_path() {
        assert!(is_hidden_entry(&entry(
            "/Users/example/Code/aicommit",
            "test"
        )));
    }
}
