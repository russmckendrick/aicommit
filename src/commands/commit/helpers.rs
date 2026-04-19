use anyhow::Result;

use crate::{config::Config, git, history_store, ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommitInputSource {
    Diff,
    Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommitInput {
    pub source: CommitInputSource,
    pub content: String,
}

pub(crate) fn staged_commit_input(staged_files: &[String]) -> Result<CommitInput> {
    let diff = git::staged_diff(staged_files)?;
    if !diff.trim().is_empty() {
        return Ok(CommitInput {
            source: CommitInputSource::Diff,
            content: diff,
        });
    }

    Ok(CommitInput {
        source: CommitInputSource::Metadata,
        content: metadata_only_commit_input(
            "staged",
            staged_files,
            &git::staged_change_summaries(staged_files)?,
        ),
    })
}

pub(crate) fn amend_commit_input(files: &[String]) -> Result<CommitInput> {
    let diff = git::last_commit_diff()?;
    if !diff.trim().is_empty() {
        return Ok(CommitInput {
            source: CommitInputSource::Diff,
            content: diff,
        });
    }

    Ok(CommitInput {
        source: CommitInputSource::Metadata,
        content: metadata_only_commit_input(
            "committed",
            files,
            &git::last_commit_change_summaries(files)?,
        ),
    })
}

pub(crate) fn enrich_context_with_branch(context: &str) -> String {
    if let Some(ticket) = git::ticket_from_branch() {
        if context.is_empty() {
            format!("Branch references ticket {ticket}.")
        } else {
            format!("Branch references ticket {ticket}. {context}")
        }
    } else {
        context.to_owned()
    }
}

pub(crate) fn append_commit_history(config: &Config, message: &str, files: &[String]) {
    if config.ai_provider == "test" {
        return;
    }

    if let Err(e) = history_store::append_entry(&history_store::HistoryEntry {
        timestamp: history_store::now_iso8601(),
        kind: "commit".to_owned(),
        message: message.to_owned(),
        repo_path: git::repo_root()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        files: files.to_vec(),
        provider: config.ai_provider.clone(),
        model: config.model.clone(),
    }) {
        ui::warn(format!("failed to save history: {e}"));
    }
}

pub(crate) fn apply_message_template(
    config: &Config,
    extra_args: &[String],
    message: &str,
) -> String {
    extra_args
        .iter()
        .find(|arg| arg.contains(&config.message_template_placeholder))
        .map(|template| template.replace(&config.message_template_placeholder, message))
        .unwrap_or_else(|| message.to_owned())
}

pub(crate) fn filtered_extra_args(config: &Config, extra_args: &[String]) -> Vec<String> {
    extra_args
        .iter()
        .filter(|arg| !arg.contains(&config.message_template_placeholder))
        .cloned()
        .collect()
}

fn metadata_only_commit_input(
    change_label: &str,
    files: &[String],
    changes: &[git::ChangeSummary],
) -> String {
    let mut lines = vec![
        "Input mode: metadata-only".to_owned(),
        "No readable textual diff is available for this change set.".to_owned(),
        "Infer a safe high-level commit message from filenames, change types, and user context only."
            .to_owned(),
        "Do not invent hidden code, prose, or binary contents.".to_owned(),
        String::new(),
        format!("{} change metadata:", capitalize(change_label)),
    ];

    if changes.is_empty() {
        for file in files {
            lines.push(format!("- changed {}: {file}", describe_path_kind(file)));
        }
    } else {
        for change in changes {
            lines.push(format!("- {}", format_change_summary(change)));
        }
    }

    lines.join("\n")
}

fn format_change_summary(change: &git::ChangeSummary) -> String {
    let kind = describe_path_kind(&change.path);
    match change.previous_path.as_deref() {
        Some(previous_path) => format!(
            "{} {}: {} (from {})",
            change.status, kind, change.path, previous_path
        ),
        None => format!("{} {}: {}", change.status, kind, change.path),
    }
}

fn describe_path_kind(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();

    if lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".gif")
        || lower.ends_with(".avif")
        || lower.ends_with(".ico")
    {
        "image asset"
    } else if lower.ends_with(".svg") {
        "vector asset"
    } else if lower.ends_with(".lock") || lower.contains("-lock.") {
        "lockfile"
    } else if lower.ends_with(".pdf") {
        "document asset"
    } else if lower.ends_with(".zip")
        || lower.ends_with(".tar")
        || lower.ends_with(".gz")
        || lower.ends_with(".tgz")
    {
        "archive"
    } else {
        "file"
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    use super::{apply_message_template, metadata_only_commit_input};

    #[test]
    fn applies_message_template() {
        let config = Config::default();
        let result =
            apply_message_template(&config, &["issue-123: $msg".to_owned()], "feat: add cli");
        assert_eq!(result, "issue-123: feat: add cli");
    }

    #[test]
    fn metadata_only_input_describes_binary_files_without_contents() {
        let input = metadata_only_commit_input(
            "staged",
            &["assets/image.png".to_owned()],
            &[crate::git::ChangeSummary {
                status: "added".to_owned(),
                path: "assets/image.png".to_owned(),
                previous_path: None,
            }],
        );

        assert!(input.contains("Input mode: metadata-only"));
        assert!(input.contains("added image asset: assets/image.png"));
        assert!(!input.contains("diff --git"));
    }
}
