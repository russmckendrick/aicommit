//! Settings screen: view and change the global `~/.aicommit` config from
//! the TUI. Every change is saved immediately through the same
//! `set_global_config` path as `aic config set`, so validation and the
//! "switching provider resets the model" rule are shared.

use std::path::PathBuf;

use anyhow::Result;

use crate::config::{Config, provider_needs_api_key, set_global_config, supported_providers};

use super::editor::TextEditor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Choice,
    Bool,
    Text,
    Secret,
}

#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub kind: FieldKind,
    pub help: &'static str,
}

pub const FIELDS: &[Field] = &[
    Field {
        key: "AIC_AI_PROVIDER",
        label: "Provider",
        group: "AI provider",
        kind: FieldKind::Choice,
        help: "Which service writes your messages. Changing it resets the model to that provider's default.",
    },
    Field {
        key: "AIC_MODEL",
        label: "Model",
        group: "AI provider",
        kind: FieldKind::Choice,
        help: "←→ to pick a suggested model, enter to type any model ID.",
    },
    Field {
        key: "AIC_API_KEY",
        label: "API key",
        group: "AI provider",
        kind: FieldKind::Secret,
        help: "Stored in ~/.aicommit. Press enter to paste or type a new key.",
    },
    Field {
        key: "AIC_API_URL",
        label: "API URL",
        group: "AI provider",
        kind: FieldKind::Text,
        help: "Leave empty for the provider's default endpoint. Required for Azure OpenAI.",
    },
    Field {
        key: "AIC_EMOJI",
        label: "GitMoji prefix",
        group: "Messages",
        kind: FieldKind::Bool,
        help: "Start subjects with a GitMoji such as ✨ or 🐛.",
    },
    Field {
        key: "AIC_DESCRIPTION",
        label: "Body bullets",
        group: "Messages",
        kind: FieldKind::Bool,
        help: "Add 2-4 bullet points after the subject line.",
    },
    Field {
        key: "AIC_ONE_LINE_COMMIT",
        label: "One line only",
        group: "Messages",
        kind: FieldKind::Bool,
        help: "Write a single subject line and nothing else.",
    },
    Field {
        key: "AIC_OMIT_SCOPE",
        label: "Omit scope",
        group: "Messages",
        kind: FieldKind::Bool,
        help: "Write 'feat: …' instead of 'feat(scope): …'.",
    },
    Field {
        key: "AIC_REPO_STYLE",
        label: "Match repo style",
        group: "Messages",
        kind: FieldKind::Bool,
        help: "Follow the scopes, casing, and subject length of recent commits.",
    },
    Field {
        key: "AIC_LANGUAGE",
        label: "Language",
        group: "Messages",
        kind: FieldKind::Text,
        help: "Language for generated messages, such as en, french, or de.",
    },
    Field {
        key: "AIC_GITPUSH",
        label: "Offer to push",
        group: "Workflow",
        kind: FieldKind::Bool,
        help: "Offer to push after committing, and check the upstream first.",
    },
    Field {
        key: "AIC_TUI",
        label: "Open this UI by default",
        group: "Workflow",
        kind: FieldKind::Bool,
        help: "When off, plain `aic` uses the prompt-based flow (same as --fast).",
    },
    Field {
        key: "AIC_THEME",
        label: "Theme",
        group: "Appearance",
        kind: FieldKind::Choice,
        help: "auto follows your terminal's light or dark background.",
    },
    Field {
        key: "AIC_ICONS",
        label: "Icons",
        group: "Appearance",
        kind: FieldKind::Choice,
        help: "auto uses Nerd Font icons when AIC_REMOTE_ICON_STYLE does (the default); unicode works in any font.",
    },
];

pub struct SettingsState {
    pub selected: usize,
    pub editing: Option<TextEditor>,
    /// Opened because the provider needs a key and none is set.
    pub first_run: bool,
    pub path: PathBuf,
}

impl SettingsState {
    pub fn new(path: PathBuf, first_run: bool) -> Self {
        let selected = if first_run {
            FIELDS
                .iter()
                .position(|field| field.key == "AIC_API_KEY")
                .unwrap_or(0)
        } else {
            0
        };
        Self {
            selected,
            editing: None,
            first_run,
            path,
        }
    }

    pub fn field(&self) -> Field {
        FIELDS[self.selected]
    }
}

