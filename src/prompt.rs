use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context, Result, bail};

use crate::{ai::ChatMessage, config::Config, git::CommitInfo};

const DEFAULT_SYSTEM_PROMPT: &str = include_str!("../prompts/commit-system.md");
const DEFAULT_REVIEW_PROMPT: &str = include_str!("../prompts/review-system.md");
const DEFAULT_PR_PROMPT: &str = include_str!("../prompts/pr-system.md");
const DEFAULT_SPLIT_PROMPT: &str = include_str!("../prompts/split-system.md");
const SHORT_GITMOJI_HELP: &str = "If an emoji is useful, use only one GitMoji prefix: 🐛 fix, ✨ feature, 📝 docs, 🚀 deploy, ✅ tests, ♻️ refactor, ⬆️ dependencies, 🔧 config, 🌐 localization, or 💡 comments.";
const FULL_GITMOJI_HELP: &str = "If an emoji is useful, use one GitMoji prefix that best matches the whole change. Prefer the official intent of each emoji; never stack multiple emojis.";
const PR_SUMMARY_PROMPT: &str = "You are helping draft a pull request from one chunk of a larger cumulative diff.\n\nOutput contract:\n- Return 2-5 terse Markdown bullet points.\n- Summarize concrete branch-level changes visible in this chunk only.\n- Do not write a pull request title.\n- Do not write headings, prose paragraphs, or testing notes.\n\nLanguage:\nUse {{language}}.\n\n{{context_instruction}}";
const SPLIT_SUMMARY_PROMPT: &str = "You are helping split one staged change set into multiple commits.\n\nOutput contract:\n- Return 2-5 terse Markdown bullet points.
- Summarize distinct change concerns visible in this diff chunk only.
- Mention concrete files or subsystems when they help separate concerns.
- Do not return JSON, commit messages, headings, or prose paragraphs.
\nLanguage:\nUse {{language}}.\n\n{{context_instruction}}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestDraft {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitPlanGroup {
    pub title: String,
    pub rationale: String,
    pub files: Vec<String>,
}

pub fn build_messages(
    config: &Config,
    diff: &str,
    full_gitmoji_spec: bool,
    context: &str,
    staged_files: &[String],
) -> Result<Vec<ChatMessage>> {
    let mut messages = initial_messages(config, full_gitmoji_spec, context, staged_files)?;
    messages.push(ChatMessage::user(diff));
    Ok(messages)
}

pub fn initial_messages(
    config: &Config,
    full_gitmoji_spec: bool,
    context: &str,
    staged_files: &[String],
) -> Result<Vec<ChatMessage>> {
    Ok(vec![
        ChatMessage::system(system_prompt(
            config,
            full_gitmoji_spec,
            context,
            staged_files,
        )?),
        ChatMessage::user(example_diff()),
        ChatMessage::assistant(example_commit(config)),
    ])
}

pub fn system_prompt(
    config: &Config,
    full_gitmoji_spec: bool,
    context: &str,
    staged_files: &[String],
) -> Result<String> {
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
            .to_owned()
    } else {
        let base = "Use at most one scope, and only when it clarifies the single overall change.";
        let hints = detect_scope_hints(staged_files);
        if hints.is_empty() {
            base.to_owned()
        } else {
            format!(
                "{base} Likely scopes based on changed files: {}.",
                hints.join(", ")
            )
        }
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
        .replace("{{scope_instruction}}", &scope_instruction)
        .replace("{{style_examples}}", &style_examples(config))
        .replace("{{language}}", &config.language)
        .replace("{{context_instruction}}", &context_instruction))
}

pub fn detect_scope_hints(files: &[String]) -> Vec<String> {
    let mut scopes = BTreeSet::new();

    for file in files {
        let path = Path::new(file);
        let components: Vec<_> = path.iter().filter_map(|c| c.to_str()).collect();

        let scope = match components.as_slice() {
            ["Cargo.toml"] | ["Cargo.lock"] => Some("deps"),
            ["docs", ..] | ["README.md"] | ["CLAUDE.md"] | ["AGENTS.md"] => Some("docs"),
            ["tests", ..] => Some("test"),
            ["prompts", ..] => Some("prompt"),
            [".github", ..] => Some("ci"),
            ["src", "ai", ..] => Some("ai"),
            ["src", "commands", ..] => Some("cli"),
            ["src", file] => {
                let stem = Path::new(file)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                match stem {
                    "config" => Some("config"),
                    "git" => Some("git"),
                    "prompt" => Some("prompt"),
                    "ui" => Some("ui"),
                    "token" => Some("token"),
                    "generator" => Some("generator"),
                    "errors" => Some("errors"),
                    _ => None,
                }
            }
            _ => None,
        };

        if let Some(s) = scope {
            scopes.insert(s.to_owned());
        }
    }

    scopes.into_iter().take(5).collect()
}

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

