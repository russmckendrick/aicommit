use std::{env, ffi::OsString, fs, path::Path, process::Command};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::json;
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

fn write_history(home: &Path, entries: &[serde_json::Value]) {
    fs::write(
        home.join(".aicommit-history.json"),
        serde_json::to_string_pretty(entries).unwrap(),
    )
    .unwrap();
}

fn history_command(repo: &Path, home: &Path) -> Command {
    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo)
        .env("HOME", home)
        .env("USERPROFILE", home);
    cmd
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
fn top_level_help_describes_all_visible_commands() {
    let repo = init_repo();

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "config       Read and update global aic configuration",
        ))
        .stdout(predicate::str::contains(
            "setup        Run interactive provider and model setup",
        ))
        .stdout(predicate::str::contains(
            "models       List available models for the configured provider",
        ))
        .stdout(predicate::str::contains(
            "hook         Manage the Git commit-msg hook",
        ))
        .stdout(predicate::str::contains(
            "pr           Generate a pull request title and description",
        ))
        .stdout(predicate::str::contains(
            "Arguments passed through to git commit",
        ));
}

#[test]
fn config_help_describes_nested_subcommands() {
    let repo = init_repo();

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .args(["config", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "set       Set one or more global config values",
        ))
        .stdout(predicate::str::contains(
            "get       Print one or more resolved config values",
        ))
        .stdout(predicate::str::contains(
            "describe  Explain supported config keys",
        ));
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
fn pr_honors_codex_provider_override_and_writes_history() {
    let repo = init_repo();
    let home = TempDir::new().unwrap();
    let bin_dir = TempDir::new().unwrap();
    install_fake_binary(
        bin_dir.path(),
        "codex",
        "feat(cli): generate pull request drafts\n\n## Summary\n- Add a local `aic pr` workflow\n\n## Testing\n- cargo test",
    );

    fs::write(repo.path().join("src.txt"), "hello\n").unwrap();
    run_git(repo.path(), ["add", "src.txt"]);
    run_git(repo.path(), ["commit", "-m", "chore: initial"]);

    fs::write(repo.path().join("src.txt"), "hello\nfeature\n").unwrap();
    run_git(repo.path(), ["add", "src.txt"]);
    run_git(repo.path(), ["commit", "-m", "feat(cli): add PR workflow"]);

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("AIC_AI_PROVIDER", "openai")
        .env("PATH", path_with_fake_bin(bin_dir.path()))
        .arg("pr")
        .arg("--base")
        .arg("HEAD~1")
        .arg("--yes")
        .arg("--provider")
        .arg("codex")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "feat(cli): generate pull request drafts",
        ))
        .stdout(predicate::str::contains("## Summary"))
        .stdout(predicate::str::contains("generated pull request draft"));

    let history = fs::read_to_string(home.path().join(".aicommit-history.json")).unwrap();
    assert!(history.contains("\"kind\": \"pr\""));
    assert!(history.contains("feat(cli): generate pull request drafts"));
}

#[test]
fn pr_honors_claude_code_provider_override() {
    let repo = init_repo();
    let bin_dir = TempDir::new().unwrap();
    install_fake_binary(
        bin_dir.path(),
        "claude",
        "feat(cli): describe branch changes\n\n## Summary\n- Summarize the feature branch\n\n## Testing\n- Not run",
    );

    fs::write(repo.path().join("src.txt"), "hello\n").unwrap();
    run_git(repo.path(), ["add", "src.txt"]);
    run_git(repo.path(), ["commit", "-m", "chore: initial"]);

    fs::write(repo.path().join("src.txt"), "hello\nfeature\n").unwrap();
    run_git(repo.path(), ["add", "src.txt"]);
    run_git(repo.path(), ["commit", "-m", "feat(cli): add PR workflow"]);

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .env("AIC_AI_PROVIDER", "openai")
        .env("PATH", path_with_fake_bin(bin_dir.path()))
        .arg("pr")
        .arg("--base")
        .arg("HEAD~1")
        .arg("--yes")
        .arg("--provider")
        .arg("claude-code")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "feat(cli): describe branch changes",
        ))
        .stdout(predicate::str::contains("## Testing"));
}

