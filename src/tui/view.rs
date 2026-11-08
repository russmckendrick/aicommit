//! Rendering for the TUI screens. Every frame also records hit-test
//! rectangles in `app.hits` so mouse clicks map back to rows and cards.

use std::time::Duration;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

use super::{
    app::{Action, App, FileMarker, Focus, Gen, Hit, RowKind, Screen, SplitState, SplitStatus},
    highlight::{hunk_lines, hunk_stats},
    theme::Palette,
};

pub const MIN_WIDTH: u16 = 60;
pub const MIN_HEIGHT: u16 = 16;
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn spinner(app: &App) -> &'static str {
    SPINNER[(app.started.elapsed().as_millis() / 80) as usize % SPINNER.len()]
}

fn block<'a>(palette: &Palette, title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused {
            palette.accent
        } else {
            palette.border
        }))
        .title(title)
        .padding(ratatui::widgets::Padding::horizontal(1))
}

/// Indeterminate "shimmer" bar: a bright segment sweeping across.
fn shimmer(app: &App, width: usize) -> Vec<Span<'static>> {
    let palette = &app.palette;
    let period = (width + 8) as u128;
    let head = (app.started.elapsed().as_millis() / 45 % period) as isize - 4;
    (0..width)
        .map(|i| {
            let distance = (i as isize - head).abs();
            let color = if distance <= 3 {
                palette.accent
            } else {
                palette.border
            };
            Span::styled("━", Style::default().fg(color))
        })
        .collect()
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    app.hits.clear();
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        draw_too_small(frame, app, area);
        return;
    }

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .areas(area.inner(Margin::new(1, 0)));

    draw_header(frame, app, header);
    match app.screen {
        Screen::Commit => draw_commit(frame, app, body),
        Screen::Split => draw_split(frame, app, body),
    }
    draw_footer(frame, app, footer);

    if let Some((message, error, at)) = &app.toast {
        draw_toast(frame, &app.palette, body, message, *error, at.elapsed());
    }
    if app.palette_overlay.is_some() {
        draw_palette(frame, app, area);
    }
    if app.help {
        draw_help(frame, app, area);
    }
}

fn draw_too_small(frame: &mut Frame, app: &App, area: Rect) {
    let palette = &app.palette;
    let text = vec![
        Line::styled("◇ aic", Style::default().fg(palette.accent)),
        Line::from(""),
        Line::styled(
            "Terminal too small",
            Style::default().fg(palette.fg).add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            format!(
                "Need {MIN_WIDTH}×{MIN_HEIGHT}, have {}×{}",
                area.width, area.height
            ),
            Style::default().fg(palette.muted),
        ),
    ];
    let height = text.len() as u16;
    let y = area.y + area.height.saturating_sub(height) / 2;
    frame.render_widget(
        Paragraph::new(text).alignment(Alignment::Center),
        Rect::new(area.x, y, area.width, height.min(area.height)),
    );
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let palette = &app.palette;
    let dot = Span::styled(" · ", Style::default().fg(palette.faint));
    let branch_glyph = if app.input.config.icons == "nerd" {
        "\u{e725} "
    } else {
        "⎇ "
    };
    let mut left = vec![
        Span::styled(
            "◇ aic",
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD),
        ),
        dot.clone(),
        Span::styled(
            match app.screen {
                Screen::Commit => "commit",
                Screen::Split => "split",
            },
            Style::default().fg(palette.fg),
        ),
    ];
    if let Some(branch) = &app.branch {
        left.push(dot.clone());
        left.push(Span::styled(
            format!("{branch_glyph}{branch}"),
            Style::default().fg(palette.keyword),
        ));
    }
    if let Some(ticket) = &app.ticket {
        left.push(dot.clone());
        left.push(Span::styled("ticket ", Style::default().fg(palette.muted)));
        left.push(Span::styled(
            ticket.clone(),
            Style::default().fg(palette.fg),
        ));
    }

    let right = match (&app.screen, &app.split) {
        (Screen::Split, Some(split)) => {
            let hunks = split.staged.hunk_ids().len();
            let groups = split.groups.iter().filter(|g| !g.hunks.is_empty()).count();
            format!(
                "{hunks} hunks · {} files → {groups} commits",
                split.staged.files.len()
            )
        }
        _ => {
            let tokens = if app.prompt_tokens >= 1000 {
                format!("{:.1}k tok", app.prompt_tokens as f64 / 1000.0)
            } else {
                format!("{} tok", app.prompt_tokens)
            };
            format!(
                "{}/{} · {tokens}",
                app.input.config.ai_provider, app.input.config.model
            )
        }
    };
    frame.render_widget(Paragraph::new(Line::from(left)), area);
    frame.render_widget(
        Paragraph::new(Line::styled(right, Style::default().fg(palette.muted)))
            .alignment(Alignment::Right),
        area,
    );
}

