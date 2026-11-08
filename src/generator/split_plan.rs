use std::collections::BTreeSet;

use anyhow::{Result, bail};
use serde::Deserialize;

use crate::{
    ai::engine_from_config,
    config::Config,
    git::{StagedDiff, diff::hunk_path, is_excluded_from_diff},
    prompt::{
        SplitPlanGroup, build_split_chunk_summary_messages, build_split_plan_messages,
        build_split_synthesis_messages, sanitize_model_output,
    },
    token::{count_messages, split_diff},
};

use super::{GenerationProgress, ProgressFn, report};

const TOKEN_ADJUSTMENT: usize = 20;

#[derive(Debug, Deserialize)]
struct SplitPlanResponse {
    groups: Vec<SplitPlanResponseGroup>,
}

#[derive(Debug, Deserialize)]
struct SplitPlanResponseGroup {
    title: String,
    rationale: String,
    /// Hunk IDs (`path#n`). A bare path selects every hunk in that file.
    #[serde(default)]
    hunks: Vec<String>,
    /// Older prompt contract: whole files. Expanded to all their hunks.
    #[serde(default)]
    files: Vec<String>,
}

/// One line per staged file listing its hunk IDs, for the prompt.
pub fn hunk_outline(staged: &StagedDiff) -> Vec<String> {
    staged
        .files
        .iter()
        .map(|file| {
            let note = if is_excluded_from_diff(&file.path) {
                " (content omitted)"
            } else if file.atomic {
                " (whole file)"
            } else {
                ""
            };
            format!("{}: {}{note}", file.path, file.hunk_ids().join(", "))
        })
        .collect()
}

/// Ask the model to group the staged hunks into 2-4 commits. `staged` must
/// contain only the files that may be planned (ignored files removed).
pub async fn generate_split_plan(
    config: &Config,
    staged: &StagedDiff,
    context: &str,
    progress: Option<ProgressFn<'_>>,
) -> Result<Vec<SplitPlanGroup>> {
    let outline = hunk_outline(staged);
    let staged_files = outline.as_slice();
    let annotated = staged.annotated(|file| is_excluded_from_diff(&file.path));
    let diff = annotated.as_str();
    let prompt_tokens = count_messages(&build_split_plan_messages(
        config,
        "",
        context,
        staged_files,
    )?);
    let max_request_tokens = config
        .tokens_max_input
        .saturating_sub(config.tokens_max_output)
        .saturating_sub(prompt_tokens)
        .saturating_sub(TOKEN_ADJUSTMENT);

    report(progress, GenerationProgress::Splitting);
    let chunks = split_diff(diff, max_request_tokens.max(1))?;
    let engine = engine_from_config(config)?;

    if chunks.len() == 1 {
        report(
            progress,
            GenerationProgress::Chunk {
                current: 1,
                total: 1,
            },
        );
        let messages = build_split_plan_messages(config, &chunks[0], context, staged_files)?;
        let response = engine.complete(&messages).await?;
        return parse_split_plan_response(&response, staged);
    }

    let mut partial_summaries = Vec::with_capacity(chunks.len());
    for (index, chunk) in chunks.iter().enumerate() {
        report(
            progress,
            GenerationProgress::Chunk {
                current: index + 1,
                total: chunks.len(),
            },
        );
        let messages = build_split_chunk_summary_messages(
            config,
            chunk,
            context,
            staged_files,
            index + 1,
            chunks.len(),
        )?;
        partial_summaries.push(engine.complete(&messages).await?);
    }

    report(progress, GenerationProgress::Synthesizing);
    let synthesis_messages =
        build_split_synthesis_messages(config, &partial_summaries, context, staged_files)?;
    let response = engine.complete(&synthesis_messages).await?;
    parse_split_plan_response(&response, staged)
}

fn parse_split_plan_response(input: &str, staged: &StagedDiff) -> Result<Vec<SplitPlanGroup>> {
    let normalized = sanitize_model_output(input);
    let json = extract_json_payload(&normalized);
    let response: SplitPlanResponse = serde_json::from_str(&json)
        .map_err(|error| anyhow::anyhow!("failed to parse split plan JSON: {error}"))?;
    validate_split_plan(response.groups, staged)
}

fn extract_json_payload(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.starts_with("```") {
        let lines = trimmed.lines().collect::<Vec<_>>();
        if lines.len() >= 3 {
            return lines[1..lines.len() - 1].join("\n").trim().to_owned();
        }
    }
    trimmed.to_owned()
}

