use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{Client, Proxy};
use serde::{Deserialize, Serialize};

use crate::{
    ai::{AiEngine, ChatMessage},
    config::Config,
    errors::normalize_provider_error,
    prompt::remove_content_tags,
};

#[derive(Debug, Clone)]
pub struct OllamaEngine {
    config: Config,
    client: Client,
}

#[derive(Debug, Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    options: OllamaOptions,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    temperature: f32,
    top_p: f32,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: Option<OllamaMessage>,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: Option<String>,
}

impl OllamaEngine {
    pub fn new(config: Config) -> Self {
        let mut builder = Client::builder();
        if let Some(proxy) = &config.proxy
            && let Ok(proxy) = Proxy::all(proxy)
        {
            builder = builder.proxy(proxy);
        }
        let client = builder.build().unwrap_or_else(|_| Client::new());
        Self { config, client }
    }

    fn chat_url(&self) -> String {
        let base_url = self
            .config
            .api_url
            .as_deref()
            .unwrap_or("http://localhost:11434");
        let base_url = base_url.trim_end_matches('/');

        if base_url.ends_with("/api/chat") {
            base_url.to_owned()
        } else {
            format!("{base_url}/api/chat")
        }
    }
}

#[async_trait]
impl AiEngine for OllamaEngine {
    async fn generate_commit_message(&self, messages: &[ChatMessage]) -> Result<String> {
        let payload = OllamaRequest {
            model: &self.config.model,
            messages,
            options: OllamaOptions {
                temperature: 0.0,
                top_p: 0.1,
            },
            stream: false,
        };

        let response = self
            .client
            .post(self.chat_url())
            .json(&payload)
            .send()
            .await
            .context("failed to call Ollama")?;
        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(normalize_provider_error(
                "ollama",
                &self.config.model,
                Some(status.as_u16()),
                &body,
            )
            .into());
        }

        let response: OllamaResponse = serde_json::from_str(&body)
            .with_context(|| format!("failed to parse Ollama response: {body}"))?;
        let content = response
            .message
            .and_then(|message| message.content)
            .map(|content| remove_content_tags(&content, "think"))
            .filter(|content| !content.is_empty())
            .ok_or(crate::errors::AicError::EmptyMessage)?;

        Ok(content)
    }
}
