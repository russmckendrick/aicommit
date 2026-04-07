use anyhow::{Result, bail};

use crate::{
    config::{CONFIG_KEYS, Config, ConfigPaths, config_descriptions, set_global_config},
    ui,
};

pub fn set(key_values: Vec<String>) -> Result<()> {
    if key_values.is_empty() {
        bail!("usage: aic config set KEY=value...");
    }

    let parsed = key_values
        .into_iter()
        .map(|entry| {
            let (key, value) = entry
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("expected KEY=value, got '{entry}'"))?;
            Ok((key.to_owned(), value.to_owned()))
        })
        .collect::<Result<Vec<_>>>()?;

    let paths = ConfigPaths::discover()?;
    set_global_config(&parsed, &paths.global)?;
    ui::success(format!("config saved to {}", paths.global.display()));
    Ok(())
}

pub fn get(keys: Vec<String>) -> Result<()> {
    if keys.is_empty() {
        bail!("usage: aic config get KEY...");
    }

    let config = Config::load()?;
    for key in keys {
        match config.get_key(&key) {
            Some(value) => println!("{key}={value}"),
            None => bail!("unknown config key: {key}"),
        }
    }
    Ok(())
}

pub fn describe(keys: Vec<String>) -> Result<()> {
    let descriptions = config_descriptions();
    let keys = if keys.is_empty() {
        CONFIG_KEYS.iter().map(|key| key.to_string()).collect()
    } else {
        keys
    };

    for key in keys {
        let description = descriptions
            .get(key.as_str())
            .ok_or_else(|| anyhow::anyhow!("unknown config key: {key}"))?;
        println!("{key}: {description}");
    }
    Ok(())
}
