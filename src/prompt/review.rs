use anyhow::Result;

use crate::{ai::ChatMessage, config::Config};

const DEFAULT_REVIEW_PROMPT: &str = include_str!("../../prompts/review-system.md");

pub fn build_review_messages(
    config: &Config,
    diff: &str,
    context: &str,
) -> Result<Vec<ChatMessage>> {
    Ok(vec![
        ChatMessage::system(review_system_prompt(config, context)?),
        ChatMessage::user(diff),
    ])
}

pub fn review_system_prompt(config: &Config, context: &str) -> Result<String> {
    let context_instruction = if context.trim().is_empty() {
        String::new()
    } else {
        format!(
            "Additional reviewer context: <context>{}</context>. Focus your review accordingly.",
            context.trim()
        )
    };

    Ok(DEFAULT_REVIEW_PROMPT
        .replace("{{language}}", &config.language)
        .replace("{{context_instruction}}", &context_instruction))
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    use super::*;

    #[test]
    fn review_prompt_includes_context() {
        let config = Config::default();
        let prompt = review_system_prompt(&config, "focus on security").unwrap();
        assert!(prompt.contains("focus on security"));
    }

    #[test]
    fn review_prompt_renders_without_context() {
        let config = Config::default();
        let prompt = review_system_prompt(&config, "").unwrap();
        assert!(!prompt.contains("<context>"));
        assert!(prompt.contains("code reviewer"));
        assert!(prompt.contains("Review the whole diff before answering"));
    }
}
