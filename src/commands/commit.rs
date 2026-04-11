use anyhow::{Result, bail};

use crate::{
    config::Config, errors::AicError, generator, git, history, prompt::SplitPlanGroup, ui,
};

const STAGE_ALL_FILES_OPTION: &str = "Stage all files";
const CHOOSE_FILES_OPTION: &str = "Choose files";
const CANCEL_STAGE_OPTION: &str = "Cancel";
const CREATE_ONE_COMMIT_OPTION: &str = "Create one commit";
const SPLIT_INTO_MULTIPLE_COMMITS_OPTION: &str = "Split into multiple commits";
const ABORT_OPTION: &str = "Abort";
const USE_SUGGESTED_GROUPS_OPTION: &str = "Use suggested groups";
const BUILD_GROUPS_MANUALLY_OPTION: &str = "Build groups manually";
const KEEP_ONE_COMMIT_OPTION: &str = "Keep one commit";
const CREATE_COMMITS_OPTION: &str = "Create commits";
const REGENERATE_ALL_MESSAGES_OPTION: &str = "Regenerate all messages";
const EDIT_A_MESSAGE_OPTION: &str = "Edit a message";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StagingState {
    UseExisting,
    AutoStageAll,
    PromptForSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StageSelectionAction {
    StageAll,
    ChooseFiles,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StagingPlan {
    AddFiles(Vec<String>),
    Abort,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PushRemoteOption {
    name: String,
    label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PushPlan {
    Skip,
    AutoPush(PushRemoteOption),
    ConfirmSingle {
        remote: PushRemoteOption,
        default: bool,
    },
    SelectRemote(Vec<PushRemoteOption>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SplitCommitDraft {
    group: SplitPlanGroup,
    message: String,
}

const MULTI_REMOTE_AUTO_PUSH_ERROR: &str = concat!(
    "cannot auto-push with --yes because multiple remotes are configured; ",
    "rerun without --yes to choose a remote or set AIC_GITPUSH=false"
);

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

    let label = if amend {
        "Last commit files"
    } else {
        "Staged files"
    };
    ui::section(format!("{label} ({})", files.len()));
    for file in &files {
        ui::bullet(file);
    }

    if diff.trim().is_empty() {
        bail!("no diff content available after applying ignore and binary filters");
    }

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

fn should_offer_split(
    staged_file_count: usize,
    skip_confirmation: bool,
    dry_run: bool,
    amend: bool,
) -> bool {
    staged_file_count >= 2 && !skip_confirmation && !dry_run && !amend
}

async fn maybe_execute_split_flow(
    config: &Config,
    diff: &str,
    extra_args: &[String],
    context: &str,
    full_gitmoji_spec: bool,
    staged_files: &[String],
) -> Result<bool> {
    if staged_files.len() < 2 {
        return Ok(false);
    }

    let partially_staged = git::partially_staged_files(staged_files)?;
    if !partially_staged.is_empty() {
        ui::warn(format!(
            "split flow is unavailable because these files also have unstaged changes: {}",
            partially_staged.join(", ")
        ));
        return Ok(false);
    }

    let selection = ui::select(
        "How would you like to commit these staged changes?",
        vec![
            CREATE_ONE_COMMIT_OPTION.to_owned(),
            SPLIT_INTO_MULTIPLE_COMMITS_OPTION.to_owned(),
            ABORT_OPTION.to_owned(),
        ],
    )?;

    match selection.as_str() {
        CREATE_ONE_COMMIT_OPTION => return Ok(false),
        ABORT_OPTION => bail!("commit aborted"),
        SPLIT_INTO_MULTIPLE_COMMITS_OPTION => {}
        _ => bail!("invalid split selection"),
    }

    let spinner = ui::spinner("Analyzing staged changes for split groups");
    let suggested_groups =
        generator::generate_split_plan(config, diff, context, staged_files).await;
    spinner.finish_and_clear();

    let suggested_groups = match suggested_groups {
        Ok(groups) => groups,
        Err(error) => {
            ui::warn(format!(
                "could not build a split plan; continuing with one commit: {error}"
            ));
            return Ok(false);
        }
    };

    let groups = choose_split_groups(&suggested_groups, staged_files)?;
    let Some(groups) = groups else {
        return Ok(false);
    };

    let mut drafts =
        generate_split_commit_drafts(config, &groups, context, full_gitmoji_spec, extra_args)
            .await?;

    loop {
        render_split_commit_preview(&drafts);
        let selection = ui::select(
            "What would you like to do with these split commits?",
            vec![
                CREATE_COMMITS_OPTION.to_owned(),
                REGENERATE_ALL_MESSAGES_OPTION.to_owned(),
                EDIT_A_MESSAGE_OPTION.to_owned(),
                ABORT_OPTION.to_owned(),
            ],
        )?;

        match selection.as_str() {
            CREATE_COMMITS_OPTION => {
                create_split_commits(config, &drafts, extra_args)?;
                return Ok(true);
            }
            REGENERATE_ALL_MESSAGES_OPTION => {
                drafts = generate_split_commit_drafts(
                    config,
                    &groups,
                    context,
                    full_gitmoji_spec,
                    extra_args,
                )
                .await?;
            }
            EDIT_A_MESSAGE_OPTION => edit_split_commit_message(&mut drafts)?,
            ABORT_OPTION => bail!("commit aborted"),
            _ => bail!("invalid split preview selection"),
        }
    }
}

fn enrich_context_with_branch(context: &str) -> String {
    if let Some(ticket) = git::ticket_from_branch() {
        if context.is_empty() {
            format!("Branch references ticket {ticket}.")
        } else {
            format!("Branch references ticket {ticket}. {context}")
        }
    } else {
        context.to_owned()
    }
}

pub async fn ensure_staged_files(skip_confirmation: bool) -> Result<()> {
    let staged = git::staged_files()?;
    let changed = git::changed_files()?;

    match resolve_staging_state(&staged, &changed, skip_confirmation)? {
        StagingState::UseExisting => Ok(()),
        StagingState::AutoStageAll => {
            let files = build_staging_plan(StageSelectionAction::StageAll, changed, vec![])?;
            apply_staging_plan(files)
        }
        StagingState::PromptForSelection => {
            let action = prompt_for_stage_selection()?;
            let selected_files = if action == StageSelectionAction::ChooseFiles {
                ui::multiselect("Select files to stage", changed.clone())?
            } else {
                Vec::new()
            };

            apply_staging_plan(build_staging_plan(action, changed, selected_files)?)
        }
    }
}

fn apply_staging_plan(plan: StagingPlan) -> Result<()> {
    match plan {
        StagingPlan::AddFiles(files) => {
            git::add_files(&files)?;
            Ok(())
        }
        StagingPlan::Abort => bail!("commit aborted"),
    }
}

fn resolve_staging_state(
    staged: &[String],
    changed: &[String],
    skip_confirmation: bool,
) -> Result<StagingState> {
    if changed.is_empty() && staged.is_empty() {
        bail!(AicError::NoChanges);
    }

    if !staged.is_empty() {
        Ok(StagingState::UseExisting)
    } else if skip_confirmation {
        Ok(StagingState::AutoStageAll)
    } else {
        Ok(StagingState::PromptForSelection)
    }
}

fn prompt_for_stage_selection() -> Result<StageSelectionAction> {
    let selection = ui::select(
        "No files are staged. What would you like to do?",
        vec![
            STAGE_ALL_FILES_OPTION.to_owned(),
            CHOOSE_FILES_OPTION.to_owned(),
            CANCEL_STAGE_OPTION.to_owned(),
        ],
    )?;

    match selection.as_str() {
        STAGE_ALL_FILES_OPTION => Ok(StageSelectionAction::StageAll),
        CHOOSE_FILES_OPTION => Ok(StageSelectionAction::ChooseFiles),
        CANCEL_STAGE_OPTION => Ok(StageSelectionAction::Cancel),
        _ => bail!("invalid staging selection"),
    }
}

fn build_staging_plan(
    action: StageSelectionAction,
    changed: Vec<String>,
    selected_files: Vec<String>,
) -> Result<StagingPlan> {
    match action {
        StageSelectionAction::StageAll => Ok(StagingPlan::AddFiles(changed)),
        StageSelectionAction::ChooseFiles => {
            if selected_files.is_empty() {
                bail!("no files selected");
            }
            Ok(StagingPlan::AddFiles(selected_files))
        }
        StageSelectionAction::Cancel => Ok(StagingPlan::Abort),
    }
}

fn choose_split_groups(
    suggested_groups: &[SplitPlanGroup],
    staged_files: &[String],
) -> Result<Option<Vec<SplitPlanGroup>>> {
    render_split_groups(suggested_groups, "Suggested split groups");
    let selection = ui::select(
        "How would you like to use these groups?",
        vec![
            USE_SUGGESTED_GROUPS_OPTION.to_owned(),
            BUILD_GROUPS_MANUALLY_OPTION.to_owned(),
            KEEP_ONE_COMMIT_OPTION.to_owned(),
            ABORT_OPTION.to_owned(),
        ],
    )?;

    match selection.as_str() {
        USE_SUGGESTED_GROUPS_OPTION => Ok(Some(suggested_groups.to_vec())),
        BUILD_GROUPS_MANUALLY_OPTION => {
            let manual = build_manual_split_groups(staged_files)?;
            render_split_groups(&manual, "Manual split groups");
            Ok(Some(manual))
        }
        KEEP_ONE_COMMIT_OPTION => Ok(None),
        ABORT_OPTION => bail!("commit aborted"),
        _ => bail!("invalid split grouping selection"),
    }
}

fn build_manual_split_groups(staged_files: &[String]) -> Result<Vec<SplitPlanGroup>> {
    let mut remaining = staged_files.to_vec();
    let mut groups = Vec::new();
    let mut index = 1;

    while remaining.len() > 1 {
        let selection = ui::multiselect(
            &format!("Select files for split commit {index}"),
            remaining.clone(),
        )?;
        if selection.is_empty() {
            bail!("no files selected");
        }
        if selection.len() == remaining.len() {
            bail!("select fewer than all remaining files to create multiple commits");
        }

        remaining.retain(|file| !selection.contains(file));
        groups.push(SplitPlanGroup {
            title: format!("Commit {index}"),
            rationale: "Manually grouped files".to_owned(),
            files: selection,
        });
        index += 1;
    }

    if !remaining.is_empty() {
        groups.push(SplitPlanGroup {
            title: format!("Commit {index}"),
            rationale: "Remaining files".to_owned(),
            files: remaining,
        });
    }

    Ok(groups)
}

fn render_split_groups(groups: &[SplitPlanGroup], title: &str) {
    ui::blank_line();
    ui::section(title);
    for (index, group) in groups.iter().enumerate() {
        ui::headline(format!("Split commit {}: {}", index + 1, group.title));
        ui::secondary(&group.rationale);
        for file in &group.files {
            ui::bullet(file);
        }
        ui::blank_line();
    }
}

async fn generate_split_commit_drafts(
    config: &Config,
    groups: &[SplitPlanGroup],
    context: &str,
    full_gitmoji_spec: bool,
    extra_args: &[String],
) -> Result<Vec<SplitCommitDraft>> {
    let mut drafts = Vec::with_capacity(groups.len());

    for group in groups {
        let group_diff = git::staged_diff(&group.files)?;
        let mut message = generator::generate_commit_message(
            config,
            &group_diff,
            full_gitmoji_spec,
            context,
            &group.files,
        )
        .await?;
        message = apply_message_template(config, extra_args, &message);
        drafts.push(SplitCommitDraft {
            group: group.clone(),
            message,
        });
    }

    Ok(drafts)
}

fn render_split_commit_preview(drafts: &[SplitCommitDraft]) {
    ui::blank_line();
    ui::section(format!("Split commit preview ({})", drafts.len()));
    for (index, draft) in drafts.iter().enumerate() {
        ui::headline(format!("Commit {}: {}", index + 1, draft.group.title));
        ui::secondary(&draft.group.rationale);
        ui::commit_message(&draft.message);
        for file in &draft.group.files {
            ui::bullet(file);
        }
        ui::blank_line();
    }
}

fn edit_split_commit_message(drafts: &mut [SplitCommitDraft]) -> Result<()> {
    let labels = drafts
        .iter()
        .enumerate()
        .map(|(index, draft)| format!("Commit {}: {}", index + 1, draft.group.title))
        .collect::<Vec<_>>();
    let selected = ui::select("Which commit message would you like to edit?", labels)?;
    let Some(index) = selected
        .split(':')
        .next()
        .and_then(|prefix| prefix.trim_start_matches("Commit ").parse::<usize>().ok())
        .and_then(|n| n.checked_sub(1))
    else {
        bail!("invalid split commit selection");
    };
    let edited = ui::editor("Edit split commit message", &drafts[index].message)?;
    let edited = edited.trim();
    if edited.is_empty() {
        bail!("split commit message cannot be empty");
    }
    drafts[index].message = edited.to_owned();
    Ok(())
}

fn create_split_commits(
    config: &Config,
    drafts: &[SplitCommitDraft],
    extra_args: &[String],
) -> Result<()> {
    let push_plan = build_push_plan(
        config.gitpush,
        false,
        &git::remote_metadata()?,
        &config.remote_icon_style,
    )?;

    for (index, draft) in drafts.iter().enumerate() {
        git::clear_index()?;
        git::add_files(&draft.group.files)?;
        let output = git::commit(&draft.message, &filtered_extra_args(config, extra_args))
            .map_err(|error| {
                if index == 0 {
                    error
                } else {
                    anyhow::anyhow!(
                        "split commit {} failed after {} earlier split commits were created: {error}",
                        index + 1,
                        index
                    )
                }
            })?;
        ui::success(format!(
            "created split commit {}/{}",
            index + 1,
            drafts.len()
        ));
        if !output.stdout.is_empty() {
            ui::secondary(output.stdout);
        }
        if !output.stderr.is_empty() {
            ui::secondary(output.stderr);
        }

        append_commit_history(config, &draft.message, &draft.group.files);
    }

    execute_push_plan(push_plan)
}

fn append_commit_history(config: &Config, message: &str, files: &[String]) {
    if config.ai_provider == "test" {
        return;
    }

    if let Err(e) = history::append_entry(&history::HistoryEntry {
        timestamp: history::now_iso8601(),
        kind: "commit".to_owned(),
        message: message.to_owned(),
        repo_path: git::repo_root()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        files: files.to_vec(),
        provider: config.ai_provider.clone(),
        model: config.model.clone(),
    }) {
        ui::warn(format!("failed to save history: {e}"));
    }
}

#[allow(clippy::too_many_arguments)]
async fn generate_confirm_and_commit(
    config: &Config,
    diff: &str,
    extra_args: &[String],
    context: &str,
    full_gitmoji_spec: bool,
    skip_confirmation: bool,
    dry_run: bool,
    staged_files: &[String],
) -> Result<()> {
    loop {
        let spinner = ui::spinner("Generating commit message");
        let commit_message = generator::generate_commit_message(
            config,
            diff,
            full_gitmoji_spec,
            context,
            staged_files,
        )
        .await;
        spinner.finish_and_clear();

        let mut commit_message = commit_message?;
        commit_message = apply_message_template(config, extra_args, &commit_message);

        ui::blank_line();
        ui::section("Generated commit message");
        ui::commit_message(&commit_message);

        if dry_run {
            return Ok(());
        }

        let action = if skip_confirmation {
            "Yes".to_owned()
        } else {
            ui::select(
                "Confirm the commit message?",
                vec!["Yes".to_owned(), "No".to_owned(), "Edit".to_owned()],
            )?
        };

        match action.as_str() {
            "Yes" => {
                return commit_and_maybe_push(
                    config,
                    &commit_message,
                    extra_args,
                    staged_files,
                    skip_confirmation,
                );
            }
            "Edit" => {
                let edited = ui::text("Edit commit message", Some(&commit_message))?;
                return commit_and_maybe_push(
                    config,
                    &edited,
                    extra_args,
                    staged_files,
                    skip_confirmation,
                );
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

fn commit_and_maybe_push(
    config: &Config,
    message: &str,
    extra_args: &[String],
    staged_files: &[String],
    skip_confirmation: bool,
) -> Result<()> {
    let push_plan = build_push_plan(
        config.gitpush,
        skip_confirmation,
        &git::remote_metadata()?,
        &config.remote_icon_style,
    )?;

    let output = git::commit(message, &filtered_extra_args(config, extra_args))?;
    ui::success("committed changes");
    if !output.stdout.is_empty() {
        ui::secondary(output.stdout);
    }
    if !output.stderr.is_empty() {
        ui::secondary(output.stderr);
    }

    append_commit_history(config, message, staged_files);

    execute_push_plan(push_plan)
}

fn build_push_plan(
    gitpush: bool,
    skip_confirmation: bool,
    remotes: &[git::GitRemoteMetadata],
    icon_style: &str,
) -> Result<PushPlan> {
    if !gitpush || remotes.is_empty() {
        return Ok(PushPlan::Skip);
    }

    let remotes = remotes
        .iter()
        .map(|remote| PushRemoteOption {
            name: remote.name.clone(),
            label: remote_display_label(remote, icon_style),
        })
        .collect::<Vec<_>>();

    match remotes.as_slice() {
        [remote] if skip_confirmation => Ok(PushPlan::AutoPush(remote.clone())),
        [remote] => Ok(PushPlan::ConfirmSingle {
            remote: remote.clone(),
            default: true,
        }),
        _ if skip_confirmation => bail!(MULTI_REMOTE_AUTO_PUSH_ERROR),
        _ => Ok(PushPlan::SelectRemote(remotes)),
    }
}

fn execute_push_plan(plan: PushPlan) -> Result<()> {
    match plan {
        PushPlan::Skip => Ok(()),
        PushPlan::AutoPush(remote) => push_to_remote(&remote),
        PushPlan::ConfirmSingle { remote, default } => {
            if ui::confirm(&format!("Run git push {}?", remote.label), default)? {
                push_to_remote(&remote)?;
            }
            Ok(())
        }
        PushPlan::SelectRemote(remotes) => {
            let mut options = remotes
                .iter()
                .map(|remote| remote.label.clone())
                .collect::<Vec<_>>();
            options.push("do not push".to_owned());
            let selected = ui::select("Choose a remote to push to", options)?;
            if let Some(remote) = remotes.iter().find(|remote| remote.label == selected) {
                push_to_remote(remote)?;
            }
            Ok(())
        }
    }
}

fn push_to_remote(remote: &PushRemoteOption) -> Result<()> {
    let output = git::push(Some(&remote.name))?;
    ui::success(format!("pushed to {}", remote.label));
    if !output.stdout.is_empty() {
        ui::secondary(output.stdout);
    }
    if !output.stderr.is_empty() {
        ui::secondary(output.stderr);
    }
    Ok(())
}

fn remote_display_label(remote: &git::GitRemoteMetadata, icon_style: &str) -> String {
    remote_display_label_with_icon_style(remote, RemoteIconStyle::from_config(icon_style))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteIconStyle {
    Auto,
    NerdFont,
    Emoji,
    Label,
}

impl RemoteIconStyle {
    fn from_config(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "nerd" | "nerd-font" | "nerdfont" => Self::NerdFont,
            "emoji" => Self::Emoji,
            "label" | "labels" | "none" | "off" => Self::Label,
            _ => Self::Auto,
        }
    }
}

fn remote_display_label_with_icon_style(
    remote: &git::GitRemoteMetadata,
    style: RemoteIconStyle,
) -> String {
    match (
        provider_display_label(&remote.provider, style).as_deref(),
        remote.web_url.as_deref(),
    ) {
        (Some(provider), Some(url)) => format!("[{provider}] {} {url}", remote.name),
        (Some(provider), None) => format!("[{provider}] {}", remote.name),
        (None, Some(url)) => format!("{} {url}", remote.name),
        (None, None) => remote.name.clone(),
    }
}

fn provider_display_label(provider: &git::GitProvider, style: RemoteIconStyle) -> Option<String> {
    let label = provider.label()?;
    let icon = match style {
        RemoteIconStyle::Auto | RemoteIconStyle::NerdFont => provider
            .nerd_font_icon()
            .or_else(|| provider.emoji_icon())
            .filter(|_| style != RemoteIconStyle::Label),
        RemoteIconStyle::Emoji => provider.emoji_icon(),
        RemoteIconStyle::Label => None,
    };

    Some(match icon {
        Some(icon) => format!("{icon} {label}"),
        None => label.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        ffi::OsStr,
        path::{Path, PathBuf},
        process::Command,
        sync::MutexGuard,
    };

    use tempfile::TempDir;

    #[test]
    fn staging_state_reports_no_changes_when_repo_is_clean() {
        let error = resolve_staging_state(&[], &[], false).unwrap_err();
        assert_eq!(error.to_string(), "no changes detected");
    }

    #[test]
    fn staging_state_bypasses_prompt_when_files_are_already_staged() {
        let staged = vec!["src/main.rs".to_owned()];
        let changed = vec!["src/main.rs".to_owned(), "README.md".to_owned()];

        assert_eq!(
            resolve_staging_state(&staged, &changed, false).unwrap(),
            StagingState::UseExisting
        );
    }

    #[test]
    fn staging_state_prompts_when_only_unstaged_files_exist() {
        let changed = vec!["src/main.rs".to_owned(), "README.md".to_owned()];

        assert_eq!(
            resolve_staging_state(&[], &changed, false).unwrap(),
            StagingState::PromptForSelection
        );
    }

    #[test]
    fn staging_state_auto_stages_all_with_yes_when_only_unstaged_files_exist() {
        let changed = vec!["src/main.rs".to_owned(), "README.md".to_owned()];

        assert_eq!(
            resolve_staging_state(&[], &changed, true).unwrap(),
            StagingState::AutoStageAll
        );
    }

    #[test]
    fn staging_plan_stages_all_changed_files() {
        let changed = vec!["src/main.rs".to_owned(), "README.md".to_owned()];

        assert_eq!(
            build_staging_plan(StageSelectionAction::StageAll, changed.clone(), vec![]).unwrap(),
            StagingPlan::AddFiles(changed)
        );
    }

    #[test]
    fn staging_plan_stages_only_selected_files() {
        let selected = vec!["README.md".to_owned()];

        assert_eq!(
            build_staging_plan(
                StageSelectionAction::ChooseFiles,
                vec!["src/main.rs".to_owned(), "README.md".to_owned()],
                selected.clone(),
            )
            .unwrap(),
            StagingPlan::AddFiles(selected)
        );
    }

    #[test]
    fn staging_plan_rejects_empty_file_selection() {
        let error = build_staging_plan(
            StageSelectionAction::ChooseFiles,
            vec!["src/main.rs".to_owned()],
            vec![],
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "no files selected");
    }

    #[test]
    fn staging_plan_aborts_when_user_cancels() {
        assert_eq!(
            build_staging_plan(
                StageSelectionAction::Cancel,
                vec!["src/main.rs".to_owned()],
                vec!["src/main.rs".to_owned()],
            )
            .unwrap(),
            StagingPlan::Abort
        );
    }

    #[test]
    fn split_prompt_is_only_offered_for_interactive_multi_file_commits() {
        assert!(should_offer_split(2, false, false, false));
        assert!(!should_offer_split(1, false, false, false));
        assert!(!should_offer_split(2, true, false, false));
        assert!(!should_offer_split(2, false, true, false));
        assert!(!should_offer_split(2, false, false, true));
    }

    #[test]
    fn applies_message_template() {
        let config = Config::default();
        let result =
            apply_message_template(&config, &["issue-123: $msg".to_owned()], "feat: add cli");
        assert_eq!(result, "issue-123: feat: add cli");
    }

    fn remote(name: &str) -> git::GitRemoteMetadata {
        git::GitRemoteMetadata {
            name: name.to_owned(),
            fetch_url: None,
            push_url: None,
            web_url: None,
            provider: git::GitProvider::unknown(),
        }
    }

    #[test]
    fn push_plan_skips_when_push_is_disabled() {
        let plan = build_push_plan(false, false, &[remote("origin")], "auto").unwrap();
        assert_eq!(plan, PushPlan::Skip);
    }

    #[test]
    fn push_plan_skips_when_no_remotes_exist() {
        let plan = build_push_plan(true, false, &[], "auto").unwrap();
        assert_eq!(plan, PushPlan::Skip);
    }

    #[test]
    fn push_plan_auto_pushes_single_remote_with_yes() {
        let plan = build_push_plan(true, true, &[remote("origin")], "auto").unwrap();
        assert_eq!(
            plan,
            PushPlan::AutoPush(PushRemoteOption {
                name: "origin".to_owned(),
                label: "origin".to_owned(),
            })
        );
    }

    #[test]
    fn push_plan_prompts_single_remote_with_default_yes() {
        let plan = build_push_plan(true, false, &[remote("origin")], "auto").unwrap();
        assert_eq!(
            plan,
            PushPlan::ConfirmSingle {
                remote: PushRemoteOption {
                    name: "origin".to_owned(),
                    label: "origin".to_owned(),
                },
                default: true,
            }
        );
    }

    #[test]
    fn push_plan_rejects_multiple_remotes_with_yes() {
        let error =
            build_push_plan(true, true, &[remote("origin"), remote("backup")], "auto").unwrap_err();
        assert_eq!(error.to_string(), MULTI_REMOTE_AUTO_PUSH_ERROR);
    }

    #[test]
    fn formats_known_remote_with_provider_and_url() {
        let remote = git::GitRemoteMetadata {
            name: "origin".to_owned(),
            fetch_url: Some("https://github.com/russmckendrick/aicommit.git".to_owned()),
            push_url: Some("https://github.com/russmckendrick/aicommit.git".to_owned()),
            web_url: Some("https://github.com/russmckendrick/aicommit".to_owned()),
            provider: git::GitProvider::known_with_icons(
                "GitHub",
                Some("GH".to_owned()),
                Some("octo".to_owned()),
            ),
        };

        assert_eq!(
            remote_display_label_with_icon_style(&remote, RemoteIconStyle::NerdFont),
            "[GH GitHub] origin https://github.com/russmckendrick/aicommit"
        );
    }

    #[test]
    fn falls_back_to_emoji_when_nerd_font_icon_is_missing() {
        let remote = git::GitRemoteMetadata {
            name: "origin".to_owned(),
            fetch_url: Some("https://gitlab.com/group/project.git".to_owned()),
            push_url: Some("https://gitlab.com/group/project.git".to_owned()),
            web_url: Some("https://gitlab.com/group/project".to_owned()),
            provider: git::GitProvider::known_with_icons("GitLab", None, Some("fox".to_owned())),
        };

        assert_eq!(
            remote_display_label_with_icon_style(&remote, RemoteIconStyle::NerdFont),
            "[fox GitLab] origin https://gitlab.com/group/project"
        );
    }

    #[test]
    fn falls_back_to_label_when_icons_are_missing() {
        let remote = git::GitRemoteMetadata {
            name: "origin".to_owned(),
            fetch_url: Some("https://github.com/russmckendrick/aicommit.git".to_owned()),
            push_url: Some("https://github.com/russmckendrick/aicommit.git".to_owned()),
            web_url: Some("https://github.com/russmckendrick/aicommit".to_owned()),
            provider: git::GitProvider::known("GitHub"),
        };

        assert_eq!(
            remote_display_label_with_icon_style(&remote, RemoteIconStyle::NerdFont),
            "[GitHub] origin https://github.com/russmckendrick/aicommit"
        );
    }

    #[test]
    fn can_force_emoji_icon_style() {
        let remote = git::GitRemoteMetadata {
            name: "origin".to_owned(),
            fetch_url: Some("https://github.com/russmckendrick/aicommit.git".to_owned()),
            push_url: Some("https://github.com/russmckendrick/aicommit.git".to_owned()),
            web_url: Some("https://github.com/russmckendrick/aicommit".to_owned()),
            provider: git::GitProvider::known_with_icons(
                "GitHub",
                Some("GH".to_owned()),
                Some("octo".to_owned()),
            ),
        };

        assert_eq!(
            remote_display_label_with_icon_style(&remote, RemoteIconStyle::Emoji),
            "[octo GitHub] origin https://github.com/russmckendrick/aicommit"
        );
    }

    #[test]
    fn can_force_label_icon_style() {
        let remote = git::GitRemoteMetadata {
            name: "origin".to_owned(),
            fetch_url: Some("https://github.com/russmckendrick/aicommit.git".to_owned()),
            push_url: Some("https://github.com/russmckendrick/aicommit.git".to_owned()),
            web_url: Some("https://github.com/russmckendrick/aicommit".to_owned()),
            provider: git::GitProvider::known_with_icons(
                "GitHub",
                Some("GH".to_owned()),
                Some("octo".to_owned()),
            ),
        };

        assert_eq!(
            remote_display_label_with_icon_style(&remote, RemoteIconStyle::Label),
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
            remote_display_label(&remote, "auto"),
            "mirror https://git.example.test/team/repo"
        );
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn split_drafts_use_group_specific_diffs() {
        let _cwd = hold_cwd_for_test();
        let repo = init_repo();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("README.md"), "docs\n").unwrap();
        std::fs::write(repo.path().join("src/lib.rs"), "pub fn value() {}\n").unwrap();
        run_git_test(repo.path(), ["add", "README.md", "src/lib.rs"]);

        let _dir = CurrentDirGuard::enter(repo.path());

        let drafts = generate_split_commit_drafts(
            &Config {
                ai_provider: "test".to_owned(),
                gitpush: false,
                ..Config::default()
            },
            &[
                SplitPlanGroup {
                    title: "Docs".to_owned(),
                    rationale: "docs".to_owned(),
                    files: vec!["README.md".to_owned()],
                },
                SplitPlanGroup {
                    title: "Library".to_owned(),
                    rationale: "lib".to_owned(),
                    files: vec!["src/lib.rs".to_owned()],
                },
            ],
            "",
            false,
            &[],
        )
        .await
        .unwrap();

        assert_eq!(drafts[0].message, "docs: update readme");
        assert_eq!(drafts[1].message, "feat: update library");
    }

    #[test]
    fn create_split_commits_creates_multiple_commits() {
        let _cwd = hold_cwd_for_test();
        let repo = init_repo();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("README.md"), "docs\n").unwrap();
        std::fs::write(repo.path().join("src/lib.rs"), "pub fn value() {}\n").unwrap();
        run_git_test(repo.path(), ["add", "README.md", "src/lib.rs"]);

        let _dir = CurrentDirGuard::enter(repo.path());

        create_split_commits(
            &Config {
                ai_provider: "test".to_owned(),
                gitpush: false,
                ..Config::default()
            },
            &[
                SplitCommitDraft {
                    group: SplitPlanGroup {
                        title: "Docs".to_owned(),
                        rationale: "docs".to_owned(),
                        files: vec!["README.md".to_owned()],
                    },
                    message: "docs: update readme".to_owned(),
                },
                SplitCommitDraft {
                    group: SplitPlanGroup {
                        title: "Library".to_owned(),
                        rationale: "lib".to_owned(),
                        files: vec!["src/lib.rs".to_owned()],
                    },
                    message: "feat: update library".to_owned(),
                },
            ],
            &[],
        )
        .unwrap();

        let subjects = git_stdout(repo.path(), ["log", "--format=%s", "-2"]);

        let lines = subjects.lines().collect::<Vec<_>>();
        assert_eq!(lines, vec!["feat: update library", "docs: update readme"]);
    }

    fn init_repo() -> TempDir {
        let temp = TempDir::new().unwrap();
        run_git_test(temp.path(), ["init", "-b", "main"]);
        run_git_test(temp.path(), ["config", "user.email", "test@example.com"]);
        run_git_test(temp.path(), ["config", "user.name", "Test User"]);
        temp
    }

    fn git_stdout<I, S>(cwd: &Path, args: I) -> String
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn run_git_test<I, S>(cwd: &Path, args: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .unwrap();
        assert!(status.success());
    }

    fn hold_cwd_for_test() -> MutexGuard<'static, ()> {
        crate::git::cwd_test_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    struct CurrentDirGuard {
        previous: PathBuf,
    }

    impl CurrentDirGuard {
        fn enter(path: &Path) -> Self {
            let previous = std::env::current_dir().unwrap();
            std::env::set_current_dir(path).unwrap();
            Self { previous }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.previous);
        }
    }
}
