use anyhow::{Result, bail};

use crate::{
    config::Config,
    errors::AicError,
    generator, git,
    output::{self, CommitOutput, OutputMode},
    prompt::detect_scope_hints,
    ui,
};

use self::{
    git_sync::enforce_pre_commit_sync_guard,
    helpers::{CommitInput, CommitInputSource, amend_commit_input, enrich_context_with_branch},
    split::{generate_confirm_and_commit, maybe_execute_split_flow, should_offer_split},
};

mod git_sync;
mod helpers;
mod push;
mod split;
mod staging;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    extra_args: Vec<String>,
    context: String,
    full_gitmoji_spec: bool,
    skip_confirmation: bool,
    dry_run: bool,
    amend: bool,
    provider_override: Option<String>,
    output_mode: OutputMode,
) -> Result<()> {
    git::assert_git_repo()?;
    let config = Config::load_with_provider_override(provider_override.as_deref())?;

    if config.provider_needs_api_key() && config.api_key.is_none() {
        bail!(AicError::MissingApiKey(config.ai_provider));
    }

    if output_mode.is_machine() {
        return run_machine(
            &config,
            extra_args,
            &context,
            full_gitmoji_spec,
            skip_confirmation,
            dry_run,
            amend,
            output_mode,
        )
        .await;
    }

    enforce_pre_commit_sync_guard(&config, dry_run).await?;

    ui::section(if amend {
        "Amend session"
    } else {
        "Commit session"
    });

    let (files, commit_input, files_already_listed) = if amend {
        let files = git::last_commit_files()?;
        if files.is_empty() {
            bail!("no files in the last commit to amend");
        }
        let commit_input = amend_commit_input(&files)?;
        (files, commit_input, false)
    } else {
        let staging = ensure_staged_files(skip_confirmation, true).await?;
        let staged = git::staged_files()?;
        if staged.is_empty() {
            bail!(AicError::NoChanges);
        }
        let commit_input = staged_commit_input(&staged)?;
        (staged, commit_input, staging.listed_files)
    };

    if commit_input.content.trim().is_empty() {
        bail!("no commit context available after applying ignore and binary filters");
    }

    render_commit_session(
        &config,
        &files,
        &commit_input,
        amend,
        &context,
        files_already_listed,
    );

    let mut effective_args = extra_args;
    if amend && !effective_args.iter().any(|a| a == "--amend") {
        effective_args.push("--amend".to_owned());
    }

    let context = enrich_context_with_branch(&context);

    if commit_input.source == CommitInputSource::Diff
        && should_offer_split(files.len(), skip_confirmation, dry_run, amend)
        && maybe_execute_split_flow(
            &config,
            &commit_input.content,
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
        &commit_input.content,
        &effective_args,
        &context,
        full_gitmoji_spec,
        skip_confirmation,
        dry_run,
        &files,
    )
    .await
}

/// `--json` / `--quiet`: no prompts, no split flow, and no commit unless
/// `--yes` is passed (and `--dry-run` is not). stdout carries only the result.
#[allow(clippy::too_many_arguments)]
async fn run_machine(
    config: &Config,
    mut extra_args: Vec<String>,
    context: &str,
    full_gitmoji_spec: bool,
    skip_confirmation: bool,
    dry_run: bool,
    amend: bool,
    output_mode: OutputMode,
) -> Result<()> {
    let (files, commit_input) = if amend {
        let files = git::last_commit_files()?;
        if files.is_empty() {
            bail!("no files in the last commit to amend");
        }
        let commit_input = amend_commit_input(&files)?;
        (files, commit_input)
    } else {
        if skip_confirmation {
            ensure_staged_files(true, false).await?;
        }
        let staged = git::staged_files()?;
        if staged.is_empty() {
            bail!(AicError::NoChanges);
        }
        let commit_input = staged_commit_input(&staged)?;
        (staged, commit_input)
    };

    if commit_input.content.trim().is_empty() {
        bail!("no commit context available after applying ignore and binary filters");
    }

    if amend && !extra_args.iter().any(|a| a == "--amend") {
        extra_args.push("--amend".to_owned());
    }

    let context = enrich_context_with_branch(context);
    let message = generator::generate_commit_message(
        config,
        &commit_input.content,
        full_gitmoji_spec,
        &context,
        &files,
        None,
    )
    .await?;
    let message = apply_message_template(config, &extra_args, &message);

    let should_commit = skip_confirmation && !dry_run;
    let commit = if should_commit {
        push::commit_and_maybe_push(config, &message, &extra_args, &files, true).await?;
        git::head_hash().ok()
    } else {
        None
    };

    let (subject, body) = output::split_message(&message);
    output::emit(
        output_mode,
        &CommitOutput {
            schema: output::SCHEMA_VERSION,
            kind: "commit",
            message: message.trim().to_owned(),
            subject,
            body,
            files,
            provider: config.ai_provider.clone(),
            model: config.model.clone(),
            committed: commit.is_some(),
            commit,
        },
        &message,
    )
}

fn render_commit_session(
    config: &Config,
    files: &[String],
    commit_input: &CommitInput,
    amend: bool,
    context: &str,
    files_already_listed: bool,
) {
    match commit_input.source {
        CommitInputSource::Diff => {
            let change_target = if amend {
                "last commit diff"
            } else {
                "staged diff"
            };
            ui::session_step(format!(
                "Reading {change_target} ({}, {} lines)",
                ui::file_count_label(files.len()),
                commit_input.content.lines().count()
            ));
        }
        CommitInputSource::Metadata => {
            let change_target = if amend {
                "last commit change metadata"
            } else {
                "staged change metadata"
            };
            ui::session_step(format!(
                "Reading {change_target} ({})",
                ui::file_count_label(files.len())
            ));
            ui::secondary(
                "No readable diff content was available, so aic is using file paths and Git change types instead of file contents.",
            );
        }
    }

    let mut context_items = Vec::new();
    if let Some(branch) = git::current_branch() {
        context_items.push(branch);
    }
    context_items.push(format!("{}/{}", config.ai_provider, config.model));
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
    if !files_already_listed {
        ui::file_list(
            if amend {
                "Last commit changes"
            } else {
                "Staged changes"
            },
            files,
        );
    }
}

pub(crate) use helpers::{apply_message_template, filtered_extra_args, staged_commit_input};
pub(crate) use staging::ensure_staged_files;
