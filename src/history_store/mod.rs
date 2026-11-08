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
    // Unit tests must never touch the real history file.
    if cfg!(test) {
        return Ok(
            env::temp_dir().join(format!("aicommit-test-history-{}.json", std::process::id()))
        );
    }
    let home = BaseDirs::new()
        .map(|base| base.home_dir().to_path_buf())
        .or_else(|| env::var_os("HOME").map(PathBuf::from))
        .context("could not determine home directory")?;
    Ok(home.join(HISTORY_FILE))
}

pub fn append_entry(entry: &HistoryEntry) -> Result<()> {
    let path = history_path()?;
    let mut entries = match load_entries() {
        Ok(entries) => entries,
        Err(_) => {
            // Keep the unreadable file for inspection and start a new one,
            // rather than refusing to record anything ever again.
            let aside = path.with_extension(format!(
                "json.corrupt-{}",
                Utc::now().format("%Y%m%dT%H%M%S")
            ));
            fs::rename(&path, &aside).with_context(|| {
                format!(
                    "history at {} is unreadable and could not be moved aside",
                    path.display()
                )
            })?;
            Vec::new()
        }
    };
    entries.push(entry.clone());
    write_atomically(&path, &serde_json::to_string_pretty(&entries)?)
}

/// Write to a sibling temp file and rename it into place, so a reader or a
/// second `aic` process never sees a half-written file.
fn write_atomically(path: &Path, content: &str) -> Result<()> {
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    Ok(())
}

pub fn load_entries() -> Result<Vec<HistoryEntry>> {
    let path = history_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)?;
    parse_entries(&content)
        .with_context(|| format!("failed to parse history from {}", path.display()))
}

/// Parse the history list, tolerating trailing garbage after a valid list
/// (left behind by older versions that wrote the file non-atomically).
fn parse_entries(content: &str) -> Result<Vec<HistoryEntry>> {
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut stream = serde_json::Deserializer::from_str(content).into_iter::<Vec<HistoryEntry>>();
    match stream.next() {
        Some(Ok(entries)) => Ok(entries),
        Some(Err(error)) => Err(error.into()),
        None => Ok(Vec::new()),
    }
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
    fn parses_history_with_trailing_garbage_and_empty_files() {
        let valid = r#"[{"timestamp":"t","kind":"commit","message":"m","repo_path":"/r","files":[],"provider":"p","model":"x"}]"#;
        assert_eq!(parse_entries(valid).unwrap().len(), 1);
        assert_eq!(parse_entries(&format!("{valid}}}\n]")).unwrap().len(), 1);
        assert!(parse_entries("").unwrap().is_empty());
        assert!(parse_entries("{not json").is_err());
    }

    #[test]
    fn append_recovers_from_an_unreadable_file() {
        let path = history_path().unwrap();
        fs::write(&path, "{not json").unwrap();
        append_entry(&entry("/repo", "openai")).unwrap();
        assert_eq!(load_entries().unwrap().len(), 1);
        let dir = path.parent().unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let moved = fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{name}.corrupt-"))
                    || e.file_name()
                        .to_string_lossy()
                        .starts_with(&name.replace(".json", ".json.corrupt-"))
            })
            .collect::<Vec<_>>();
        assert!(!moved.is_empty());
        for file in moved {
            let _ = fs::remove_file(file.path());
        }
        let _ = fs::remove_file(path);
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
