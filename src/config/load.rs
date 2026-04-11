use std::env;

use anyhow::Result;

use super::{
    CONFIG_KEYS, Config, ConfigPaths,
    model::{default_model_for_provider, is_local_cli_provider},
    parse::normalize_provider,
    validate::validate_config,
};

pub fn load_from_with_provider_override(
    paths: &ConfigPaths,
    provider_override: Option<&str>,
) -> Result<Config> {
    let mut config = Config::default();
    apply_file(&mut config, &paths.global)?;
    apply_process_env(&mut config)?;

    if let Some(provider) = provider_override {
        let previous_provider = config.ai_provider.clone();
        let previous_model = config.model.clone();
        apply_value(&mut config, "AIC_AI_PROVIDER", provider)?;
        if is_local_cli_provider(&config.ai_provider)
            || previous_model == default_model_for_provider(&previous_provider)
        {
            config.model = default_model_for_provider(&config.ai_provider).to_owned();
        }
    }

    if config.proxy.is_none() {
        config.proxy = env::var("HTTPS_PROXY")
            .ok()
            .or_else(|| env::var("HTTP_PROXY").ok());
    }

    validate_config(&config)?;
    Ok(config)
}

pub fn apply_file(config: &mut Config, path: &std::path::Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(path)
        .map_err(anyhow::Error::from)
        .and_then(|content| {
            content
                .parse::<toml_edit::DocumentMut>()
                .map_err(anyhow::Error::from)
        })?;
    let doc = content;

    for key in CONFIG_KEYS {
        if let Some(item) = doc.get(key)
            && !item.is_none()
        {
            apply_toml_item(config, key, item)?;
        }
    }
    Ok(())
}

pub fn apply_process_env(config: &mut Config) -> Result<()> {
    for key in CONFIG_KEYS {
        if let Ok(value) = env::var(key) {
            apply_value(config, key, &value)?;
        }
    }
    Ok(())
}

pub fn apply_toml_item(config: &mut Config, key: &str, item: &toml_edit::Item) -> Result<()> {
    let value = item
        .as_value()
        .map(super::parse::toml_value_to_string)
        .unwrap_or_else(|| item.to_string());
    apply_value(config, key, &value)
}

pub fn apply_value(config: &mut Config, key: &str, value: &str) -> Result<()> {
    if !CONFIG_KEYS.contains(&key) {
        return Err(crate::errors::AicError::UnsupportedConfigKey(key.to_owned()).into());
    }

    match key {
        "AIC_AI_PROVIDER" => config.ai_provider = normalize_provider(value),
        "AIC_API_KEY" => config.api_key = optional_string(value),
        "AIC_API_URL" => config.api_url = optional_string(value),
        "AIC_API_CUSTOM_HEADERS" => {
            config.api_custom_headers = super::parse::parse_headers(value)?;
        }
        "AIC_PROXY" => config.proxy = optional_string(value),
        "AIC_TOKENS_MAX_INPUT" => config.tokens_max_input = super::parse::parse_usize(key, value)?,
        "AIC_TOKENS_MAX_OUTPUT" => {
            config.tokens_max_output = super::parse::parse_usize(key, value)?
        }
        "AIC_DESCRIPTION" => config.description = super::parse::parse_bool(key, value)?,
        "AIC_EMOJI" => config.emoji = super::parse::parse_bool(key, value)?,
        "AIC_MODEL" => config.model = value.to_owned(),
        "AIC_LANGUAGE" => config.language = value.to_owned(),
        "AIC_MESSAGE_TEMPLATE_PLACEHOLDER" => {
            config.message_template_placeholder = value.to_owned()
        }
        "AIC_PROMPT_FILE" => config.prompt_file = optional_string(value),
        "AIC_ONE_LINE_COMMIT" => config.one_line_commit = super::parse::parse_bool(key, value)?,
        "AIC_OMIT_SCOPE" => config.omit_scope = super::parse::parse_bool(key, value)?,
        "AIC_GITPUSH" => config.gitpush = super::parse::parse_bool(key, value)?,
        "AIC_REMOTE_ICON_STYLE" => {
            config.remote_icon_style = super::parse::normalize_remote_icon_style(value)?
        }
        "AIC_HOOK_AUTO_UNCOMMENT" => {
            config.hook_auto_uncomment = super::parse::parse_bool(key, value)?
        }
        _ => unreachable!("all config keys are handled"),
    }
    Ok(())
}

