use anyhow::Result;

use crate::{
    git::{self, stats},
    map::activity as activity_render,
    ui,
};

pub fn run(output: Option<String>, commits: usize) -> Result<()> {
    git::assert_git_repo()?;

    ui::section(format!("Building activity graph ({commits} commits)"));

    let commit_data = stats::timestamped_commits(commits)?;
    if commit_data.is_empty() {
        anyhow::bail!("no commits found");
    }

    let dates: Vec<String> = commit_data.iter().map(|c| c.timestamp.clone()).collect();

    ui::bullet(format!("{} commits loaded", commit_data.len()));

    let doc = activity_render::render(&dates, None);

    let output_path = output.unwrap_or_else(|| "aic-activity.svg".to_owned());
    svg::save(&output_path, &doc)?;
    ui::success(format!("Activity graph saved to {output_path}"));

    Ok(())
}
