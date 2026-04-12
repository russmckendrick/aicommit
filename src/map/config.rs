use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

use super::theme::DEFAULT_THEME;
use crate::git::repo_root;

const MAP_CONFIG_FILE: &str = ".aicommit-map";

/// Configurable defaults for `aic map` subcommands.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MapConfig {
    pub theme: String,
    pub history_commits: usize,
    pub heat_commits: usize,
    pub activity_commits: usize,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME.to_owned(),
            history_commits: 20,
            heat_commits: 50,
            activity_commits: 500,
        }
    }
}

impl MapConfig {
    /// Load map config from `.aicommit-map` in the repo root.
    /// Falls back to defaults if the file does not exist.
    pub fn load() -> Result<Self> {
        let root = repo_root()?;
        let config_path = root.join(MAP_CONFIG_FILE);
        Self::load_from(&config_path)
    }

    fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        let config: MapConfig = toml_edit::de::from_str(&content)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_github_light() {
        let config = MapConfig::default();
        assert_eq!(config.theme, "github-light");
        assert_eq!(config.history_commits, 20);
        assert_eq!(config.heat_commits, 50);
        assert_eq!(config.activity_commits, 500);
    }

    #[test]
    fn load_from_missing_file_returns_defaults() {
        let config = MapConfig::load_from(Path::new("/nonexistent/.aicommit-map")).unwrap();
        assert_eq!(config.theme, "github-light");
    }

    #[test]
    fn load_from_partial_file_fills_defaults() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "theme = \"dracula\"\n").unwrap();
        let config = MapConfig::load_from(temp.path()).unwrap();
        assert_eq!(config.theme, "dracula");
        assert_eq!(config.history_commits, 20);
    }
}
