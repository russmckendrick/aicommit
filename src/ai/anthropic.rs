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

const ANTHROPIC_VERSION: &str = "2023-06-01";
const CONNECT_TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone)]
pub struct AnthropicEngine {
    config: Config,
    client: Client,
    base_url: String,
}

#[derive(Debug, Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    delta: Option<StreamDelta>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ResponseBlock>,
}

#[derive(Debug, Deserialize)]
struct ResponseBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

impl AnthropicEngine {
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
        let base_url = config
            .api_url
            .clone()
            .or_else(|| default_api_url_for_provider(&config.ai_provider).map(str::to_owned))
            .unwrap_or_else(|| "https://api.anthropic.com/v1".to_owned());

        Ok(Self {
            config,
            client,
            base_url,
        })
    }

    fn messages_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        if base.ends_with("/messages") {
            base.to_owned()
        } else {
            format!("{base}/messages")
        }
    }

    fn build_request(&self, messages: &[ChatMessage]) -> MessagesRequest {
        let mut system_messages = Vec::new();
        let mut anthropic_messages = Vec::new();

        for message in messages {
            match message.role.as_str() {
                "system" => system_messages.push(message.content.clone()),
                "user" | "assistant" => anthropic_messages.push(AnthropicMessage {
                    role: message.role.clone(),
                    content: message.content.clone(),
                }),
                _ => anthropic_messages.push(AnthropicMessage {
                    role: "user".to_owned(),
                    content: message.content.clone(),
                }),
            }
        }

        if anthropic_messages.is_empty() {
            anthropic_messages.push(AnthropicMessage {
                role: "user".to_owned(),
                content: String::new(),
            });
        }

        MessagesRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.tokens_max_output,
            system: (!system_messages.is_empty()).then(|| system_messages.join("\n\n")),
            messages: anthropic_messages,
            stream: None,
        }
    }
}

impl AnthropicEngine {
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

        let mut payload = self.build_request(messages);
        payload.stream = stream.then_some(true);
        let mut request = self
            .client
            .post(self.messages_url())
            .header("x-api-key", self.config.api_key.clone().unwrap_or_default())
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&payload);

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
impl AiEngine for AnthropicEngine {
    async fn complete(&self, messages: &[ChatMessage]) -> Result<String> {
        let response = self.send(self.request(messages, false)?).await?;
        let body = response.text().await?;

        let response: MessagesResponse = serde_json::from_str(&body)
            .with_context(|| format!("failed to parse AI response: {body}"))?;
        let content = response
            .content
            .into_iter()
            .filter(|block| block.kind == "text")
            .filter_map(|block| block.text)
            .map(|text| sanitize_model_output(&text))
            .filter(|content| !content.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned();

        if content.is_empty() {
            return Err(crate::errors::AicError::EmptyMessage.into());
        }

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
                let event: StreamEvent = serde_json::from_str(&data)
                    .with_context(|| format!("failed to parse AI stream event: {data}"))?;
                match event.kind.as_str() {
                    "content_block_delta" => {
                        if let Some(delta) = event
                            .delta
                            .filter(|delta| delta.kind == "text_delta")
                            .and_then(|delta| delta.text)
                            .filter(|delta| !delta.is_empty())
                        {
                            text.push_str(&delta);
                            on_delta(&delta);
                        }
                    }
                    "error" => {
                        return Err(normalize_provider_error(
                            &self.config.ai_provider,
                            &self.config.model,
                            None,
                            &data,
                        )
                        .into());
                    }
                    "message_stop" => {
                        done = true;
                        break;
                    }
                    _ => {}
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
    fn appends_messages_path() {
        let config = Config {
            ai_provider: "anthropic".to_owned(),
            api_url: Some("http://localhost:9000/v1".to_owned()),
            ..Config::default()
        };
        let engine = AnthropicEngine::new(config).unwrap();
        assert_eq!(engine.messages_url(), "http://localhost:9000/v1/messages");
    }

    #[test]
    fn folds_system_messages_into_top_level_system_field() {
        let config = Config {
            ai_provider: "anthropic".to_owned(),
            ..Config::default()
        };
        let engine = AnthropicEngine::new(config).unwrap();
        let payload = engine.build_request(&[
            ChatMessage::system("system one"),
            ChatMessage::user("user diff"),
            ChatMessage::assistant("assistant example"),
            ChatMessage::system("system two"),
        ]);

        assert_eq!(payload.system.as_deref(), Some("system one\n\nsystem two"));
        assert_eq!(payload.messages.len(), 2);
        assert_eq!(payload.messages[0].role, "user");
        assert_eq!(payload.messages[1].role, "assistant");
    }
}
