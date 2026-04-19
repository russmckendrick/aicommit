use anyhow::Result;

use crate::{ai::ChatMessage, config::Config};

const DEFAULT_GIT_GUIDANCE_PROMPT: &str = include_str!("../../prompts/git-guidance-system.md");

pub fn build_git_guidance_messages(config: &Config, facts: &str) -> Result<Vec<ChatMessage>> {
    Ok(vec![
        ChatMessage::system(git_guidance_system_prompt(config)?),
        ChatMessage::user(facts),
    ])
}

pub fn git_guidance_system_prompt(config: &Config) -> Result<String> {
    Ok(DEFAULT_GIT_GUIDANCE_PROMPT.replace("{{language}}", &config.language))
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    use super::*;

    #[test]
    fn git_guidance_prompt_uses_configured_language() {
        let config = Config {
            language: "fr".to_owned(),
            ..Config::default()
        };

        let prompt = git_guidance_system_prompt(&config).unwrap();
        assert!(prompt.contains("Reply in fr."));
        assert!(prompt.contains("Git recovery guidance"));
    }
}
