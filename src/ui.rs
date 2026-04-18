use std::{collections::BTreeMap, fmt::Display, path::Path};

use anyhow::{Error, Result};
use console::{Alignment, Term, pad_str, style};
use indicatif::{ProgressBar, ProgressStyle};
use inquire::{Confirm, Editor, InquireError, MultiSelect, Select, Text};
use textwrap::Options;

const DEFAULT_MIN_CARD_WIDTH: usize = 44;
const MAX_CARD_WIDTH: usize = 92;
const DEFAULT_FILE_LIMIT: usize = 5;
const DEFAULT_ROOT_LIMIT: usize = 4;

pub fn info(message: impl AsRef<str>) {
    println!("{}", message.as_ref());
}

pub fn success(message: impl AsRef<str>) {
    println!("{} {}", style("✔").green(), style(message.as_ref()).green());
}

pub fn warn(message: impl AsRef<str>) {
    eprintln!(
        "{} {}",
        style("warning:").yellow(),
        style(message.as_ref()).yellow()
    );
}

pub fn section(title: impl AsRef<str>) {
    println!("{} {}", style("◇").cyan(), style(title.as_ref()).bold());
}

pub fn session_step(message: impl AsRef<str>) {
    println!(
        "{} {}",
        style("•").cyan().dim(),
        style(message.as_ref()).dim()
    );
}

pub fn blank_line() {
    println!();
}

pub fn bullet(message: impl AsRef<str>) {
    println!("  {} {}", style("•").cyan().dim(), message.as_ref());
}

pub fn secondary(message: impl AsRef<str>) {
    for line in message.as_ref().lines() {
        println!("  {}", style(line).dim());
    }
}

pub fn metadata_row(items: &[String]) {
    if items.is_empty() {
        return;
    }

    secondary(items.join("  •  "));
}

pub fn headline(message: impl AsRef<str>) {
    println!("  {}", style(message.as_ref()).bold());
}

pub fn file_list(title: impl AsRef<str>, files: &[String]) {
    let title = title.as_ref();
    section(format!("{title} ({})", file_count_label(files.len())));
    for line in summarize_files(files, DEFAULT_FILE_LIMIT, DEFAULT_ROOT_LIMIT) {
        bullet(line);
    }
}

pub fn file_metadata(files: &[String]) {
    let summary = summarize_roots(files, DEFAULT_ROOT_LIMIT);
    let mut items = vec![file_count_label(files.len())];
    if !summary.is_empty() {
        items.push(format!("paths: {summary}"));
    }
    metadata_row(&items);
}

pub fn commit_message(message: impl AsRef<str>) {
    for (index, line) in message.as_ref().lines().enumerate() {
        if index == 0 {
            println!("  {}", style(line).bold());
        } else if line.trim().is_empty() {
            println!();
        } else {
            println!("  {line}");
        }
    }
}

pub fn spinner(message: impl Into<String>) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb.set_message(message.into());
    pb
}

pub fn primary_card(title: &str, body: &str) {
    for line in render_card_lines(title, body, card_width()) {
        println!("{line}");
    }
}

pub fn confirm(message: &str, default: bool) -> Result<bool> {
    Ok(Confirm::new(message).with_default(default).prompt()?)
}

pub fn select<T>(message: &str, options: Vec<T>) -> Result<T>
where
    T: Clone + Display,
{
    Ok(Select::new(message, options).prompt()?)
}

pub fn multiselect(message: &str, options: Vec<String>) -> Result<Vec<String>> {
    Ok(MultiSelect::new(message, options).prompt()?)
}

pub fn markdown(text: &str) {
    let skin = termimad::MadSkin::default();
    skin.print_text(text);
}

pub fn text(message: &str, initial: Option<&str>) -> Result<String> {
    let prompt = Text::new(message);
    let prompt = if let Some(initial) = initial {
        prompt.with_initial_value(initial)
    } else {
        prompt
    };
    Ok(prompt.prompt()?)
}

pub fn editor(message: &str, initial: &str) -> Result<String> {
    Ok(Editor::new(message)
        .with_predefined_text(initial)
        .with_file_extension(".md")
        .prompt()?)
}

pub fn is_prompt_cancelled(error: &Error) -> bool {
    matches!(
        error.downcast_ref::<InquireError>(),
        Some(InquireError::OperationCanceled | InquireError::OperationInterrupted)
    )
}

pub(crate) fn file_count_label(count: usize) -> String {
    match count {
        1 => "1 file".to_owned(),
        value => format!("{value} files"),
    }
}

