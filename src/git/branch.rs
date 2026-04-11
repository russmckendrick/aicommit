use std::{
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Result, bail};

use super::{
    exec::run_git_in,
    history::{CommitInfo, parse_commit_blocks},
    repo::{filter_ignored, parse_lines, repo_root},
};

pub fn current_branch() -> Option<String> {
    let root = repo_root().ok()?;
    current_branch_in(&root)
}

pub fn resolve_base_ref(explicit_base: Option<&str>) -> Result<String> {
    let root = repo_root()?;
    resolve_base_ref_in(&root, explicit_base)
}

pub fn merge_base_with_head(base_ref: &str) -> Result<String> {
    let root = repo_root()?;
    merge_base_with_head_in(&root, base_ref)
}

pub fn commits_since(base_ref: &str) -> Result<Vec<CommitInfo>> {
    let root = repo_root()?;
    commits_since_in(&root, base_ref)
}

pub fn diff_since(base_ref: &str) -> Result<String> {
    let root = repo_root()?;
    diff_since_in(&root, base_ref)
}

pub fn files_since(base_ref: &str) -> Result<Vec<String>> {
    let root = repo_root()?;
    files_since_in(&root, base_ref)
}

pub fn ticket_from_branch() -> Option<String> {
    let branch = current_branch()?;
    extract_ticket(&branch)
}

fn current_branch_in(root: &Path) -> Option<String> {
    run_git_in(root, ["rev-parse", "--abbrev-ref", "HEAD"])
        .ok()
        .map(|output| output.stdout)
        .filter(|branch| !branch.is_empty() && branch != "HEAD")
}

pub(crate) fn extract_ticket(branch: &str) -> Option<String> {
    let mut start = None;
    let chars: Vec<char> = branch.chars().collect();
    for i in 0..chars.len() {
        let c = chars[i];
        if c.is_ascii_uppercase() {
            if start.is_none() {
                start = Some(i);
            }
        } else if c == '-' {
            if let Some(s) = start
                && i > s
            {
                let digit_start = i + 1;
                let mut digit_end = digit_start;
                while digit_end < chars.len() && chars[digit_end].is_ascii_digit() {
                    digit_end += 1;
                }
                if digit_end > digit_start {
                    let ticket: String = chars[s..digit_end].iter().collect();
                    return Some(ticket);
                }
            }
            start = None;
        } else {
            start = None;
        }
    }

    if let Some(pos) = branch.find('#') {
        let rest = &branch[pos + 1..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            return Some(format!("#{digits}"));
        }
    }

    None
}

pub(crate) fn resolve_base_ref_in(root: &Path, explicit_base: Option<&str>) -> Result<String> {
    if let Some(base_ref) = explicit_base {
        if git_ref_exists_in(root, base_ref) {
            return Ok(base_ref.to_owned());
        }
        bail!("base branch '{base_ref}' was not found; pass an existing ref to --base");
    }

    let remote_default = remote_default_branch_in(root);
    if let Some(base_ref) = choose_default_base_ref(remote_default.as_deref(), |candidate| {
        git_ref_exists_in(root, candidate)
    }) {
        return Ok(base_ref);
    }

    bail!("could not determine a base branch; pass --base <branch-or-ref>")
}

pub(crate) fn merge_base_with_head_in(root: &Path, base_ref: &str) -> Result<String> {
    Ok(run_git_in(root, ["merge-base", base_ref, "HEAD"])?.stdout)
}

pub(crate) fn commits_since_in(root: &Path, base_ref: &str) -> Result<Vec<CommitInfo>> {
    let merge_base = merge_base_with_head_in(root, base_ref)?;
    let output = run_git_in(
        root,
        [
            "log",
            "--format=%H%x00%s%x00%b%x00--AIC-END--",
            &format!("{merge_base}..HEAD"),
        ],
    )?;

    let mut commits = parse_commit_blocks(&output.stdout);
    commits.reverse();
    Ok(commits)
}

pub(crate) fn diff_since_in(root: &Path, base_ref: &str) -> Result<String> {
    let merge_base = merge_base_with_head_in(root, base_ref)?;
    Ok(run_git_in(root, ["diff", &format!("{merge_base}..HEAD")])?.stdout)
}

pub(crate) fn files_since_in(root: &Path, base_ref: &str) -> Result<Vec<String>> {
    let merge_base = merge_base_with_head_in(root, base_ref)?;
    let output = run_git_in(
        root,
        ["diff", "--name-only", &format!("{merge_base}..HEAD")],
    )?;
    let files = parse_lines(&output.stdout);
    filter_ignored(root, files)
}

