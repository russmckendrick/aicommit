use std::{
    collections::{BTreeMap, HashMap},
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Value};

use crate::errors::AicError;

pub const GLOBAL_CONFIG_FILE: &str = ".aicommit";
pub const MODEL_CACHE_FILE: &str = ".aicommit-models.json";
pub const REPO_IGNORE_FILE: &str = ".aicommitignore";
pub const DEFAULT_MAX_TOKENS_INPUT: usize = 128_000;
pub const DEFAULT_MAX_TOKENS_OUTPUT: usize = 500;

pub const CONFIG_KEYS: &[&str] = &[
    "AIC_AI_PROVIDER",
    "AIC_API_KEY",
    "AIC_API_URL",
    "AIC_API_CUSTOM_HEADERS",
    "AIC_PROXY",
    "AIC_TOKENS_MAX_INPUT",
    "AIC_TOKENS_MAX_OUTPUT",
    "AIC_DESCRIPTION",
    "AIC_EMOJI",
    "AIC_MODEL",
    "AIC_LANGUAGE",
    "AIC_MESSAGE_TEMPLATE_PLACEHOLDER",
    "AIC_PROMPT_FILE",
    "AIC_ONE_LINE_COMMIT",
    "AIC_OMIT_SCOPE",
    "AIC_GITPUSH",
    "AIC_REMOTE_ICON_STYLE",
    "AIC_HOOK_AUTO_UNCOMMENT",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub ai_provider: String,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
    pub api_custom_headers: BTreeMap<String, String>,
    pub proxy: Option<String>,
    pub tokens_max_input: usize,
    pub tokens_max_output: usize,
    pub description: bool,
    pub emoji: bool,
    pub model: String,
    pub language: String,
    pub message_template_placeholder: String,
    pub prompt_file: Option<String>,
    pub one_line_commit: bool,
    pub omit_scope: bool,
    pub gitpush: bool,
    pub remote_icon_style: String,
    pub hook_auto_uncomment: bool,
}

#[derive(Debug, Clone)]
pub struct ConfigPaths {
    pub global: PathBuf,
    pub env_file: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ai_provider: "openai".to_owned(),
            api_key: None,
            api_url: None,
            api_custom_headers: BTreeMap::new(),
            proxy: None,
            tokens_max_input: DEFAULT_MAX_TOKENS_INPUT,
            tokens_max_output: DEFAULT_MAX_TOKENS_OUTPUT,
            description: true,
            emoji: true,
            model: default_model_for_provider("openai").to_owned(),
            language: "en".to_owned(),
            message_template_placeholder: "$msg".to_owned(),
            prompt_file: None,
            one_line_commit: false,
            omit_scope: false,
            gitpush: true,
            remote_icon_style: "auto".to_owned(),
            hook_auto_uncomment: false,
        }
    }
}

impl ConfigPaths {
    pub fn discover() -> Result<Self> {
        let home = BaseDirs::new()
            .map(|base| base.home_dir().to_path_buf())
            .or_else(|| env::var_os("HOME").map(PathBuf::from))
            .context("could not determine home directory")?;

        Ok(Self {
            global: home.join(GLOBAL_CONFIG_FILE),
            env_file: env::current_dir()?.join(".env"),
        })
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        Self::load_from(&ConfigPaths::discover()?)
    }

    pub fn load_from(paths: &ConfigPaths) -> Result<Self> {
        let mut config = Self::default();
        apply_file(&mut config, &paths.global)?;
        apply_env_file(&mut config, &paths.env_file)?;
        apply_process_env(&mut config)?;

        if config.proxy.is_none() {
            config.proxy = env::var("HTTPS_PROXY")
                .ok()
                .or_else(|| env::var("HTTP_PROXY").ok());
        }

        validate_config(&config)?;
        Ok(config)
    }

