use anyhow::{Result, bail};

use crate::{errors::AicError, git, ui};

const STAGE_ALL_FILES_OPTION: &str = "Stage all";
const CHOOSE_FILES_OPTION: &str = "Choose files";
const ABORT_STAGE_OPTION: &str = "Abort";

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
    Abort,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StagingPlan {
    AddFiles(Vec<String>),
    Abort,
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
    ui::section("Commit session");
    ui::session_step("No files are staged yet");
    ui::file_list("Changed files", &git::changed_files()?);

    let selection = ui::select(
        "No files are staged. What would you like to do?",
        stage_selection_options(),
    )?;

    match selection.as_str() {
        STAGE_ALL_FILES_OPTION => Ok(StageSelectionAction::StageAll),
        CHOOSE_FILES_OPTION => Ok(StageSelectionAction::ChooseFiles),
        ABORT_STAGE_OPTION => Ok(StageSelectionAction::Abort),
        _ => bail!("invalid staging selection"),
    }
}

pub(crate) fn stage_selection_options() -> Vec<String> {
    vec![
        STAGE_ALL_FILES_OPTION.to_owned(),
        CHOOSE_FILES_OPTION.to_owned(),
        ABORT_STAGE_OPTION.to_owned(),
    ]
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
        StageSelectionAction::Abort => Ok(StagingPlan::Abort),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                StageSelectionAction::Abort,
                vec!["src/main.rs".to_owned()],
                vec!["src/main.rs".to_owned()],
            )
            .unwrap(),
            StagingPlan::Abort
        );
    }

    #[test]
    fn stage_selection_options_use_explicit_action_labels() {
        assert_eq!(
            stage_selection_options(),
            vec![
                "Stage all".to_owned(),
                "Choose files".to_owned(),
                "Abort".to_owned()
            ]
        );
    }
}
