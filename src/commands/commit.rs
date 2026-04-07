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

    ui::section(format!("Staged files ({})", staged.len()));
    for file in &staged {
        ui::bullet(file);
    }

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

        ui::blank_line();
        ui::section("Generated commit message");
        ui::commit_message(&commit_message);

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
        ui::secondary(output.stdout);
    }
    if !output.stderr.is_empty() {
        ui::secondary(output.stderr);
    }

    if !config.gitpush {
        return Ok(());
    }

    let remotes = git::remote_metadata()?;
    match remotes.as_slice() {
        [] => Ok(()),
        [remote] => {
            let label = remote_display_label(remote);
            if ui::confirm(&format!("Run git push {label}?"), false)? {
                let output = git::push(Some(&remote.name))?;
                ui::success(format!("pushed to {label}"));
                if !output.stdout.is_empty() {
                    ui::secondary(output.stdout);
                }
            }
            Ok(())
        }
        remotes => {
            let remote_options = remotes
                .iter()
                .map(|remote| (remote.name.clone(), remote_display_label(remote)))
                .collect::<Vec<_>>();
            let mut options = remote_options
                .iter()
                .map(|(_, label)| label.clone())
                .collect::<Vec<_>>();
            options.push("do not push".to_owned());
            let selected = ui::select("Choose a remote to push to", options)?;
            if let Some((remote, label)) = remote_options
                .iter()
                .find(|(_, label)| label.as_str() == selected)
            {
                let output = git::push(Some(remote))?;
                ui::success(format!("pushed to {label}"));
                if !output.stdout.is_empty() {
                    ui::secondary(output.stdout);
                }
            }
            Ok(())
        }
    }
}

fn remote_display_label(remote: &git::GitRemoteMetadata) -> String {
    match (remote.provider.label(), remote.web_url.as_deref()) {
        (Some(provider), Some(url)) => format!("[{provider}] {} {url}", remote.name),
        (Some(provider), None) => format!("[{provider}] {}", remote.name),
        (None, Some(url)) => format!("{} {url}", remote.name),
        (None, None) => remote.name.clone(),
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

    #[test]
    fn formats_known_remote_with_provider_and_url() {
        let remote = git::GitRemoteMetadata {
            name: "origin".to_owned(),
            fetch_url: Some("https://github.com/russmckendrick/aicommit.git".to_owned()),
            push_url: Some("https://github.com/russmckendrick/aicommit.git".to_owned()),
            web_url: Some("https://github.com/russmckendrick/aicommit".to_owned()),
            provider: git::GitProvider::known("GitHub"),
        };

        assert_eq!(
            remote_display_label(&remote),
            "[GitHub] origin https://github.com/russmckendrick/aicommit"
        );
    }

    #[test]
    fn formats_unknown_remote_with_url_but_no_provider_label() {
        let remote = git::GitRemoteMetadata {
            name: "mirror".to_owned(),
            fetch_url: Some("https://git.example.test/team/repo.git".to_owned()),
            push_url: Some("https://git.example.test/team/repo.git".to_owned()),
            web_url: Some("https://git.example.test/team/repo".to_owned()),
            provider: git::GitProvider::unknown(),
        };

        assert_eq!(
            remote_display_label(&remote),
            "mirror https://git.example.test/team/repo"
        );
    }
}