    pub fn as_key_values(&self) -> Vec<(&'static str, String)> {
        vec![
            ("AIC_AI_PROVIDER", self.ai_provider.clone()),
            ("AIC_API_KEY", self.api_key.clone().unwrap_or_default()),
            ("AIC_API_URL", self.api_url.clone().unwrap_or_default()),
            (
                "AIC_API_CUSTOM_HEADERS",
                serde_json::to_string(&self.api_custom_headers).unwrap_or_default(),
            ),
            ("AIC_PROXY", self.proxy.clone().unwrap_or_default()),
            ("AIC_TOKENS_MAX_INPUT", self.tokens_max_input.to_string()),
            ("AIC_TOKENS_MAX_OUTPUT", self.tokens_max_output.to_string()),
            ("AIC_DESCRIPTION", self.description.to_string()),
            ("AIC_EMOJI", self.emoji.to_string()),
            ("AIC_MODEL", self.model.clone()),
            ("AIC_LANGUAGE", self.language.clone()),
            (
                "AIC_MESSAGE_TEMPLATE_PLACEHOLDER",
                self.message_template_placeholder.clone(),
            ),
            (
                "AIC_PROMPT_FILE",
                self.prompt_file.clone().unwrap_or_default(),
            ),
            ("AIC_ONE_LINE_COMMIT", self.one_line_commit.to_string()),
            ("AIC_OMIT_SCOPE", self.omit_scope.to_string()),
            ("AIC_GITPUSH", self.gitpush.to_string()),
            ("AIC_REMOTE_ICON_STYLE", self.remote_icon_style.clone()),
            (
                "AIC_HOOK_AUTO_UNCOMMENT",
                self.hook_auto_uncomment.to_string(),
            ),
        ]
    }

    pub fn get_key(&self, key: &str) -> Option<String> {
        self.as_key_values()
            .into_iter()
            .find_map(|(candidate, value)| (candidate == key).then_some(value))
    }

    pub fn provider_needs_api_key(&self) -> bool {
        !matches!(self.ai_provider.as_str(), "test")
    }
}

pub fn global_model_cache_path() -> Result<PathBuf> {
    let home = BaseDirs::new()
        .map(|base| base.home_dir().to_path_buf())
        .or_else(|| env::var_os("HOME").map(PathBuf::from))
        .context("could not determine home directory")?;
    Ok(home.join(MODEL_CACHE_FILE))
}

pub fn default_model_for_provider(provider: &str) -> &'static str {
    match provider {
        "azure-openai" => "gpt-5.4-mini",
        _ => "gpt-5.4-mini",
    }
}

pub fn supported_providers() -> &'static [&'static str] {
    &["openai", "azure-openai"]
}

pub fn enabled_providers() -> &'static [&'static str] {
    supported_providers()
}

pub fn model_list(provider: &str) -> &'static [&'static str] {
    match provider {
        "azure-openai" => &["gpt-5.4-mini", "gpt-5.4", "gpt-5.4-nano"],
        _ => &["gpt-5.4-mini", "gpt-5.4", "gpt-5.4-nano"],
    }
}

pub fn config_descriptions() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        (
            "AIC_AI_PROVIDER",
            "AI provider to use for commit generation",
        ),
        ("AIC_API_KEY", "API key for the selected provider"),
        (
            "AIC_API_URL",
            "Custom provider API URL; required for Azure OpenAI",
        ),
        (
            "AIC_API_CUSTOM_HEADERS",
            "JSON object of custom HTTP headers",
        ),
        ("AIC_PROXY", "HTTP or HTTPS proxy URL"),
        ("AIC_TOKENS_MAX_INPUT", "Maximum input token budget"),
        ("AIC_TOKENS_MAX_OUTPUT", "Maximum generated output tokens"),
        ("AIC_DESCRIPTION", "Include a short body after the subject"),
        ("AIC_EMOJI", "Prefix generated messages with GitMoji"),
        ("AIC_MODEL", "AI model to use"),
        ("AIC_LANGUAGE", "Language for generated commit messages"),
        (
            "AIC_MESSAGE_TEMPLATE_PLACEHOLDER",
            "Placeholder used in message templates",
        ),
        ("AIC_PROMPT_FILE", "Path to a custom system prompt template"),
        ("AIC_ONE_LINE_COMMIT", "Generate a single-line message"),
        ("AIC_OMIT_SCOPE", "Avoid conventional commit scopes"),
        ("AIC_GITPUSH", "Ask whether to push after committing"),
        (
            "AIC_REMOTE_ICON_STYLE",
            "Remote host icon style for push prompts: auto, nerd-font, emoji, or label",
        ),
        (
            "AIC_HOOK_AUTO_UNCOMMENT",
            "Write hook-generated messages uncommented",
        ),
    ])
}

