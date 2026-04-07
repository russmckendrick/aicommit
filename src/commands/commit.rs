use anyhow::{Result, bail};

use crate::{config::Config, errors::AicError, generator, git, ui};

pub async fn run(
    extra_args: Vec<String>,
    context: String,
    full_gitmoji_spec: bool,
    skip_confirmation: bool,
) -> Result<()> {
    git::assert_git_repo()?;
    let config = Config::load()?;

    if config.provider_needs_api_key() && config.api_key.is_none() {
        bail!(AicError::MissingApiKey(config.ai_provider));
    }

    ensure_staged_files().await?;
    let staged = git::staged_files()?;
    if staged.is_empty() {
        bail!(AicError::NoChanges);
    }

    ui::info(format!(
        "{} staged file(s):\n{}",
        staged.len(),
        staged
            .iter()
            .map(|file| format!("  {file}"))
            .collect::<Vec<_>>()
            .join("\n")
    ));

    let diff = git::staged_diff(&staged)?;
    if diff.trim().is_empty() {
        bail!("no diff content available after applying ignore and binary filters");
    }

    generate_confirm_and_commit(
        &config,
        &diff,
        &extra_args,
        &context,
        full_gitmoji_spec,
        skip_confirmation,
    )
    .await
}

pub async fn ensure_staged_files() -> Result<()> {
    let staged = git::staged_files()?;
    let changed = git::changed_files()?;

    if changed.is_empty() && staged.is_empty() {
        bail!(AicError::NoChanges);
    }

    if !staged.is_empty() {
        return Ok(());
    }

    if ui::confirm("No files are staged. Stage all files and continue?", true)? {
        git::add_files(&changed)?;
        return Ok(());
    }

    let files = ui::multiselect("Select files to stage", changed)?;
    if files.is_empty() {
        bail!("no files selected");
    }
    git::add_files(&files)?;
    Ok(())
}

async fn generate_confirm_and_commit(
    config: &Config,
    diff: &str,
    extra_args: &[String],
    context: &str,
    full_gitmoji_spec: bool,
    skip_confirmation: bool,
) -> Result<()> {
    loop {
        let spinner = ui::spinner("Generating commit message");
        let commit_message =
            generator::generate_commit_message(config, diff, full_gitmoji_spec, context).await;
        spinner.finish_and_clear();

        let mut commit_message = commit_message?;
        commit_message = apply_message_template(config, extra_args, &commit_message);

        ui::info(format!(
            "Generated commit message:\n------------------------------\n{commit_message}\n------------------------------"
        ));

        let action = if skip_confirmation {
            "Yes".to_owned()
        } else {
            ui::select(
                "Confirm the commit message?",
                vec!["Yes".to_owned(), "No".to_owned(), "Edit".to_owned()],
            )?
        };

        match action.as_str() {
            "Yes" => return commit_and_maybe_push(config, &commit_message, extra_args),
            "Edit" => {
                let edited = ui::text("Edit commit message", Some(&commit_message))?;
                return commit_and_maybe_push(config, &edited, extra_args);
            }
            "No" if ui::confirm("Regenerate the message?", true)? => continue,
            _ => bail!("commit aborted"),
        }
    }
}

fn apply_message_template(config: &Config, extra_args: &[String], message: &str) -> String {
    extra_args
        .iter()
        .find(|arg| arg.contains(&config.message_template_placeholder))
        .map(|template| template.replace(&config.message_template_placeholder, message))
        .unwrap_or_else(|| message.to_owned())
}

fn filtered_extra_args(config: &Config, extra_args: &[String]) -> Vec<String> {
    extra_args
        .iter()
        .filter(|arg| !arg.contains(&config.message_template_placeholder))
        .cloned()
        .collect()
}

fn commit_and_maybe_push(config: &Config, message: &str, extra_args: &[String]) -> Result<()> {
    let output = git::commit(message, &filtered_extra_args(config, extra_args))?;
    ui::success("committed changes");
    if !output.stdout.is_empty() {
        ui::info(output.stdout);
    }
    if !output.stderr.is_empty() {
        ui::info(output.stderr);
    }

    if !config.gitpush {
        return Ok(());
    }

    let remotes = git::remotes()?;
    match remotes.as_slice() {
        [] => Ok(()),
        [remote] => {
            if ui::confirm(&format!("Run git push {remote}?"), false)? {
                let output = git::push(Some(remote))?;
                ui::success(format!("pushed to {remote}"));
                if !output.stdout.is_empty() {
                    ui::info(output.stdout);
                }
            }
            Ok(())
        }
        remotes => {
            let mut options = remotes.to_vec();
            options.push("do not push".to_owned());
            let selected = ui::select("Choose a remote to push to", options)?;
            if selected != "do not push" {
                let output = git::push(Some(&selected))?;
                ui::success(format!("pushed to {selected}"));
                if !output.stdout.is_empty() {
                    ui::info(output.stdout);
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_message_template() {
        let config = Config::default();
        let result =
            apply_message_template(&config, &["issue-123: $msg".to_owned()], "feat: add cli");
        assert_eq!(result, "issue-123: feat: add cli");
    }
}
