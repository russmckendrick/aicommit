use std::{fs, path::Path};

use anyhow::Result;
use toml_edit::DocumentMut;

use super::{
    Config,
    load::{apply_file, apply_value},
    model::default_model_for_provider,
    validate::validate_config,
};

pub fn set_global_config(key_values: &[(String, String)], global_path: &Path) -> Result<Config> {
    let mut config = Config::default();
    apply_file(&mut config, global_path)?;

    for (key, value) in key_values {
        apply_value(&mut config, key, value)?;
    }

    let provider_was_set = key_values.iter().any(|(key, _)| key == "AIC_AI_PROVIDER");
    let model_was_set = key_values.iter().any(|(key, _)| key == "AIC_MODEL");
    if provider_was_set && !model_was_set {
        config.model = default_model_for_provider(&config.ai_provider).to_owned();
    }

    validate_config(&config)?;
    write_global_config(&config, global_path)?;
    Ok(config)
}

const FILE_HEADER: &str = "# aic configuration. Settings left out use aic's built-in defaults.\n";

/// Values the config file can leave out: `Config::default()`, with the
/// model default following the configured provider.
fn default_values(config: &Config) -> std::collections::BTreeMap<&'static str, String> {
    let defaults = Config {
        ai_provider: config.ai_provider.clone(),
        model: default_model_for_provider(&config.ai_provider).to_owned(),
        ..Config::default()
    };
    let mut values = defaults
        .as_key_values()
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    // The provider itself is only a default when it is the built-in one.
    values.insert("AIC_AI_PROVIDER", Config::default().ai_provider);
    values
}

/// Write the config, keeping only values that differ from the defaults, so
/// a later release can change a default and existing users pick it up.
pub fn write_global_config(config: &Config, global_path: &Path) -> Result<()> {
    if let Some(parent) = global_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let defaults = default_values(config);
    let mut doc = DocumentMut::new();
    for (key, value) in config.as_key_values() {
        if value.is_empty() || defaults.get(key) == Some(&value) {
            continue;
        }

        doc[key] = match key {
            "AIC_TOKENS_MAX_INPUT" | "AIC_TOKENS_MAX_OUTPUT" | "AIC_HTTP_TIMEOUT" => {
                value.parse::<i64>()?.into()
            }
            "AIC_DESCRIPTION"
            | "AIC_EMOJI"
            | "AIC_ONE_LINE_COMMIT"
            | "AIC_OMIT_SCOPE"
            | "AIC_GITPUSH"
            | "AIC_HOOK_AUTO_UNCOMMENT"
            | "AIC_TUI"
            | "AIC_REPO_STYLE" => value.parse::<bool>()?.into(),
            _ => value.into(),
        };
    }

    fs::write(global_path, format!("{FILE_HEADER}{doc}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn set_global_config_writes_only_non_default_values() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");

        let config = set_global_config(
            &[
                ("AIC_API_KEY".to_owned(), "key".to_owned()),
                ("AIC_EMOJI".to_owned(), "true".to_owned()),
            ],
            &global,
        )
        .unwrap();

        assert_eq!(config.api_key.as_deref(), Some("key"));
        assert!(config.emoji);
        let content = std::fs::read_to_string(&global).unwrap();
        assert!(content.contains("AIC_API_KEY"));
        // Defaults are left out, so a future default change applies.
        assert!(!content.contains("AIC_EMOJI"));
        assert!(!content.contains("AIC_MODEL"));
        assert!(!content.contains("AIC_TUI"));
        assert!(content.starts_with("# aic configuration."));

        let config =
            set_global_config(&[("AIC_EMOJI".to_owned(), "false".to_owned())], &global).unwrap();
        assert!(!config.emoji);
        let content = std::fs::read_to_string(&global).unwrap();
        assert!(content.contains("AIC_EMOJI = false"));
        assert!(content.contains("AIC_API_KEY"));
    }

    #[test]
    fn rewriting_drops_values_that_match_defaults() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");
        std::fs::write(
            &global,
            "AIC_AI_PROVIDER = \"openai\"\nAIC_MODEL = \"gpt-5.4-mini\"\nAIC_TUI = false\nAIC_THEME = \"auto\"\nAIC_API_KEY = \"k\"\n",
        )
        .unwrap();

        set_global_config(&[("AIC_TUI".to_owned(), "true".to_owned())], &global).unwrap();
        let content = std::fs::read_to_string(&global).unwrap();
        assert!(!content.contains("AIC_TUI"));
        assert!(!content.contains("AIC_THEME"));
        assert!(!content.contains("AIC_AI_PROVIDER"));
        // An explicit, non-default model is kept.
        assert!(content.contains("AIC_MODEL = \"gpt-5.4-mini\""));
        assert!(content.contains("AIC_API_KEY = \"k\""));
    }

    #[test]
    fn non_default_provider_is_kept_and_its_default_model_is_omitted() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");
        set_global_config(
            &[("AIC_AI_PROVIDER".to_owned(), "anthropic".to_owned())],
            &global,
        )
        .unwrap();
        let content = std::fs::read_to_string(&global).unwrap();
        assert!(content.contains("AIC_AI_PROVIDER = \"anthropic\""));
        assert!(!content.contains("AIC_MODEL"));
        let config = Config::load_from(&crate::config::ConfigPaths { global }).unwrap();
        assert_eq!(config.model, "claude-sonnet-5-5");
    }

    #[test]
    fn setting_remote_provider_without_model_uses_provider_default_model() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");

        let config = set_global_config(
            &[("AIC_AI_PROVIDER".to_owned(), "groq".to_owned())],
            &global,
        )
        .unwrap();

        assert_eq!(config.ai_provider, "groq");
        assert_eq!(config.model, "openai/gpt-oss-20b");
    }
}
