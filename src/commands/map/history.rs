use anyhow::Result;

use crate::{
    git::{self, stats},
    map::timeline,
    ui,
};

pub fn run(output: Option<String>, commits: usize) -> Result<()> {
    git::assert_git_repo()?;

    ui::section(format!("Building commit timeline ({commits} commits)"));

    let commit_data = stats::timestamped_commits(commits)?;
    if commit_data.is_empty() {
        anyhow::bail!("no commits found");
    }

    ui::bullet(format!("{} commits loaded", commit_data.len()));

    let doc = timeline::render(&commit_data, None);

    let output_path = output.unwrap_or_else(|| "aic-timeline.svg".to_owned());
    svg::save(&output_path, &doc)?;
    ui::success(format!("Timeline saved to {output_path}"));

    Ok(())
}
