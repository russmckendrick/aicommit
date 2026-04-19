use std::{
    path::Path,
    process::{Command, Output},
};

use anyhow::{Context, Result};

use super::{
    exec::{GitOutput, run_git_in},
    repo::repo_root,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitSyncState {
    NoUpstream,
    UpToDate,
    AheadOnly,
    BehindOnly,
    Diverged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitRecoveryScenario {
    PreCommitBehind,
    PreCommitDiverged,
    PushRejected,
    RebaseConflict,
    RebaseFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSyncSnapshot {
    pub branch: Option<String>,
    pub upstream_ref: Option<String>,
    pub remote: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub state: GitSyncState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullRebaseOutcome {
    Clean(GitOutput),
    Conflicted { output: String },
    Failed { output: String },
}

pub fn tracking_upstream() -> Result<Option<String>> {
    let root = repo_root()?;
    tracking_upstream_in(&root)
}

pub fn tracking_remote() -> Result<Option<String>> {
    let root = repo_root()?;
    tracking_remote_in(&root)
}

pub fn fetch_tracking_remote() -> Result<Option<GitOutput>> {
    let root = repo_root()?;
    fetch_tracking_remote_in(&root)
}

pub fn sync_snapshot() -> Result<GitSyncSnapshot> {
    let root = repo_root()?;
    sync_snapshot_in(&root)
}

pub fn fetch_sync_snapshot() -> Result<GitSyncSnapshot> {
    let root = repo_root()?;
    let _ = fetch_tracking_remote_in(&root)?;
    sync_snapshot_in(&root)
}

pub fn is_clean_for_pull_rebase() -> Result<bool> {
    let root = repo_root()?;
    is_clean_for_pull_rebase_in(&root)
}

pub fn has_unmerged_paths() -> Result<bool> {
    let root = repo_root()?;
    has_unmerged_paths_in(&root)
}

pub fn pull_rebase() -> Result<PullRebaseOutcome> {
    let root = repo_root()?;
    pull_rebase_in(&root)
}

pub(crate) fn tracking_upstream_in(root: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .args([
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ])
        .current_dir(root)
        .output()
        .context("failed to resolve tracking upstream")?;
    Ok(output_value(output))
}

pub(crate) fn tracking_remote_in(root: &Path) -> Result<Option<String>> {
    let Some(branch) = current_branch_in(root)? else {
        return Ok(None);
    };
    let output = Command::new("git")
        .args(["config", "--get", &format!("branch.{branch}.remote")])
        .current_dir(root)
        .output()
        .with_context(|| format!("failed to resolve tracking remote for branch {branch}"))?;
    Ok(output_value(output))
}

pub(crate) fn fetch_tracking_remote_in(root: &Path) -> Result<Option<GitOutput>> {
    let Some(remote) = tracking_remote_in(root)? else {
        return Ok(None);
    };
    Ok(Some(run_git_in(root, ["fetch", &remote])?))
}

pub(crate) fn sync_snapshot_in(root: &Path) -> Result<GitSyncSnapshot> {
    let branch = current_branch_in(root)?;
    let upstream_ref = tracking_upstream_in(root)?;
    let remote = tracking_remote_in(root)?;

    let Some(upstream_ref) = upstream_ref else {
        return Ok(GitSyncSnapshot {
            branch,
            upstream_ref: None,
            remote,
            ahead: 0,
            behind: 0,
            state: GitSyncState::NoUpstream,
        });
    };

    let counts = run_git_in(
        root,
        [
            "rev-list",
            "--left-right",
            "--count",
            &format!("HEAD...{upstream_ref}"),
        ],
    )?;
    let (ahead, behind) = parse_ahead_behind_counts(&counts.stdout)?;

    Ok(GitSyncSnapshot {
        branch,
        upstream_ref: Some(upstream_ref),
        remote,
        ahead,
        behind,
        state: classify_sync_state(ahead, behind),
    })
}

pub(crate) fn classify_sync_state(ahead: usize, behind: usize) -> GitSyncState {
    match (ahead, behind) {
        (0, 0) => GitSyncState::UpToDate,
        (0, _) => GitSyncState::BehindOnly,
        (_, 0) => GitSyncState::AheadOnly,
        _ => GitSyncState::Diverged,
    }
}

pub(crate) fn parse_ahead_behind_counts(input: &str) -> Result<(usize, usize)> {
    let mut parts = input.split_whitespace();
    let ahead = parts
        .next()
        .context("missing ahead count from git rev-list output")?
        .parse::<usize>()
        .context("invalid ahead count from git rev-list output")?;
    let behind = parts
        .next()
        .context("missing behind count from git rev-list output")?
        .parse::<usize>()
        .context("invalid behind count from git rev-list output")?;
    Ok((ahead, behind))
}

pub(crate) fn is_clean_for_pull_rebase_in(root: &Path) -> Result<bool> {
    let output = run_git_in(root, ["status", "--porcelain"])?;
    Ok(output.stdout.trim().is_empty())
}

pub(crate) fn has_unmerged_paths_in(root: &Path) -> Result<bool> {
    let output = run_git_in(root, ["diff", "--name-only", "--diff-filter=U"])?;
    Ok(!output.stdout.trim().is_empty())
}

pub(crate) fn pull_rebase_in(root: &Path) -> Result<PullRebaseOutcome> {
    let output = Command::new("git")
        .args(["pull", "--rebase"])
        .current_dir(root)
        .output()
        .context("failed to run git pull --rebase")?;

    let formatted = git_output_from_command(output);
    if formatted.2 {
        return Ok(PullRebaseOutcome::Clean(formatted.0));
    }

    let best_output = best_output(&formatted.0);
    if has_unmerged_paths_in(root)? {
        Ok(PullRebaseOutcome::Conflicted {
            output: best_output.to_owned(),
        })
    } else {
        Ok(PullRebaseOutcome::Failed {
            output: best_output.to_owned(),
        })
    }
}

fn current_branch_in(root: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(root)
        .output()
        .context("failed to resolve current branch")?;
    Ok(output_value(output))
}

fn output_value(output: Output) -> Option<String> {
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!stdout.is_empty()).then_some(stdout)
}

fn git_output_from_command(output: Output) -> (GitOutput, String, bool) {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    (
        GitOutput {
            stdout: stdout.clone(),
            stderr: stderr.clone(),
        },
        if stderr.is_empty() { stdout } else { stderr },
        output.status.success(),
    )
}

fn best_output(output: &GitOutput) -> &str {
    if output.stderr.is_empty() {
        &output.stdout
    } else {
        &output.stderr
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsStr,
        path::{Path, PathBuf},
        process::Command,
        sync::MutexGuard,
    };

    use tempfile::TempDir;

    use super::*;
    use crate::git::cwd_test_lock;

    #[test]
    fn classifies_sync_states_from_ahead_and_behind_counts() {
        assert_eq!(classify_sync_state(0, 0), GitSyncState::UpToDate);
        assert_eq!(classify_sync_state(3, 0), GitSyncState::AheadOnly);
        assert_eq!(classify_sync_state(0, 2), GitSyncState::BehindOnly);
        assert_eq!(classify_sync_state(1, 1), GitSyncState::Diverged);
    }

    #[test]
    fn parses_ahead_and_behind_counts() {
        assert_eq!(parse_ahead_behind_counts("3\t5").unwrap(), (3, 5));
        assert_eq!(parse_ahead_behind_counts("0 1").unwrap(), (0, 1));
    }

    #[test]
    fn sync_snapshot_detects_remote_ahead_after_fetch() {
        let _cwd = hold_cwd_for_test();
        let remote = init_bare_repo();
        let local = clone_repo(remote.path());
        commit_file(local.path(), "src.txt", "base\n", "feat: base");
        run_git_test(local.path(), ["push", "-u", "origin", "HEAD"]);

        let peer = clone_repo(remote.path());
        commit_file(peer.path(), "src.txt", "base\nremote\n", "feat: remote");
        run_git_test(peer.path(), ["push"]);

        let snapshot = {
            let _dir = CurrentDirGuard::enter(local.path());
            fetch_sync_snapshot().unwrap()
        };
        let branch = current_branch_name(local.path());

        assert_eq!(snapshot.branch.as_deref(), Some(branch.as_str()));
        assert_eq!(
            snapshot.upstream_ref.as_deref(),
            Some(format!("origin/{branch}").as_str())
        );
        assert_eq!(snapshot.remote.as_deref(), Some("origin"));
        assert_eq!(snapshot.ahead, 0);
        assert_eq!(snapshot.behind, 1);
        assert_eq!(snapshot.state, GitSyncState::BehindOnly);
    }

    #[test]
    fn pull_rebase_reports_conflicts() {
        let _cwd = hold_cwd_for_test();
        let remote = init_bare_repo();
        let local = clone_repo(remote.path());
        commit_file(local.path(), "src.txt", "base\n", "feat: base");
        run_git_test(local.path(), ["push", "-u", "origin", "HEAD"]);

        let peer = clone_repo(remote.path());
        commit_file(peer.path(), "src.txt", "base\nremote\n", "feat: remote");
        run_git_test(peer.path(), ["push"]);

        commit_file(local.path(), "src.txt", "base\nlocal\n", "feat: local");

        let outcome = {
            let _dir = CurrentDirGuard::enter(local.path());
            pull_rebase().unwrap()
        };

        assert!(matches!(outcome, PullRebaseOutcome::Conflicted { .. }));
    }

    fn init_bare_repo() -> TempDir {
        let temp = TempDir::new().unwrap();
        run_git_test(temp.path(), ["init", "--bare"]);
        temp
    }

    fn clone_repo(source: &Path) -> TempDir {
        let temp = TempDir::new().unwrap();
        let status = Command::new("git")
            .args([
                "clone",
                source.to_str().unwrap(),
                temp.path().to_str().unwrap(),
            ])
            .status()
            .unwrap();
        assert!(status.success());
        run_git_test(temp.path(), ["config", "user.email", "test@example.com"]);
        run_git_test(temp.path(), ["config", "user.name", "Test User"]);
        temp
    }

    fn commit_file(repo: &Path, file: &str, content: &str, message: &str) {
        std::fs::write(repo.join(file), content).unwrap();
        run_git_test(repo, ["add", file]);
        run_git_test(repo, ["commit", "-m", message]);
    }

    fn current_branch_name(repo: &Path) -> String {
        let output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn run_git_test<I, S>(cwd: &Path, args: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .unwrap();
        assert!(status.success());
    }

    fn hold_cwd_for_test() -> MutexGuard<'static, ()> {
        cwd_test_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    struct CurrentDirGuard {
        previous: PathBuf,
    }

    impl CurrentDirGuard {
        fn enter(path: &Path) -> Self {
            let previous = std::env::current_dir().unwrap();
            std::env::set_current_dir(path).unwrap();
            Self { previous }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.previous);
        }
    }
}
