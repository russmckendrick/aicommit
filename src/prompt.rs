use crate::{ai::ChatMessage, config::Config};

const IDENTITY: &str = "You write clear Git commit messages.";
const SHORT_GITMOJI_HELP: &str = "Use GitMoji when helpful: bug fixes use 🐛, features use ✨, docs use 📝, deployments use 🚀, tests use ✅, refactors use ♻️, dependency updates use ⬆️, configuration uses 🔧, localization uses 🌐, and comments use 💡.";
const FULL_GITMOJI_HELP: &str = "Use GitMoji when helpful. Prefer the official intent of each emoji, including 🐛 for fixes, ✨ for features, 📝 for docs, 🚀 for deploys, ✅ for tests, ♻️ for refactors, ⬆️ for dependency upgrades, 🔧 for configuration, 🌐 for localization, 💡 for comments, 🎨 for code structure, ⚡️ for performance, 🔥 for removals, 🚑️ for hotfixes, 💄 for UI styles, 🔒️ for security, 🚨 for warning fixes, 👷 for CI, 📦️ for packaging, 💥 for breaking changes, ♿️ for accessibility, 💬 for copy changes, 🏗️ for architecture, 🧑‍💻 for developer experience, and 🦺 for validation.";

pub fn build_messages(
    config: &Config,
    diff: &str,
    full_gitmoji_spec: bool,
    context: &str,
) -> Vec<ChatMessage> {
    let mut messages = initial_messages(config, full_gitmoji_spec, context);
    messages.push(ChatMessage::user(diff));
    messages
}

pub fn initial_messages(
    config: &Config,
    full_gitmoji_spec: bool,
    context: &str,
) -> Vec<ChatMessage> {
    vec![
        ChatMessage::system(system_prompt(config, full_gitmoji_spec, context)),
        ChatMessage::user(example_diff()),
        ChatMessage::assistant(example_commit(config)),
    ]
}

pub fn system_prompt(config: &Config, full_gitmoji_spec: bool, context: &str) -> String {
    let convention = if config.emoji {
        if full_gitmoji_spec {
            FULL_GITMOJI_HELP
        } else {
            SHORT_GITMOJI_HELP
        }
    } else {
        "Use conventional commit keywords only: fix, feat, build, chore, ci, docs, style, refactor, perf, or test."
    };

    let description = if config.description {
        "Add a short body explaining why the change was made. Do not start the body with 'This commit'."
    } else {
        "Return only the commit message, without a body or extra explanation."
    };

    let one_line = if config.one_line_commit {
        "Generate a single concise sentence that captures the primary change."
    } else {
        ""
    };

    let scope = if config.omit_scope {
        "Do not include a scope; use '<type>: <subject>' when using conventional commits."
    } else {
        "Include a scope only when it improves clarity."
    };

    let context = if context.trim().is_empty() {
        String::new()
    } else {
        format!(
            "Additional user context: <context>{}</context>. Use it when relevant.",
            context.trim()
        )
    };

    format!(
        "{IDENTITY}\nConvert the user's staged Git diff into one commit message.\n{convention}\n{description}\n{one_line}\n{scope}\nUse present tense. Keep lines under 74 characters. Use language: {}.\n{context}",
        config.language
    )
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
        let prompt = system_prompt(&config, false, "issue 123");
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
