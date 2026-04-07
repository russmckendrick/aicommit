use std::fs;

use anyhow::{Context, Result};

use crate::{ai::ChatMessage, config::Config};

const DEFAULT_SYSTEM_PROMPT: &str = include_str!("../prompts/commit-system.md");
const SHORT_GITMOJI_HELP: &str = "If an emoji is useful, use only one GitMoji prefix: 🐛 fix, ✨ feature, 📝 docs, 🚀 deploy, ✅ tests, ♻️ refactor, ⬆️ dependencies, 🔧 config, 🌐 localization, or 💡 comments.";
const FULL_GITMOJI_HELP: &str = "If an emoji is useful, use one GitMoji prefix that best matches the whole change. Prefer the official intent of each emoji; never stack multiple emojis.";

pub fn build_messages(
    config: &Config,
    diff: &str,
    full_gitmoji_spec: bool,
    context: &str,
) -> Result<Vec<ChatMessage>> {
    let mut messages = initial_messages(config, full_gitmoji_spec, context)?;
    messages.push(ChatMessage::user(diff));
    Ok(messages)
}

pub fn initial_messages(
    config: &Config,
    full_gitmoji_spec: bool,
    context: &str,
) -> Result<Vec<ChatMessage>> {
    Ok(vec![
        ChatMessage::system(system_prompt(config, full_gitmoji_spec, context)?),
        ChatMessage::user(example_diff()),
        ChatMessage::assistant(example_commit(config)),
    ])
}

pub fn system_prompt(config: &Config, full_gitmoji_spec: bool, context: &str) -> Result<String> {
    let convention = if config.emoji {
        if full_gitmoji_spec {
            FULL_GITMOJI_HELP
        } else {
            SHORT_GITMOJI_HELP
        }
    } else {
        "Use conventional commit keywords only: fix, feat, build, chore, ci, docs, style, refactor, perf, or test."
    };

    let body_instruction = if config.description {
        "After the subject, add one blank line, then 2-4 tight bullet points. Each bullet should explain a meaningful change or why it matters. Do not repeat the subject."
    } else {
        "Return only the subject line. Do not add a body, bullet list, markdown, or explanation."
    };

    let line_mode_instruction = if config.one_line_commit {
        "Use exactly one concise subject line."
    } else {
        "Use a subject plus body when body output is enabled. Keep the body scannable and useful in GitHub's commit view."
    };

    let scope_instruction = if config.omit_scope {
        "Do not include a scope; use '<type>: <subject>' when using conventional commits."
    } else {
        "Use at most one scope, and only when it clarifies the single overall change."
    };

    let context_instruction = if context.trim().is_empty() {
        String::new()
    } else {
        format!(
            "Additional user context: <context>{}</context>. Use it when relevant.",
            context.trim()
        )
    };

    let template = prompt_template(config)?;
    Ok(template
        .replace("{{commit_convention}}", convention)
        .replace("{{body_instruction}}", body_instruction)
        .replace("{{line_mode_instruction}}", line_mode_instruction)
        .replace("{{scope_instruction}}", scope_instruction)
        .replace("{{style_examples}}", &style_examples(config))
        .replace("{{language}}", &config.language)
        .replace("{{context_instruction}}", &context_instruction))
}

fn style_examples(config: &Config) -> String {
    let prompt_subject = if config.emoji {
        "✨ feat(prompt): make commit generation prompt-driven and resilient"
    } else {
        "feat(prompt): make commit generation prompt-driven and resilient"
    };
    let diff_subject = if config.emoji {
        "🐛 fix(diff): prevent oversized staged changes from aborting commits"
    } else {
        "fix(diff): prevent oversized staged changes from aborting commits"
    };

    format!(
        "{prompt_subject}\n\n- Move the default system prompt into a reusable template\n- Teach generation to synthesize chunked diffs into one polished message\n- Document how to tune prompt behavior without rebuilding\n\n{diff_subject}\n\n- Split oversized diff lines instead of failing the whole generation flow\n- Raise the default input budget for newer OpenAI models"
    )
}

fn prompt_template(config: &Config) -> Result<String> {
    match &config.prompt_file {
        Some(path) => fs::read_to_string(path)
            .with_context(|| format!("failed to read prompt template from {path}")),
        None => Ok(DEFAULT_SYSTEM_PROMPT.to_owned()),
    }
}

fn example_diff() -> String {
    r#"diff --git a/src/server.rs b/src/server.rs
--- a/src/server.rs
+++ b/src/server.rs
@@ -1,5 +1,5 @@
-let port = 7799;
+let port = std::env::var("PORT").unwrap_or_else(|_| "7799".into());"#
        .to_owned()
}

fn example_commit(config: &Config) -> String {
    let prefix = if config.emoji { "✨ " } else { "" };
    if config.omit_scope {
        format!("{prefix}feat: read server port from environment")
    } else {
        format!("{prefix}feat(server): read port from environment")
    }
}

pub fn remove_content_tags(input: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut output = input.to_owned();

    while let (Some(start), Some(end)) = (output.find(&open), output.find(&close)) {
        if end < start {
            break;
        }
        let close_end = end + close.len();
        output.replace_range(start..close_end, "");
    }

    output.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_uses_context_and_aic_config() {
        let config = Config {
            emoji: true,
            ..Config::default()
        };
        let prompt = system_prompt(&config, false, "issue 123").unwrap();
        assert!(prompt.contains("issue 123"));
        assert!(prompt.contains("GitMoji"));
    }

    #[test]
    fn removes_reasoning_tags() {
        assert_eq!(
            remove_content_tags("<think>hidden</think>\nfeat: add cli", "think"),
            "feat: add cli"
        );
    }
}
