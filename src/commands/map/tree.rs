use std::collections::BTreeMap;

use anyhow::Result;

use crate::{
    git::{self, stats},
    map::treemap,
    ui,
};

pub fn run(output: Option<String>, no_ai: bool) -> Result<()> {
    let _ = no_ai; // reserved for future AI annotation
    git::assert_git_repo()?;

    ui::section("Building codebase treemap");

    let files = stats::tracked_files()?;
    ui::bullet(format!("{} tracked files", files.len()));

    let spinner = ui::spinner("Counting lines");
    let mut file_sizes: BTreeMap<String, usize> = BTreeMap::new();
    for file in &files {
        let lines = stats::count_file_lines(file)?;
        if lines > 0 {
            file_sizes.insert(file.clone(), lines);
        }
    }
    spinner.finish_and_clear();

    ui::bullet(format!(
        "{} non-empty files, {} total lines",
        file_sizes.len(),
        file_sizes.values().sum::<usize>()
    ));

    let tree = treemap::build_tree(&file_sizes);
    let doc = treemap::render(&tree, None);

    let output_path = output.unwrap_or_else(|| "aic-treemap.svg".to_owned());
    svg::save(&output_path, &doc)?;
    ui::success(format!("Treemap saved to {output_path}"));

    Ok(())
}