#[test]
fn pr_reports_missing_explicit_base() {
    let repo = init_repo();
    fs::write(repo.path().join("src.txt"), "hello\nfeature\n").unwrap();
    run_git(repo.path(), ["add", "src.txt"]);
    run_git(repo.path(), ["commit", "-m", "feat(cli): add PR workflow"]);

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .env("AIC_AI_PROVIDER", "test")
        .arg("pr")
        .arg("--base")
        .arg("origin/does-not-exist")
        .arg("--yes")
        .assert()
        .failure()
        .stderr(predicate::str::contains("pass an existing ref to --base"));
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

#[test]
fn models_command_supports_anthropic_provider_override() {
    let repo = init_repo();

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .env("AIC_AI_PROVIDER", "openai")
        .env("AIC_MODEL", "gpt-5.4-mini")
        .arg("models")
        .arg("--provider")
        .arg("anthropic")
        .assert()
        .success()
        .stdout(predicate::str::contains("Available models for anthropic:"))
        .stdout(predicate::str::contains("* claude-sonnet-4-20250514"))
        .stdout(predicate::str::contains("claude-opus-4-20250514"));
}

#[test]
fn models_command_supports_groq_provider_override() {
    let repo = init_repo();

    let mut cmd = Command::cargo_bin("aic").unwrap();
    cmd.current_dir(repo.path())
        .env("AIC_AI_PROVIDER", "openai")
        .env("AIC_MODEL", "gpt-5.4-mini")
        .arg("models")
        .arg("--provider")
        .arg("groq")
        .assert()
        .success()
        .stdout(predicate::str::contains("Available models for groq:"))
        .stdout(predicate::str::contains("* llama-3.1-8b-instant"))
        .stdout(predicate::str::contains("llama-3.3-70b-versatile"));
}

#[test]
fn history_hides_temp_entries_by_default_and_shows_compact_view() {
    let repo = init_repo();
    let home = TempDir::new().unwrap();
    let temp_repo = env::temp_dir().join(".tmpaic-history-noise");
    write_history(
        home.path(),
        &[
            json!({
                "timestamp": "2024-01-15T14:30:00Z",
                "kind": "commit",
                "message": "feat(history): improve timeline\n\n- add friendlier rendering",
                "repo_path": "/Users/example/Code/aicommit",
                "files": ["src/cli.rs", "src/history.rs", "README.md"],
                "provider": "openai",
                "model": "gpt-5.4-mini"
            }),
            json!({
                "timestamp": "2024-01-15T14:35:00Z",
                "kind": "commit",
                "message": "feat: hidden temp entry",
                "repo_path": temp_repo,
                "files": ["src.txt"],
                "provider": "test",
                "model": "default"
            }),
        ],
    );

    history_command(repo.path(), home.path())
        .arg("history")
        .arg("--non-interactive")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Recent history entries (1 main, 1 hidden)",
        ))
        .stdout(predicate::str::contains("Recent entries"))
        .stdout(predicate::str::contains("feat(history): improve timeline"))
        .stdout(predicate::str::contains(
            "commit | 2024-01-15 14:30 | openai/gpt-5.4-mini | aicommit",
        ))
        .stdout(predicate::str::contains(
            "src/cli.rs, src/history.rs +1 more (3 files)",
        ))
        .stdout(predicate::str::contains("hidden temp entry").not())
        .stdout(predicate::str::contains(".tmpaic-history-noise").not());
}

#[test]
fn history_all_includes_hidden_entries() {
    let repo = init_repo();
    let home = TempDir::new().unwrap();
    let temp_repo = env::temp_dir().join(".tmpaic-history-noise");
    write_history(
        home.path(),
        &[
            json!({
                "timestamp": "2024-01-15T14:30:00Z",
                "kind": "commit",
                "message": "feat(history): improve timeline",
                "repo_path": "/Users/example/Code/aicommit",
                "files": ["src/cli.rs"],
                "provider": "openai",
                "model": "gpt-5.4-mini"
            }),
            json!({
                "timestamp": "2024-01-15T14:35:00Z",
                "kind": "commit",
                "message": "feat: hidden temp entry",
                "repo_path": temp_repo,
                "files": ["src.txt"],
                "provider": "test",
                "model": "default"
            }),
        ],
    );

    history_command(repo.path(), home.path())
        .arg("history")
        .arg("--non-interactive")
        .arg("--all")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Recent history entries (1 main, 1 hidden)",
        ))
        .stdout(predicate::str::contains("Hidden test/temp entries (1)"))
        .stdout(predicate::str::contains("feat: hidden temp entry"));
}