pub fn build_split_plan_messages(
    config: &Config,
    diff: &str,
    context: &str,
    staged_files: &[String],
) -> Result<Vec<ChatMessage>> {
    Ok(vec![
        ChatMessage::system(split_system_prompt(config, context)?),
        ChatMessage::user(split_diff_input(diff, context, staged_files)),
    ])
}

pub fn build_split_chunk_summary_messages(
    config: &Config,
    diff: &str,
    context: &str,
    staged_files: &[String],
    chunk_index: usize,
    chunk_total: usize,
) -> Result<Vec<ChatMessage>> {
    let summary_context = if context.trim().is_empty() {
        format!(
            "This is diff chunk {} of {} for one staged change set.",
            chunk_index, chunk_total
        )
    } else {
        format!(
            "Additional user context: <context>{}</context>. This is diff chunk {} of {} for one staged change set.",
            context.trim(),
            chunk_index,
            chunk_total
        )
    };

    Ok(vec![
        ChatMessage::system(
            SPLIT_SUMMARY_PROMPT
                .replace("{{language}}", &config.language)
                .replace("{{context_instruction}}", &summary_context),
        ),
        ChatMessage::user(split_diff_input(diff, context, staged_files)),
    ])
}

pub fn build_split_synthesis_messages(
    config: &Config,
    partial_summaries: &[String],
    context: &str,
    staged_files: &[String],
) -> Result<Vec<ChatMessage>> {
    Ok(vec![
        ChatMessage::system(split_system_prompt(config, context)?),
        ChatMessage::user(split_summary_input(
            partial_summaries,
            context,
            staged_files,
        )),
    ])
}

pub fn split_system_prompt(config: &Config, context: &str) -> Result<String> {
    let context_instruction = if context.trim().is_empty() {
        String::new()
    } else {
        format!(
            "Additional user context: <context>{}</context>. Use it when relevant.",
            context.trim()
        )
    };

    Ok(DEFAULT_SPLIT_PROMPT
        .replace("{{language}}", &config.language)
        .replace("{{context_instruction}}", &context_instruction))
}

#[allow(clippy::too_many_arguments)]
pub fn build_pr_messages(
    config: &Config,
    diff: &str,
    context: &str,
    base_ref: &str,
    branch_name: Option<&str>,
    ticket: Option<&str>,
    commits: &[CommitInfo],
    changed_files: &[String],
) -> Result<Vec<ChatMessage>> {
    Ok(vec![
        ChatMessage::system(pr_system_prompt(config, context)?),
        ChatMessage::user(pr_diff_input(
            diff,
            context,
            base_ref,
            branch_name,
            ticket,
            commits,
            changed_files,
        )),
    ])
}

#[allow(clippy::too_many_arguments)]
pub fn build_pr_chunk_summary_messages(
    config: &Config,
    diff: &str,
    context: &str,
    base_ref: &str,
    branch_name: Option<&str>,
    ticket: Option<&str>,
    commits: &[CommitInfo],
    changed_files: &[String],
    chunk_index: usize,
    chunk_total: usize,
) -> Result<Vec<ChatMessage>> {
    let summary_context = if context.trim().is_empty() {
        format!(
            "This is diff chunk {} of {} for one branch-level change set.",
            chunk_index, chunk_total
        )
    } else {
        format!(
            "Additional user context: <context>{}</context>. This is diff chunk {} of {} for one branch-level change set.",
            context.trim(),
            chunk_index,
            chunk_total
        )
    };

    Ok(vec![
        ChatMessage::system(
            PR_SUMMARY_PROMPT
                .replace("{{language}}", &config.language)
                .replace("{{context_instruction}}", &summary_context),
        ),
        ChatMessage::user(pr_diff_input(
            diff,
            context,
            base_ref,
            branch_name,
            ticket,
            commits,
            changed_files,
        )),
    ])
}