fn remote_default_branch_in(root: &Path) -> Option<String> {
    run_git_in(
        root,
        ["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
    )
    .ok()
    .map(|output| output.stdout.trim().to_owned())
    .filter(|value| !value.is_empty())
}

pub(crate) fn choose_default_base_ref<F>(
    remote_default: Option<&str>,
    ref_exists: F,
) -> Option<String>
where
    F: Fn(&str) -> bool,
{
    remote_default
        .into_iter()
        .chain(["origin/main", "origin/master", "main", "master"])
        .find(|candidate| ref_exists(candidate))
        .map(str::to_owned)
}

fn git_ref_exists_in(root: &Path, ref_name: &str) -> bool {
    Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{ref_name}^{{commit}}"),
        ])
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsStr, path::Path, process::Command};

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn extracts_jira_ticket_from_branch() {
        assert_eq!(
            extract_ticket("feature/PROJ-123-add-auth"),
            Some("PROJ-123".to_owned())
        );
    }

    #[test]
    fn extracts_ticket_at_start_of_branch() {
        assert_eq!(
            extract_ticket("FEAT-42-new-button"),
            Some("FEAT-42".to_owned())
        );
    }

    #[test]
    fn extracts_github_issue_ref_from_branch() {
        assert_eq!(extract_ticket("fix-#456-typo"), Some("#456".to_owned()));
    }

    #[test]
    fn returns_none_when_no_ticket_in_branch() {
        assert_eq!(extract_ticket("main"), None);
        assert_eq!(extract_ticket("feature/add-auth"), None);
    }

    #[test]
    fn choose_default_base_ref_prefers_remote_head_then_fallbacks() {
        let chosen = choose_default_base_ref(Some("refs/remotes/origin/dev"), |candidate| {
            matches!(
                candidate,
                "refs/remotes/origin/dev" | "origin/main" | "main"
            )
        });
        assert_eq!(chosen, Some("refs/remotes/origin/dev".to_owned()));

        let chosen = choose_default_base_ref(Some("refs/remotes/origin/dev"), |candidate| {
            matches!(candidate, "origin/main" | "main")
        });
        assert_eq!(chosen, Some("origin/main".to_owned()));
    }

    #[test]
    fn choose_default_base_ref_returns_none_when_no_candidates_exist() {
        assert_eq!(choose_default_base_ref(None, |_| false), None);
    }

    #[test]
    fn pr_range_helpers_use_merge_base_range() {
        let repo = init_repo();
        run_git_test(repo.path(), ["checkout", "-b", "feature/pr-draft"]);
        std::fs::write(repo.path().join("src.txt"), "base\nfeature\n").unwrap();
        run_git_test(repo.path(), ["add", "src.txt"]);
        run_git_test(repo.path(), ["commit", "-m", "feat(cli): add PR flow"]);

        let base = resolve_base_ref_in(repo.path(), None).unwrap();
        assert_eq!(base, "main");

        let merge_base = merge_base_with_head_in(repo.path(), "main").unwrap();
        assert!(!merge_base.is_empty());

        let commits = commits_since_in(repo.path(), "main").unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].subject, "feat(cli): add PR flow");

        let diff = diff_since_in(repo.path(), "main").unwrap();
        assert!(diff.contains("+feature"));

        let files = files_since_in(repo.path(), "main").unwrap();
        assert_eq!(files, vec!["src.txt".to_owned()]);
    }

    #[test]
    fn resolve_base_ref_in_reports_missing_explicit_base() {
        let repo = init_repo();
        let error = resolve_base_ref_in(repo.path(), Some("origin/nope")).unwrap_err();
        assert!(error.to_string().contains("pass an existing ref to --base"));
    }

    #[test]
    fn pr_range_helpers_return_empty_when_head_matches_base() {
        let repo = init_repo();
        let commits = commits_since_in(repo.path(), "main").unwrap();
        assert!(commits.is_empty());
        assert!(diff_since_in(repo.path(), "main").unwrap().is_empty());
        assert!(files_since_in(repo.path(), "main").unwrap().is_empty());
    }

    fn init_repo() -> TempDir {
        let temp = TempDir::new().unwrap();
        run_git_test(temp.path(), ["init", "-b", "main"]);
        run_git_test(temp.path(), ["config", "user.email", "test@example.com"]);
        run_git_test(temp.path(), ["config", "user.name", "Test User"]);
        std::fs::write(temp.path().join("src.txt"), "base\n").unwrap();
        run_git_test(temp.path(), ["add", "src.txt"]);
        run_git_test(temp.path(), ["commit", "-m", "feat: initial"]);
        temp
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
}
