use std::sync::OnceLock;

use anyhow::{Result, anyhow};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    pub name: String,
    pub variant: String,
    pub background: String,
    pub primary_text: String,
    pub secondary_text: String,
    pub tertiary_text: String,
    pub border: String,
    pub surface: String,
    pub accent: String,
    pub accent_text: String,
    pub heat_stops: Vec<String>,
    pub activity_stops: Vec<String>,
    pub directory_palette: Vec<String>,
}

pub const DEFAULT_THEME: &str = "github-light";

const THEME_SOURCES: &[(&str, &str)] = &[
    (
        "classic-light",
        include_str!("../../themes/classic-light.toml"),
    ),
    (
        "classic-dark",
        include_str!("../../themes/classic-dark.toml"),
    ),
    (
        "solarized-light",
        include_str!("../../themes/solarized-light.toml"),
    ),
    (
        "solarized-dark",
        include_str!("../../themes/solarized-dark.toml"),
    ),
    (
        "github-light",
        include_str!("../../themes/github-light.toml"),
    ),
    ("github-dark", include_str!("../../themes/github-dark.toml")),
    ("monokai", include_str!("../../themes/monokai.toml")),
    ("dracula", include_str!("../../themes/dracula.toml")),
];

fn all_themes() -> &'static Vec<Theme> {
    static THEMES: OnceLock<Vec<Theme>> = OnceLock::new();
    THEMES.get_or_init(|| {
        THEME_SOURCES
            .iter()
            .map(|(name, toml)| {
                toml_edit::de::from_str(toml)
                    .unwrap_or_else(|e| panic!("invalid embedded theme '{name}': {e}"))
            })
            .collect()
    })
}

pub fn load_theme(name: &str) -> Result<&'static Theme> {
    let key = name.to_lowercase().replace(' ', "-");
    all_themes()
        .iter()
        .find(|t| {
            let id = theme_id(t);
            id == key
        })
        .ok_or_else(|| {
            anyhow!(
                "unknown theme '{}'; available themes: {}",
                name,
                available_theme_names().join(", ")
            )
        })
}

pub fn available_theme_names() -> Vec<String> {
    THEME_SOURCES.iter().map(|(k, _)| (*k).to_owned()).collect()
}

fn theme_id(theme: &Theme) -> String {
    theme.name.to_lowercase().replace(' ', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_all_embedded_themes() {
        let themes = all_themes();
        assert_eq!(themes.len(), THEME_SOURCES.len());
    }

    #[test]
    fn loads_default_theme() {
        let theme = load_theme(DEFAULT_THEME).unwrap();
        assert_eq!(theme.name, "GitHub Light");
    }

    #[test]
    fn loads_classic_theme_by_name() {
        let theme = load_theme("classic-light").unwrap();
        assert_eq!(theme.background, "#fafafa");
    }

    #[test]
    fn loads_theme_case_insensitive() {
        let theme = load_theme("Dracula").unwrap();
        assert_eq!(theme.variant, "dark");
    }

    #[test]
    fn unknown_theme_returns_error() {
        let err = load_theme("nonexistent").unwrap_err();
        assert!(err.to_string().contains("unknown theme"));
        assert!(err.to_string().contains("available themes"));
    }
}