#[allow(clippy::too_many_arguments)]
pub fn build_pr_synthesis_messages(
    config: &Config,
    partial_summaries: &[String],
    context: &str,
    base_ref: &str,
    branch_name: Option<&str>,
    ticket: Option<&str>,
    commits: &[CommitInfo],
    changed_files: &[String],
) -> Result<Vec<ChatMessage>> {
    Ok(vec![
        ChatMessage::system(pr_system_prompt(config, context)?),
        ChatMessage::user(pr_summary_input(
            partial_summaries,
            context,
            base_ref,
            branch_name,
            ticket,
            commits,
            changed_files,
        )),
    ])
}

pub fn pr_system_prompt(config: &Config, context: &str) -> Result<String> {
    let context_instruction = if context.trim().is_empty() {
        String::new()
    } else {
        format!(
            "Additional PR context: <context>{}</context>. Use it when relevant.",
            context.trim()
        )
    };

    Ok(DEFAULT_PR_PROMPT
        .replace("{{language}}", &config.language)
        .replace("{{context_instruction}}", &context_instruction))
}

pub fn parse_pull_request_response(input: &str) -> Result<PullRequestDraft> {
    let lines: Vec<&str> = input.lines().collect();
    let Some(title_index) = lines.iter().position(|line| !line.trim().is_empty()) else {
        bail!("AI provider returned an empty PR title");
    };

    let title = lines[title_index].trim().to_owned();
    if title.is_empty() {
        bail!("AI provider returned an empty PR title");
    }

    let body = lines[title_index + 1..].join("\n").trim().to_owned();
    Ok(PullRequestDraft { title, body })
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

#[allow(clippy::too_many_arguments)]
fn pr_diff_input(
    diff: &str,
    context: &str,
    base_ref: &str,
    branch_name: Option<&str>,
    ticket: Option<&str>,
    commits: &[CommitInfo],
    changed_files: &[String],
) -> String {
    let metadata = pr_metadata(
        context,
        base_ref,
        branch_name,
        ticket,
        commits,
        changed_files,
    );
    format!("{metadata}\n\nDiff against base:\n```diff\n{diff}\n```")
}

#[allow(clippy::too_many_arguments)]
fn pr_summary_input(
    partial_summaries: &[String],
    context: &str,
    base_ref: &str,
    branch_name: Option<&str>,
    ticket: Option<&str>,
    commits: &[CommitInfo],
    changed_files: &[String],
) -> String {
    let metadata = pr_metadata(
        context,
        base_ref,
        branch_name,
        ticket,
        commits,
        changed_files,
    );
    let summaries = partial_summaries
        .iter()
        .enumerate()
        .map(|(index, summary)| format!("Chunk {}:\n{}", index + 1, summary.trim()))
        .collect::<Vec<_>>()
        .join("\n\n");

    format!("{metadata}\n\nPartial summaries from cumulative PR diff:\n{summaries}")
}

fn pr_metadata(
    context: &str,
    base_ref: &str,
    branch_name: Option<&str>,
    ticket: Option<&str>,
    commits: &[CommitInfo],
    changed_files: &[String],
) -> String {
    let branch_line = branch_name.unwrap_or("detached HEAD");
    let ticket_line = ticket.unwrap_or("none detected");
    let file_lines = if changed_files.is_empty() {
        "- none".to_owned()
    } else {
        changed_files
            .iter()
            .map(|file| format!("- {file}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let commit_lines = if commits.is_empty() {
        "- none".to_owned()
    } else {
        commits
            .iter()
            .map(|commit| format!("- {}", commit.subject))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let context_line = if context.trim().is_empty() {
        "none".to_owned()
    } else {
        context.trim().to_owned()
    };

    format!(
        "Base ref: {base_ref}\nCurrent branch: {branch_line}\nDetected ticket: {ticket_line}\nUser context: {context_line}\nChanged files:\n{file_lines}\nCommits since base:\n{commit_lines}"
    )
}

fn split_diff_input(diff: &str, context: &str, staged_files: &[String]) -> String {
    let metadata = split_metadata(context, staged_files);
    format!("{metadata}\n\nStaged diff:\n```diff\n{diff}\n```")
}

fn split_summary_input(
    partial_summaries: &[String],
    context: &str,
    staged_files: &[String],
) -> String {
    let metadata = split_metadata(context, staged_files);
    let summaries = partial_summaries
        .iter()
        .enumerate()
        .map(|(index, summary)| format!("Chunk {}:\n{}", index + 1, summary.trim()))
        .collect::<Vec<_>>()
        .join("\n\n");

    format!("{metadata}\n\nPartial summaries from a large staged diff:\n{summaries}")
}

fn split_metadata(context: &str, staged_files: &[String]) -> String {
    let file_lines = if staged_files.is_empty() {
        "- none".to_owned()
    } else {
        staged_files
            .iter()
            .map(|file| format!("- {file}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let context_line = if context.trim().is_empty() {
        "none".to_owned()
    } else {
        context.trim().to_owned()
    };

    format!("User context: {context_line}\nStaged files:\n{file_lines}")
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

pub fn sanitize_model_output(input: &str) -> String {
    let mut output = input.trim().to_owned();
    for tag in ["think", "thinking"] {
        output = remove_content_tags(&output, tag);
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
        let prompt = system_prompt(&config, false, "issue 123", &[]).unwrap();
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

    #[test]
    fn sanitize_model_output_removes_known_reasoning_tags() {
        assert_eq!(
            sanitize_model_output("  <thinking>hidden</thinking>\nfeat: add cli  "),
            "feat: add cli"
        );
    }

    #[test]
    fn scope_hints_detects_known_directories() {
        let files = vec![
            "src/ai/openai_compat.rs".to_owned(),
            "src/ai/mod.rs".to_owned(),
        ];
        assert_eq!(detect_scope_hints(&files), vec!["ai"]);
    }

    #[test]
    fn scope_hints_detects_multiple_scopes() {
        let files = vec![
            "src/commands/commit.rs".to_owned(),
            "src/git.rs".to_owned(),
            "Cargo.toml".to_owned(),
        ];
        let hints = detect_scope_hints(&files);
        assert_eq!(hints, vec!["cli", "deps", "git"]);
    }

    #[test]
    fn scope_hints_caps_at_five() {
        let files = vec![
            "src/ai/mod.rs".to_owned(),
            "src/commands/commit.rs".to_owned(),
            "src/config.rs".to_owned(),
            "src/git.rs".to_owned(),
            "src/token.rs".to_owned(),
            "src/ui.rs".to_owned(),
            "docs/roadmap.md".to_owned(),
        ];
        assert_eq!(detect_scope_hints(&files).len(), 5);
    }

    #[test]
    fn scope_hints_empty_for_no_files() {
        assert!(detect_scope_hints(&[]).is_empty());
    }

    #[test]
    fn scope_hints_appear_in_prompt_when_not_omitted() {
        let config = Config::default();
        assert!(!config.omit_scope);
        let files = vec!["src/git.rs".to_owned()];
        let prompt = system_prompt(&config, false, "", &files).unwrap();
        assert!(prompt.contains("Likely scopes based on changed files: git."));
    }

    #[test]
    fn scope_hints_absent_when_omit_scope() {
        let config = Config {
            omit_scope: true,
            ..Config::default()
        };
        let files = vec!["src/git.rs".to_owned()];
        let prompt = system_prompt(&config, false, "", &files).unwrap();
        assert!(!prompt.contains("Likely scopes"));
    }

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
    }

    #[test]
    fn pr_prompt_includes_context() {
        let config = Config::default();
        let prompt = pr_system_prompt(&config, "mention rollout risk").unwrap();
        assert!(prompt.contains("mention rollout risk"));
        assert!(prompt.contains("pull request"));
    }

    #[test]
    fn parse_pull_request_response_reads_title_and_body() {
        let draft = parse_pull_request_response(
            "\nfeat(cli): generate PR text\n\n## Summary\n- Add a new command\n",
        )
        .unwrap();
        assert_eq!(draft.title, "feat(cli): generate PR text");
        assert_eq!(draft.body, "## Summary\n- Add a new command");
    }

    #[test]
    fn parse_pull_request_response_allows_empty_body() {
        let draft = parse_pull_request_response("feat(cli): generate PR text").unwrap();
        assert_eq!(draft.title, "feat(cli): generate PR text");
        assert!(draft.body.is_empty());
    }

    #[test]
    fn parse_pull_request_response_rejects_empty_title() {
        let error = parse_pull_request_response(" \n\t ").unwrap_err();
        assert!(error.to_string().contains("empty PR title"));
    }
}
