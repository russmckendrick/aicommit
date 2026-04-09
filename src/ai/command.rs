use std::{
    env,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;

use crate::{
    ai::{AiEngine, ChatMessage},
    config::Config,
    git,
    prompt::sanitize_model_output,
};

#[derive(Debug, Clone)]
pub struct CommandEngine {
    config: Config,
    program: String,
    args: Vec<String>,
    cwd: PathBuf,
}

impl CommandEngine {
    pub fn new(config: Config) -> Result<Self> {
        let cwd = git::repo_root().or_else(|_| env::current_dir())?;
        match config.ai_provider.as_str() {
            "claude-code" => Ok(Self::with_command(config, "claude", ["-p"], cwd)),
            "codex" => Ok(Self::with_command(config, "codex", ["exec"], cwd)),
            unsupported => bail!("provider '{unsupported}' is not supported by the command engine"),
        }
    }

    pub(crate) fn with_command<S, I>(config: Config, program: S, args: I, cwd: PathBuf) -> Self
    where
        S: Into<String>,
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        Self {
            config,
            program: program.into(),
            args: args
                .into_iter()
                .map(|arg| arg.as_ref().to_owned())
                .collect(),
            cwd,
        }
    }

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

    fn provider_label(&self) -> &'static str {
        match self.config.ai_provider.as_str() {
            "claude-code" => "claude-code",
            "codex" => "codex",
            _ => "command provider",
        }
    }

    fn binary_hint(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
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

fn resolve_program_path(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.components().count() > 1 || path.is_absolute() {
        return path.exists().then(|| path.to_path_buf());
    }

    let path_var = env::var_os("PATH")?;
    for base in env::split_paths(&path_var) {
        for candidate in executable_candidates(&base, program) {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn executable_candidates(base: &Path, program: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let raw = base.join(program);
    candidates.push(raw.clone());

    #[cfg(windows)]
    {
        if Path::new(program).extension().is_none() {
            let pathext = env::var_os("PATHEXT")
                .unwrap_or_else(|| std::ffi::OsString::from(".COM;.EXE;.BAT;.CMD"));
            for ext in pathext
                .to_string_lossy()
                .split(';')
                .filter(|ext| !ext.is_empty())
            {
                let trimmed = ext.trim();
                let suffix = if trimmed.starts_with('.') {
                    trimmed.to_owned()
                } else {
                    format!(".{trimmed}")
                };
                candidates.push(base.join(format!("{program}{suffix}")));
            }
        }
    }

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

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

    fn install_test_binary(
        dir: &Path,
        name: &str,
        stdout: &str,
        stderr: &str,
        exit_code: i32,
    ) -> PathBuf {
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

        std::fs::write(&path, script).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&path, permissions).unwrap();
        }

        path
    }

    #[tokio::test]
    async fn command_engine_strips_reasoning_tags() {
        let temp = TempDir::new().unwrap();
        let program = install_test_binary(
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
            program.to_string_lossy().to_string(),
            std::iter::empty::<&str>(),
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
        let program = install_test_binary(temp.path(), "claude-fail", "", "boom", 9);
        let engine = CommandEngine::with_command(
            Config {
                ai_provider: "claude-code".to_owned(),
                model: "default".to_owned(),
                ..Config::default()
            },
            program.to_string_lossy().to_string(),
            std::iter::empty::<&str>(),
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
