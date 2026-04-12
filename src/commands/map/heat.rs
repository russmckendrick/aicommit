use anyhow::Result;

use crate::{
    git::{self, stats},
    map::heatmap,
    ui,
};

pub fn run(output: Option<String>, commits: usize) -> Result<()> {
    git::assert_git_repo()?;

    ui::section(format!("Building change heatmap ({commits} commits)"));

    let freq = stats::file_change_frequency(commits)?;
    if freq.is_empty() {
        anyhow::bail!("no file changes found in the last {commits} commits");
    }

    ui::bullet(format!("{} files changed", freq.len()));

    let doc = heatmap::render(&freq, commits, None);

    let output_path = output.unwrap_or_else(|| "aic-heatmap.svg".to_owned());
    svg::save(&output_path, &doc)?;
    ui::success(format!("Heatmap saved to {output_path}"));

    Ok(())
}
