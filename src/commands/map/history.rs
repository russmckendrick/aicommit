use anyhow::Result;

use crate::{
    git::{self, stats},
    map::{config::MapConfig, theme, timeline},
    ui,
};

pub fn run(
    output: Option<String>,
    commits: Option<usize>,
    theme_override: Option<&str>,
) -> Result<()> {
    git::assert_git_repo()?;
    let map_config = MapConfig::load()?;
    let commits = commits.unwrap_or(map_config.history_commits);

    ui::section(format!("Building commit timeline ({commits} commits)"));

    let commit_data = stats::timestamped_commits(commits)?;
    if commit_data.is_empty() {
        anyhow::bail!("no commits found");
    }

    ui::bullet(format!("{} commits loaded", commit_data.len()));

    let theme_name = theme_override.unwrap_or(&map_config.theme);
    let theme = theme::load_theme(theme_name)?;
    let doc = timeline::render(&commit_data, None, theme);

    let output_path = output.unwrap_or_else(|| "aic-timeline.svg".to_owned());
    svg::save(&output_path, &doc)?;
    ui::success(format!("Timeline saved to {output_path}"));

    Ok(())
}
