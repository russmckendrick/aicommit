use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use ignore::gitignore::GitignoreBuilder;

use crate::{config::REPO_IGNORE_FILE, errors::AicError};

#[derive(Debug, Clone)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
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

pub fn commit(message: &str, extra_args: &[String]) -> Result<GitOutput> {
    let root = repo_root()?;
    let mut args = vec!["commit".to_owned(), "-m".to_owned(), message.to_owned()];
    args.extend(extra_args.iter().cloned());
    run_git_dynamic_in(&root, args)
}

pub fn remotes() -> Result<Vec<String>> {
    Ok(parse_lines(&run_git(["remote"])?.stdout))
}

pub fn push(remote: Option<&str>) -> Result<GitOutput> {
    let root = repo_root()?;
    let mut args = vec!["push".to_owned()];
    if let Some(remote) = remote {
        args.push(remote.to_owned());
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

fn parse_lines(input: &str) -> Vec<String> {
    input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
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
        "#!/bin/sh\nexec \"{}\" __hook-run \"$@\"\n",
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
    if !content.contains(&binary_path.display().to_string()) || !content.contains("__hook-run") {
        bail!("prepare-commit-msg already exists and is not managed by aicommit");
    }

    fs::remove_file(&hook_path)?;
    Ok(Some(hook_path))
}