pub fn set_global_config(key_values: &[(String, String)], global_path: &Path) -> Result<Config> {
    let paths = ConfigPaths {
        global: global_path.to_path_buf(),
        env_file: PathBuf::from("__aicommit_no_env__"),
    };
    let mut config = Config::default();
    apply_file(&mut config, &paths.global)?;

    for (key, value) in key_values {
        apply_value(&mut config, key, value)?;
    }

    validate_config(&config)?;
    write_global_config(&config, global_path)?;
    Ok(config)
}

pub fn write_global_config(config: &Config, global_path: &Path) -> Result<()> {
    if let Some(parent) = global_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut doc = DocumentMut::new();
    for (key, value) in config.as_key_values() {
        if value.is_empty() {
            continue;
        }

        doc[key] = match key {
            "AIC_TOKENS_MAX_INPUT" | "AIC_TOKENS_MAX_OUTPUT" => value.parse::<i64>()?.into(),
            "AIC_DESCRIPTION"
            | "AIC_EMOJI"
            | "AIC_ONE_LINE_COMMIT"
            | "AIC_OMIT_SCOPE"
            | "AIC_GITPUSH"
            | "AIC_HOOK_AUTO_UNCOMMENT" => value.parse::<bool>()?.into(),
            _ => value.into(),
        };
    }

    fs::write(global_path, doc.to_string())?;
    Ok(())
}

fn apply_file(config: &mut Config, path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    let doc = content
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse config file {}", path.display()))?;

    for key in CONFIG_KEYS {
        if let Some(item) = doc.get(key)
            && !item.is_none()
        {
            apply_toml_item(config, key, item)?;
        }
    }
    Ok(())
}

fn apply_env_file(config: &mut Config, path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    for item in dotenvy::from_path_iter(path)? {
        let (key, value) = item?;
        if CONFIG_KEYS.contains(&key.as_str()) {
            apply_value(config, &key, &value)?;
        }
    }

    Ok(())
}

fn apply_process_env(config: &mut Config) -> Result<()> {
    for key in CONFIG_KEYS {
        if let Ok(value) = env::var(key) {
            apply_value(config, key, &value)?;
        }
    }
    Ok(())
}

fn apply_toml_item(config: &mut Config, key: &str, item: &Item) -> Result<()> {
    let value = item
        .as_value()
        .map(toml_value_to_string)
        .unwrap_or_else(|| item.to_string());
    apply_value(config, key, &value)
}

fn toml_value_to_string(value: &Value) -> String {
    if let Some(value) = value.as_str() {
        value.to_owned()
    } else if let Some(value) = value.as_integer() {
        value.to_string()
    } else if let Some(value) = value.as_bool() {
        value.to_string()
    } else {
        value.to_string()
    }
}

fn apply_value(config: &mut Config, key: &str, value: &str) -> Result<()> {
    if !CONFIG_KEYS.contains(&key) {
        return Err(AicError::UnsupportedConfigKey(key.to_owned()).into());
    }

    match key {
        "AIC_AI_PROVIDER" => {
            config.ai_provider = match value.to_lowercase().as_str() {
                "azure" => "azure-openai".to_owned(),
                provider => provider.to_owned(),
            }
        }
        "AIC_API_KEY" => config.api_key = optional_string(value),
        "AIC_API_URL" => config.api_url = optional_string(value),
        "AIC_API_CUSTOM_HEADERS" => {
            config.api_custom_headers = parse_headers(value)?;
        }
        "AIC_PROXY" => config.proxy = optional_string(value),
        "AIC_TOKENS_MAX_INPUT" => config.tokens_max_input = parse_usize(key, value)?,
        "AIC_TOKENS_MAX_OUTPUT" => config.tokens_max_output = parse_usize(key, value)?,
        "AIC_DESCRIPTION" => config.description = parse_bool(key, value)?,
        "AIC_EMOJI" => config.emoji = parse_bool(key, value)?,
        "AIC_MODEL" => config.model = value.to_owned(),
        "AIC_LANGUAGE" => config.language = value.to_owned(),
        "AIC_MESSAGE_TEMPLATE_PLACEHOLDER" => {
            config.message_template_placeholder = value.to_owned()
        }
        "AIC_PROMPT_FILE" => config.prompt_file = optional_string(value),
        "AIC_ONE_LINE_COMMIT" => config.one_line_commit = parse_bool(key, value)?,
        "AIC_OMIT_SCOPE" => config.omit_scope = parse_bool(key, value)?,
        "AIC_GITPUSH" => config.gitpush = parse_bool(key, value)?,
        "AIC_REMOTE_ICON_STYLE" => config.remote_icon_style = normalize_remote_icon_style(value)?,
        "AIC_HOOK_AUTO_UNCOMMENT" => config.hook_auto_uncomment = parse_bool(key, value)?,
        _ => unreachable!("all config keys are handled"),
    }
    Ok(())
}