#[test]
fn history_verbose_shows_full_message_and_repo_path() {
    let repo = init_repo();
    let home = TempDir::new().unwrap();
    write_history(
        home.path(),
        &[json!({
            "timestamp": "2024-01-15T14:30:00Z",
            "kind": "commit",
            "message": "feat(history): improve timeline\n\n- add friendlier rendering\n- keep full detail in verbose mode",
            "repo_path": "/Users/example/Code/aicommit",
            "files": ["src/commands/history.rs", "src/history.rs"],
            "provider": "openai",
            "model": "gpt-5.4-mini"
        })],
    );

    history_command(repo.path(), home.path())
        .arg("history")
        .arg("--non-interactive")
        .arg("--verbose")
        .assert()
        .success()
        .stdout(predicate::str::contains("- add friendlier rendering"))
        .stdout(predicate::str::contains(
            "repo: /Users/example/Code/aicommit",
        ))
        .stdout(predicate::str::contains("src/commands/history.rs"))
        .stdout(predicate::str::contains("src/history.rs"));
}

#[test]
fn history_kind_review_uses_compact_excerpt() {
    let repo = init_repo();
    let home = TempDir::new().unwrap();
    write_history(
        home.path(),
        &[
            json!({
                "timestamp": "2024-01-15T14:30:00Z",
                "kind": "commit",
                "message": "feat(history): improve timeline",
                "repo_path": "/Users/example/Code/aicommit",
                "files": ["src/history.rs"],
                "provider": "openai",
                "model": "gpt-5.4-mini"
            }),
            json!({
                "timestamp": "2024-01-15T14:35:00Z",
                "kind": "review",
                "message": "# Critical\n- Avoid panic in `src/lib.rs` while loading history\n- Keep compact output readable",
                "repo_path": "/Users/example/Code/aicommit",
                "files": ["src/lib.rs"],
                "provider": "codex",
                "model": "default"
            }),
        ],
    );

    history_command(repo.path(), home.path())
        .arg("history")
        .arg("--non-interactive")
        .arg("--kind")
        .arg("review")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Recent review entries (1 main, 0 hidden)",
        ))
        .stdout(predicate::str::contains(
            "Critical Avoid panic in src/lib.rs while loading history Keep compact output readable",
        ))
        .stdout(predicate::str::contains("feat(history): improve timeline").not());
}

#[test]
fn history_hides_test_provider_entries_even_when_path_is_not_temp() {
    let repo = init_repo();
    let home = TempDir::new().unwrap();
    write_history(
        home.path(),
        &[
            json!({
                "timestamp": "2024-01-15T14:30:00Z",
                "kind": "commit",
                "message": "feat: real entry",
                "repo_path": "/Users/example/Code/aicommit",
                "files": ["src/history.rs"],
                "provider": "openai",
                "model": "gpt-5.4-mini"
            }),
            json!({
                "timestamp": "2024-01-15T14:35:00Z",
                "kind": "commit",
                "message": "feat: hidden provider entry",
                "repo_path": "/Users/example/Code/not-a-temp-path",
                "files": ["src.txt"],
                "provider": "test",
                "model": "default"
            }),
        ],
    );

    history_command(repo.path(), home.path())
        .arg("history")
        .arg("--non-interactive")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Recent history entries (1 main, 1 hidden)",
        ))
        .stdout(predicate::str::contains("feat: hidden provider entry").not());
}

#[test]
fn history_hides_tmp_basename_entries_even_outside_temp_dir() {
    let repo = init_repo();
    let home = TempDir::new().unwrap();
    write_history(
        home.path(),
        &[
            json!({
                "timestamp": "2024-01-15T14:30:00Z",
                "kind": "commit",
                "message": "feat: visible entry",
                "repo_path": "/Users/example/Code/aicommit",
                "files": ["src/history.rs"],
                "provider": "openai",
                "model": "gpt-5.4-mini"
            }),
            json!({
                "timestamp": "2024-01-15T14:35:00Z",
                "kind": "review",
                "message": "P1: hidden tmp basename",
                "repo_path": "/Users/example/.tmpBROWSER123",
                "files": ["src.txt"],
                "provider": "codex",
                "model": "default"
            }),
        ],
    );

    history_command(repo.path(), home.path())
        .arg("history")
        .arg("--non-interactive")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Recent history entries (1 main, 1 hidden)",
        ))
        .stdout(predicate::str::contains("P1: hidden tmp basename").not());
}

#[test]
fn history_invalid_timestamp_falls_back_to_raw_value() {
    let repo = init_repo();
    let home = TempDir::new().unwrap();
    write_history(
        home.path(),
        &[json!({
            "timestamp": "yesterday-ish",
            "kind": "commit",
            "message": "feat(history): keep raw timestamps",
            "repo_path": "/Users/example/Code/aicommit",
            "files": ["src/history.rs"],
            "provider": "openai",
            "model": "gpt-5.4-mini"
        })],
    );

    history_command(repo.path(), home.path())
        .arg("history")
        .arg("--non-interactive")
        .assert()
        .success()
        .stdout(predicate::str::contains("yesterday-ish"));
}
