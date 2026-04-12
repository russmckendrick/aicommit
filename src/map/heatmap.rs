use std::collections::BTreeMap;

use svg::Document;

use super::{
    palette::heat_colour,
    svg_util::{
        group, new_document, rect, rounded_rect, subtitle_text, text, title_text, truncate_to_width,
    },
};

/// Render a heatmap of file modification frequency.
pub fn render(freq: &BTreeMap<String, usize>, commits: usize, label: Option<&str>) -> Document {
    let padding = 40.0;
    let header_height = 60.0;
    let row_height = 22.0;
    let bar_max_width = 500.0;
    let label_width = 340.0;
    let count_width = 60.0;

    let max_freq = freq.values().copied().max().unwrap_or(1).max(1);

    // Sort by frequency descending, cap at 50 files
    let mut sorted: Vec<(&String, &usize)> = freq.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    let capped = sorted.len().min(50);
    let sorted = &sorted[..capped];

    let map_height = capped as f64 * row_height + 20.0;
    let total_width = padding * 2.0 + label_width + bar_max_width + count_width + 20.0;
    let total_height = map_height + header_height + padding * 2.0 + 40.0;

    let mut doc = new_document(total_width, total_height);
    doc = doc.add(rect(0.0, 0.0, total_width, total_height, "#fafafa"));

    let title = label.unwrap_or("Change Heatmap");
    doc = doc.add(title_text(padding, padding + 20.0, title));
    doc = doc.add(subtitle_text(
        padding,
        padding + 38.0,
        &format!(
            "Top {} files by modification frequency over {} commits",
            capped, commits
        ),
    ));

    let start_y = header_height + padding + 10.0;

    for (i, (file, count)) in sorted.iter().enumerate() {
        let y = start_y + i as f64 * row_height;
        let t = **count as f64 / max_freq as f64;
        let colour = heat_colour(t);
        let bar_width = (t * bar_max_width).max(2.0);

        let mut row = group();

        // File name
        let name = truncate_to_width(file, label_width - 10.0, 10.0);
        row = row.add(text(padding, y + 14.0, &name, 10.0));

        // Heat bar
        row = row.add(rounded_rect(
            padding + label_width,
            y + 3.0,
            bar_width,
            row_height - 6.0,
            &colour,
            3.0,
        ));

        // Count label
        row = row.add(
            text(
                padding + label_width + bar_max_width + 8.0,
                y + 14.0,
                &count.to_string(),
                10.0,
            )
            .set("fill", "#666666"),
        );

        doc = doc.add(row);
    }

    // Legend
    let legend_y = start_y + capped as f64 * row_height + 20.0;
    doc = doc.add(text(padding, legend_y, "Low", 9.0).set("fill", "#999999"));
    let legend_steps = 10;
    for i in 0..legend_steps {
        let t = i as f64 / (legend_steps - 1) as f64;
        let colour = heat_colour(t);
        doc = doc.add(rounded_rect(
            padding + 30.0 + i as f64 * 18.0,
            legend_y - 10.0,
            16.0,
            12.0,
            &colour,
            2.0,
        ));
    }
    doc = doc.add(
        text(
            padding + 30.0 + legend_steps as f64 * 18.0 + 4.0,
            legend_y,
            "High",
            9.0,
        )
        .set("fill", "#999999"),
    );

    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_produces_non_empty_svg() {
        let mut freq = BTreeMap::new();
        freq.insert("src/main.rs".to_owned(), 5);
        freq.insert("src/lib.rs".to_owned(), 2);
        let doc = render(&freq, 10, None);
        let svg_string = doc.to_string();
        assert!(svg_string.contains("Change Heatmap"));
        assert!(svg_string.contains("src/main.rs"));
    }
}
