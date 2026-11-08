use anyhow::{Result, bail};

use crate::{config::Config, generator, git, prompt::SplitPlanGroup, ui};

use super::super::{
    apply_message_template,
    push::{build_push_plan, execute_push_plan},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SplitCommitDraft {
    pub(crate) group: SplitPlanGroup,
    pub(crate) message: String,
}

pub(crate) async fn generate_split_commit_drafts(
    config: &Config,
    staged: &git::StagedDiff,
    groups: &[SplitPlanGroup],
    context: &str,
    full_gitmoji_spec: bool,
    extra_args: &[String],
) -> Result<Vec<SplitCommitDraft>> {
    let mut drafts = Vec::with_capacity(groups.len());

    for (index, group) in groups.iter().enumerate() {
        let group_diff =
            staged.display_for(&group.hunks, |file| git::is_excluded_from_diff(&file.path));
        let draft_label = format!("commit {}/{}", index + 1, groups.len());
        let spinner = ui::StatusSpinner::start(
            format!("Drafting {}: {}", draft_label, group.title),
            ui::StatusPool::Waiting,
        );
        let progress = |event: generator::GenerationProgress| {
            spinner.on_generation_progress(event, &draft_label)
        };
        let mut message = generator::generate_commit_message(
            config,
            &group_diff,
            full_gitmoji_spec,
            context,
            &group.files,
            Some(&progress),
        )
        .await?;
        spinner.finish_and_clear();
        message = apply_message_template(config, extra_args, &message);
        drafts.push(SplitCommitDraft {
            group: group.clone(),
            message,
        });
    }

    Ok(drafts)
}

pub(super) fn render_split_commit_preview(drafts: &[SplitCommitDraft]) {
    ui::blank_line();
    ui::section(format!("Split commit preview ({})", drafts.len()));
    for (index, draft) in drafts.iter().enumerate() {
        ui::blank_line();
        ui::headline(format!("Commit {}: {}", index + 1, draft.group.title));
        ui::secondary(&draft.group.rationale);
        ui::file_metadata(&draft.group.files);
        for line in ui::summarize_files(&draft.group.files, 4, 3) {
            ui::bullet(line);
        }
        ui::primary_card("Commit message", &draft.message);
    }
}

pub(super) fn edit_split_commit_message(drafts: &mut [SplitCommitDraft]) -> Result<()> {
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

/// Create one commit per draft by staging exactly that group's hunks.
///
/// The index is snapshotted first (`git write-tree`), reset to `HEAD`, and
/// each group's patch is applied with `git apply --cached`. The working tree
/// is never modified, so partially staged files are safe to split. Whatever
/// happens, the index ends as the snapshot: after success that leaves only
/// changes no group claimed (files hidden by `.aicommitignore`) staged; after
/// a failure it restores the staged state on top of any commits already made.
pub(crate) async fn create_split_commits(
    config: &Config,
    staged: &git::StagedDiff,
    drafts: &[SplitCommitDraft],
    extra_args: &[String],
) -> Result<()> {
    let push_plan = build_push_plan(
        config.gitpush,
        false,
        &git::remote_metadata()?,
        &config.remote_icon_style,
    )?;

    let groups = drafts
        .iter()
        .map(|draft| (draft.group.clone(), draft.message.clone()))
        .collect::<Vec<_>>();
    super::super::session::commit_split_groups(
        config,
        staged,
        &groups,
        extra_args,
        |index, total, record| {
            ui::section(format!("Split commit {}/{} created", index + 1, total));
            ui::headline(record.subject());
            if let Some(error) = &record.history_error {
                ui::warn(format!("failed to save history: {error}"));
            }
        },
    )?;

    execute_push_plan(push_plan, config, false)
        .await
        .map(|_| ())
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

        let staged = git::staged_patch().unwrap();
        let drafts = generate_split_commit_drafts(
            &Config {
                ai_provider: "test".to_owned(),
                gitpush: false,
                ..Config::default()
            },
            &staged,
            &[
                SplitPlanGroup {
                    title: "Docs".to_owned(),
                    rationale: "docs".to_owned(),
                    files: vec!["README.md".to_owned()],
                    hunks: vec!["README.md#1".to_owned()],
                },
                SplitPlanGroup {
                    title: "Library".to_owned(),
                    rationale: "lib".to_owned(),
                    files: vec!["src/lib.rs".to_owned()],
                    hunks: vec!["src/lib.rs#1".to_owned()],
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

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn create_split_commits_creates_multiple_commits() {
        let _cwd = hold_cwd_for_test();
        let repo = init_repo();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("README.md"), "docs\n").unwrap();
        std::fs::write(repo.path().join("src/lib.rs"), "pub fn value() {}\n").unwrap();
        run_git_test(repo.path(), ["add", "README.md", "src/lib.rs"]);

        let _dir = CurrentDirGuard::enter(repo.path());

        let staged = git::staged_patch().unwrap();
        create_split_commits(
            &Config {
                ai_provider: "test".to_owned(),
                gitpush: false,
                ..Config::default()
            },
            &staged,
            &[
                SplitCommitDraft {
                    group: SplitPlanGroup {
                        title: "Docs".to_owned(),
                        rationale: "docs".to_owned(),
                        files: vec!["README.md".to_owned()],
                        hunks: vec!["README.md#1".to_owned()],
                    },
                    message: "docs: update readme".to_owned(),
                },
                SplitCommitDraft {
                    group: SplitPlanGroup {
                        title: "Library".to_owned(),
                        rationale: "lib".to_owned(),
                        files: vec!["src/lib.rs".to_owned()],
                        hunks: vec!["src/lib.rs#1".to_owned()],
                    },
                    message: "feat: update library".to_owned(),
                },
            ],
            &[],
        )
        .await
        .unwrap();

        let subjects = git_stdout(repo.path(), ["log", "--format=%s", "-2"]);
        let lines = subjects.lines().collect::<Vec<_>>();
        assert_eq!(lines, vec!["feat: update library", "docs: update readme"]);
    }

    fn draft(
        title: &str,
        hunks: &[&str],
        message: &str,
        staged: &git::StagedDiff,
    ) -> SplitCommitDraft {
        let hunks = hunks.iter().map(|id| (*id).to_owned()).collect::<Vec<_>>();
        SplitCommitDraft {
            group: SplitPlanGroup {
                title: title.to_owned(),
                rationale: title.to_owned(),
                files: staged.files_for(&hunks),
                hunks,
            },
            message: message.to_owned(),
        }
    }

    fn test_config() -> Config {
        Config {
            ai_provider: "test".to_owned(),
            gitpush: false,
            ..Config::default()
        }
    }

    fn numbered_lines(count: usize, changed: &[usize]) -> String {
        (1..=count)
            .map(|n| {
                if changed.contains(&n) {
                    format!("line {n} changed\n")
                } else {
                    format!("line {n}\n")
                }
            })
            .collect()
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn splits_one_file_across_commits_and_keeps_unstaged_work() {
        let _cwd = hold_cwd_for_test();
        let repo = init_repo();
        let file = repo.path().join("notes.txt");
        std::fs::write(&file, numbered_lines(40, &[])).unwrap();
        run_git_test(repo.path(), ["add", "notes.txt"]);
        run_git_test(repo.path(), ["commit", "-m", "init"]);

        // Stage two separate hunks, then make a third, unstaged edit.
        std::fs::write(&file, numbered_lines(40, &[2, 35])).unwrap();
        run_git_test(repo.path(), ["add", "notes.txt"]);
        std::fs::write(&file, numbered_lines(40, &[2, 20, 35])).unwrap();

        let _dir = CurrentDirGuard::enter(repo.path());
        let staged = git::staged_patch().unwrap();
        assert_eq!(staged.hunk_ids(), vec!["notes.txt#1", "notes.txt#2"]);

        create_split_commits(
            &test_config(),
            &staged,
            &[
                draft("bottom", &["notes.txt#2"], "fix: bottom", &staged),
                draft("top", &["notes.txt#1"], "fix: top", &staged),
            ],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(
            git_stdout(repo.path(), ["log", "--format=%s", "-3"]),
            "fix: top\nfix: bottom\ninit"
        );
        let first = git_stdout(repo.path(), ["show", "HEAD~1:notes.txt"]);
        assert!(first.contains("line 35 changed") && !first.contains("line 2 changed"));
        let second = git_stdout(repo.path(), ["show", "HEAD:notes.txt"]);
        assert!(second.contains("line 35 changed") && second.contains("line 2 changed"));
        // Nothing left staged, and the unstaged edit is still in the working tree.
        assert_eq!(
            git_stdout(repo.path(), ["diff", "--cached", "--name-only"]),
            ""
        );
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            numbered_lines(40, &[2, 20, 35])
        );
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn splits_new_renamed_and_binary_files() {
        let _cwd = hold_cwd_for_test();
        let repo = init_repo();
        std::fs::write(repo.path().join("old.txt"), "keep me\n".repeat(20)).unwrap();
        run_git_test(repo.path(), ["add", "old.txt"]);
        run_git_test(repo.path(), ["commit", "-m", "init"]);

        run_git_test(repo.path(), ["mv", "old.txt", "new.txt"]);
        std::fs::write(repo.path().join("added.md"), "# hello\n").unwrap();
        std::fs::write(repo.path().join("pixel.bin"), [0u8, 159, 146, 150, 0, 1, 2]).unwrap();
        run_git_test(repo.path(), ["add", "added.md", "pixel.bin"]);

        let _dir = CurrentDirGuard::enter(repo.path());
        let snapshot = git::write_tree().unwrap();
        let staged = git::staged_patch().unwrap();
        let mut ids = staged.hunk_ids();
        ids.sort();
        assert_eq!(ids, vec!["added.md#1", "new.txt#1", "pixel.bin#1"]);

        create_split_commits(
            &test_config(),
            &staged,
            &[
                draft("rename", &["new.txt#1"], "refactor: rename notes", &staged),
                draft(
                    "assets",
                    &["added.md#1", "pixel.bin#1"],
                    "feat: add assets",
                    &staged,
                ),
            ],
            &[],
        )
        .await
        .unwrap();

        assert_eq!(
            git_stdout(repo.path(), ["rev-parse", "HEAD^{tree}"]),
            snapshot
        );
        assert_eq!(
            git_stdout(
                repo.path(),
                ["show", "--name-status", "--format=", "HEAD~1"]
            ),
            "R100\told.txt\tnew.txt"
        );
    }

    #[cfg(unix)]
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn failed_split_commit_restores_the_staged_state() {
        use std::os::unix::fs::PermissionsExt;

        let _cwd = hold_cwd_for_test();
        let repo = init_repo();
        std::fs::write(repo.path().join("a.txt"), "a\n").unwrap();
        std::fs::write(repo.path().join("b.txt"), "b\n").unwrap();
        run_git_test(repo.path(), ["add", "a.txt", "b.txt"]);
        let hook = repo.path().join(".git/hooks/commit-msg");
        std::fs::write(&hook, "#!/bin/sh\ngrep -q boom \"$1\" && exit 1\nexit 0\n").unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();

        let _dir = CurrentDirGuard::enter(repo.path());
        let staged = git::staged_patch().unwrap();
        let error = create_split_commits(
            &test_config(),
            &staged,
            &[
                draft("a", &["a.txt#1"], "feat: add a", &staged),
                draft("b", &["b.txt#1"], "feat: boom", &staged),
            ],
            &[],
        )
        .await
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("split commit 2 failed after 1 earlier")
        );
        assert_eq!(
            git_stdout(repo.path(), ["log", "--format=%s"]),
            "feat: add a"
        );
        assert_eq!(
            git_stdout(repo.path(), ["diff", "--cached", "--name-only"]),
            "b.txt"
        );
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
