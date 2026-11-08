//! Machine-readable output for `--json` and `--quiet`.
//!
//! In either mode stdout carries only the result, so `aic` can be piped into
//! other tools or called by coding agents. The JSON shape is versioned by the
//! `schema` field; additive changes keep the number, breaking ones bump it.
//! See `docs/automation.md`.

use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    #[default]
    Human,
    Quiet,
    Json,
}

impl OutputMode {
    pub fn from_flags(json: bool, quiet: bool) -> Self {
        if json {
            Self::Json
        } else if quiet {
            Self::Quiet
        } else {
            Self::Human
        }
    }

    pub fn is_machine(self) -> bool {
        self != Self::Human
    }
}

#[derive(Debug, Serialize)]
pub struct CommitOutput {
    pub schema: u32,
    pub kind: &'static str,
    pub message: String,
    pub subject: String,
    pub body: String,
    pub files: Vec<String>,
    pub provider: String,
    pub model: String,
    pub committed: bool,
    pub commit: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReviewOutput {
    pub schema: u32,
    pub kind: &'static str,
    pub review: String,
    pub files: Vec<String>,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Serialize)]
pub struct PullRequestOutput {
    pub schema: u32,
    pub kind: &'static str,
    pub title: String,
    pub body: String,
    pub base: String,
    pub commits: usize,
    pub files: Vec<String>,
    pub provider: String,
    pub model: String,
}

/// Split a commit message into its subject line and the body after it.
pub fn split_message(message: &str) -> (String, String) {
    let message = message.trim();
    match message.split_once('\n') {
        Some((subject, body)) => (subject.trim().to_owned(), body.trim().to_owned()),
        None => (message.to_owned(), String::new()),
    }
}

/// Print `value` as JSON, or `quiet_text` in quiet mode.
pub fn emit<T: Serialize>(mode: OutputMode, value: &T, quiet_text: &str) -> anyhow::Result<()> {
    match mode {
        OutputMode::Json => println!("{}", serde_json::to_string_pretty(value)?),
        OutputMode::Quiet => println!("{}", quiet_text.trim_end()),
        OutputMode::Human => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_wins_over_quiet() {
        assert_eq!(OutputMode::from_flags(true, true), OutputMode::Json);
        assert_eq!(OutputMode::from_flags(false, true), OutputMode::Quiet);
        assert_eq!(OutputMode::from_flags(false, false), OutputMode::Human);
    }

    #[test]
    fn splits_subject_and_body() {
        assert_eq!(
            split_message("feat: add x\n\n- one\n- two\n"),
            ("feat: add x".to_owned(), "- one\n- two".to_owned())
        );
        assert_eq!(
            split_message("fix: y"),
            ("fix: y".to_owned(), String::new())
        );
    }
}
