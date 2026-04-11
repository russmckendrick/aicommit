use anyhow::Result;

use crate::config::Config;

mod display;
mod plan;

pub(crate) use plan::build_push_plan;
use plan::{PushPlan, PushRemoteOption};

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
    crate::ui::success("committed changes");
    if !output.stdout.is_empty() {
        crate::ui::secondary(output.stdout);
    }
    if !output.stderr.is_empty() {
        crate::ui::secondary(output.stderr);
    }

    super::helpers::append_commit_history(config, message, staged_files);

    execute_push_plan(push_plan)
}

pub(crate) fn execute_push_plan(plan: PushPlan) -> Result<()> {
    match plan {
        PushPlan::Skip => Ok(()),
        PushPlan::AutoPush(remote) => push_to_remote(&remote),
        PushPlan::ConfirmSingle { remote, default } => {
            if crate::ui::confirm(&format!("Run git push {}?", remote.label), default)? {
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
            let selected = crate::ui::select("Choose a remote to push to", options)?;
            if let Some(remote) = remotes.iter().find(|remote| remote.label == selected) {
                push_to_remote(remote)?;
            }
            Ok(())
        }
    }
}

fn push_to_remote(remote: &PushRemoteOption) -> Result<()> {
    let output = crate::git::push(Some(&remote.name))?;
    crate::ui::success(format!("pushed to {}", remote.label));
    if !output.stdout.is_empty() {
        crate::ui::secondary(output.stdout);
    }
    if !output.stderr.is_empty() {
        crate::ui::secondary(output.stderr);
    }
    Ok(())
}
