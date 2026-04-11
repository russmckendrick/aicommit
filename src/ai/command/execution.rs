use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;

use crate::{
    ai::{AiEngine, ChatMessage},
    config::Config,
    prompt::sanitize_model_output,
};

use super::path::resolve_program_path;

#[derive(Debug, Clone)]
pub struct CommandEngine {
    pub(super) config: Config,
    pub(super) program: String,
    pub(super) args: Vec<String>,
    pub(super) cwd: PathBuf,
}

impl CommandEngine {
    fn render_prompt(messages: &[ChatMessage]) -> String {
        let mut prompt = String::from(
            "Return only the assistant reply for the final user message. Do not add commentary about your process.\n",
        );

        for message in messages {
            prompt.push_str("\n<message role=\"");
            prompt.push_str(&message.role);
            prompt.push_str("\">\n");
            prompt.push_str(&message.content);
            if !message.content.ends_with('\n') {
                prompt.push('\n');
            }
            prompt.push_str("</message>\n");
        }

        prompt
    }

    fn resolved_program(&self) -> Option<PathBuf> {
        resolve_program_path(&self.program)
    }
}

#[async_trait]
impl AiEngine for CommandEngine {
    async fn generate_commit_message(&self, messages: &[ChatMessage]) -> Result<String> {
        let prompt = Self::render_prompt(messages);
        let program = self
            .resolved_program()
            .unwrap_or_else(|| PathBuf::from(&self.program));
        let mut child = Command::new(&program)
            .args(&self.args)
            .current_dir(&self.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => anyhow::anyhow!(
                    "{} provider requires `{}` on PATH",
                    self.provider_label(),
                    self.program
                ),
                _ => anyhow::anyhow!(
                    "failed to start {} provider via `{}`: {error}",
                    self.provider_label(),
                    self.binary_hint()
                ),
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .with_context(|| format!("failed to write prompt to `{}`", self.binary_hint()))?;
        }

        let output = child.wait_with_output().with_context(|| {
            format!(
                "failed to read output from {} provider via `{}`",
                self.provider_label(),
                self.binary_hint()
            )
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let detail = if stderr.is_empty() {
                format!("exit status {}", output.status)
            } else {
                stderr
            };
            bail!(
                "{} provider failed via `{}`: {}",
                self.provider_label(),
                self.binary_hint(),
                detail
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let content = sanitize_model_output(&stdout);
        if content.is_empty() {
            bail!(
                "{} provider returned an empty response via `{}`",
                self.provider_label(),
                self.binary_hint()
            );
        }

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Write, path::Path};

    use tempfile::TempDir;

    use super::*;

    struct TestCommand {
        program: String,
        args: Vec<String>,
    }

    fn test_messages() -> Vec<ChatMessage> {
        vec![ChatMessage::user("diff --git a/src/lib.rs b/src/lib.rs")]
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

    fn install_test_command(
        dir: &Path,
        name: &str,
        stdout: &str,
        stderr: &str,
        exit_code: i32,
    ) -> TestCommand {
        #[cfg(unix)]
        let path = dir.join(name);
        #[cfg(windows)]
        let path = dir.join(format!("{name}.cmd"));

        #[cfg(unix)]
        let script = {
            let mut script = String::from("#!/bin/sh\ncat >/dev/null\n");
            if !stdout.is_empty() {
                script.push_str("cat <<'AIC_STDOUT'\n");
                script.push_str(stdout);
                if !stdout.ends_with('\n') {
                    script.push('\n');
                }
                script.push_str("AIC_STDOUT\n");
            }
            if !stderr.is_empty() {
                script.push_str("cat <<'AIC_STDERR' >&2\n");
                script.push_str(stderr);
                if !stderr.ends_with('\n') {
                    script.push('\n');
                }
                script.push_str("AIC_STDERR\n");
            }
            script.push_str(&format!("exit {exit_code}\n"));
            script
        };

        #[cfg(windows)]
        let script = {
            let mut script = String::from("@echo off\r\nmore >NUL\r\n");
            for line in stdout.lines() {
                script.push_str("echo(");
                script.push_str(&escape_cmd_echo(line));
                script.push_str("\r\n");
            }
            if stdout.ends_with('\n') {
                script.push_str("echo(\r\n");
            }
            for line in stderr.lines() {
                script.push_str(">&2 echo(");
                script.push_str(&escape_cmd_echo(line));
                script.push_str("\r\n");
            }
            if stderr.ends_with('\n') {
                script.push_str(">&2 echo(\r\n");
            }
            script.push_str(&format!("exit /b {exit_code}\r\n"));
            script
        };

        let temp_path = dir.join(format!(".{name}.tmp"));
        let mut file = std::fs::File::create(&temp_path).unwrap();
        file.write_all(script.as_bytes()).unwrap();
        file.sync_all().unwrap();
        drop(file);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&temp_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&temp_path, permissions).unwrap();
        }

        std::fs::rename(&temp_path, &path).unwrap();

        #[cfg(unix)]
        {
            TestCommand {
                program: "/bin/sh".to_owned(),
                args: vec![path.to_string_lossy().to_string()],
            }
        }

        #[cfg(windows)]
        {
            TestCommand {
                program: path.to_string_lossy().to_string(),
                args: Vec::new(),
            }
        }
    }

    #[tokio::test]
    async fn command_engine_strips_reasoning_tags() {
        let temp = TempDir::new().unwrap();
        let command = install_test_command(
            temp.path(),
            "claude-test",
            "<think>hidden</think>\nfeat: add cli\n",
            "",
            0,
        );
        let engine = CommandEngine::with_command(
            Config {
                ai_provider: "claude-code".to_owned(),
                model: "default".to_owned(),
                ..Config::default()
            },
            command.program,
            command.args,
            std::env::temp_dir(),
        );

        let response = engine
            .generate_commit_message(&test_messages())
            .await
            .unwrap();

        assert_eq!(response, "feat: add cli");
    }

    #[tokio::test]
    async fn command_engine_reports_missing_binary() {
        let engine = CommandEngine::with_command(
            Config {
                ai_provider: "codex".to_owned(),
                model: "default".to_owned(),
                ..Config::default()
            },
            "__missing_binary__",
            ["exec"],
            std::env::temp_dir(),
        );

        let error = engine
            .generate_commit_message(&test_messages())
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("codex provider requires `__missing_binary__` on PATH"));
    }

    #[tokio::test]
    async fn command_engine_reports_non_zero_exit() {
        let temp = TempDir::new().unwrap();
        let command = install_test_command(temp.path(), "claude-fail", "", "boom", 9);
        let engine = CommandEngine::with_command(
            Config {
                ai_provider: "claude-code".to_owned(),
                model: "default".to_owned(),
                ..Config::default()
            },
            command.program,
            command.args,
            std::env::temp_dir(),
        );

        let error = engine
            .generate_commit_message(&test_messages())
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("claude-code provider failed"));
        assert!(error.contains("boom"));
    }
}
