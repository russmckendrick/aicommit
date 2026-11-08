//! Quiet building blocks for creating and pushing commits. They return data
//! instead of printing, so the terminal UI can run them in the background
//! and show the results itself, while the prompt flow prints them through
//! `ui` as before.

use anyhow::Result;

use crate::{config::Config, git, prompt::SplitPlanGroup};

use super::{
    filtered_extra_args,
    push::{PushPlan, PushRemoteOption, build_push_plan},
};

/// One commit `aic` created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRecord {
    pub hash: String,
    pub short_hash: String,
    pub branch: Option<String>,
    pub message: String,
    pub files: Vec<String>,
    pub insertions: Option<u64>,
    pub deletions: Option<u64>,
    /// Set when the commit worked but recording it in history failed.
    pub history_error: Option<String>,
}

impl CommitRecord {
    pub fn subject(&self) -> &str {
        self.message.lines().next().unwrap_or(&self.message)
    }

    pub fn diffstat(&self) -> Option<String> {
        format_diffstat(self.insertions, self.deletions)
    }
}

/// "+40 −2", "+1", or "−5".
pub(crate) fn format_diffstat(insertions: Option<u64>, deletions: Option<u64>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(insertions) = insertions {
        parts.push(format!("+{insertions}"));
    }
    if let Some(deletions) = deletions {
        parts.push(format!("−{deletions}"));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// Counts from git commit's " 3 files changed, 40 insertions(+), 2 deletions(-)".
pub(crate) fn parse_diffstat(commit_stdout: &str) -> (Option<u64>, Option<u64>) {
    let Some(stat_line) = commit_stdout.lines().find(|line| line.contains(" changed")) else {
        return (None, None);
    };
    let count_before = |marker: &str| -> Option<u64> {
        let end = stat_line.find(marker)?;
        stat_line[..end]
            .rsplit(|c: char| !c.is_ascii_digit())
            .find(|part| !part.is_empty())?
            .parse()
            .ok()
    };
    (count_before(" insertion"), count_before(" deletion"))
}

pub(crate) fn record_history(config: &Config, message: &str, files: &[String]) -> Option<String> {
    if config.ai_provider == "test" {
        return None;
    }
    crate::history_store::append_entry(&crate::history_store::HistoryEntry {
        timestamp: crate::history_store::now_iso8601(),
        kind: "commit".to_owned(),
        message: message.to_owned(),
        repo_path: git::repo_root()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        files: files.to_vec(),
        provider: config.ai_provider.clone(),
        model: config.model.clone(),
    })
    .err()
    .map(|error| error.to_string())
}

/// Commit the current index with `message` and record it in history.
pub fn commit_staged(
    config: &Config,
    message: &str,
    extra_args: &[String],
    files: &[String],
) -> Result<CommitRecord> {
    let output = git::commit(message, &filtered_extra_args(config, extra_args))?;
    let (insertions, deletions) = parse_diffstat(&output.stdout);
    Ok(CommitRecord {
        hash: git::head_hash().unwrap_or_default(),
        short_hash: git::head_short_hash().unwrap_or_default(),
        branch: git::current_branch(),
        message: message.to_owned(),
        files: files.to_vec(),
        insertions,
        deletions,
        history_error: record_history(config, message, files),
    })
}

/// Create one commit per group from an index snapshot (see
/// `split::create_split_commits`). `on_commit(index, total, record)` runs
/// after each commit. The index always ends as the snapshot.
pub fn commit_split_groups(
    config: &Config,
    staged: &git::StagedDiff,
    groups: &[(SplitPlanGroup, String)],
    extra_args: &[String],
    mut on_commit: impl FnMut(usize, usize, &CommitRecord),
) -> Result<Vec<CommitRecord>> {
    let snapshot = git::write_tree()?;
    let mut records = Vec::with_capacity(groups.len());
    let mut result = Ok(());
    for (index, (group, message)) in groups.iter().enumerate() {
        let attempt = git::reset_index_to_head()
            .and_then(|_| git::apply_to_index(&staged.patch_for(&group.hunks)))
            .and_then(|_| commit_staged(config, message, extra_args, &group.files));
        match attempt {
            Ok(record) => {
                on_commit(index, groups.len(), &record);
                records.push(record);
            }
            Err(error) => {
                result = Err(if index == 0 {
                    error
                } else {
                    anyhow::anyhow!(
                        "split commit {} failed after {} earlier split commits were created; the remaining changes are still staged: {error}",
                        index + 1,
                        index
                    )
                });
                break;
            }
        }
    }
    let restored = git::read_tree(&snapshot);
    result?;
    restored.map_err(|error| {
        anyhow::anyhow!(
            "split commits were created, but restoring the index snapshot {snapshot} failed: {error}"
        )
    })?;
    Ok(records)
}

/// Remotes the user may push to after committing; empty when pushing is
/// turned off (`AIC_GITPUSH=false`) or the repository has no remotes.
pub fn push_targets(config: &Config) -> Result<Vec<PushRemoteOption>> {
    Ok(
        match build_push_plan(
            config.gitpush,
            false,
            &git::remote_metadata()?,
            &config.remote_icon_style,
        )? {
            PushPlan::Skip => Vec::new(),
            PushPlan::AutoPush(remote) | PushPlan::ConfirmSingle { remote } => vec![remote],
            PushPlan::SelectRemote(remotes) => remotes,
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushResult {
    Pushed,
    /// The remote has commits this branch lacks. `can_rebase` is true when
    /// the branch is behind and the working tree is clean enough for
    /// `git pull --rebase`.
    Rejected {
        message: String,
        can_rebase: bool,
    },
}

pub(crate) fn is_sync_push_rejection(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("failed to push some refs")
        && (lower.contains("fetch first")
            || lower.contains("non-fast-forward")
            || lower.contains("rejected"))
}

/// Push the current branch to `remote`.
pub fn push_branch(remote: &str) -> Result<PushResult> {
    match git::push(Some(remote)) {
        Ok(_) => Ok(PushResult::Pushed),
        Err(error) => {
            let message = error.to_string();
            if !is_sync_push_rejection(&message) {
                return Err(error);
            }
            let can_rebase = git::fetch_sync_snapshot()
                .map(|snapshot| snapshot.behind > 0)
                .unwrap_or(false)
                && git::is_clean_for_pull_rebase().unwrap_or(false);
            Ok(PushResult::Rejected {
                message,
                can_rebase,
            })
        }
    }
}

/// `git pull --rebase`, then push again.
pub fn rebase_and_push(remote: &str) -> Result<PushResult> {
    match git::pull_rebase()? {
        git::PullRebaseOutcome::Clean(_) => push_branch(remote),
        git::PullRebaseOutcome::Conflicted { output } => Err(anyhow::anyhow!(
            "the rebase stopped with conflicts; resolve them, run `git rebase --continue`, then push\n{output}"
        )),
        git::PullRebaseOutcome::Failed { output } => {
            Err(anyhow::anyhow!("git pull --rebase failed\n{output}"))
        }
    }
}

/// After a rebase rewrote the session's commits (the last `records.len()`
/// commits on the branch), point the records at their new hashes.
pub fn refresh_hashes(records: &mut [CommitRecord]) {
    if let Ok(hashes) = git::recent_hashes(records.len())
        && hashes.len() == records.len()
    {
        for (record, (full, short)) in records.iter_mut().zip(hashes) {
            record.hash = full;
            record.short_hash = short;
        }
    }
}

/// The commit's page on the remote's web host, when it has one.
pub fn commit_url(remote: &PushRemoteOption, hash: &str) -> Option<String> {
    remote
        .commit_url_base
        .as_ref()
        .map(|base| format!("{base}/{hash}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_diffstat_counts() {
        assert_eq!(
            parse_diffstat(" 3 files changed, 40 insertions(+), 2 deletions(-)"),
            (Some(40), Some(2))
        );
        assert_eq!(
            parse_diffstat(" 1 file changed, 1 insertion(+)"),
            (Some(1), None)
        );
        assert_eq!(parse_diffstat("nothing"), (None, None));
    }

    #[test]
    fn recognises_push_rejections() {
        assert!(is_sync_push_rejection(
            "! [rejected] main -> main (fetch first)\nerror: failed to push some refs"
        ));
        assert!(!is_sync_push_rejection("fatal: could not read from remote"));
    }
}
