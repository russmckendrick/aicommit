use std::{fs, process::Command};

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
