use crate::{
    ai::engine_from_config,
    config::Config,
    git::{GitRecoveryScenario, GitSyncSnapshot, GitSyncState},
    prompt::build_git_guidance_messages,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitGuidanceRequest {
    pub scenario: GitRecoveryScenario,
    pub snapshot: GitSyncSnapshot,
    pub commit_created: bool,
    pub git_output: Option<String>,
}

impl GitGuidanceRequest {
    fn prompt_facts(&self) -> String {
        let mut lines = vec![
            format!("scenario: {}", scenario_label(self.scenario)),
            format!("branch: {}", branch_label(self.snapshot.branch.as_deref())),
            format!(
                "upstream: {}",
                self.snapshot
                    .upstream_ref
                    .as_deref()
                    .unwrap_or("none configured")
            ),
            format!(
                "remote: {}",
                self.snapshot.remote.as_deref().unwrap_or("none configured")
            ),
            format!("sync_state: {}", sync_state_label(self.snapshot.state)),
            format!("ahead: {}", self.snapshot.ahead),
            format!("behind: {}", self.snapshot.behind),
            format!("commit_created: {}", self.commit_created),
        ];

        let commands = suggested_commands(self);
        if !commands.is_empty() {
            lines.push("suggested_commands:".to_owned());
            for command in commands {
                lines.push(format!("- {command}"));
            }
        }

        if let Some(output) = self.git_output.as_deref() {
            lines.push("git_output:".to_owned());
            lines.extend(
                compact_git_output(output)
                    .into_iter()
                    .map(|line| format!("- {line}")),
            );
        }

        lines.join("\n")
    }
}

pub async fn generate_git_guidance(config: &Config, request: &GitGuidanceRequest) -> String {
    let fallback = fallback_git_guidance(request);
    let messages = match build_git_guidance_messages(config, &request.prompt_facts()) {
        Ok(messages) => messages,
        Err(_) => return fallback,
    };

    let engine = match engine_from_config(config) {
        Ok(engine) => engine,
        Err(_) => return fallback,
    };

    match engine.generate_commit_message(&messages).await {
        Ok(output) if !output.trim().is_empty() => output.trim().to_owned(),
        _ => fallback,
    }
}

pub fn fallback_git_guidance(request: &GitGuidanceRequest) -> String {
    let branch = branch_label(request.snapshot.branch.as_deref());
    let upstream = request
        .snapshot
        .upstream_ref
        .as_deref()
        .unwrap_or("its upstream");
    let summary = match request.scenario {
        GitRecoveryScenario::PreCommitBehind => format!(
            "`{branch}` is behind `{upstream}`, so `aic` stopped before creating a new commit."
        ),
        GitRecoveryScenario::PreCommitDiverged => format!(
            "`{branch}` has diverged from `{upstream}`, so `aic` stopped before creating a new commit."
        ),
        GitRecoveryScenario::PushRejected => {
            if request.commit_created {
                format!(
                    "Your commit was created locally, but the push was rejected because `{branch}` is out of sync with `{upstream}`."
                )
            } else {
                format!(
                    "The push was rejected because `{branch}` is out of sync with `{upstream}`."
                )
            }
        }
        GitRecoveryScenario::RebaseConflict => format!(
            "`git pull --rebase` left conflicts while trying to sync `{branch}` with `{upstream}`."
        ),
        GitRecoveryScenario::RebaseFailed => {
            "The rebase step did not complete cleanly, so `aic` could not finish the sync recovery."
                .to_owned()
        }
    };

    let next_action = match request.scenario {
        GitRecoveryScenario::PreCommitBehind | GitRecoveryScenario::PreCommitDiverged => {
            "Sync the branch first, then rerun `aic`."
        }
        GitRecoveryScenario::PushRejected => {
            "Rebase your local branch onto the upstream branch before trying to push again."
        }
        GitRecoveryScenario::RebaseConflict => {
            "Resolve the conflicts, continue the rebase, then push once the branch is clean."
        }
        GitRecoveryScenario::RebaseFailed => {
            "Inspect the Git output, fix the rebase issue manually, then retry the push."
        }
    };

    let mut lines = vec![summary, String::new(), next_action.to_owned()];
    for (index, command) in suggested_commands(request).iter().enumerate() {
        lines.push(format!("{}. `{command}`", index + 1));
    }
    lines.join("\n")
}

fn suggested_commands(request: &GitGuidanceRequest) -> Vec<String> {
    match request.scenario {
        GitRecoveryScenario::PreCommitBehind | GitRecoveryScenario::PreCommitDiverged => {
            vec!["git pull --rebase".to_owned(), "aic".to_owned()]
        }
        GitRecoveryScenario::PushRejected => {
            vec!["git pull --rebase".to_owned(), "git push".to_owned()]
        }
        GitRecoveryScenario::RebaseConflict => vec![
            "git status".to_owned(),
            "git rebase --continue".to_owned(),
            "git push".to_owned(),
        ],
        GitRecoveryScenario::RebaseFailed => vec!["git status".to_owned()],
    }
}

fn branch_label(branch: Option<&str>) -> &str {
    branch.unwrap_or("the current branch")
}

fn scenario_label(scenario: GitRecoveryScenario) -> &'static str {
    match scenario {
        GitRecoveryScenario::PreCommitBehind => "pre_commit_behind",
        GitRecoveryScenario::PreCommitDiverged => "pre_commit_diverged",
        GitRecoveryScenario::PushRejected => "push_rejected",
        GitRecoveryScenario::RebaseConflict => "rebase_conflict",
        GitRecoveryScenario::RebaseFailed => "rebase_failed",
    }
}

fn sync_state_label(state: GitSyncState) -> &'static str {
    match state {
        GitSyncState::NoUpstream => "no_upstream",
        GitSyncState::UpToDate => "up_to_date",
        GitSyncState::AheadOnly => "ahead_only",
        GitSyncState::BehindOnly => "behind_only",
        GitSyncState::Diverged => "diverged",
    }
}

fn compact_git_output(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(4)
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::git::{GitRecoveryScenario, GitSyncSnapshot, GitSyncState};

    use super::*;

    fn snapshot(state: GitSyncState) -> GitSyncSnapshot {
        GitSyncSnapshot {
            branch: Some("main".to_owned()),
            upstream_ref: Some("origin/main".to_owned()),
            remote: Some("origin".to_owned()),
            ahead: 0,
            behind: 1,
            state,
        }
    }

    #[test]
    fn fallback_guidance_for_pre_commit_sync_points_back_to_aic() {
        let guidance = fallback_git_guidance(&GitGuidanceRequest {
            scenario: GitRecoveryScenario::PreCommitBehind,
            snapshot: snapshot(GitSyncState::BehindOnly),
            commit_created: false,
            git_output: None,
        });

        assert!(guidance.contains("stopped before creating a new commit"));
        assert!(guidance.contains("`git pull --rebase`"));
        assert!(guidance.contains("`aic`"));
    }

    #[test]
    fn fallback_guidance_for_push_rejection_points_to_git_push() {
        let guidance = fallback_git_guidance(&GitGuidanceRequest {
            scenario: GitRecoveryScenario::PushRejected,
            snapshot: GitSyncSnapshot {
                ahead: 1,
                behind: 1,
                state: GitSyncState::Diverged,
                ..snapshot(GitSyncState::Diverged)
            },
            commit_created: true,
            git_output: Some("failed to push".to_owned()),
        });

        assert!(guidance.contains("commit was created locally"));
        assert!(guidance.contains("`git push`"));
    }
}
