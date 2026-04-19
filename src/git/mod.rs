#[cfg(test)]
pub(crate) fn cwd_test_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

mod branch;
mod exec;
mod history;
mod hooks;
mod remote;
mod repo;
pub mod stats;

pub use branch::{
    commits_since, current_branch, diff_since, files_since, head_short_hash, merge_base_with_head,
    resolve_base_ref, ticket_from_branch,
};
pub use exec::{GitOutput, run_git, run_git_in};
pub use history::{
    CommitInfo, assert_no_merges, commit_diff, commit_files, last_commit_change_summaries,
    last_commit_diff, last_commit_files, last_n_commits, reword_commits,
};
pub use hooks::{hooks_path, remove_hook_if_owned, write_hook};
pub use remote::{GitProvider, GitRemoteMetadata, commit, push, remote_metadata, remotes};
pub use repo::{
    ChangeSummary, add_files, assert_clean_worktree, assert_git_repo, changed_files, clear_index,
    partially_staged_files, repo_root, staged_change_summaries, staged_diff, staged_files,
    unstage_files,
};
