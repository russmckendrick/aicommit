use std::collections::BTreeMap;

use svg::Document;

use super::{
    palette::directory_colour,
    svg_util::{
        group, new_document, rect, rounded_rect, subtitle_text, text, title_text, truncate_to_width,
    },
};
use crate::git::stats::TimestampedCommit;

/// Render a vertical zigzag timeline of commits, alternating left and right.
pub fn render(commits: &[TimestampedCommit], label: Option<&str>) -> Document {
    let padding = 40.0;
    let header_height = 60.0;
    let circle_radius = 24.0;
    let row_height = 110.0;
    let centre_x = 400.0;
    let arm_length = 60.0;
    let detail_width = 280.0;
    let n = commits.len().max(1);

    let total_width = centre_x * 2.0;
    let timeline_height = n as f64 * row_height;
    let legend_height = 50.0;
    let total_height = padding * 2.0 + header_height + timeline_height + legend_height + 10.0;

    let mut doc = new_document(total_width, total_height);
    doc = doc.add(rect(0.0, 0.0, total_width, total_height, "#fafafa"));

    let title = label.unwrap_or("Commit Timeline");
    doc = doc.add(title_text(padding, padding + 20.0, title));
    doc = doc.add(subtitle_text(
        padding,
        padding + 38.0,
        &format!("{} commits", commits.len()),
    ));

    // Build directory colour map
    let all_dirs = collect_directories(commits);
    let dir_colours: BTreeMap<String, String> = all_dirs
        .iter()
        .enumerate()
        .map(|(i, d)| (d.clone(), directory_colour(i)))
        .collect();

    let start_y = header_height + padding + 20.0;

    // Draw vertical centre line
    if commits.len() > 1 {
        let y1 = start_y + circle_radius;
        let y2 = start_y + (n - 1) as f64 * row_height + circle_radius;
        doc = doc.add(rect(centre_x - 1.5, y1, 3.0, y2 - y1, "#e0e0e0").set("rx", 1.5));
    }

    // Draw commits newest-first (reverse the oldest-first vec)
    let display_commits: Vec<&TimestampedCommit> = commits.iter().rev().collect();
    for (i, commit) in display_commits.iter().enumerate() {
        let cy = start_y + i as f64 * row_height;
        let is_left = i % 2 == 0;
        let g = render_commit(
            commit,
            centre_x,
            cy,
            circle_radius,
            arm_length,
            detail_width,
            is_left,
            &dir_colours,
        );
        doc = doc.add(g);
    }

    // Legend
    let legend_y = start_y + n as f64 * row_height + 16.0;
    doc = doc.add(text(padding, legend_y, "Directories:", 11.0).set("font-weight", "600"));
    let mut lx = padding + 90.0;
    for (dir, colour) in &dir_colours {
        doc = doc.add(rounded_rect(lx, legend_y - 10.0, 12.0, 12.0, colour, 2.0));
        doc = doc.add(text(lx + 16.0, legend_y, dir, 10.0));
        lx += dir.len() as f64 * 7.0 + 30.0;
        if lx > total_width - padding {
            break;
        }
    }

    doc
}

#[allow(clippy::too_many_arguments)]
fn render_commit(
    commit: &TimestampedCommit,
    centre_x: f64,
    y: f64,
    radius: f64,
    arm_length: f64,
    detail_width: f64,
    is_left: bool,
    dir_colours: &BTreeMap<String, String>,
) -> svg::node::element::Group {
    let mut g = group();
    let cy = y + radius;

    // --- Circle on the centre line with date inside ---
    g = g.add(
        svg::node::element::Circle::new()
            .set("cx", centre_x)
            .set("cy", cy)
            .set("r", radius)
            .set("fill", "#ffffff")
            .set("stroke", "#90a4ae")
            .set("stroke-width", 2),
    );

    // Date inside the circle (two lines: month+day, year)
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
            text(centre_x - 12.0, cy - 2.0, &month_day, 9.0)
                .set("fill", "#546e7a")
                .set("font-weight", "600"),
        );
        g = g.add(text(centre_x - 10.0, cy + 10.0, year, 8.0).set("fill", "#90a4ae"));
    } else {
        g = g.add(text(centre_x - 16.0, cy + 4.0, ts, 8.0).set("fill", "#546e7a"));
    }

    // --- Horizontal arm from circle to detail card ---
    let (arm_start, arm_end, detail_x) = if is_left {
        (
            centre_x - radius,
            centre_x - radius - arm_length,
            centre_x - radius - arm_length - detail_width,
        )
    } else {
        (
            centre_x + radius,
            centre_x + radius + arm_length,
            centre_x + radius + arm_length + 8.0,
        )
    };
    let arm_left = arm_start.min(arm_end);
    let arm_w = (arm_start - arm_end).abs();
    g = g.add(rect(arm_left, cy - 1.5, arm_w, 3.0, "#e0e0e0").set("rx", 1.5));

    // Small dot at the end of the arm
    g = g.add(
        svg::node::element::Circle::new()
            .set("cx", arm_end)
            .set("cy", cy)
            .set("r", 4.0)
            .set("fill", "#90a4ae"),
    );

    // --- Detail card ---
    let card_height = 70.0;
    let card_y = cy - card_height / 2.0;
    g = g.add(
        rounded_rect(detail_x, card_y, detail_width, card_height, "#ffffff", 6.0)
            .set("stroke", "#e0e0e0")
            .set("stroke-width", 1),
    );

    let text_x = detail_x + 12.0;
    let text_max = detail_width - 24.0;

    // Short hash
    let short_hash = &commit.hash[..7.min(commit.hash.len())];
    g = g.add(text(text_x, card_y + 16.0, short_hash, 8.0).set("fill", "#90a4ae"));

    // Subject
    let subject = truncate_to_width(&commit.subject, text_max, 10.0);
    g = g.add(
        text(text_x, card_y + 30.0, &subject, 10.0)
            .set("fill", "#1a1a1a")
            .set("font-weight", "600"),
    );

    // File dots
    let dot_y = card_y + 40.0;
    let dot_size = 10.0;
    let max_dots = ((text_max) / (dot_size + 3.0)).floor() as usize;
    for (j, file) in commit.files.iter().take(max_dots).enumerate() {
        let dir = top_directory(file);
        let colour = dir_colours
            .get(&dir)
            .cloned()
            .unwrap_or_else(|| "#cccccc".to_owned());
        let dx = text_x + j as f64 * (dot_size + 3.0);
        g = g.add(rounded_rect(dx, dot_y, dot_size, dot_size, &colour, 2.0).set("opacity", 0.9));
    }
    if commit.files.len() > max_dots {
        let overflow = format!("+{}", commit.files.len() - max_dots);
        let overflow_x = text_x + max_dots as f64 * (dot_size + 3.0) + 4.0;
        g = g.add(text(overflow_x, dot_y + dot_size, &overflow, 8.0).set("fill", "#999999"));
    }

    g
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
}