fn hints_for(app: &App) -> Vec<(&'static str, &'static str)> {
    if let Some(split) = &app.split
        && app.screen == Screen::Split
    {
        if split.editing.is_some() {
            return vec![("esc", "save message")];
        }
        return match split.status {
            SplitStatus::Ready => vec![
                ("↑↓", "select"),
                ("tab", "column"),
                ("←→", "move hunk"),
                ("n", "new group"),
                ("m", "merge"),
                ("r", "regen message"),
                ("e", "edit"),
                ("enter", "commit all"),
                ("esc", "one commit"),
            ],
            _ => vec![("esc", "back")],
        };
    }
    if app.focus == Focus::Message {
        return vec![("esc", "done editing"), ("ctrl-s", "commit")];
    }
    let mut hints = vec![("space", "stage"), ("g", "generate"), ("e", "edit")];
    hints.extend([("s", "split"), ("c", "commit")]);
    if app.is_generating() {
        hints.push(("esc", "cancel"));
    }
    hints.extend([("t", "theme"), ("^k", "palette"), ("?", "help")]);
    hints
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let palette = &app.palette;
    let mut spans = Vec::new();
    let mut width = 0usize;
    for (key, label) in hints_for(app) {
        let needed = key.chars().count() + label.chars().count() + 4;
        if width + needed > area.width as usize {
            break;
        }
        width += needed;
        spans.push(Span::styled(
            format!(" {key} "),
            Style::default().fg(palette.bright).bg(palette.key_bg),
        ));
        spans.push(Span::styled(
            format!(" {label} "),
            Style::default().fg(palette.muted),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ---------------------------------------------------------------------
// Commit screen
// ---------------------------------------------------------------------

fn draw_commit(frame: &mut Frame, app: &mut App, area: Rect) {
    let message_height = message_height(app, area.height);
    let [top, message] =
        Layout::vertical([Constraint::Min(5), Constraint::Length(message_height)]).areas(area);
    let (tree, diff) = if area.width >= 100 {
        let tree_width = (area.width / 3).clamp(32, 48);
        let [tree, diff] =
            Layout::horizontal([Constraint::Length(tree_width), Constraint::Min(20)]).areas(top);
        (tree, diff)
    } else {
        let [tree, diff] =
            Layout::vertical([Constraint::Percentage(40), Constraint::Percentage(60)]).areas(top);
        (tree, diff)
    };
    draw_tree(frame, app, tree);
    draw_diff(frame, app, diff);
    draw_message(frame, app, message);
}

fn message_height(app: &App, available: u16) -> u16 {
    let lines = app.message.lines().len() as u16;
    (lines + 2).clamp(5, (available / 3).max(5))
}

fn draw_tree(frame: &mut Frame, app: &mut App, area: Rect) {
    let palette = app.palette.clone();
    let outer = block(&palette, "", false);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let height = inner.height as usize;
    let offset = if app.selected >= height {
        app.selected + 1 - height
    } else {
        0
    };
    let width = inner.width as usize;
    let mut lines = Vec::new();
    for (index, row) in app.rows.iter().enumerate().skip(offset).take(height) {
        let selected = index == app.selected;
        let line = tree_line(&palette, row, selected, width);
        app.hits.push((
            Rect::new(inner.x, inner.y + lines.len() as u16, inner.width, 1),
            Hit::TreeRow(index),
        ));
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn truncate_left(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count <= width {
        return text.to_owned();
    }
    let keep = width.saturating_sub(1);
    format!("…{}", text.chars().skip(count - keep).collect::<String>())
}

fn truncate_right(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn tree_line(palette: &Palette, row: &RowKind, selected: bool, width: usize) -> Line<'static> {
    let bar = if selected {
        Span::styled("▌", Style::default().fg(palette.accent))
    } else {
        Span::raw(" ")
    };
    let base = if selected {
        Style::default().bg(palette.selection)
    } else {
        Style::default()
    };
    let mut spans = vec![bar];
    match row {
        RowKind::Section { title } => {
            return Line::from(vec![Span::styled(
                title.clone(),
                Style::default()
                    .fg(palette.muted)
                    .add_modifier(Modifier::BOLD),
            )]);
        }
        RowKind::Empty { text } => {
            return Line::from(vec![Span::styled(
                format!("  {text}"),
                Style::default()
                    .fg(palette.faint)
                    .add_modifier(Modifier::ITALIC),
            )]);
        }
        RowKind::File { path, marker, .. } => {
            let (glyph, color) = match marker {
                FileMarker::Full => ("●", palette.add),
                FileMarker::Partial => ("◐", palette.warn),
                FileMarker::Unstaged => ("○", palette.faint),
            };
            spans.push(Span::styled(
                format!("{glyph} "),
                Style::default().fg(color),
            ));
            spans.push(Span::styled(
                truncate_left(path, width.saturating_sub(3)),
                Style::default().fg(palette.fg),
            ));
        }
        RowKind::Hunk {
            id,
            staged,
            label,
            stats,
            ..
        } => {
            let number = id.rsplit('#').next().unwrap_or("1");
            let (glyph, color) = if *staged {
                ("✓", palette.add)
            } else {
                ("○", palette.faint)
            };
            let stat = format!(" +{} −{}", stats.0, stats.1);
            let prefix = format!("  {glyph} #{number} ");
            let room = width
                .saturating_sub(prefix.chars().count() + stat.chars().count() + 1)
                .max(4);
            spans.push(Span::styled(prefix, Style::default().fg(color)));
            spans.push(Span::styled(
                truncate_right(label, room),
                Style::default().fg(if *staged { palette.fg } else { palette.muted }),
            ));
            spans.push(Span::styled(
                format!(" +{}", stats.0),
                Style::default().fg(palette.add),
            ));
            spans.push(Span::styled(
                format!(" −{}", stats.1),
                Style::default().fg(palette.del),
            ));
        }
        RowKind::Untracked { path } => {
            spans.push(Span::styled("○ ", Style::default().fg(palette.faint)));
            spans.push(Span::styled(
                truncate_left(path, width.saturating_sub(8)),
                Style::default().fg(palette.muted),
            ));
            spans.push(Span::styled(" new", Style::default().fg(palette.add)));
        }
    }
    Line::from(spans).style(base)
}

fn draw_diff(frame: &mut Frame, app: &mut App, area: Rect) {
    let palette = app.palette.clone();
    let Some((path, hunks, selected)) = app.selected_diff() else {
        let outer = block(&palette, "", false);
        let inner = outer.inner(area);
        frame.render_widget(outer, area);
        frame.render_widget(
            Paragraph::new(Line::styled(
                "Select a file or hunk to see its diff",
                Style::default().fg(palette.faint),
            )),
            inner,
        );
        return;
    };

    let mut lines = Vec::new();
    let mut selected_offset = 0;
    for (index, text) in hunks.iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        if Some(index) == selected {
            selected_offset = lines.len();
        }
        let emphasis = selected.is_none() || Some(index) == selected;
        lines.extend(hunk_lines(text, &path, &palette, emphasis));
    }

    let counter = match selected {
        Some(index) => format!(" hunk {}/{} ", index + 1, hunks.len()),
        None if hunks.len() == 1 => " 1 hunk ".to_owned(),
        None => format!(" {} hunks ", hunks.len()),
    };
    let title = Line::from(vec![Span::styled(
        format!(" {path} "),
        Style::default().fg(palette.fg),
    )]);
    let outer = block(&palette, title, false)
        .title(Line::styled(counter, Style::default().fg(palette.faint)).right_aligned());
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    app.diff_height = inner.height;
    app.hits.push((inner, Hit::Diff));

    let base = selected_offset.saturating_sub(1) as u16;
    let max = (lines.len() as u16).saturating_sub(1);
    let scroll = base.saturating_add(app.diff_scroll).min(max);
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);
}

fn draw_message(frame: &mut Frame, app: &mut App, area: Rect) {
    let palette = app.palette.clone();
    let focused = app.focus == Focus::Message;
    let (border, status): (_, Vec<Span>) = match &app.generation {
        Gen::Running { stage, started, .. } => {
            let mut spans = vec![Span::styled(
                format!("{} ", spinner(app)),
                Style::default().fg(palette.accent),
            )];
            spans.extend(shimmer(app, 18));
            spans.push(Span::styled(
                format!(" {stage} {}s ", started.elapsed().as_secs()),
                Style::default().fg(palette.muted),
            ));
            (palette.accent, spans)
        }
        Gen::Failed(_) => (
            palette.del,
            vec![Span::styled(
                " couldn't generate · g to retry ",
                Style::default().fg(palette.del),
            )],
        ),
        Gen::Idle if focused => (
            palette.accent,
            vec![Span::styled(
                " editing · esc when done ",
                Style::default().fg(palette.accent),
            )],
        ),
        Gen::Idle if app.message_stale => (
            palette.warn,
            vec![Span::styled(
                " staged changes moved on · g to regenerate ",
                Style::default().fg(palette.warn),
            )],
        ),
        Gen::Idle => (palette.border, Vec::new()),
    };

    let subject_len = app.message.lines().first().map_or(0, |l| l.chars().count());
    let subject_style = if subject_len > 72 {
        Style::default().fg(palette.warn)
    } else {
        Style::default().fg(palette.faint)
    };
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .padding(ratatui::widgets::Padding::horizontal(1))
        .title(Line::styled(
            " Message ",
            Style::default().fg(palette.muted),
        ))
        .title(Line::from(status).right_aligned())
        .title_bottom(Line::styled(format!(" {subject_len}/72 "), subject_style).right_aligned());
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    app.hits.push((inner, Hit::Message));

    if let Gen::Failed(error) = &app.generation {
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(error.clone(), Style::default().fg(palette.del)),
                Line::styled(
                    "Check the provider settings with `aic config get`, or run `aic setup`.",
                    Style::default().fg(palette.muted),
                ),
            ])
            .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }

    if app.message.is_empty() && !app.is_generating() && !focused {
        let hint = if app.staged_files.is_empty() {
            "Stage hunks with space, then press g to write a message.".to_owned()
        } else {
            format!(
                "Press g to write a message with {}, or e to type your own.",
                app.input.config.ai_provider
            )
        };
        frame.render_widget(
            Paragraph::new(Line::styled(hint, Style::default().fg(palette.faint))),
            inner,
        );
        return;
    }

    let (cursor_row, cursor_col) = app.message.cursor();
    let height = inner.height as usize;
    let offset = cursor_row.saturating_sub(height.saturating_sub(1));
    let blink = (app.started.elapsed().as_millis() / 500).is_multiple_of(2);
    let last = app.message.lines().len().saturating_sub(1);
    let lines: Vec<Line> = app
        .message
        .lines()
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(index, text)| {
            let style = if index == 0 {
                Style::default()
                    .fg(palette.bright)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.fg)
            };
            let mut spans = vec![Span::styled(text.clone(), style)];
            if app.is_generating() && index == last {
                spans.push(Span::styled(
                    if blink { "▌" } else { " " },
                    Style::default().fg(palette.accent),
                ));
            }
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);

    if focused {
        let prefix: String = app.message.lines()[cursor_row]
            .chars()
            .take(cursor_col)
            .collect();
        let x = inner.x + unicode_width(&prefix).min(inner.width.saturating_sub(1) as usize) as u16;
        let y = inner.y + (cursor_row - offset) as u16;
        frame.set_cursor_position((x, y));
    }
}

fn unicode_width(text: &str) -> usize {
    Line::from(text).width()
}

// ---------------------------------------------------------------------
// Split screen
// ---------------------------------------------------------------------

fn draw_split(frame: &mut Frame, app: &mut App, area: Rect) {
    let palette = app.palette.clone();
    let shared = app.shared_files();
    let preview_text = app
        .split
        .as_ref()
        .and_then(|split| split.selected_hunk().cloned())
        .and_then(|id| app.split_hunk_text(&id).map(|text| (id, text)));
    let spin = spinner(app);
    let shimmer_spans = shimmer(app, 24);
    let Some(split) = app.split.as_mut() else {
        return;
    };

    match &split.status {
        SplitStatus::Planning { started } => {
            let mut bar = shimmer_spans;
            bar.insert(0, Span::raw("  "));
            let lines = vec![
                Line::from(vec![
                    Span::styled(format!("{spin} "), Style::default().fg(palette.accent)),
                    Span::styled(
                        "Grouping hunks into commits",
                        Style::default().fg(palette.fg).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
                Line::from(bar),
                Line::from(""),
                Line::styled(
                    format!(
                        "{} hunks across {} files · {}s",
                        split.staged.hunk_ids().len(),
                        split.staged.files.len(),
                        started.elapsed().as_secs()
                    ),
                    Style::default().fg(palette.muted),
                ),
            ];
            let panel = centered(area, 56, 9);
            frame.render_widget(block(&palette, "", true), panel);
            frame.render_widget(
                Paragraph::new(lines).alignment(Alignment::Center),
                panel.inner(Margin::new(2, 1)),
            );
            return;
        }
        SplitStatus::Failed(error) => {
            let panel = centered(area, 70, 8);
            frame.render_widget(
                block(
                    &palette,
                    Line::styled(" Couldn't plan a split ", Style::default().fg(palette.del)),
                    false,
                )
                .border_style(Style::default().fg(palette.del)),
                panel,
            );
            frame.render_widget(
                Paragraph::new(vec![
                    Line::styled(error.clone(), Style::default().fg(palette.fg)),
                    Line::from(""),
                    Line::styled(
                        "Press esc to go back and make one commit.",
                        Style::default().fg(palette.muted),
                    ),
                ])
                .wrap(Wrap { trim: false }),
                panel.inner(Margin::new(2, 1)),
            );
            return;
        }
        SplitStatus::Ready => {}
    }

    let preview_height = 7u16.min(area.height / 3).max(4);
    let [columns_area, preview_area] =
        Layout::vertical([Constraint::Min(6), Constraint::Length(preview_height)]).areas(area);

    let count = split.groups.len().max(1) as u32;
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints((0..count).map(|_| Constraint::Ratio(1, count)))
        .split(columns_area);

    let elapsed = split.appeared.elapsed();
    for (col, rect) in columns.iter().enumerate() {
        draw_group(
            frame,
            &palette,
            split,
            col,
            *rect,
            &shared,
            elapsed,
            spin,
            &mut app.hits,
        );
    }

    // Preview of the selected hunk.
    let (title, lines) = match &preview_text {
        Some((id, text)) => (
            format!(" {id} "),
            hunk_lines(text, super::super::git::diff::hunk_path(id), &palette, true),
        ),
        None => (
            " preview ".to_owned(),
            vec![Line::styled(
                "Select a hunk to preview it",
                Style::default().fg(palette.faint),
            )],
        ),
    };
    let preview = block(
        &palette,
        Line::styled(title, Style::default().fg(palette.fg)),
        false,
    )
    .title(
        Line::styled(
            format!(" in commit {} ", split.col + 1),
            Style::default().fg(palette.faint),
        )
        .right_aligned(),
    );
    let inner = preview.inner(preview_area);
    frame.render_widget(preview, preview_area);
    frame.render_widget(Paragraph::new(lines), inner);

    if let Some(editor) = &split.editing {
        draw_group_editor(frame, &palette, area, editor, split.col);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_group(
    frame: &mut Frame,
    palette: &Palette,
    split: &SplitState,
    col: usize,
    rect: Rect,
    shared: &std::collections::BTreeMap<String, usize>,
    elapsed: Duration,
    spin: &str,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let Some(group) = split.groups.get(col) else {
        return;
    };
    let focused = col == split.col;
    let title = Line::from(vec![
        Span::styled(
            format!(" {} ", col + 1),
            Style::default()
                .fg(palette.add)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "{} ",
                truncate_right(&group.title, rect.width.saturating_sub(8) as usize)
            ),
            Style::default().fg(palette.fg),
        ),
    ]);
    let outer = block(palette, title, focused);
    let inner = outer.inner(rect);
    frame.render_widget(outer, rect);
    hits.push((rect, Hit::SplitColumn(col)));

    let width = inner.width as usize;
    let mut lines = vec![Line::styled(
        truncate_right(&group.rationale, width),
        Style::default().fg(palette.muted),
    )];
    let mut y = inner.y + 1;
    for (row, id) in group.hunks.iter().enumerate() {
        // Staggered reveal: cards slide in column by column.
        let reveal = Duration::from_millis(((col * 3 + row) * 45) as u64);
        if elapsed < reveal {
            lines.push(Line::from(""));
            y += 1;
            continue;
        }
        let since = (elapsed - reveal).as_millis();
        let slide = 4usize.saturating_sub((since / 30) as usize);
        let selected = focused && row == split.row;
        let path = super::super::git::diff::hunk_path(id);
        let number = id.rsplit('#').next().unwrap_or("1");
        let note = if shared.contains_key(path) {
            " same file"
        } else {
            ""
        };
        let (adds, dels) = split
            .staged
            .file(path)
            .and_then(|file| {
                let index = file.hunk_ids().iter().position(|c| c == id)?;
                super::app::hunk_texts(file)
                    .get(index)
                    .map(|text| hunk_stats(text))
            })
            .unwrap_or((0, 0));
        let stats = format!("+{adds} −{dels}");
        // bar(2) + " #n" + gap(1) + note + stats
        let fixed = slide + 2 + 2 + number.len() + 1 + note.len() + stats.chars().count();
        let label = truncate_left(path, width.saturating_sub(fixed).max(4));
        let pad = width.saturating_sub(fixed + label.chars().count());
        let style = if selected {
            Style::default().bg(palette.selection).fg(palette.bright)
        } else {
            Style::default().fg(palette.fg)
        };
        let mut spans = vec![
            Span::raw(" ".repeat(slide)),
            Span::styled(
                if selected { "▌ " } else { "│ " },
                Style::default().fg(if selected {
                    palette.accent
                } else {
                    palette.border
                }),
            ),
            Span::styled(label, style),
            Span::styled(format!(" #{number}"), style.fg(palette.faint)),
            Span::styled(" ".repeat(pad + 1), style),
        ];
        if !note.is_empty() {
            spans.push(Span::styled(note.trim_start(), style.fg(palette.warn)));
            spans.push(Span::styled(" ", style));
        }
        spans.push(Span::styled(format!("+{adds}"), style.fg(palette.add)));
        spans.push(Span::styled(format!(" −{dels}"), style.fg(palette.del)));
        hits.push((
            Rect::new(inner.x, y, inner.width, 1),
            Hit::SplitCard(col, row),
        ));
        lines.push(Line::from(spans));
        y += 1;
    }
    if split.drag.is_some_and(|(from, _)| from != col) {
        lines.push(Line::styled(
            "┆ drop here",
            Style::default().fg(palette.accent),
        ));
    }
    if group.hunks.is_empty() {
        lines.push(Line::styled(
            "Empty · move hunks here with ←→",
            Style::default()
                .fg(palette.faint)
                .add_modifier(Modifier::ITALIC),
        ));
    }
    lines.push(Line::styled(
        "─".repeat(width),
        Style::default().fg(palette.border),
    ));
    if group.generating {
        lines.push(Line::from(vec![
            Span::styled(format!("{spin} "), Style::default().fg(palette.accent)),
            Span::styled("writing message…", Style::default().fg(palette.muted)),
        ]));
    } else if group.stale && !group.hunks.is_empty() {
        lines.push(Line::styled(
            "hunks changed · rewriting soon",
            Style::default().fg(palette.warn),
        ));
    } else {
        for (index, line) in group.message.lines().enumerate() {
            lines.push(Line::styled(
                line.to_owned(),
                if index == 0 {
                    Style::default()
                        .fg(palette.bright)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(palette.muted)
                },
            ));
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_group_editor(
    frame: &mut Frame,
    palette: &Palette,
    area: Rect,
    editor: &super::editor::TextEditor,
    col: usize,
) {
    let panel = centered(area, 80, 12);
    frame.render_widget(Clear, panel);
    let outer = block(
        palette,
        Line::styled(
            format!(" Message for commit {} ", col + 1),
            Style::default().fg(palette.fg),
        ),
        true,
    )
    .title_bottom(
        Line::styled(" esc to save ", Style::default().fg(palette.faint)).right_aligned(),
    );
    let inner = outer.inner(panel);
    frame.render_widget(outer, panel);
    let lines: Vec<Line> = editor
        .lines()
        .iter()
        .map(|line| Line::styled(line.clone(), Style::default().fg(palette.fg)))
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
    let (row, col) = editor.cursor();
    let prefix: String = editor.lines()[row].chars().take(col).collect();
    frame.set_cursor_position((
        inner.x + (unicode_width(&prefix) as u16).min(inner.width.saturating_sub(1)),
        inner.y + (row as u16).min(inner.height.saturating_sub(1)),
    ));
}

// ---------------------------------------------------------------------
// Overlays
// ---------------------------------------------------------------------

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn draw_toast(
    frame: &mut Frame,
    palette: &Palette,
    area: Rect,
    message: &str,
    error: bool,
    age: Duration,
) {
    let width = (message.chars().count() as u16 + 6).min(area.width.saturating_sub(4));
    // Slide up into place over the first 120 ms.
    let lift = 2u16.saturating_sub((age.as_millis() / 60) as u16);
    let rect = Rect::new(
        area.x + area.width.saturating_sub(width + 1),
        (area.y + area.height).saturating_sub(4) + lift,
        width,
        3,
    );
    if rect.bottom() > area.bottom() + 1 {
        return;
    }
    frame.render_widget(Clear, rect);
    let color = if error { palette.del } else { palette.accent };
    frame.render_widget(
        Paragraph::new(Line::styled(
            message.to_owned(),
            Style::default().fg(palette.fg),
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(color))
                .padding(ratatui::widgets::Padding::horizontal(1)),
        ),
        rect,
    );
}

fn draw_palette(frame: &mut Frame, app: &App, area: Rect) {
    let palette = &app.palette;
    let Some(state) = &app.palette_overlay else {
        return;
    };
    let matches = state.matches();
    let height = (matches.len() as u16 + 4).min(14);
    let width = 56u16.min(area.width.saturating_sub(4));
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + area.height / 5,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    let outer = block(
        palette,
        Line::styled(" Command palette ", Style::default().fg(palette.muted)),
        true,
    );
    let inner = outer.inner(rect);
    frame.render_widget(outer, rect);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("› ", Style::default().fg(palette.accent)),
            Span::styled(state.query.clone(), Style::default().fg(palette.bright)),
            Span::styled("▌", Style::default().fg(palette.accent)),
        ]),
        Line::styled(
            "─".repeat(inner.width as usize),
            Style::default().fg(palette.border),
        ),
    ];
    for (index, action) in matches.iter().enumerate() {
        let selected = index == state.selected;
        let label = action.label();
        let key = action.key();
        let pad = (inner.width as usize).saturating_sub(label.chars().count() + key.len() + 3);
        let style = if selected {
            Style::default().bg(palette.selection).fg(palette.bright)
        } else {
            Style::default().fg(palette.fg)
        };
        lines.push(Line::from(vec![
            Span::styled(
                if selected { "▌" } else { " " },
                Style::default().fg(palette.accent),
            ),
            Span::styled(format!("{label}{}", " ".repeat(pad)), style),
            Span::styled(
                format!(" {key} "),
                Style::default().fg(palette.bright).bg(palette.key_bg),
            ),
        ]));
    }
    if matches.is_empty() {
        lines.push(Line::styled(
            "No matching action",
            Style::default().fg(palette.faint),
        ));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let palette = &app.palette;
    let rows: Vec<(&str, &str)> = match app.screen {
        Screen::Commit => {
            let mut rows = vec![("↑↓ / j k", "move"), ("J K / pgup pgdn", "scroll diff")];
            rows.extend(Action::ALL.iter().map(|a| (a.key(), a.label())));
            rows.extend([
                (":  /  ctrl-k", "command palette"),
                ("esc", "cancel generation"),
                ("ctrl-s", "commit while editing"),
                ("mouse", "click to select, click again to stage, scroll"),
            ]);
            rows
        }
        Screen::Split => vec![
            ("↑↓ / j k", "select hunk"),
            ("tab / h l", "change column"),
            ("←→ / H L", "move hunk to another commit"),
            ("drag", "move a hunk with the mouse"),
            ("n", "new empty commit"),
            ("m", "merge commit into its neighbour"),
            ("r", "rewrite this commit's message"),
            ("e", "edit this commit's message"),
            ("enter", "create all commits"),
            ("esc", "back to a single commit"),
        ],
    };
    let height = rows.len() as u16 + 4;
    let rect = centered(area, 62, height);
    frame.render_widget(Clear, rect);
    let outer = block(
        palette,
        Line::styled(" Keyboard shortcuts ", Style::default().fg(palette.muted)),
        true,
    )
    .title_bottom(
        Line::styled(" any key to close ", Style::default().fg(palette.faint)).right_aligned(),
    );
    let inner = outer.inner(rect);
    frame.render_widget(outer, rect);
    let key_width = rows
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(4)
        + 2;
    let lines: Vec<Line> = rows
        .into_iter()
        .map(|(key, label)| {
            Line::from(vec![
                Span::styled(
                    format!("{key:<key_width$}"),
                    Style::default().fg(palette.accent),
                ),
                Span::styled(label.to_owned(), Style::default().fg(palette.fg)),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner.inner(Margin::new(0, 1)));
}