/// Whether a field applies to the current provider (keys and URLs do not
/// apply to local CLI providers).
pub fn field_applies(field: &Field, config: &Config) -> bool {
    match field.key {
        "AIC_API_KEY" => provider_needs_api_key(&config.ai_provider),
        "AIC_API_URL" => !crate::config::is_local_cli_provider(&config.ai_provider),
        _ => true,
    }
}

pub fn choices(key: &str, config: &Config) -> Vec<String> {
    match key {
        "AIC_AI_PROVIDER" => supported_providers()
            .iter()
            .map(|p| (*p).to_owned())
            .collect(),
        "AIC_MODEL" => {
            let mut models = crate::commands::models::model_choices(&config.ai_provider);
            if !models.contains(&config.model) {
                models.insert(0, config.model.clone());
            }
            models
        }
        "AIC_THEME" => {
            let mut themes = vec!["auto".to_owned()];
            themes.extend(crate::map::theme::available_theme_names());
            themes
        }
        "AIC_ICONS" => vec!["auto".to_owned(), "nerd".to_owned(), "unicode".to_owned()],
        _ => Vec::new(),
    }
}

/// The value shown for a field, with secrets masked.
pub fn display_value(field: &Field, config: &Config) -> String {
    let raw = config.get_key(field.key).unwrap_or_default();
    match field.kind {
        FieldKind::Bool => raw,
        FieldKind::Secret if raw.is_empty() => "not set".to_owned(),
        FieldKind::Secret => mask(&raw),
        _ if raw.is_empty() => "default".to_owned(),
        _ => raw,
    }
}

pub fn mask(secret: &str) -> String {
    let chars = secret.chars().collect::<Vec<_>>();
    if chars.len() <= 8 {
        return "•".repeat(chars.len());
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

/// Environment variables win over the config file, so say when a saved
/// value will not take effect.
pub fn env_override(key: &str) -> bool {
    std::env::var_os(key).is_some_and(|value| !value.is_empty())
}

/// The next or previous choice for a choice field.
pub fn cycle(key: &str, config: &Config, step: isize) -> Option<String> {
    let options = choices(key, config);
    if options.is_empty() {
        return None;
    }
    let current = config.get_key(key).unwrap_or_default();
    let index = options
        .iter()
        .position(|option| *option == current)
        .unwrap_or(0) as isize;
    let next = (index + step).rem_euclid(options.len() as isize) as usize;
    Some(options[next].clone())
}

/// Save one key to the global config and return the reloaded config with
/// the process environment applied, as the rest of `aic` sees it.
pub fn save(path: &std::path::Path, key: &str, value: &str) -> Result<Config> {
    set_global_config(&[(key.to_owned(), value.to_owned())], path)?;
    Config::load_from(&crate::config::ConfigPaths {
        global: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_secrets() {
        assert_eq!(mask("sk-proj-abcdefgh1234"), "sk-p…1234");
        assert_eq!(mask("short"), "•••••");
    }

    #[test]
    fn cycles_choices_and_wraps() {
        let config = Config {
            icons: "unicode".to_owned(),
            ..Config::default()
        };
        assert_eq!(cycle("AIC_ICONS", &config, 1).as_deref(), Some("auto"));
        assert_eq!(cycle("AIC_ICONS", &config, -1).as_deref(), Some("nerd"));
        assert_eq!(cycle("AIC_EMOJI", &config, 1), None);
    }

    #[test]
    fn saving_a_provider_resets_the_model_and_keeps_other_keys() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join(".aicommit");
        save(&path, "AIC_EMOJI", "false").unwrap();
        let config = save(&path, "AIC_AI_PROVIDER", "anthropic").unwrap();
        assert_eq!(config.ai_provider, "anthropic");
        assert_eq!(config.model, "claude-sonnet-5-5");
        assert!(!config.emoji);
        assert!(save(&path, "AIC_THEME", "not-a-theme").is_err());
    }

    #[test]
    fn keys_only_apply_to_remote_providers() {
        let key = FIELDS.iter().find(|f| f.key == "AIC_API_KEY").unwrap();
        let local = Config {
            ai_provider: "claude-code".to_owned(),
            ..Config::default()
        };
        assert!(!field_applies(key, &local));
        assert!(field_applies(key, &Config::default()));
    }
}
