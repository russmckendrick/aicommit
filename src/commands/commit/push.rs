use anyhow::{Result, bail};

use crate::{config::Config, git, ui};

use super::{filtered_extra_args, helpers::append_commit_history};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PushRemoteOption {
    name: String,
    label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PushPlan {
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

pub(crate) fn execute_push_plan(plan: PushPlan) -> Result<()> {
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

pub(crate) fn remote_display_label(remote: &git::GitRemoteMetadata, icon_style: &str) -> String {
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
    use crate::git;

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
}