fn validate_split_plan(
    groups: Vec<SplitPlanResponseGroup>,
    staged: &StagedDiff,
) -> Result<Vec<SplitPlanGroup>> {
    if groups.len() < 2 {
        bail!("split plan must contain at least 2 groups");
    }
    if groups.len() > 4 {
        bail!("split plan must contain at most 4 groups");
    }

    let all_hunks = staged.hunk_ids();
    let known = all_hunks.iter().cloned().collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut validated = Vec::with_capacity(groups.len());

    for group in groups {
        let title = group.title.trim().to_owned();
        let rationale = group.rationale.trim().to_owned();
        if title.is_empty() {
            bail!("split plan groups must include a title");
        }
        if rationale.is_empty() {
            bail!("split plan groups must include a rationale");
        }

        let mut hunks = Vec::new();
        for reference in group.hunks.iter().chain(group.files.iter()) {
            let reference = reference.trim();
            if reference.is_empty() {
                bail!("split plan contained an empty hunk reference");
            }
            let expanded = if known.contains(reference) {
                vec![reference.to_owned()]
            } else if staged.file(reference).is_some() {
                staged.hunk_ids_for_files(&[reference.to_owned()])
            } else if staged.file(hunk_path(reference)).is_some() {
                bail!("split plan referenced unknown hunk '{reference}'");
            } else {
                bail!("split plan referenced unknown file '{reference}'");
            };
            for id in expanded {
                if !seen.insert(id.clone()) {
                    bail!("split plan referenced '{id}' more than once");
                }
                hunks.push(id);
            }
        }

        if hunks.is_empty() {
            bail!("split plan groups must not be empty");
        }

        // Keep diff order inside each group so patches apply top to bottom.
        hunks.sort_by_key(|id| all_hunks.iter().position(|known| known == id));
        let files = staged.files_for(&hunks);
        validated.push(SplitPlanGroup {
            title,
            rationale,
            files,
            hunks,
        });
    }

    if seen != known {
        let missing = known.difference(&seen).cloned().collect::<Vec<_>>();
        bail!(
            "split plan did not assign every staged hunk exactly once (missing: {})",
            missing.join(", ")
        );
    }

    Ok(validated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff::parse_diff;

    fn staged() -> StagedDiff {
        parse_diff(
            "diff --git a/src/cli.rs b/src/cli.rs
index 1..2 100644
--- a/src/cli.rs
+++ b/src/cli.rs
@@ -1,3 +1,3 @@
 a
-b
+B
 c
@@ -40,3 +40,3 @@
 x
-y
+Y
 z
diff --git a/README.md b/README.md
index 3..4 100644
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-old
+new
",
        )
    }

    #[test]
    fn split_plan_rejects_single_group() {
        let error = parse_split_plan_response(
            r#"{"groups":[{"title":"one","rationale":"one change","files":["src/cli.rs","README.md"]}]}"#,
            &staged(),
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "split plan must contain at least 2 groups"
        );
    }

    #[test]
    fn split_plan_rejects_unknown_files_and_hunks() {
        let error = parse_split_plan_response(
            r#"{"groups":[
                {"title":"one","rationale":"one change","files":["src/cli.rs","README.md"]},
                {"title":"two","rationale":"two change","files":["src/main.rs"]}
            ]}"#,
            &staged(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("unknown file"));

        let error = parse_split_plan_response(
            r#"{"groups":[
                {"title":"one","rationale":"one change","hunks":["src/cli.rs#1","src/cli.rs#9"]},
                {"title":"two","rationale":"two change","hunks":["src/cli.rs#2","README.md#1"]}
            ]}"#,
            &staged(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("unknown hunk 'src/cli.rs#9'"));
    }

    #[test]
    fn split_plan_accepts_file_groups_from_the_old_contract() {
        let groups = parse_split_plan_response(
            r#"{"groups":[
                {"title":"cli","rationale":"cli changes","files":["src/cli.rs"]},
                {"title":"docs","rationale":"docs changes","files":["README.md"]}
            ]}"#,
            &staged(),
        )
        .unwrap();

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].title, "cli");
        assert_eq!(groups[0].hunks, vec!["src/cli.rs#1", "src/cli.rs#2"]);
        assert_eq!(groups[1].files, vec!["README.md".to_owned()]);
    }

    #[test]
    fn split_plan_splits_one_file_across_groups() {
        let groups = parse_split_plan_response(
            r#"{"groups":[
                {"title":"rename","rationale":"rename flag","hunks":["src/cli.rs#2"]},
                {"title":"docs","rationale":"docs and help","hunks":["README.md#1","src/cli.rs#1"]}
            ]}"#,
            &staged(),
        )
        .unwrap();

        assert_eq!(groups[0].hunks, vec!["src/cli.rs#2"]);
        assert_eq!(groups[1].hunks, vec!["src/cli.rs#1", "README.md#1"]);
        assert_eq!(groups[1].files, vec!["src/cli.rs", "README.md"]);
    }

    #[test]
    fn split_plan_requires_every_hunk() {
        let error = parse_split_plan_response(
            r#"{"groups":[
                {"title":"one","rationale":"a","hunks":["src/cli.rs#1"]},
                {"title":"two","rationale":"b","hunks":["README.md#1"]}
            ]}"#,
            &staged(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("missing: src/cli.rs#2"));
    }

    #[test]
    fn hunk_outline_lists_ids_per_file() {
        assert_eq!(
            hunk_outline(&staged()),
            vec![
                "src/cli.rs: src/cli.rs#1, src/cli.rs#2",
                "README.md: README.md#1"
            ]
        );
    }
}
