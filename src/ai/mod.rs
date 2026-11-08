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
pub mod sse;

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

/// Callback that receives each streamed text fragment as it arrives.
pub type DeltaFn<'a> = &'a (dyn Fn(&str) + Send + Sync);

#[async_trait]
pub trait AiEngine: Send + Sync {
    /// Send chat messages and return the model's text response.
    /// Used for commit generation, review, and any other text completion task.
    async fn complete(&self, messages: &[ChatMessage]) -> Result<String>;

    /// Like [`AiEngine::complete`], but reports text fragments to `on_delta`
    /// as they arrive. The returned string is the full, sanitized response,
    /// which can differ from the concatenated fragments (for example after
    /// `<think>` blocks are stripped), so callers should prefer it once done.
    ///
    /// The default implementation does not stream: it calls `complete` and
    /// reports the whole response as one fragment. Providers opt in by
    /// overriding it.
    async fn complete_streaming(
        &self,
        messages: &[ChatMessage],
        on_delta: DeltaFn<'_>,
    ) -> Result<String> {
        let response = self.complete(messages).await?;
        on_delta(&response);
        Ok(response)
    }

    /// Whether `complete_streaming` delivers real incremental fragments.
    fn streams(&self) -> bool {
        false
    }
}

pub fn engine_from_config(config: &Config) -> Result<Box<dyn AiEngine>> {
    match config.ai_provider.as_str() {
        "openai" | "azure-openai" | "groq" | "ollama" => {
            Ok(Box::new(OpenAiCompatEngine::new(config.clone())?))
        }
        "anthropic" => Ok(Box::new(AnthropicEngine::new(config.clone())?)),
        "claude-code" | "codex" | "copilot" | "apple" => {
            Ok(Box::new(CommandEngine::new(config.clone())?))
        }
        "test" => Ok(Box::new(TestEngine)),
        unsupported => {
            bail!(
                "provider '{unsupported}' is not supported; use openai, azure-openai, anthropic, groq, ollama, claude-code, codex, copilot, or apple"
            )
        }
    }
}

struct TestEngine;

#[async_trait]
impl AiEngine for TestEngine {
    async fn complete(&self, messages: &[ChatMessage]) -> Result<String> {
        let system = messages
            .iter()
            .find(|message| message.role == "system")
            .map(|message| message.content.as_str())
            .unwrap_or_default();
        let user = messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .map(|message| message.content.as_str())
            .unwrap_or_default();

        if system.contains("Return valid JSON only.") {
            let files = extract_split_files(user);
            return Ok(test_split_plan_response(&files));
        }

        if system.contains("Git recovery guidance") {
            if user.contains("commit_created: true") {
                return Ok(
                    "The commit is local, but the branch needs to be rebased before you push.\n\n1. `git pull --rebase`\n2. `git push`"
                        .to_owned(),
                );
            }

            return Ok(
                "The branch needs to be synced before `aic` can continue.\n\n1. `git pull --rebase`\n2. `aic`"
                    .to_owned(),
            );
        }

        if user.contains("README.md") {
            return Ok("docs: update readme".to_owned());
        }
        if user.contains("src/lib.rs") {
            return Ok("feat: update library".to_owned());
        }

        Ok("feat: add generated commit message".to_owned())
    }
}

/// Hunk IDs from the split prompt's "Staged files and their hunk IDs"
/// section (`- path: path#1, path#2 (whole file)`).
fn extract_split_files(input: &str) -> Vec<String> {
    let mut hunks = Vec::new();
    let mut in_files = false;

    for line in input.lines() {
        if line.trim() == "Staged files and their hunk IDs:" {
            in_files = true;
            continue;
        }

        if !in_files {
            continue;
        }

        if let Some(entry) = line.trim().strip_prefix("- ") {
            let ids = entry.rsplit_once(": ").map_or(entry, |(_, ids)| ids);
            let ids = ids.split(" (").next().unwrap_or(ids);
            hunks.extend(ids.split(", ").map(|id| id.trim().to_owned()));
        } else if !line.trim().is_empty() {
            break;
        }
    }

    hunks
}

fn test_split_plan_response(hunks: &[String]) -> String {
    if hunks.len() < 2 {
        return r#"{"groups":[{"title":"one","rationale":"single change","hunks":["src.txt#1"]}]}"#
            .to_owned();
    }

    let first = serde_json::to_string(&hunks[..1]).unwrap_or_else(|_| "[]".to_owned());
    let second = serde_json::to_string(&hunks[1..]).unwrap_or_else(|_| "[]".to_owned());
    format!(
        r#"{{"groups":[{{"title":"Primary change","rationale":"Keep the first concern separate","hunks":{first}}},{{"title":"Follow-up change","rationale":"Keep the remaining concern separate","hunks":{second}}}]}}"#
    )
}
