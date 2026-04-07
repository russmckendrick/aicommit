use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{
    config::{Config, global_model_cache_path, model_list},
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
        "openai" => fetch_openai_models("https://api.openai.com/v1/models", config).await,
        "groq" => fetch_openai_models("https://api.groq.com/openai/v1/models", config).await,
        "deepseek" => fetch_openai_models("https://api.deepseek.com/v1/models", config).await,
        "ollama" => fetch_ollama_models(config).await,
        _ => Ok(model_list(provider)
            .iter()
            .map(|model| model.to_string())
            .collect()),
    }
}

async fn fetch_openai_models(url: &str, config: &Config) -> Result<Vec<String>> {
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
        request = request.bearer_auth(api_key);
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
                || model.starts_with("deepseek")
                || model.starts_with("llama")
                || model.starts_with("gemma")
        })
        .collect::<Vec<_>>();
    models.sort();
    Ok(models)
}

async fn fetch_ollama_models(config: &Config) -> Result<Vec<String>> {
    #[derive(Debug, Deserialize)]
    struct Response {
        models: Vec<Model>,
    }

    #[derive(Debug, Deserialize)]
    struct Model {
        name: String,
    }

    let base = config
        .api_url
        .as_deref()
        .unwrap_or("http://localhost:11434")
        .trim_end_matches('/')
        .to_owned();
    let response = Client::new()
        .get(format!("{base}/api/tags"))
        .send()
        .await?
        .error_for_status()?
        .json::<Response>()
        .await?;
    Ok(response
        .models
        .into_iter()
        .map(|model| model.name)
        .collect())
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
