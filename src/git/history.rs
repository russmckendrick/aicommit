use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Result, bail};

use super::{
    exec::run_git_in,
    repo::{filter_ignored, repo_root},
};

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub hash: String,
    pub subject: String,
    pub body: String,
}

pub fn last_commit_files() -> Result<Vec<String>> {
    let root = repo_root()?;
    let output = run_git_in(&root, ["diff", "--name-only", "HEAD~1", "HEAD"])?;
    let files = crate::git::repo::parse_lines(&output.stdout);
    filter_ignored(&root, files)
}

pub fn last_commit_diff() -> Result<String> {
    let root = repo_root()?;
    Ok(run_git_in(&root, ["diff", "HEAD~1", "HEAD"])?.stdout)
}

pub fn last_n_commits(n: usize) -> Result<Vec<CommitInfo>> {
    let root = repo_root()?;
    let output = run_git_in(
        &root,
        [
            "log",
            &format!("-{n}"),
            "--format=%H%x00%s%x00%b%x00--AIC-END--",
        ],
    )?;
    let mut commits = parse_commit_blocks(&output.stdout);
    commits.reverse();
    Ok(commits)
}

pub fn commit_diff(hash: &str) -> Result<String> {
    let root = repo_root()?;
    let output = run_git_in(&root, ["diff", &format!("{hash}^"), hash]);
    match output {
        Ok(o) => Ok(o.stdout),
        Err(_) => Ok(run_git_in(&root, ["show", "--format=", hash])?.stdout),
    }
}

pub fn commit_files(hash: &str) -> Result<Vec<String>> {
    let root = repo_root()?;
    let output = run_git_in(
        &root,
        ["diff-tree", "--no-commit-id", "--name-only", "-r", hash],
    )?;
    Ok(crate::git::repo::parse_lines(&output.stdout))
}

pub fn assert_no_merges(n: usize) -> Result<()> {
    let root = repo_root()?;
    let output = run_git_in(&root, ["log", "--merges", "-1", &format!("HEAD~{n}..HEAD")])?;
    if !output.stdout.trim().is_empty() {
        bail!("merge commits found in the last {n} commits; aic log cannot rewrite across merges");
    }
    Ok(())
}

pub fn reword_commits(n: usize, new_messages: &[String]) -> Result<()> {
    let root = repo_root()?;
    let tmp_dir = std::env::temp_dir().join("aic-reword");
    fs::create_dir_all(&tmp_dir)?;

    let _cleanup = RewordCleanup(tmp_dir.clone());

    for (i, msg) in new_messages.iter().enumerate() {
        fs::write(tmp_dir.join(format!("{i}.txt")), msg)?;
    }
    fs::write(tmp_dir.join("counter"), "0")?;

    let tmp_dir_str = tmp_dir.display().to_string();
    let seq_editor = r#"#!/bin/sh
TODO_FILE="$1"
awk '{sub(/^pick /, "reword ")} 1' "$TODO_FILE" > "$TODO_FILE.tmp" && mv "$TODO_FILE.tmp" "$TODO_FILE"
"#;
    let msg_editor = format!(
        r#"#!/bin/sh
N=$(cat "{tmp_dir_str}/counter")
cat "{tmp_dir_str}/$N.txt" > "$1"
echo $((N + 1)) > "{tmp_dir_str}/counter"
"#
    );

    let sequence_editor_script = tmp_dir.join("sequence-editor.sh");
    write_executable_script(&sequence_editor_script, seq_editor)?;
    let editor_script = tmp_dir.join("editor.sh");
    write_executable_script(&editor_script, &msg_editor)?;

    let output = Command::new("git")
        .args(["rebase", "-i", &format!("HEAD~{n}")])
        .env(
            "GIT_SEQUENCE_EDITOR",
            git_shell_command(&sequence_editor_script),
        )
        .env("GIT_EDITOR", git_shell_command(&editor_script))
        .current_dir(&root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = Command::new("git")
            .args(["rebase", "--abort"])
            .current_dir(&root)
            .output();
        bail!("rebase failed: {stderr}");
    }

    Ok(())
}

pub(crate) fn parse_commit_blocks(stdout: &str) -> Vec<CommitInfo> {
    let mut commits = Vec::new();
    for block in stdout.split("--AIC-END--") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let parts: Vec<&str> = block.splitn(3, '\0').collect();
        if parts.len() >= 2 {
            commits.push(CommitInfo {
                hash: parts[0].trim().to_owned(),
                subject: parts[1].trim().to_owned(),
                body: parts.get(2).unwrap_or(&"").trim().to_owned(),
            });
        }
    }
    commits
}

fn write_executable_script(path: &Path, content: &str) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("aic-script");
    let temp_path = path.with_file_name(format!(".{file_name}.tmp"));
    fs::write(&temp_path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o755))?;
    }
    fs::rename(&temp_path, path)?;
    Ok(())
}

struct RewordCleanup(PathBuf);

impl Drop for RewordCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn git_shell_command(script: &Path) -> String {
    #[cfg(windows)]
    {
        let path = script.display().to_string().replace('\\', "/");
        format!("sh \"{path}\"")
    }

    #[cfg(not(windows))]
    {
        shell_quote(&script.display().to_string())
    }
}

#[cfg(not(windows))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}
