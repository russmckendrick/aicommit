use anyhow::{Result, bail};

use crate::{config::Config, generator, git, prompt::SplitPlanGroup, ui};

use super::{
    apply_message_template, filtered_extra_args,
    helpers::append_commit_history,
    push::{build_push_plan, execute_push_plan},
};

const CREATE_ONE_COMMIT_OPTION: &str = "Create one commit";
const SPLIT_INTO_MULTIPLE_COMMITS_OPTION: &str = "Split into multiple commits";
const ABORT_OPTION: &str = "Abort";
const USE_SUGGESTED_GROUPS_OPTION: &str = "Use suggested groups";
const BUILD_GROUPS_MANUALLY_OPTION: &str = "Build groups manually";
const KEEP_ONE_COMMIT_OPTION: &str = "Keep one commit";
const CREATE_COMMITS_OPTION: &str = "Create commits";
const REGENERATE_ALL_MESSAGES_OPTION: &str = "Regenerate all messages";
const EDIT_A_MESSAGE_OPTION: &str = "Edit a message";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SplitCommitDraft {
    pub(crate) group: SplitPlanGroup,
    pub(crate) message: String,
}

pub(crate) fn should_offer_split(
    staged_file_count: usize,
    skip_confirmation: bool,
    dry_run: bool,
    amend: bool,
) -> bool {
    staged_file_count >= 2 && !skip_confirmation && !dry_run && !amend
}

pub(crate) async fn maybe_execute_split_flow(
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

pub(crate) async fn generate_split_commit_drafts(
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

pub(crate) fn create_split_commits(
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

#[allow(clippy::too_many_arguments)]
pub(crate) async fn generate_confirm_and_commit(
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
                return super::push::commit_and_maybe_push(
                    config,
                    &commit_message,
                    extra_args,
                    staged_files,
                    skip_confirmation,
                );
            }
            "Edit" => {
                let edited = ui::text("Edit commit message", Some(&commit_message))?;
                return super::push::commit_and_maybe_push(
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

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsStr,
        path::{Path, PathBuf},
        process::Command,
        sync::MutexGuard,
    };

    use tempfile::TempDir;

    use super::*;
    use crate::{config::Config, git::cwd_test_lock};

    #[test]
    fn split_prompt_is_only_offered_for_interactive_multi_file_commits() {
        assert!(should_offer_split(2, false, false, false));
        assert!(!should_offer_split(1, false, false, false));
        assert!(!should_offer_split(2, true, false, false));
        assert!(!should_offer_split(2, false, true, false));
        assert!(!should_offer_split(2, false, false, true));
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
        cwd_test_lock()
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