pub(crate) fn summarize_files(
    files: &[String],
    direct_limit: usize,
    root_limit: usize,
) -> Vec<String> {
    let mut lines = files.iter().take(direct_limit).cloned().collect::<Vec<_>>();

    let remaining = files.len().saturating_sub(direct_limit);
    if remaining > 0 {
        let roots = summarize_roots(files, root_limit);
        if roots.is_empty() {
            lines.push(format!("+{remaining} more"));
        } else {
            lines.push(format!("+{remaining} more across {roots}"));
        }
    }

    lines
}

pub(crate) fn summarize_roots(files: &[String], limit: usize) -> String {
    let mut groups = BTreeMap::<String, usize>::new();
    for file in files {
        *groups.entry(file_root(file)).or_default() += 1;
    }

    let mut groups = groups.into_iter().collect::<Vec<_>>();
    groups.sort_by(|(left_root, left_count), (right_root, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left_root.cmp(right_root))
    });

    groups
        .into_iter()
        .map(|(root, count)| format!("{root} ({count})"))
        .take(limit)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn render_card_lines(title: &str, body: &str, width: usize) -> Vec<String> {
    let width = width.clamp(DEFAULT_MIN_CARD_WIDTH, MAX_CARD_WIDTH);
    let inner_width = width.saturating_sub(4).max(20);
    let title = format!(" {title} ");
    let title_width = console::measure_text_width(&title);
    let top_fill = "─".repeat(inner_width.saturating_sub(title_width));
    let mut lines = vec![format!("  {}", style(format!("┌{title}{top_fill}┐")).dim())];

    let mut wrapped_lines = Vec::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            wrapped_lines.push(String::new());
            continue;
        }

        let options = Options::new(inner_width)
            .break_words(false)
            .word_separator(textwrap::WordSeparator::AsciiSpace);
        wrapped_lines.extend(
            textwrap::wrap(line, &options)
                .into_iter()
                .map(|segment| segment.into_owned()),
        );
    }

    if wrapped_lines.is_empty() {
        wrapped_lines.push(String::new());
    }

    for line in wrapped_lines {
        let content = pad_str(&line, inner_width, Alignment::Left, None);
        lines.push(format!(
            "  {}{}{}",
            style("│ ").dim(),
            content,
            style(" │").dim()
        ));
    }

    lines.push(format!(
        "  {}",
        style(format!("└{}┘", "─".repeat(inner_width))).dim()
    ));
    lines
}

fn card_width() -> usize {
    let (_, columns) = Term::stdout().size();
    usize::from(columns)
        .saturating_sub(4)
        .clamp(DEFAULT_MIN_CARD_WIDTH, MAX_CARD_WIDTH)
}

fn file_root(file: &str) -> String {
    let path = Path::new(file);
    let mut parts = path.iter().filter_map(|part| part.to_str());
    match (parts.next(), parts.next()) {
        (Some(first), Some(_)) => format!("{first}/"),
        (None, _) => "root files".to_owned(),
        _ => "root files".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_files_truncates_large_lists() {
        let files = vec![
            "src/main.rs".to_owned(),
            "src/lib.rs".to_owned(),
            "src/ui.rs".to_owned(),
            "docs/usage.md".to_owned(),
            "README.md".to_owned(),
            "tests/cli.rs".to_owned(),
        ];

        let lines = summarize_files(&files, 3, 3);
        assert_eq!(lines[0], "src/main.rs");
        assert_eq!(lines[1], "src/lib.rs");
        assert_eq!(lines[2], "src/ui.rs");
        assert_eq!(
            lines[3],
            "+3 more across src/ (3), docs/ (1), root files (1)"
        );
    }

    #[test]
    fn summarize_roots_groups_top_level_paths() {
        let files = vec![
            "src/main.rs".to_owned(),
            "src/lib.rs".to_owned(),
            "README.md".to_owned(),
            "docs/usage.md".to_owned(),
        ];

        assert_eq!(
            summarize_roots(&files, 4),
            "src/ (2), docs/ (1), root files (1)"
        );
    }

    #[test]
    fn render_card_lines_wraps_long_content() {
        let lines = render_card_lines(
            "Generated commit",
            "feat(ui): add a much longer line that should wrap inside the bordered card cleanly",
            44,
        );

        assert!(lines[0].contains("Generated commit"));
        assert!(lines.len() > 4);
        assert!(lines.iter().all(|line| line.starts_with("  ")));
    }
}
