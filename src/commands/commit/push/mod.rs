use anyhow::Result;

use crate::config::Config;

mod display;
mod plan;

pub(crate) use plan::build_push_plan;
use plan::{PushPlan, PushRemoteOption};

const PUSH_NOW_OPTION: &str = "Push now";
const SKIP_PUSH_OPTION: &str = "Skip";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PushOutcome {
    Skipped,
    Pushed(String),
}

pub(crate) fn commit_and_maybe_push(
    config: &Config,
    message: &str,
    extra_args: &[String],
    staged_files: &[String],
    skip_confirmation: bool,
) -> Result<()> {
    let push_plan = build_push_plan(
        config.gitpush,
        skip_confirmation,
        &crate::git::remote_metadata()?,
        &config.remote_icon_style,
    )?;

    let output = crate::git::commit(message, &super::filtered_extra_args(config, extra_args))?;
    let commit_hash = crate::git::head_short_hash().ok();
    let branch = crate::git::current_branch();

    super::helpers::append_commit_history(config, message, staged_files);

    let push_outcome = execute_push_plan(push_plan)?;

    crate::ui::blank_line();
    crate::ui::section("Commit created");
    let mut items = Vec::new();
    if let Some(commit_hash) = commit_hash {
        items.push(format!("hash: {commit_hash}"));
    }
    if let Some(branch) = branch {
        items.push(format!("branch: {branch}"));
    }
    if let PushOutcome::Pushed(remote) = &push_outcome {
        items.push(format!("pushed: {remote}"));
    }
    crate::ui::metadata_row(&items);
    crate::ui::headline(message.lines().next().unwrap_or(message));

    render_git_output(&output);

    Ok(())
}

pub(crate) fn execute_push_plan(plan: PushPlan) -> Result<PushOutcome> {
    match plan {
        PushPlan::Skip => Ok(PushOutcome::Skipped),
        PushPlan::AutoPush(remote) => push_to_remote(&remote),
        PushPlan::ConfirmSingle { remote } => {
            let selection = crate::ui::select(
                &format!("Push this commit to {}?", remote.label),
                single_remote_push_actions(),
            )?;
            if selection == PUSH_NOW_OPTION {
                push_to_remote(&remote)
            } else {
                Ok(PushOutcome::Skipped)
            }
        }
        PushPlan::SelectRemote(remotes) => {
            let selected =
                crate::ui::select("Choose a remote to push to", remote_push_options(&remotes))?;
            if let Some(remote) = remotes.iter().find(|remote| remote.label == selected) {
                push_to_remote(remote)
            } else {
                Ok(PushOutcome::Skipped)
            }
        }
    }
}

fn push_to_remote(remote: &PushRemoteOption) -> Result<PushOutcome> {
    let output = crate::git::push(Some(&remote.name))?;
    crate::ui::success(format!("Pushed to {}", remote.label));
    render_git_output(&output);
    Ok(PushOutcome::Pushed(remote.label.clone()))
}

pub(crate) fn single_remote_push_actions() -> Vec<String> {
    vec![PUSH_NOW_OPTION.to_owned(), SKIP_PUSH_OPTION.to_owned()]
}

fn remote_push_options(remotes: &[PushRemoteOption]) -> Vec<String> {
    let mut options = remotes
        .iter()
        .map(|remote| remote.label.clone())
        .collect::<Vec<_>>();
    options.push(SKIP_PUSH_OPTION.to_owned());
    options
}

fn render_git_output(output: &crate::git::GitOutput) {
    if !output.stderr.is_empty() {
        crate::ui::secondary(&output.stderr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_remote_push_actions_are_explicit() {
        assert_eq!(
            single_remote_push_actions(),
            vec!["Push now".to_owned(), "Skip".to_owned()]
        );
    }

    #[test]
    fn remote_push_options_append_skip() {
        let options = remote_push_options(&[
            PushRemoteOption {
                name: "origin".to_owned(),
                label: "origin".to_owned(),
            },
            PushRemoteOption {
                name: "backup".to_owned(),
                label: "backup".to_owned(),
            },
        ]);

        assert_eq!(
            options,
            vec!["origin".to_owned(), "backup".to_owned(), "Skip".to_owned()]
        );
    }
}
