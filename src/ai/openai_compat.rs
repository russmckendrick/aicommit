use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{Client, Proxy};
use serde::{Deserialize, Serialize};

use crate::{
    ai::{AiEngine, ChatMessage},
    config::{Config, provider_base_url},
    errors::normalize_provider_error,
    prompt::remove_content_tags,
    token::count_messages,
};

#[derive(Debug, Clone)]
pub struct OpenAiCompatEngine {
    config: Config,
    client: Client,
    base_url: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

impl OpenAiCompatEngine {
    pub fn new(config: Config) -> Result<Self> {
        let mut builder = Client::builder();
        if let Some(proxy) = &config.proxy {
            builder = builder.proxy(Proxy::all(proxy)?);
        }

        let client = builder.build()?;
        let base_url = config
            .api_url
            .clone()
            .or_else(|| provider_base_url(&config.ai_provider).map(str::to_owned))
            .unwrap_or_else(|| "https://api.openai.com/v1".to_owned());

        Ok(Self {
            config,
            client,
            base_url,
        })
    }

    fn chat_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        if base.ends_with("/chat/completions") {
            base.to_owned()
        } else {
            format!("{base}/chat/completions")
        }
    }
}

#[async_trait]
impl AiEngine for OpenAiCompatEngine {
    async fn generate_commit_message(&self, messages: &[ChatMessage]) -> Result<String> {
        let request_tokens = count_messages(messages);
        if request_tokens > self.config.tokens_max_input - self.config.tokens_max_output {
            return Err(crate::errors::AicError::TooManyTokens.into());
        }

        let is_reasoning_model = self.config.model.starts_with("o1")
            || self.config.model.starts_with("o3")
            || self.config.model.starts_with("o4")
            || self.config.model.starts_with("gpt-5");

        let payload = ChatRequest {
            model: &self.config.model,
            messages,
            temperature: (!is_reasoning_model).then_some(0.0),
            top_p: (!is_reasoning_model).then_some(0.1),
            max_tokens: (!is_reasoning_model).then_some(self.config.tokens_max_output),
            max_completion_tokens: is_reasoning_model.then_some(self.config.tokens_max_output),
        };

        let mut request = self.client.post(self.chat_url()).json(&payload);

        if let Some(api_key) = &self.config.api_key {
            request = request.bearer_auth(api_key);
        }

        for (key, value) in &self.config.api_custom_headers {
            request = request.header(key, value);
        }

        if self.config.ai_provider == "openrouter" || self.config.ai_provider == "aimlapi" {
            request = request
                .header("HTTP-Referer", "https://github.com/aicommit/aicommit")
                .header("X-Title", "aicommit");
        }

        let response = request.send().await.context("failed to call AI provider")?;
        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(normalize_provider_error(
                &self.config.ai_provider,
                &self.config.model,
                Some(status.as_u16()),
                &body,
            )
            .into());
        }

        let response: ChatResponse = serde_json::from_str(&body)
            .with_context(|| format!("failed to parse AI response: {body}"))?;
        let content = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .map(|content| remove_content_tags(content, "think"))
            .filter(|content| !content.is_empty())
            .ok_or(crate::errors::AicError::EmptyMessage)?;

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_chat_completions_path() {
        let config = Config {
            api_url: Some("http://localhost:9000/v1".to_owned()),
            ..Config::default()
        };
        let engine = OpenAiCompatEngine::new(config).unwrap();
        assert_eq!(
            engine.chat_url(),
            "http://localhost:9000/v1/chat/completions"
        );
    }
}