fn optional_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "null" || trimmed == "undefined" {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn parse_bool(key: &str, value: &str) -> Result<bool> {
    value.parse::<bool>().map_err(|_| {
        AicError::InvalidConfigValue {
            key: key.to_owned(),
            message: "expected true or false".to_owned(),
        }
        .into()
    })
}

fn parse_usize(key: &str, value: &str) -> Result<usize> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| AicError::InvalidConfigValue {
            key: key.to_owned(),
            message: "expected a positive integer".to_owned(),
        })?;
    if parsed == 0 {
        return Err(AicError::InvalidConfigValue {
            key: key.to_owned(),
            message: "expected a positive integer".to_owned(),
        }
        .into());
    }
    Ok(parsed)
}

fn parse_headers(value: &str) -> Result<BTreeMap<String, String>> {
    if value.trim().is_empty() {
        return Ok(BTreeMap::new());
    }

    serde_json::from_str(value).map_err(|error| {
        AicError::InvalidConfigValue {
            key: "AIC_API_CUSTOM_HEADERS".to_owned(),
            message: format!("expected a JSON object of string headers: {error}"),
        }
        .into()
    })
}

fn normalize_remote_icon_style(value: &str) -> Result<String> {
    match value.trim().to_lowercase().as_str() {
        "auto" | "" => Ok("auto".to_owned()),
        "nerd" | "nerd-font" | "nerdfont" => Ok("nerd-font".to_owned()),
        "emoji" => Ok("emoji".to_owned()),
        "label" | "labels" | "none" | "off" => Ok("label".to_owned()),
        _ => Err(AicError::InvalidConfigValue {
            key: "AIC_REMOTE_ICON_STYLE".to_owned(),
            message: "expected auto, nerd-font, emoji, or label".to_owned(),
        }
        .into()),
    }
}

fn validate_config(config: &Config) -> Result<()> {
    if !supported_providers().contains(&config.ai_provider.as_str())
        && config.ai_provider.as_str() != "test"
    {
        bail!(AicError::InvalidConfigValue {
            key: "AIC_AI_PROVIDER".to_owned(),
            message: format!("supported values: {}", supported_providers().join(", ")),
        });
    }

    if config.ai_provider == "azure-openai" && config.api_url.is_none() {
        bail!(AicError::InvalidConfigValue {
            key: "AIC_API_URL".to_owned(),
            message: "required for Azure OpenAI; use https://<resource>.openai.azure.com/openai/v1"
                .to_owned(),
        });
    }

    if !config.message_template_placeholder.starts_with('$') {
        bail!(AicError::InvalidConfigValue {
            key: "AIC_MESSAGE_TEMPLATE_PLACEHOLDER".to_owned(),
            message: "must start with '$'".to_owned(),
        });
    }

    if config.tokens_max_input <= config.tokens_max_output {
        bail!(AicError::InvalidConfigValue {
            key: "AIC_TOKENS_MAX_INPUT".to_owned(),
            message: "must be greater than AIC_TOKENS_MAX_OUTPUT".to_owned(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn env_file_overrides_global_config() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join(".aicommit");
        let env_file = temp.path().join(".env");
        fs::write(
            &global,
            "AIC_API_KEY = \"global\"\nAIC_MODEL = \"gpt-5.4-mini\"\n",
        )
        .unwrap();
        fs::write(&env_file, "AIC_API_KEY=local\nAIC_DESCRIPTION=true\n").unwrap();

        let config = Config::load_from(&ConfigPaths { global, env_file }).unwrap();

        assert_eq!(config.api_key.as_deref(), Some("local"));
        assert_eq!(config.model, "gpt-5.4-mini");
        assert!(config.description);
    }

    #[test]
    fn set_global_config_writes_defaults_and_values() {
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
        let content = fs::read_to_string(global).unwrap();
        assert!(content.contains("AIC_API_KEY"));
        assert!(content.contains("AIC_EMOJI"));
    }
}
