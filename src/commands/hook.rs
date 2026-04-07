use std::{env, fs, path::PathBuf};

use anyhow::Result;

use crate::{config::Config, errors::AicError, generator, git, ui};

pub fn set() -> Result<()> {
    git::assert_git_repo()?;
    let binary = current_binary()?;
    let path = git::write_hook(&binary)?;
    ui::success(format!("hook set at {}", path.display()));
    Ok(())
}

pub fn unset() -> Result<()> {
    git::assert_git_repo()?;
    let binary = current_binary()?;
    match git::remove_hook_if_owned(&binary)? {
        Some(path) => ui::success(format!("hook removed from {}", path.display())),
        None => ui::info("no aicommit hook was set"),
    }
    Ok(())
}

pub async fn run_hook(message_file: String, commit_source: Option<String>) -> Result<()> {
    if commit_source.is_some() {
        return Ok(());
    }

    let config = Config::load()?;
    if config.provider_needs_api_key() && config.api_key.is_none() {
        return Err(AicError::MissingApiKey(config.ai_provider).into());
    }

    let staged = git::staged_files()?;
    if staged.is_empty() {
        return Ok(());
    }

    let diff = git::staged_diff(&staged)?;
    if diff.trim().is_empty() {
        return Ok(());
    }

    let message = generator::generate_commit_message(&config, &diff, false, "").await?;
    let existing = fs::read_to_string(&message_file)?;
    let content = if config.hook_auto_uncomment {
        format!("{message}\n\n{existing}")
    } else {
        format!(
            "# {message}\n\n# ---------- [aicommit] ---------- #\n# Remove the leading # above to use this generated message.\n# Close the editor without saving to cancel the commit.\n\n{existing}"
        )
    };
    fs::write(message_file, content)?;
    Ok(())
}

fn current_binary() -> Result<PathBuf> {
    env::current_exe().or_else(|_| {
        env::args_os()
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("could not determine current executable"))
    })
}
