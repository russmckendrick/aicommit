use std::collections::BTreeMap;

use svg::Document;

use super::{
    palette::directory_colour,
    svg_util::{
        group, new_document, rect, rounded_rect, subtitle_text, text, title_text, truncate_to_width,
    },
};
use crate::git::stats::TimestampedCommit;

const CARD_WIDTH: f64 = 300.0;
const CARD_PAD: f64 = 10.0;
const LINE_HEIGHT: f64 = 13.0;
const SUBJECT_SIZE: f64 = 10.0;
const BODY_SIZE: f64 = 9.0;
const DOT_SIZE: f64 = 9.0;
const DOT_GAP: f64 = 3.0;
const ARM_LENGTH: f64 = 40.0;
const CIRCLE_RADIUS: f64 = 22.0;
/// Minimum vertical distance between circle centres.
const MIN_CIRCLE_SPACING: f64 = 52.0;

/// Render a vertical zigzag timeline of commits, alternating left and right.
pub fn render(commits: &[TimestampedCommit], label: Option<&str>) -> Document {
    let padding = 30.0;
    let header_height = 50.0;
    let centre_x = CARD_WIDTH + ARM_LENGTH + CIRCLE_RADIUS + padding + 10.0;
    let total_width = centre_x * 2.0;

    // Build directory colour map
    let all_dirs = collect_directories(commits);
    let dir_colours: BTreeMap<String, String> = all_dirs
        .iter()
        .enumerate()
        .map(|(i, d)| (d.clone(), directory_colour(i)))
        .collect();

    // Pre-calculate card heights to position everything
    let text_max = CARD_WIDTH - CARD_PAD * 2.0;
    let card_heights: Vec<f64> = commits
        .iter()
        .rev()
        .map(|c| card_height(c, text_max))
        .collect();

    let start_y = header_height + padding + 10.0;
    let mut y_positions: Vec<f64> = Vec::with_capacity(card_heights.len());
    if !card_heights.is_empty() {
        y_positions.push(start_y);
    }
    for i in 1..card_heights.len() {
        let prev_card_bottom = y_positions[i - 1] + card_heights[i - 1];
        let prev_circle_y = y_positions[i - 1] + card_heights[i - 1] / 2.0;

        // Same-side card is 2 positions back (if it exists)
        let same_side_bottom = if i >= 2 {
            y_positions[i - 2] + card_heights[i - 2] + 6.0
        } else {
            start_y
        };

        // Circle must be at least MIN_CIRCLE_SPACING below previous circle
        let min_from_circle = prev_circle_y + MIN_CIRCLE_SPACING - card_heights[i] / 2.0;
        // Card must not overlap same-side card
        let min_from_same_side = same_side_bottom;
        // Card must start after previous card's top (avoid arm crossing)
        let min_from_prev = prev_card_bottom - card_heights[i] * 0.4;

        let y = min_from_circle.max(min_from_same_side).max(min_from_prev);
        y_positions.push(y);
    }
    let timeline_bottom = if let Some(&last_y) = y_positions.last() {
        last_y + card_heights.last().copied().unwrap_or(0.0) + 10.0
    } else {
        start_y
    };
    let legend_height = 40.0;
    let total_height = timeline_bottom + legend_height + padding;

    let mut doc = new_document(total_width, total_height);
    doc = doc.add(rect(0.0, 0.0, total_width, total_height, "#fafafa"));

    let title = label.unwrap_or("Commit Timeline");
    doc = doc.add(title_text(padding, padding + 18.0, title));
    doc = doc.add(subtitle_text(
        padding,
        padding + 34.0,
        &format!("{} commits", commits.len()),
    ));

    // Draw vertical centre line
    if commits.len() > 1 {
        let y1 = start_y + CIRCLE_RADIUS;
        let last = y_positions.len() - 1;
        let y2 = y_positions[last] + card_heights[last] / 2.0;
        doc = doc.add(rect(centre_x - 1.5, y1, 3.0, y2 - y1, "#e0e0e0").set("rx", 1.5));
    }

    // Draw commits newest-first
    let display_commits: Vec<&TimestampedCommit> = commits.iter().rev().collect();
    for (i, commit) in display_commits.iter().enumerate() {
        let is_left = i % 2 == 0;
        let g = render_commit(
            commit,
            centre_x,
            y_positions[i],
            card_heights[i],
            text_max,
            is_left,
            &dir_colours,
        );
        doc = doc.add(g);
    }

    // Legend
    let legend_y = timeline_bottom + 10.0;
    doc = doc.add(text(padding, legend_y, "Directories:", 10.0).set("font-weight", "600"));
    let mut lx = padding + 80.0;
    for (dir, colour) in &dir_colours {
        doc = doc.add(rounded_rect(lx, legend_y - 9.0, 10.0, 10.0, colour, 2.0));
        doc = doc.add(text(lx + 14.0, legend_y, dir, 9.0));
        lx += dir.len() as f64 * 6.0 + 28.0;
        if lx > total_width - padding {
            break;
        }
    }

    doc
}