pub fn optional_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "null" || trimmed == "undefined" {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::config::{
        Config, ConfigPaths, model::provider_needs_api_key, write::set_global_config,
    };

    #[test]
    fn load_from_ignores_neighbor_dotenv_file() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");
        let dotenv_path = temp.path().join(".env");
        std::fs::write(
            &global,
            "AIC_API_KEY = \"global\"\nAIC_MODEL = \"gpt-5.4-mini\"\n",
        )
        .unwrap();
        std::fs::write(
            &dotenv_path,
            "AIC_API_KEY=local\nsid=1:abc; _cfuvid=value; uid=123\nAIC_MODEL=gpt-5.4\n",
        )
        .unwrap();

        let config = Config::load_from(&ConfigPaths { global }).unwrap();

        assert_eq!(config.api_key.as_deref(), Some("global"));
        assert_eq!(config.model, "gpt-5.4-mini");
    }

    #[test]
    fn normalizes_claudecode_provider_alias() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");
        std::fs::write(&global, "AIC_AI_PROVIDER = \"claudecode\"\n").unwrap();

        let config = Config::load_from(&ConfigPaths { global }).unwrap();

        assert_eq!(config.ai_provider, "claude-code");
    }

    #[test]
    fn accepts_local_cli_providers() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");
        std::fs::write(
            &global,
            "AIC_AI_PROVIDER = \"codex\"\nAIC_MODEL = \"default\"\n",
        )
        .unwrap();

        let config = Config::load_from(&ConfigPaths { global }).unwrap();

        assert_eq!(config.ai_provider, "codex");
    }

    #[test]
    fn accepts_new_remote_providers() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");
        std::fs::write(
            &global,
            "AIC_AI_PROVIDER = \"anthropic\"\nAIC_MODEL = \"claude-sonnet-4-20250514\"\n",
        )
        .unwrap();

        let config = Config::load_from(&ConfigPaths { global }).unwrap();

        assert_eq!(config.ai_provider, "anthropic");
    }

    #[test]
    fn local_cli_providers_do_not_need_api_keys() {
        let config = Config {
            ai_provider: "claude-code".to_owned(),
            ..Config::default()
        };

        assert!(!config.provider_needs_api_key());
        assert!(!provider_needs_api_key("codex"));
    }

    #[test]
    fn provider_override_switches_ollama_to_provider_default() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");
        std::fs::write(
            &global,
            "AIC_AI_PROVIDER = \"openai\"\nAIC_MODEL = \"gpt-5.4-mini\"\n",
        )
        .unwrap();

        let config =
            Config::load_from_with_provider_override(&ConfigPaths { global }, Some("ollama"))
                .unwrap();

        assert_eq!(config.ai_provider, "ollama");
        assert_eq!(config.model, "llama3.2");
    }

    #[test]
    fn provider_override_switches_local_cli_model_to_default() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");
        std::fs::write(
            &global,
            "AIC_AI_PROVIDER = \"openai\"\nAIC_MODEL = \"gpt-5.4\"\n",
        )
        .unwrap();

        let config =
            Config::load_from_with_provider_override(&ConfigPaths { global }, Some("codex"))
                .unwrap();

        assert_eq!(config.ai_provider, "codex");
        assert_eq!(config.model, "default");
    }

    #[test]
    fn provider_override_switches_remote_default_model_to_provider_default() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");
        std::fs::write(
            &global,
            "AIC_AI_PROVIDER = \"openai\"\nAIC_MODEL = \"gpt-5.4-mini\"\n",
        )
        .unwrap();

        let config =
            Config::load_from_with_provider_override(&ConfigPaths { global }, Some("anthropic"))
                .unwrap();

        assert_eq!(config.ai_provider, "anthropic");
        assert_eq!(config.model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn provider_override_preserves_explicit_remote_model() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");
        std::fs::write(
            &global,
            "AIC_AI_PROVIDER = \"openai\"\nAIC_MODEL = \"gpt-5.4\"\n",
        )
        .unwrap();

        let config =
            Config::load_from_with_provider_override(&ConfigPaths { global }, Some("groq"))
                .unwrap();

        assert_eq!(config.ai_provider, "groq");
        assert_eq!(config.model, "gpt-5.4");
    }

    #[test]
    fn setting_local_provider_without_model_uses_default_model() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");

        let config = set_global_config(
            &[("AIC_AI_PROVIDER".to_owned(), "claudecode".to_owned())],
            &global,
        )
        .unwrap();

        assert_eq!(config.ai_provider, "claude-code");
        assert_eq!(config.model, "default");
    }
}
