use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
};

use anyhow::{Context, Result, bail};
use ignore::gitignore::GitignoreBuilder;
use serde::Deserialize;

use crate::{config::REPO_IGNORE_FILE, errors::AicError};

#[derive(Debug, Clone)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
}

const GIT_HOSTS_TOML: &str = include_str!("git_hosts.toml");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitProvider {
    label: Option<String>,
    nerd_font_icon: Option<String>,
    emoji_icon: Option<String>,
}

impl GitProvider {
    pub fn known(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            nerd_font_icon: None,
            emoji_icon: None,
        }
    }

    pub fn known_with_icons(
        label: impl Into<String>,
        nerd_font_icon: Option<String>,
        emoji_icon: Option<String>,
    ) -> Self {
        Self {
            label: Some(label.into()),
            nerd_font_icon,
            emoji_icon,
        }
    }

    pub fn unknown() -> Self {
        Self {
            label: None,
            nerd_font_icon: None,
            emoji_icon: None,
        }
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn nerd_font_icon(&self) -> Option<&str> {
        self.nerd_font_icon.as_deref()
    }

    pub fn emoji_icon(&self) -> Option<&str> {
        self.emoji_icon.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRemoteMetadata {
    pub name: String,
    pub fetch_url: Option<String>,
    pub push_url: Option<String>,
    pub web_url: Option<String>,
    pub provider: GitProvider,
}

impl GitRemoteMetadata {
    fn from_urls(name: String, fetch_url: Option<String>, push_url: Option<String>) -> Self {
        let info = push_url
            .as_deref()
            .or(fetch_url.as_deref())
            .and_then(remote_url_info);
        let (web_url, provider) = info
            .map(|info| (info.web_url, info.provider))
            .unwrap_or((None, GitProvider::unknown()));

        Self {
            name,
            fetch_url,
            push_url,
            web_url,
            provider,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteUrlInfo {
    web_url: Option<String>,
    provider: GitProvider,
}

#[derive(Debug, Deserialize)]
struct GitHostConfig {
    #[serde(default)]
    providers: Vec<GitHostProvider>,
}

#[derive(Debug, Deserialize)]
struct GitHostProvider {
    label: String,
    nerd_font_icon: Option<String>,
    emoji_icon: Option<String>,
    #[serde(default)]
    hosts: Vec<String>,
    #[serde(default)]
    host_suffixes: Vec<String>,
    #[serde(default)]
    rewrites: Vec<GitHostRewrite>,
}

#[derive(Debug, Deserialize)]
struct GitHostRewrite {
    host: String,
    path_prefix: String,
    template: String,
}

pub fn assert_git_repo() -> Result<()> {
    run_git(["rev-parse"])?;
    Ok(())
}

pub fn repo_root() -> Result<PathBuf> {
    let output = run_git(["rev-parse", "--show-toplevel"])?;
    if output.stdout.trim().is_empty() {
        bail!(AicError::NotGitRepository);
    }
    Ok(PathBuf::from(output.stdout.trim()))
}

pub fn staged_files() -> Result<Vec<String>> {
    let root = repo_root()?;
    let output = run_git_in(&root, ["diff", "--name-only", "--cached", "--relative"])?;
    let files = parse_lines(&output.stdout);
    filter_ignored(&root, files)
}

pub fn changed_files() -> Result<Vec<String>> {
    let root = repo_root()?;
    let modified = run_git_in(&root, ["ls-files", "--modified"])?;
    let others = run_git_in(&root, ["ls-files", "--others", "--exclude-standard"])?;
    let mut files = parse_lines(&modified.stdout);
    files.extend(parse_lines(&others.stdout));
    files.sort();
    files.dedup();
    Ok(files)
}

pub fn add_files(files: &[String]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }

    let root = repo_root()?;
    let mut args = vec!["add".to_owned()];
    args.extend(files.iter().cloned());
    run_git_dynamic_in(&root, args)?;
    Ok(())
}

pub fn staged_diff(files: &[String]) -> Result<String> {
    let root = repo_root()?;
    let files = files
        .iter()
        .filter(|file| !is_excluded_from_diff(file))
        .cloned()
        .collect::<Vec<_>>();

    if files.is_empty() {
        return Ok(String::new());
    }

    let mut args = vec!["diff".to_owned(), "--staged".to_owned(), "--".to_owned()];
    args.extend(files);
    Ok(run_git_dynamic_in(&root, args)?.stdout)
}

pub fn last_commit_files() -> Result<Vec<String>> {
    let root = repo_root()?;
    let output = run_git_in(&root, ["diff", "--name-only", "HEAD~1", "HEAD"])?;
    let files = parse_lines(&output.stdout);
    filter_ignored(&root, files)
}

pub fn last_commit_diff() -> Result<String> {
    let root = repo_root()?;
    Ok(run_git_in(&root, ["diff", "HEAD~1", "HEAD"])?.stdout)
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub hash: String,
    pub subject: String,
    pub body: String,
}

/// Get the last N commits from HEAD, returned oldest-to-newest.
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

/// Get the diff for a specific commit by hash.
pub fn commit_diff(hash: &str) -> Result<String> {
    let root = repo_root()?;
    let output = run_git_in(&root, ["diff", &format!("{hash}^"), hash]);
    match output {
        Ok(o) => Ok(o.stdout),
        Err(_) => {
            // First commit in repo has no parent — use --root
            Ok(run_git_in(&root, ["show", "--format=", hash])?.stdout)
        }
    }
}

/// Get the list of files changed in a specific commit.
pub fn commit_files(hash: &str) -> Result<Vec<String>> {
    let root = repo_root()?;
    let output = run_git_in(
        &root,
        ["diff-tree", "--no-commit-id", "--name-only", "-r", hash],
    )?;
    Ok(parse_lines(&output.stdout))
}

/// Check that the working tree is clean (no uncommitted changes).
pub fn assert_clean_worktree() -> Result<()> {
    let root = repo_root()?;
    let output = run_git_in(&root, ["status", "--porcelain"])?;
    if !output.stdout.trim().is_empty() {
        bail!("working tree has uncommitted changes; commit or stash them first");
    }
    Ok(())
}

/// Check that the last N commits contain no merge commits.
pub fn assert_no_merges(n: usize) -> Result<()> {
    let root = repo_root()?;
    let output = run_git_in(&root, ["log", "--merges", "-1", &format!("HEAD~{n}..HEAD")])?;
    if !output.stdout.trim().is_empty() {
        bail!("merge commits found in the last {n} commits; aic log cannot rewrite across merges");
    }
    Ok(())
}

/// Programmatically rebase to reword the last N commits.
/// `new_messages` must be ordered oldest-to-newest.
pub fn reword_commits(n: usize, new_messages: &[String]) -> Result<()> {
    let root = repo_root()?;
    let tmp_dir = std::env::temp_dir().join("aic-reword");
    fs::create_dir_all(&tmp_dir)?;

    let _cleanup = RewordCleanup(tmp_dir.clone());

    // Write each new message to a numbered file
    for (i, msg) in new_messages.iter().enumerate() {
        fs::write(tmp_dir.join(format!("{i}.txt")), msg)?;
    }

    // Write counter file
    fs::write(tmp_dir.join("counter"), "0")?;

    let tmp_dir_str = tmp_dir.display().to_string();

    let seq_editor = r#"#!/bin/sh
TODO_FILE="$1"
awk '{sub(/^pick /, "reword ")} 1' "$TODO_FILE" > "$TODO_FILE.tmp" && mv "$TODO_FILE.tmp" "$TODO_FILE"
"#;

    // Message editor: read the next message file by counter
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
        // Abort any in-progress rebase before returning error
        let _ = Command::new("git")
            .args(["rebase", "--abort"])
            .current_dir(&root)
            .output();
        bail!("rebase failed: {stderr}");
    }

    Ok(())
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

/// Guard to clean up reword temp files on drop.
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

pub fn current_branch() -> Option<String> {
    let root = repo_root().ok()?;
    current_branch_in(&root)
}

fn current_branch_in(root: &Path) -> Option<String> {
    run_git_in(root, ["rev-parse", "--abbrev-ref", "HEAD"])
        .ok()
        .map(|output| output.stdout)
        .filter(|branch| !branch.is_empty() && branch != "HEAD")
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

fn extract_ticket(branch: &str) -> Option<String> {
    // Match JIRA-style tickets: PROJ-123
    let mut start = None;
    let chars: Vec<char> = branch.chars().collect();
    for i in 0..chars.len() {
        let c = chars[i];
        if c.is_ascii_uppercase() {
            if start.is_none() {
                start = Some(i);
            }
        } else if c == '-' {
            if let Some(s) = start {
                // Check we had at least one uppercase letter before the dash
                if i > s {
                    // Check digits follow the dash
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
            }
            start = None;
        } else {
            start = None;
        }
    }

    // Match GitHub-style issue refs: #123
    if let Some(pos) = branch.find('#') {
        let rest = &branch[pos + 1..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            return Some(format!("#{digits}"));
        }
    }

    None
}

fn resolve_base_ref_in(root: &Path, explicit_base: Option<&str>) -> Result<String> {
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

fn merge_base_with_head_in(root: &Path, base_ref: &str) -> Result<String> {
    Ok(run_git_in(root, ["merge-base", base_ref, "HEAD"])?.stdout)
}

fn commits_since_in(root: &Path, base_ref: &str) -> Result<Vec<CommitInfo>> {
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

fn diff_since_in(root: &Path, base_ref: &str) -> Result<String> {
    let merge_base = merge_base_with_head_in(root, base_ref)?;
    Ok(run_git_in(root, ["diff", &format!("{merge_base}..HEAD")])?.stdout)
}

fn files_since_in(root: &Path, base_ref: &str) -> Result<Vec<String>> {
    let merge_base = merge_base_with_head_in(root, base_ref)?;
    let output = run_git_in(
        root,
        ["diff", "--name-only", &format!("{merge_base}..HEAD")],
    )?;
    let files = parse_lines(&output.stdout);
    filter_ignored(root, files)
}

pub fn commit(message: &str, extra_args: &[String]) -> Result<GitOutput> {
    let root = repo_root()?;
    let mut args = vec!["commit".to_owned(), "-m".to_owned(), message.to_owned()];
    args.extend(extra_args.iter().cloned());
    run_git_dynamic_in(&root, args)
}

pub fn remotes() -> Result<Vec<String>> {
    Ok(parse_lines(&run_git(["remote"])?.stdout))
}

pub fn remote_metadata() -> Result<Vec<GitRemoteMetadata>> {
    let root = repo_root()?;
    let remotes = remotes()?;
    Ok(remotes
        .into_iter()
        .map(|remote| {
            let fetch_url = remote_get_url(&root, &remote, false);
            let push_url = remote_get_url(&root, &remote, true);
            GitRemoteMetadata::from_urls(remote, fetch_url, push_url)
        })
        .collect())
}

pub fn push(remote: Option<&str>) -> Result<GitOutput> {
    let root = repo_root()?;
    let mut args = vec!["push".to_owned()];
    if let Some(remote) = remote {
        if let Some(branch) = current_branch_in(&root) {
            args.push("--set-upstream".to_owned());
            args.push(remote.to_owned());
            args.push(branch);
        } else {
            args.push(remote.to_owned());
        }
    }
    run_git_dynamic_in(&root, args)
}

pub fn hooks_path() -> Result<PathBuf> {
    let root = repo_root()?;
    let configured = run_git_in(&root, ["config", "core.hooksPath"]);
    let path = match configured {
        Ok(output) if !output.stdout.trim().is_empty() => PathBuf::from(output.stdout.trim()),
        _ => root.join(".git").join("hooks"),
    };

    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(root.join(path))
    }
}

pub fn run_git<I, S>(args: I) -> Result<GitOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_git_in(std::env::current_dir()?, args)
}

pub fn run_git_in<I, S>(cwd: impl AsRef<Path>, args: I) -> Result<GitOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    command_output(output)
}

fn run_git_dynamic_in(cwd: impl AsRef<Path>, args: Vec<String>) -> Result<GitOutput> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    command_output(output)
}

fn remote_get_url(root: &Path, remote: &str, push: bool) -> Option<String> {
    let mut args = vec!["remote".to_owned(), "get-url".to_owned()];
    if push {
        args.push("--push".to_owned());
    }
    args.push(remote.to_owned());

    run_git_dynamic_in(root, args)
        .ok()
        .map(|output| output.stdout)
        .filter(|url| !url.trim().is_empty())
}

fn command_output(output: std::process::Output) -> Result<GitOutput> {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if !output.status.success() {
        bail!(
            "{}",
            if stderr.is_empty() {
                stdout.clone()
            } else {
                stderr.clone()
            }
        );
    }
    Ok(GitOutput { stdout, stderr })
}

fn parse_commit_blocks(stdout: &str) -> Vec<CommitInfo> {
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

fn parse_lines(input: &str) -> Vec<String> {
    input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn remote_url_info(url: &str) -> Option<RemoteUrlInfo> {
    let (host, path) = remote_url_host_and_path(url)?;
    let provider_config = provider_for_host(&host);
    let provider = provider_config
        .map(|provider| {
            GitProvider::known_with_icons(
                provider.label.clone(),
                provider.nerd_font_icon.clone(),
                provider.emoji_icon.clone(),
            )
        })
        .unwrap_or_else(GitProvider::unknown);
    let web_url = web_url_for_remote(&host, &path, provider_config);

    Some(RemoteUrlInfo { web_url, provider })
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

fn choose_default_base_ref<F>(remote_default: Option<&str>, ref_exists: F) -> Option<String>
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

fn remote_url_host_and_path(url: &str) -> Option<(String, String)> {
    let trimmed = url.trim().trim_end_matches('/');

    if let Some(rest) = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
    {
        return split_host_and_path(rest);
    }

    if let Some(rest) = trimmed.strip_prefix("ssh://") {
        return split_host_and_path(rest);
    }

    if !trimmed.contains("://") {
        return split_scp_style_remote(trimmed);
    }

    None
}

fn split_host_and_path(input: &str) -> Option<(String, String)> {
    let (host, path) = input.split_once('/')?;
    let host = normalize_host(host);
    let path = normalize_path(path);

    if host.is_empty() || path.is_empty() {
        return None;
    }

    Some((host, path))
}

fn split_scp_style_remote(input: &str) -> Option<(String, String)> {
    let (host, path) = input.split_once(':')?;

    if host.contains('/') || (!host.contains('@') && !host.contains('.')) {
        return None;
    }

    let host = normalize_host(host);
    let path = normalize_path(path);

    if host.is_empty() || path.is_empty() {
        return None;
    }

    Some((host, path))
}

fn normalize_host(host: &str) -> String {
    host.rsplit('@')
        .next()
        .unwrap_or(host)
        .split(':')
        .next()
        .unwrap_or(host)
        .to_lowercase()
}

fn normalize_path(path: &str) -> String {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let path = path.trim_matches('/');
    path.strip_suffix(".git").unwrap_or(path).to_owned()
}

fn provider_for_host(host: &str) -> Option<&'static GitHostProvider> {
    host_provider_config()
        .providers
        .iter()
        .find(|provider| provider.matches_host(host))
}

fn web_url_for_remote(
    host: &str,
    path: &str,
    provider: Option<&GitHostProvider>,
) -> Option<String> {
    if let Some(url) = provider.and_then(|provider| provider.rewrite_web_url(host, path)) {
        return Some(url);
    }

    provider.map(|_| format!("https://{host}/{path}"))
}

fn host_provider_config() -> &'static GitHostConfig {
    static CONFIG: OnceLock<GitHostConfig> = OnceLock::new();
    CONFIG.get_or_init(|| {
        toml_edit::de::from_str(GIT_HOSTS_TOML).expect("embedded git host config should be valid")
    })
}

impl GitHostProvider {
    fn matches_host(&self, host: &str) -> bool {
        self.hosts.iter().any(|candidate| candidate == host)
            || self
                .host_suffixes
                .iter()
                .any(|suffix| host.ends_with(suffix))
    }

    fn rewrite_web_url(&self, host: &str, path: &str) -> Option<String> {
        self.rewrites
            .iter()
            .find(|rewrite| rewrite.host == host)
            .and_then(|rewrite| rewrite.render(path))
    }
}

impl GitHostRewrite {
    fn render(&self, path: &str) -> Option<String> {
        let prefix_parts = self
            .path_prefix
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let path_parts = path.split('/').collect::<Vec<_>>();

        if path_parts.len() <= prefix_parts.len()
            || !path_parts
                .iter()
                .zip(prefix_parts.iter())
                .all(|(path, prefix)| path == prefix)
        {
            return None;
        }

        let values = &path_parts[prefix_parts.len()..];
        let mut rendered = self.template.clone();

        for (index, value) in values.iter().enumerate() {
            rendered = rendered.replace(&format!("{{{}}}", index + 1), value);
            rendered = rendered.replace(&format!("{{{}+}}", index + 1), &values[index..].join("/"));
        }

        if rendered.contains('{') || rendered.contains('}') {
            return None;
        }

        Some(rendered)
    }
}

fn filter_ignored(root: &Path, files: Vec<String>) -> Result<Vec<String>> {
    let ignore_path = root.join(REPO_IGNORE_FILE);
    if !ignore_path.exists() {
        return Ok(files);
    }

    let mut builder = GitignoreBuilder::new(root);
    builder
        .add(ignore_path)
        .context("failed to read .aicommitignore")?;
    let matcher = builder.build()?;
    Ok(files
        .into_iter()
        .filter(|file| !matcher.matched_path_or_any_parents(file, false).is_ignore())
        .collect())
}

fn is_excluded_from_diff(file: &str) -> bool {
    let lower = file.to_lowercase();
    lower.contains(".lock")
        || lower.contains("-lock.")
        || lower.ends_with(".svg")
        || lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".gif")
}

pub fn write_hook(binary_path: &Path) -> Result<PathBuf> {
    let hook_path = hooks_path()?.join("prepare-commit-msg");
    if let Some(parent) = hook_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let script = format!(
        "#!/bin/sh\nexec \"{}\" hookrun \"$@\"\n",
        binary_path.display()
    );
    fs::write(&hook_path, script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&hook_path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&hook_path, permissions)?;
    }

    Ok(hook_path)
}

pub fn remove_hook_if_owned(binary_path: &Path) -> Result<Option<PathBuf>> {
    let hook_path = hooks_path()?.join("prepare-commit-msg");
    if !hook_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&hook_path)?;
    if !content.contains(&binary_path.display().to_string()) || !content.contains("hookrun") {
        bail!("prepare-commit-msg already exists and is not managed by aic");
    }

    fs::remove_file(&hook_path)?;
    Ok(Some(hook_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn known_provider(label: &str, nerd_font_icon: &str, emoji_icon: &str) -> GitProvider {
        GitProvider::known_with_icons(
            label,
            Some(nerd_font_icon.to_owned()),
            Some(emoji_icon.to_owned()),
        )
    }

    #[test]
    fn parses_github_https_remote() {
        assert_eq!(
            remote_url_info("https://github.com/russmckendrick/aicommit.git"),
            Some(RemoteUrlInfo {
                web_url: Some("https://github.com/russmckendrick/aicommit".to_owned()),
                provider: known_provider("GitHub", "", "🐙"),
            })
        );
    }

    #[test]
    fn parses_bitbucket_ssh_remote() {
        assert_eq!(
            remote_url_info("git@bitbucket.org:workspace/project.git"),
            Some(RemoteUrlInfo {
                web_url: Some("https://bitbucket.org/workspace/project".to_owned()),
                provider: known_provider("Bitbucket", "", "🪣"),
            })
        );
    }

    #[test]
    fn parses_gitlab_ssh_remote() {
        assert_eq!(
            remote_url_info("git@gitlab.com:group/project.git"),
            Some(RemoteUrlInfo {
                web_url: Some("https://gitlab.com/group/project".to_owned()),
                provider: known_provider("GitLab", "", "🦊"),
            })
        );
    }

    #[test]
    fn parses_azure_devops_https_remote() {
        assert_eq!(
            remote_url_info("https://organization@dev.azure.com/organization/project/_git/repo"),
            Some(RemoteUrlInfo {
                web_url: Some("https://dev.azure.com/organization/project/_git/repo".to_owned()),
                provider: known_provider("Azure DevOps", "", "☁"),
            })
        );
    }

    #[test]
    fn parses_azure_devops_ssh_remote() {
        assert_eq!(
            remote_url_info("git@ssh.dev.azure.com:v3/organization/project/repo"),
            Some(RemoteUrlInfo {
                web_url: Some("https://dev.azure.com/organization/project/_git/repo".to_owned()),
                provider: known_provider("Azure DevOps", "", "☁"),
            })
        );
    }

    #[test]
    fn does_not_guess_web_url_for_unknown_scp_style_host() {
        assert_eq!(
            remote_url_info("git@example.com:team/repo.git"),
            Some(RemoteUrlInfo {
                web_url: None,
                provider: GitProvider::unknown(),
            })
        );
    }

    #[test]
    fn strips_git_suffix_from_remote_url() {
        assert_eq!(
            remote_url_info("ssh://git@github.com:22/team/repo.git"),
            Some(RemoteUrlInfo {
                web_url: Some("https://github.com/team/repo".to_owned()),
                provider: known_provider("GitHub", "", "🐙"),
            })
        );
    }

    #[test]
    fn does_not_guess_web_url_for_unknown_https_host() {
        assert_eq!(
            remote_url_info("https://git.example.test/team/repo.git"),
            Some(RemoteUrlInfo {
                web_url: None,
                provider: GitProvider::unknown(),
            })
        );
    }

    #[test]
    fn prefers_push_url_over_fetch_url_for_remote_metadata() {
        assert_eq!(
            GitRemoteMetadata::from_urls(
                "origin".to_owned(),
                Some("git@github.com:team/repo.git".to_owned()),
                Some(
                    "git@azure-devops-node4:v3/node4ltd/OCTO-FinOps/cloudmore-excel-overview"
                        .to_owned()
                ),
            ),
            GitRemoteMetadata {
                name: "origin".to_owned(),
                fetch_url: Some("git@github.com:team/repo.git".to_owned()),
                push_url: Some(
                    "git@azure-devops-node4:v3/node4ltd/OCTO-FinOps/cloudmore-excel-overview"
                        .to_owned(),
                ),
                web_url: None,
                provider: GitProvider::unknown(),
            }
        );
    }

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
    fn handles_missing_remote_urls() {
        assert_eq!(
            GitRemoteMetadata::from_urls("origin".to_owned(), None, None),
            GitRemoteMetadata {
                name: "origin".to_owned(),
                fetch_url: None,
                push_url: None,
                web_url: None,
                provider: GitProvider::unknown(),
            }
        );
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