/// Calculate the height of a card for a given commit.
fn card_height(commit: &TimestampedCommit, text_max: f64) -> f64 {
    let mut h = CARD_PAD; // top padding
    h += LINE_HEIGHT; // hash line
    h += LINE_HEIGHT; // subject line

    // Body lines
    let body_lines = wrap_text(&commit.body, text_max, BODY_SIZE);
    h += body_lines.len() as f64 * LINE_HEIGHT;

    // Gap before dots
    if !commit.files.is_empty() {
        h += 4.0;
        h += DOT_SIZE + 2.0;
    }

    h += CARD_PAD; // bottom padding
    h.max(CIRCLE_RADIUS * 2.0 + 8.0) // minimum height to contain the circle
}

#[allow(clippy::too_many_arguments)]
fn render_commit(
    commit: &TimestampedCommit,
    centre_x: f64,
    y: f64,
    card_h: f64,
    text_max: f64,
    is_left: bool,
    dir_colours: &BTreeMap<String, String>,
) -> svg::node::element::Group {
    let mut g = group();
    let cy = y + card_h / 2.0;

    // --- Circle on the centre line with date inside ---
    g = g.add(
        svg::node::element::Circle::new()
            .set("cx", centre_x)
            .set("cy", cy)
            .set("r", CIRCLE_RADIUS)
            .set("fill", "#ffffff")
            .set("stroke", "#90a4ae")
            .set("stroke-width", 2),
    );

    let ts = commit
        .timestamp
        .split('T')
        .next()
        .unwrap_or(&commit.timestamp);
    let date_parts: Vec<&str> = ts.split('-').collect();
    if date_parts.len() == 3 {
        let month_day = format!("{}/{}", date_parts[1], date_parts[2]);
        let year = date_parts[0];
        g = g.add(
            text(centre_x - 11.0, cy - 2.0, &month_day, 8.0)
                .set("fill", "#546e7a")
                .set("font-weight", "600"),
        );
        g = g.add(text(centre_x - 10.0, cy + 9.0, year, 7.0).set("fill", "#90a4ae"));
    } else {
        g = g.add(text(centre_x - 14.0, cy + 3.0, ts, 7.0).set("fill", "#546e7a"));
    }

    // --- Horizontal arm ---
    let (arm_start, arm_end, detail_x) = if is_left {
        (
            centre_x - CIRCLE_RADIUS,
            centre_x - CIRCLE_RADIUS - ARM_LENGTH,
            centre_x - CIRCLE_RADIUS - ARM_LENGTH - CARD_WIDTH,
        )
    } else {
        (
            centre_x + CIRCLE_RADIUS,
            centre_x + CIRCLE_RADIUS + ARM_LENGTH,
            centre_x + CIRCLE_RADIUS + ARM_LENGTH + 6.0,
        )
    };
    let arm_left = arm_start.min(arm_end);
    let arm_w = (arm_start - arm_end).abs();
    g = g.add(rect(arm_left, cy - 1.0, arm_w, 2.0, "#e0e0e0").set("rx", 1.0));

    // Small dot at the arm end
    g = g.add(
        svg::node::element::Circle::new()
            .set("cx", arm_end)
            .set("cy", cy)
            .set("r", 3.0)
            .set("fill", "#90a4ae"),
    );

    // --- Detail card ---
    let card_y = y;
    g = g.add(
        rounded_rect(detail_x, card_y, CARD_WIDTH, card_h, "#ffffff", 5.0)
            .set("stroke", "#e0e0e0")
            .set("stroke-width", 1),
    );

    let tx = detail_x + CARD_PAD;
    let mut ty = card_y + CARD_PAD;

    // Short hash
    let short_hash = &commit.hash[..7.min(commit.hash.len())];
    ty += LINE_HEIGHT;
    g = g.add(text(tx, ty - 3.0, short_hash, 8.0).set("fill", "#90a4ae"));

    // Subject
    let subject = truncate_to_width(&commit.subject, text_max, SUBJECT_SIZE);
    ty += LINE_HEIGHT;
    g = g.add(
        text(tx, ty - 3.0, &subject, SUBJECT_SIZE)
            .set("fill", "#1a1a1a")
            .set("font-weight", "600"),
    );

    // Body lines
    let body_lines = wrap_text(&commit.body, text_max, BODY_SIZE);
    for line in &body_lines {
        ty += LINE_HEIGHT;
        g = g.add(text(tx, ty - 3.0, line, BODY_SIZE).set("fill", "#666666"));
    }

    // File dots
    if !commit.files.is_empty() {
        ty += 4.0 + DOT_SIZE;
        let max_dots = ((text_max) / (DOT_SIZE + DOT_GAP)).floor() as usize;
        for (j, file) in commit.files.iter().take(max_dots).enumerate() {
            let dir = top_directory(file);
            let colour = dir_colours
                .get(&dir)
                .cloned()
                .unwrap_or_else(|| "#cccccc".to_owned());
            let dx = tx + j as f64 * (DOT_SIZE + DOT_GAP);
            g = g.add(
                rounded_rect(dx, ty - DOT_SIZE, DOT_SIZE, DOT_SIZE, &colour, 2.0)
                    .set("opacity", 0.9),
            );
        }
        if commit.files.len() > max_dots {
            let overflow = format!("+{}", commit.files.len() - max_dots);
            let overflow_x = tx + max_dots as f64 * (DOT_SIZE + DOT_GAP) + 2.0;
            g = g.add(text(overflow_x, ty, &overflow, 7.0).set("fill", "#999999"));
        }
    }

    g
}

