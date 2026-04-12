use anyhow::Result;

use crate::{
    git::{self, stats},
    map::{config::MapConfig, heatmap, theme},
    ui,
};

pub fn run(
    output: Option<String>,
    commits: Option<usize>,
    theme_override: Option<&str>,
) -> Result<()> {
    git::assert_git_repo()?;
    let map_config = MapConfig::load()?;
    let commits = commits.unwrap_or(map_config.heat_commits);

    ui::section(format!("Building change heatmap ({commits} commits)"));

    let freq = stats::file_change_frequency(commits)?;
    if freq.is_empty() {
        anyhow::bail!("no file changes found in the last {commits} commits");
    }

    ui::bullet(format!("{} files changed", freq.len()));

    let theme_name = theme_override.unwrap_or(&map_config.theme);
    let theme = theme::load_theme(theme_name)?;
    let doc = heatmap::render(&freq, commits, None, theme);

    let output_path = output.unwrap_or_else(|| "aic-heatmap.svg".to_owned());
    svg::save(&output_path, &doc)?;
    ui::success(format!("Heatmap saved to {output_path}"));

    Ok(())
}
