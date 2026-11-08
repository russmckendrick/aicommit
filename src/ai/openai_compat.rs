use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{Client, Proxy, RequestBuilder, Response};
use serde::{Deserialize, Serialize};

use crate::{
    ai::{AiEngine, ChatMessage, DeltaFn, sse::SseParser},
    config::{Config, default_api_url_for_provider},
    errors::normalize_provider_error,
    prompt::sanitize_model_output,
    token::count_messages,
};

const CONNECT_TIMEOUT_SECS: u64 = 10;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
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

#[derive(Debug, Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
}

#[derive(Debug, Default, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

impl OpenAiCompatEngine {
    pub fn new(config: Config) -> Result<Self> {
        let mut builder =
            Client::builder().connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS));
        if config.http_timeout > 0 {
            builder = builder.timeout(std::time::Duration::from_secs(config.http_timeout as u64));
        }
        if let Some(proxy) = &config.proxy {
            builder = builder.proxy(Proxy::all(proxy)?);
        }

        let client = builder.build()?;
        let base_url = match config.ai_provider.as_str() {
            "azure-openai" => config.api_url.clone().context(
                "AIC_API_URL is required for Azure OpenAI; use https://<resource>.openai.azure.com/openai/v1",
            )?,
            "groq" | "ollama" => config
                .api_url
                .clone()
                .or_else(|| default_api_url_for_provider(&config.ai_provider).map(str::to_owned))
                .unwrap_or_else(|| {
                    if config.ai_provider == "ollama" {
                        "http://localhost:11434/v1".to_owned()
                    } else {
                        "https://api.groq.com/openai/v1".to_owned()
                    }
                }),
            _ => config
                .api_url
                .clone()
                .unwrap_or_else(|| "https://api.openai.com/v1".to_owned()),
        };

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

impl OpenAiCompatEngine {
    fn request(&self, messages: &[ChatMessage], stream: bool) -> Result<RequestBuilder> {
        let request_tokens = count_messages(messages);
        if request_tokens
            > self
                .config
                .tokens_max_input
                .saturating_sub(self.config.tokens_max_output)
        {
            return Err(crate::errors::AicError::TooManyTokens.into());
        }

        let is_reasoning_model = self.config.model.starts_with("o1")
            || self.config.model.starts_with("o3")
            || self.config.model.starts_with("o4")
            || self.config.model.starts_with("gpt-5")
            || self.config.model.starts_with("gpt-6");

        let payload = ChatRequest {
            model: &self.config.model,
            messages,
            temperature: (!is_reasoning_model).then_some(0.0),
            top_p: (!is_reasoning_model).then_some(0.1),
            max_tokens: (!is_reasoning_model).then_some(self.config.tokens_max_output),
            max_completion_tokens: is_reasoning_model.then_some(self.config.tokens_max_output),
            stream: stream.then_some(true),
        };

        let mut request = self.client.post(self.chat_url()).json(&payload);

        if let Some(api_key) = &self.config.api_key {
            request = if self.config.ai_provider == "azure-openai" {
                request.header("api-key", api_key)
            } else {
                request.bearer_auth(api_key)
            };
        }

        for (key, value) in &self.config.api_custom_headers {
            request = request.header(key, value);
        }

        Ok(request)
    }

    async fn send(&self, request: RequestBuilder) -> Result<Response> {
        let response = request
            .send()
            .await
            .map_err(|error| self.transport_error(error))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(normalize_provider_error(
                &self.config.ai_provider,
                &self.config.model,
                Some(status.as_u16()),
                &body,
            )
            .into());
        }
        Ok(response)
    }

    fn transport_error(&self, error: reqwest::Error) -> anyhow::Error {
        if error.is_timeout() {
            anyhow::anyhow!(
                "request to {} timed out after {}s - raise AIC_HTTP_TIMEOUT or reduce the staged diff",
                self.config.ai_provider,
                self.config.http_timeout
            )
        } else {
            anyhow::Error::new(error).context("failed to call AI provider")
        }
    }
}

#[async_trait]
impl AiEngine for OpenAiCompatEngine {
    async fn complete(&self, messages: &[ChatMessage]) -> Result<String> {
        let response = self.send(self.request(messages, false)?).await?;
        let body = response.text().await?;

        let response: ChatResponse = serde_json::from_str(&body)
            .with_context(|| format!("failed to parse AI response: {body}"))?;
        let content = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .map(sanitize_model_output)
            .filter(|content| !content.is_empty())
            .ok_or(crate::errors::AicError::EmptyMessage)?;

        Ok(content)
    }

    async fn complete_streaming(
        &self,
        messages: &[ChatMessage],
        on_delta: DeltaFn<'_>,
    ) -> Result<String> {
        let mut response = self.send(self.request(messages, true)?).await?;
        let mut parser = SseParser::default();
        let mut text = String::new();
        let mut done = false;

        while !done {
            let events = match response
                .chunk()
                .await
                .map_err(|error| self.transport_error(error))?
            {
                Some(chunk) => parser.push(&chunk),
                None => {
                    done = true;
                    parser.finish()
                }
            };
            for data in events {
                if data == "[DONE]" {
                    done = true;
                    break;
                }
                let event: StreamChunk = serde_json::from_str(&data)
                    .with_context(|| format!("failed to parse AI stream event: {data}"))?;
                if let Some(error) = event.error {
                    return Err(normalize_provider_error(
                        &self.config.ai_provider,
                        &self.config.model,
                        None,
                        &serde_json::json!({ "error": error }).to_string(),
                    )
                    .into());
                }
                if let Some(delta) = event
                    .choices
                    .first()
                    .and_then(|choice| choice.delta.content.as_deref())
                    .filter(|delta| !delta.is_empty())
                {
                    text.push_str(delta);
                    on_delta(delta);
                }
            }
        }

        let content = sanitize_model_output(&text);
        if content.is_empty() {
            return Err(crate::errors::AicError::EmptyMessage.into());
        }
        Ok(content)
    }

    fn streams(&self) -> bool {
        true
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
