use anyhow::{Result, bail};

use crate::{ai::ChatMessage, config::Config, git::CommitInfo};

const DEFAULT_PR_PROMPT: &str = include_str!("../../prompts/pr-system.md");
const PR_SUMMARY_PROMPT: &str = "You are helping draft a pull request from one chunk of a larger cumulative diff.\n\nOutput contract:\n- Return 2-5 terse Markdown bullet points.\n- Summarize concrete branch-level changes visible in this chunk only.\n- Do not write a pull request title.\n- Do not write headings, prose paragraphs, or testing notes.\n\nLanguage:\nUse {{language}}.\n\n{{context_instruction}}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestDraft {
    pub title: String,
    pub body: String,
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

#[cfg(test)]
mod tests {
    use crate::config::Config;

    use super::*;

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
