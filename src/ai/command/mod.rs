use std::{env, path::PathBuf};

use anyhow::{Result, bail};

use crate::{config::Config, git};

mod execution;
mod path;

pub use execution::CommandEngine;

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
}
