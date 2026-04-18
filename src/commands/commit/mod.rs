use anyhow::{Result, bail};

use crate::{config::Config, errors::AicError, git, prompt::detect_scope_hints, ui};

use self::{
    helpers::enrich_context_with_branch,
    split::{generate_confirm_and_commit, maybe_execute_split_flow, should_offer_split},
};

mod helpers;
mod push;
mod split;
mod staging;

pub async fn run(
    extra_args: Vec<String>,
    context: String,
    full_gitmoji_spec: bool,
    skip_confirmation: bool,
    dry_run: bool,
    amend: bool,
    provider_override: Option<String>,
) -> Result<()> {
    git::assert_git_repo()?;
    let config = Config::load_with_provider_override(provider_override.as_deref())?;

    if config.provider_needs_api_key() && config.api_key.is_none() {
        bail!(AicError::MissingApiKey(config.ai_provider));
    }

    let (files, diff) = if amend {
        let files = git::last_commit_files()?;
        if files.is_empty() {
            bail!("no files in the last commit to amend");
        }
        let diff = git::last_commit_diff()?;
        (files, diff)
    } else {
        ensure_staged_files(skip_confirmation).await?;
        let staged = git::staged_files()?;
        if staged.is_empty() {
            bail!(AicError::NoChanges);
        }
        let diff = git::staged_diff(&staged)?;
        (staged, diff)
    };

    if diff.trim().is_empty() {
        bail!("no diff content available after applying ignore and binary filters");
    }

    render_commit_session(&config, &files, &diff, amend, &context);

    let mut effective_args = extra_args;
    if amend && !effective_args.iter().any(|a| a == "--amend") {
        effective_args.push("--amend".to_owned());
    }

    let context = enrich_context_with_branch(&context);

    if should_offer_split(files.len(), skip_confirmation, dry_run, amend)
        && maybe_execute_split_flow(
            &config,
            &diff,
            &effective_args,
            &context,
            full_gitmoji_spec,
            &files,
        )
        .await?
    {
        return Ok(());
    }

    generate_confirm_and_commit(
        &config,
        &diff,
        &effective_args,
        &context,
        full_gitmoji_spec,
        skip_confirmation,
        dry_run,
        &files,
    )
    .await
}

fn render_commit_session(
    config: &Config,
    files: &[String],
    diff: &str,
    amend: bool,
    context: &str,
) {
    let change_target = if amend {
        "last commit diff"
    } else {
        "staged diff"
    };
    ui::section(if amend {
        "Amend session"
    } else {
        "Commit session"
    });
    ui::session_step(format!(
        "Reading {change_target} ({}, {} lines)",
        ui::file_count_label(files.len()),
        diff.lines().count()
    ));

    let mut context_items = Vec::new();
    if let Some(branch) = git::current_branch() {
        context_items.push(format!("branch: {branch}"));
    }
    if let Some(ticket) = git::ticket_from_branch() {
        context_items.push(format!("ticket: {ticket}"));
    }
    let scope_hints = detect_scope_hints(files);
    if !scope_hints.is_empty() {
        context_items.push(format!("scopes: {}", scope_hints.join(", ")));
    }
    if !context.trim().is_empty() {
        context_items.push("extra context provided".to_owned());
    }
    ui::metadata_row(&context_items);
    ui::metadata_row(&[
        format!("provider: {}", config.ai_provider),
        format!("model: {}", config.model),
    ]);
    ui::file_list(
        if amend {
            "Last commit changes"
        } else {
            "Staged changes"
        },
        files,
    );
}

pub(crate) use helpers::{apply_message_template, filtered_extra_args};
pub(crate) use staging::ensure_staged_files;
