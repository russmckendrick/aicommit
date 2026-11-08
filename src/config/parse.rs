use std::collections::BTreeMap;

use anyhow::Result;
use toml_edit::Value;

use crate::errors::AicError;

pub fn toml_value_to_string(value: &Value) -> String {
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

pub fn normalize_provider(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "azure" => "azure-openai".to_owned(),
        "claudecode" => "claude-code".to_owned(),
        "fm" => "apple".to_owned(),
        provider => provider.to_owned(),
    }
}

pub fn parse_bool(key: &str, value: &str) -> Result<bool> {
    value.parse::<bool>().map_err(|_| {
        AicError::InvalidConfigValue {
            key: key.to_owned(),
            message: "expected true or false".to_owned(),
        }
        .into()
    })
}

pub fn parse_usize(key: &str, value: &str) -> Result<usize> {
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

pub fn parse_headers(value: &str) -> Result<BTreeMap<String, String>> {
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

/// `auto` picks github-dark or github-light from the terminal background;
/// any `aic map` theme name is also accepted.
pub fn normalize_theme(value: &str) -> Result<String> {
    let key = value.trim().to_lowercase().replace(' ', "-");
    if key.is_empty() || key == "auto" {
        return Ok("auto".to_owned());
    }
    crate::map::theme::load_theme(&key)
        .map(|_| key)
        .map_err(|error| {
            AicError::InvalidConfigValue {
                key: "AIC_THEME".to_owned(),
                message: format!("expected auto or a theme name ({error})"),
            }
            .into()
        })
}

pub fn normalize_icons(value: &str) -> Result<String> {
    match value.trim().to_lowercase().as_str() {
        "" | "auto" | "default" => Ok("auto".to_owned()),
        "unicode" | "plain" => Ok("unicode".to_owned()),
        "nerd" | "nerd-font" | "nerdfont" => Ok("nerd".to_owned()),
        _ => Err(AicError::InvalidConfigValue {
            key: "AIC_ICONS".to_owned(),
            message: "expected auto, nerd, or unicode".to_owned(),
        }
        .into()),
    }
}

pub fn normalize_remote_icon_style(value: &str) -> Result<String> {
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
