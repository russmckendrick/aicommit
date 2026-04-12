use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use svg::Document;

use super::{
    palette::activity_colour,
    svg_util::{new_document, rect, rounded_rect, subtitle_text, text, title_text},
    theme::Theme,
};

/// Render a GitHub-style activity grid from commit dates.
pub fn render(dates: &[String], label: Option<&str>, theme: &Theme) -> Document {
    let padding = 40.0;
    let header_height = 60.0;
    let cell_size = 14.0;
    let cell_gap = 3.0;
    let weeks: i64 = 52;
    let days_per_week: i64 = 7;

    // Count commits per date
    let mut counts: BTreeMap<NaiveDate, usize> = BTreeMap::new();
    for ts in dates {
        if let Some(date) = parse_date(ts) {
            *counts.entry(date).or_insert(0) += 1;
        }
    }

    let max_count = counts.values().copied().max().unwrap_or(1).max(1);

    // Determine the date range: last 52 weeks ending today
    let today = chrono::Local::now().date_naive();
    let start = today - chrono::Duration::weeks(weeks);
    // Align to the start of the week (Monday)
    let start_weekday = start.weekday().num_days_from_monday();
    let start = start - chrono::Duration::days(start_weekday as i64);

    let day_labels_width = 30.0;
    let grid_width = weeks as f64 * (cell_size + cell_gap) + cell_size + cell_gap;
    let grid_height = days_per_week as f64 * (cell_size + cell_gap);
    let total_width = padding * 2.0 + day_labels_width + grid_width;
    let total_height = padding * 2.0 + header_height + grid_height + 60.0;

    let mut doc = new_document(total_width, total_height);
    doc = doc.add(rect(0.0, 0.0, total_width, total_height, &theme.background));

    let title = label.unwrap_or("Activity Graph");
    doc = doc.add(title_text(padding, padding + 20.0, title, theme));

    let total_commits: usize = counts.values().sum();
    let active_days = counts.len();
    doc = doc.add(subtitle_text(
        padding,
        padding + 38.0,
        &format!("{total_commits} commits across {active_days} active days"),
        theme,
    ));

    let grid_x = padding + day_labels_width;
    let grid_y = header_height + padding;

    // Day labels (Mon, Wed, Fri)
    let day_names = ["Mon", "", "Wed", "", "Fri", "", ""];
    for (d, name) in day_names.iter().enumerate() {
        if !name.is_empty() {
            doc = doc.add(
                text(
                    padding,
                    grid_y + d as f64 * (cell_size + cell_gap) + cell_size - 2.0,
                    name,
                    9.0,
                    theme,
                )
                .set("fill", theme.tertiary_text.as_str()),
            );
        }
    }

    // Month labels across the top
    let mut current_month = None;
    for week in 0..=weeks {
        let week_start = start + chrono::Duration::weeks(week);
        let month = week_start.month();
        if current_month != Some(month) {
            current_month = Some(month);
            let month_name = month_abbreviation(month);
            let mx = grid_x + week as f64 * (cell_size + cell_gap);
            doc = doc.add(
                text(mx, grid_y - 4.0, month_name, 9.0, theme)
                    .set("fill", theme.tertiary_text.as_str()),
            );
        }
    }

    // Draw cells
    for week in 0..=weeks {
        for day in 0..days_per_week {
            let date = start + chrono::Duration::days(week * 7 + day);
            if date > today {
                continue;
            }
            let count = counts.get(&date).copied().unwrap_or(0);
            let t = if count == 0 {
                0.0
            } else {
                (count as f64 / max_count as f64).max(0.15)
            };
            let colour = activity_colour(t, theme);
            let cx = grid_x + week as f64 * (cell_size + cell_gap);
            let cy = grid_y + day as f64 * (cell_size + cell_gap);
            doc = doc.add(rounded_rect(cx, cy, cell_size, cell_size, &colour, 2.0));
        }
    }

    // Legend
    let legend_y = grid_y + grid_height + 24.0;
    doc = doc
        .add(text(padding, legend_y, "Less", 9.0, theme).set("fill", theme.tertiary_text.as_str()));
    let legend_steps = 5;
    for i in 0..legend_steps {
        let t = i as f64 / (legend_steps - 1) as f64;
        let colour = activity_colour(t, theme);
        doc = doc.add(rounded_rect(
            padding + 32.0 + i as f64 * (cell_size + 2.0),
            legend_y - 10.0,
            cell_size,
            cell_size,
            &colour,
            2.0,
        ));
    }
    doc = doc.add(
        text(
            padding + 32.0 + legend_steps as f64 * (cell_size + 2.0) + 4.0,
            legend_y,
            "More",
            9.0,
            theme,
        )
        .set("fill", theme.tertiary_text.as_str()),
    );

    doc
}

fn parse_date(ts: &str) -> Option<NaiveDate> {
    // Accept ISO-8601 timestamps or plain dates
    let date_part = ts.split('T').next().unwrap_or(ts);
    NaiveDate::parse_from_str(date_part, "%Y-%m-%d").ok()
}

fn month_abbreviation(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_date_handles_iso8601() {
        let d = parse_date("2026-04-10T10:30:00+00:00").unwrap();
        assert_eq!(d.month(), 4);
        assert_eq!(d.day(), 10);
    }

    #[test]
    fn parse_date_handles_plain_date() {
        let d = parse_date("2026-01-15").unwrap();
        assert_eq!(d.month(), 1);
    }

    #[test]
    fn render_produces_svg_with_title() {
        let theme = crate::map::theme::load_theme("classic-light").unwrap();
        let dates = vec!["2026-04-10T10:00:00+00:00".to_owned()];
        let doc = render(&dates, None, theme);
        assert!(doc.to_string().contains("Activity Graph"));
    }
}
