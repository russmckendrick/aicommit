use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{
    config::{Config, global_model_cache_path, model_list, supported_providers},
    ui,
};

const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Debug, Default, Serialize, Deserialize)]
struct ModelCache {
    timestamp: u64,
    models: BTreeMap<String, Vec<String>>,
}

pub async fn run(provider: Option<String>, refresh: bool) -> Result<()> {
    let config = Config::load()?;
    let provider = provider.unwrap_or_else(|| config.ai_provider.clone());
    if !supported_providers().contains(&provider.as_str()) {
        bail!(
            "unsupported provider '{provider}'; supported values: {}",
            supported_providers().join(", ")
        );
    }

    let cache_path = global_model_cache_path()?;
    let mut cache = read_cache(&cache_path).unwrap_or_default();

    let models = if !refresh && cache_is_fresh(&cache) {
        cache.models.get(&provider).cloned()
    } else {
        None
    };

    let models = match models {
        Some(models) => models,
        None => {
            let fetched = fetch_models(&provider, &config).await.unwrap_or_else(|_| {
                model_list(&provider)
                    .iter()
                    .map(|model| model.to_string())
                    .collect()
            });
            cache.timestamp = current_timestamp();
            cache.models.insert(provider.clone(), fetched.clone());
            write_cache(&cache_path, &cache)?;
            fetched
        }
    };

    ui::info(format!("Available models for {provider}:"));
    if models.is_empty() {
        ui::info("  no models found");
    } else {
        for model in models {
            if model == config.model {
                println!("* {model}");
            } else {
                println!("  {model}");
            }
        }
    }
    Ok(())
}

async fn fetch_models(provider: &str, config: &Config) -> Result<Vec<String>> {
    match provider {
        "openai" => {
            let base = config
                .api_url
                .as_deref()
                .unwrap_or("https://api.openai.com/v1");
            fetch_openai_models(
                provider,
                &format!("{}/models", base.trim_end_matches('/')),
                config,
            )
            .await
        }
        "azure-openai" => {
            if let Some(base) = config.api_url.as_deref() {
                fetch_openai_models(
                    provider,
                    &format!("{}/models", base.trim_end_matches('/')),
                    config,
                )
                .await
            } else {
                Ok(model_list(provider)
                    .iter()
                    .map(|model| model.to_string())
                    .collect())
            }
        }
        _ => Ok(model_list(provider)
            .iter()
            .map(|model| model.to_string())
            .collect()),
    }
}

async fn fetch_openai_models(provider: &str, url: &str, config: &Config) -> Result<Vec<String>> {
    #[derive(Debug, Deserialize)]
    struct Response {
        data: Vec<Model>,
    }

    #[derive(Debug, Deserialize)]
    struct Model {
        id: String,
    }

    let mut request = Client::new().get(url);
    if let Some(api_key) = &config.api_key {
        request = if provider == "azure-openai" {
            request.header("api-key", api_key)
        } else {
            request.bearer_auth(api_key)
        };
    }
    let response = request
        .send()
        .await?
        .error_for_status()?
        .json::<Response>()
        .await?;
    let mut models = response
        .data
        .into_iter()
        .map(|model| model.id)
        .filter(|model| {
            model.starts_with("gpt-")
                || model.starts_with("o1")
                || model.starts_with("o3")
                || model.starts_with("o4")
        })
        .collect::<Vec<_>>();
    models.sort();
    Ok(models)
}

fn read_cache(path: &std::path::Path) -> Result<ModelCache> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn write_cache(path: &std::path::Path, cache: &ModelCache) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(cache)?)?;
    Ok(())
}

fn cache_is_fresh(cache: &ModelCache) -> bool {
    current_timestamp().saturating_sub(cache.timestamp) < CACHE_TTL.as_secs()
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
