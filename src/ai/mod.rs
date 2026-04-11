use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    ai::{anthropic::AnthropicEngine, command::CommandEngine, openai_compat::OpenAiCompatEngine},
    config::Config,
};

pub mod anthropic;
pub mod command;
pub mod openai_compat;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_owned(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_owned(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_owned(),
            content: content.into(),
        }
    }
}

#[async_trait]
pub trait AiEngine: Send + Sync {
    /// Send chat messages and return the model's text response.
    /// Used for commit generation, review, and any other text completion task.
    async fn generate_commit_message(&self, messages: &[ChatMessage]) -> Result<String>;
}

pub fn engine_from_config(config: &Config) -> Result<Box<dyn AiEngine>> {
    match config.ai_provider.as_str() {
        "openai" | "azure-openai" | "groq" => {
            Ok(Box::new(OpenAiCompatEngine::new(config.clone())?))
        }
        "anthropic" => Ok(Box::new(AnthropicEngine::new(config.clone())?)),
        "claude-code" | "codex" => Ok(Box::new(CommandEngine::new(config.clone())?)),
        "test" => Ok(Box::new(TestEngine)),
        unsupported => {
            bail!(
                "provider '{unsupported}' is not supported; use openai, azure-openai, anthropic, groq, claude-code, or codex"
            )
        }
    }
}

struct TestEngine;

#[async_trait]
impl AiEngine for TestEngine {
    async fn generate_commit_message(&self, _messages: &[ChatMessage]) -> Result<String> {
        Ok("feat: add generated commit message".to_owned())
    }
}