/// Word-wrap text to fit within a pixel width at a given font size.
fn wrap_text(input: &str, max_width: f64, font_size: f64) -> Vec<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let char_width = font_size * 0.6;
    let max_chars = (max_width / char_width).floor() as usize;
    let mut lines = Vec::new();

    for raw_line in trimmed.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.len() <= max_chars {
            lines.push(line.to_owned());
        } else {
            // Word-wrap
            let words: Vec<&str> = line.split_whitespace().collect();
            let mut current = String::new();
            for word in words {
                if current.is_empty() {
                    current = word.to_owned();
                } else if current.len() + 1 + word.len() <= max_chars {
                    current.push(' ');
                    current.push_str(word);
                } else {
                    lines.push(current);
                    current = word.to_owned();
                }
            }
            if !current.is_empty() {
                lines.push(current);
            }
        }
    }
    lines
}

fn collect_directories(commits: &[TimestampedCommit]) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for commit in commits {
        for file in &commit.files {
            if let Some(pos) = file.find('/') {
                let dir = &file[..pos];
                if seen.insert(dir.to_owned()) {
                    result.push(dir.to_owned());
                }
            }
        }
    }
    result
}

fn top_directory(file: &str) -> String {
    file.split('/').next().unwrap_or("").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_directory_extracts_first_component() {
        assert_eq!(top_directory("src/main.rs"), "src");
        assert_eq!(top_directory("README.md"), "README.md");
    }

    #[test]
    fn wrap_text_handles_short_lines() {
        let lines = wrap_text("hello world", 200.0, 10.0);
        assert_eq!(lines, vec!["hello world"]);
    }

    #[test]
    fn wrap_text_wraps_long_lines() {
        let lines = wrap_text("this is a longer line that should wrap", 100.0, 10.0);
        assert!(lines.len() >= 2);
    }

    #[test]
    fn wrap_text_empty_returns_empty() {
        assert!(wrap_text("", 200.0, 10.0).is_empty());
        assert!(wrap_text("   ", 200.0, 10.0).is_empty());
    }
}
