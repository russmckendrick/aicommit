use std::{env, ffi::OsString, fs, path::Path, process::Command};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

fn init_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    run_git(temp.path(), ["init"]);
    run_git(temp.path(), ["config", "user.email", "test@example.com"]);
    run_git(temp.path(), ["config", "user.name", "Test User"]);
    temp
}

fn run_git<I, S>(cwd: &std::path::Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(status.success());
}

fn install_fake_binary(dir: &Path, name: &str, output: &str) {
    #[cfg(unix)]
    let path = dir.join(name);
    #[cfg(windows)]
    let path = dir.join(format!("{name}.cmd"));

    #[cfg(unix)]
    let script = format!("#!/bin/sh\ncat >/dev/null\ncat <<'EOF'\n{output}\nEOF\n");
    #[cfg(windows)]
    let script = {
        let mut script = String::from("@echo off\r\nmore >NUL\r\n");
        for line in output.lines() {
            script.push_str("echo(");
            script.push_str(&escape_cmd_echo(line));
            script.push_str("\r\n");
        }
        if output.ends_with('\n') {
            script.push_str("echo(\r\n");
        }
        script
    };

    fs::write(&path, script).unwrap();
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
    }
}

#[cfg(windows)]
fn escape_cmd_echo(line: &str) -> String {
    line.replace('^', "^^")
        .replace('%', "%%")
        .replace('&', "^&")
        .replace('|', "^|")
        .replace('<', "^<")
        .replace('>', "^>")
        .replace('(', "^(")
        .replace(')', "^)")
}

fn path_with_fake_bin(dir: &Path) -> OsString {
    let mut paths = vec![dir.to_path_buf()];
    if let Some(current) = env::var_os("PATH") {
        paths.extend(env::split_paths(&current));
    }
    env::join_paths(paths).unwrap()
}

#[test]
fn reports_no_changes() {
    let repo = init_repo();

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .env("AIC_AI_PROVIDER", "test")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no changes detected"));
}

#[test]
fn commits_staged_file_with_test_provider() {
    let repo = init_repo();
    fs::write(repo.path().join("src.txt"), "hello\n").unwrap();
    run_git(repo.path(), ["add", "src.txt"]);

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .env("AIC_AI_PROVIDER", "test")
        .env("AIC_GITPUSH", "false")
        .arg("--yes")
        .assert()
        .success()
        .stdout(predicate::str::contains("committed changes"));

    let output = Command::new("git")
        .args(["log", "--format=%s", "-1"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "feat: add generated commit message"
    );
}

#[test]
fn hook_run_writes_commented_message() {
    let repo = init_repo();
    fs::write(repo.path().join("src.txt"), "hello\n").unwrap();
    run_git(repo.path(), ["add", "src.txt"]);
    let message_file = repo.path().join("COMMIT_EDITMSG");
    fs::write(&message_file, "\n").unwrap();

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .env("AIC_AI_PROVIDER", "test")
        .arg("hookrun")
        .arg(&message_file)
        .assert()
        .success();

    let content = fs::read_to_string(message_file).unwrap();
    assert!(content.contains("# feat: add generated commit message"));
    assert!(content.contains("[aic]"));
}

#[test]
fn provider_override_uses_claude_code_binary() {
    let repo = init_repo();
    let bin_dir = TempDir::new().unwrap();
    install_fake_binary(
        bin_dir.path(),
        "claude",
        "<think>hidden</think>\nfeat(cli): use claude override\n\n- route commit generation through the local CLI",
    );

    fs::write(repo.path().join("src.txt"), "hello\n").unwrap();
    run_git(repo.path(), ["add", "src.txt"]);

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .env("AIC_AI_PROVIDER", "openai")
        .env("AIC_GITPUSH", "false")
        .env("PATH", path_with_fake_bin(bin_dir.path()))
        .arg("--provider")
        .arg("claude-code")
        .arg("--yes")
        .assert()
        .success()
        .stdout(predicate::str::contains("committed changes"));

    let output = Command::new("git")
        .args(["log", "--format=%B", "-1"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("feat(cli): use claude override"));
}

#[test]
fn review_honors_codex_provider_override() {
    let repo = init_repo();
    let bin_dir = TempDir::new().unwrap();
    install_fake_binary(bin_dir.path(), "codex", "P1: stub review from codex");

    fs::write(repo.path().join("src.txt"), "hello\n").unwrap();
    run_git(repo.path(), ["add", "src.txt"]);

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .env("AIC_AI_PROVIDER", "openai")
        .env("PATH", path_with_fake_bin(bin_dir.path()))
        .arg("review")
        .arg("--provider")
        .arg("codex")
        .assert()
        .success()
        .stdout(predicate::str::contains("P1: stub review from codex"));
}

#[test]
fn log_honors_codex_provider_override() {
    let repo = init_repo();
    let bin_dir = TempDir::new().unwrap();
    install_fake_binary(bin_dir.path(), "codex", "feat(log): rewrite via codex");

    fs::write(repo.path().join("src.txt"), "hello\n").unwrap();
    run_git(repo.path(), ["add", "src.txt"]);
    run_git(repo.path(), ["commit", "-m", "initial message"]);
    fs::write(repo.path().join("src.txt"), "hello again\n").unwrap();
    run_git(repo.path(), ["add", "src.txt"]);
    run_git(repo.path(), ["commit", "-m", "old message"]);

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .env("AIC_AI_PROVIDER", "openai")
        .env("PATH", path_with_fake_bin(bin_dir.path()))
        .arg("log")
        .arg("-n")
        .arg("1")
        .arg("--yes")
        .arg("--provider")
        .arg("codex")
        .assert()
        .success()
        .stdout(predicate::str::contains("rewrote 1 commit messages"));

    let output = Command::new("git")
        .args(["log", "--format=%s", "-1"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "feat(log): rewrite via codex"
    );
}

#[test]
fn models_command_shows_local_provider_note_for_override() {
    let repo = init_repo();

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .env("AIC_AI_PROVIDER", "openai")
        .arg("models")
        .arg("--provider")
        .arg("claude-code")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Available models for claude-code:",
        ))
        .stdout(predicate::str::contains("* default"))
        .stdout(predicate::str::contains("installed `claude` CLI"));
}
