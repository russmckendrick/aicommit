use anyhow::{Result, bail};

use crate::git;

use super::display::remote_display_label;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PushRemoteOption {
    pub(crate) name: String,
    pub(crate) label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PushPlan {
    Skip,
    AutoPush(PushRemoteOption),
    ConfirmSingle {
        remote: PushRemoteOption,
        default: bool,
    },
    SelectRemote(Vec<PushRemoteOption>),
}

const MULTI_REMOTE_AUTO_PUSH_ERROR: &str = concat!(
    "cannot auto-push with --yes because multiple remotes are configured; ",
    "rerun without --yes to choose a remote or set AIC_GITPUSH=false"
);

pub(crate) fn build_push_plan(
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

#[cfg(test)]
mod tests {
    use crate::git;

    use super::super::display::remote_display_label;
    use super::*;

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
}
