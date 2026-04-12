use anyhow::Result;

use crate::{
    git::{self, stats},
    map::{activity as activity_render, config::MapConfig, theme},
    ui,
};

pub fn run(
    output: Option<String>,
    commits: Option<usize>,
    theme_override: Option<&str>,
) -> Result<()> {
    git::assert_git_repo()?;
    let map_config = MapConfig::load()?;
    let commits = commits.unwrap_or(map_config.activity_commits);

    ui::section(format!("Building activity graph ({commits} commits)"));

    let commit_data = stats::timestamped_commits(commits)?;
    if commit_data.is_empty() {
        anyhow::bail!("no commits found");
    }

    let dates: Vec<String> = commit_data.iter().map(|c| c.timestamp.clone()).collect();

    ui::bullet(format!("{} commits loaded", commit_data.len()));

    let theme_name = theme_override.unwrap_or(&map_config.theme);
    let theme = theme::load_theme(theme_name)?;
    let doc = activity_render::render(&dates, None, theme);

    let output_path = output.unwrap_or_else(|| "aic-activity.svg".to_owned());
    svg::save(&output_path, &doc)?;
    ui::success(format!("Activity graph saved to {output_path}"));

    Ok(())
}
