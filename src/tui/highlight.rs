//! Diff rendering: add/remove tints, word-level changes inside paired
//! lines, and a small built-in syntax highlighter (keywords, strings,
//! comments, and type names) for common languages. It is deliberately
//! light: no grammar files, so the binary stays small and startup instant.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::theme::Palette;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Lang {
    Rust,
    CLike,
    Python,
    Shell,
    Config,
    Plain,
}

fn lang_for(path: &str) -> Lang {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "rs" => Lang::Rust,
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "go" | "c" | "h" | "cc" | "cpp" | "hpp"
        | "java" | "kt" | "swift" | "cs" | "scala" | "dart" => Lang::CLike,
        "py" | "rb" => Lang::Python,
        "sh" | "bash" | "zsh" | "fish" => Lang::Shell,
        "toml" | "yaml" | "yml" | "ini" | "cfg" | "conf" => Lang::Config,
        _ => Lang::Plain,
    }
}

fn keywords(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Rust => &[
            "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
            "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
            "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
            "trait", "true", "type", "unsafe", "use", "where", "while",
        ],
        Lang::CLike => &[
            "async",
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "default",
            "defer",
            "else",
            "enum",
            "export",
            "extends",
            "false",
            "finally",
            "for",
            "from",
            "func",
            "function",
            "go",
            "if",
            "import",
            "in",
            "interface",
            "let",
            "new",
            "null",
            "package",
            "private",
            "public",
            "return",
            "static",
            "struct",
            "switch",
            "this",
            "throw",
            "true",
            "try",
            "type",
            "typeof",
            "var",
            "void",
            "while",
        ],
        Lang::Python => &[
            "and", "as", "async", "await", "break", "class", "continue", "def", "elif", "else",
            "end", "except", "False", "finally", "for", "from", "if", "import", "in", "is",
            "lambda", "None", "not", "or", "pass", "raise", "return", "True", "try", "while",
            "with", "yield",
        ],
        Lang::Shell => &[
            "case", "do", "done", "elif", "else", "esac", "export", "fi", "for", "function", "if",
            "in", "local", "return", "then", "while",
        ],
        Lang::Config => &["true", "false", "null"],
        Lang::Plain => &[],
    }
}

fn line_comment(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Rust | Lang::CLike => &["//"],
        Lang::Python | Lang::Shell | Lang::Config => &["#"],
        Lang::Plain => &[],
    }
}

/// Foreground colour per character for one line of source.
fn syntax_colors(text: &[char], lang: Lang, palette: &Palette, base: Color) -> Vec<Color> {
    let mut colors = vec![base; text.len()];
    if lang == Lang::Plain {
        return colors;
    }
    let keywords = keywords(lang);
    let comments = line_comment(lang);
    let mut i = 0;
    while i < text.len() {
        let rest: String = text[i..].iter().take(2).collect();
        if comments.iter().any(|marker| rest.starts_with(marker)) {
            for color in &mut colors[i..] {
                *color = palette.comment;
            }
            break;
        }
        let ch = text[i];
        if ch == '"' || (ch == '\'' && lang != Lang::Rust) || ch == '`' {
            let start = i;
            i += 1;
            while i < text.len() && text[i] != ch {
                if text[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            let end = (i + 1).min(text.len());
            for color in &mut colors[start..end] {
                *color = palette.string;
            }
            i = end;
            continue;
        }
        if ch.is_alphabetic() || ch == '_' {
            let start = i;
            while i < text.len() && (text[i].is_alphanumeric() || text[i] == '_') {
                i += 1;
            }
            let word: String = text[start..i].iter().collect();
            let color = if keywords.contains(&word.as_str()) {
                Some(palette.keyword)
            } else if word.chars().next().is_some_and(char::is_uppercase)
                && word.chars().any(char::is_lowercase)
            {
                Some(palette.ty)
            } else if text.get(i) == Some(&'(') {
                Some(palette.accent)
            } else {
                None
            };
            if let Some(color) = color {
                for slot in &mut colors[start..i] {
                    *slot = color;
                }
            }
            continue;
        }
        i += 1;
    }
    colors
}

/// Range of characters that differ between a removed and an added line,
/// found by trimming the common prefix and suffix.
fn changed_range(old: &[char], new: &[char]) -> ((usize, usize), (usize, usize)) {
    let prefix = old
        .iter()
        .zip(new.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let max_suffix = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    (
        snap_to_words(old, prefix, old.len() - suffix),
        snap_to_words(new, prefix, new.len() - suffix),
    )
}

/// Widen a character range so it never starts or ends inside a word.
fn snap_to_words(text: &[char], mut start: usize, mut end: usize) -> (usize, usize) {
    let is_word = |ch: &char| ch.is_alphanumeric() || *ch == '_';
    while start > 0 && text.get(start).is_some_and(is_word) && is_word(&text[start - 1]) {
        start -= 1;
    }
    while end < text.len() && end > 0 && is_word(&text[end - 1]) && is_word(&text[end]) {
        end += 1;
    }
    (start, end.max(start))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Add,
    Del,
    Context,
    Header,
    Meta,
}

fn kind_of(line: &str) -> Kind {
    if line.starts_with("@@") {
        Kind::Header
    } else if line.starts_with("+++") || line.starts_with("---") || line.starts_with('\\') {
        Kind::Meta
    } else if line.starts_with('+') {
        Kind::Add
    } else if line.starts_with('-') {
        Kind::Del
    } else {
        Kind::Context
    }
}

fn expand_tabs(text: &str) -> String {
    text.replace('\t', "    ")
}

/// Render hunk text (starting at its `@@` line) as styled lines.
/// `emphasis` brightens the hunk (the selected one).
pub fn hunk_lines(text: &str, path: &str, palette: &Palette, emphasis: bool) -> Vec<Line<'static>> {
    let lang = lang_for(path);
    let raw: Vec<&str> = text.lines().collect();
    let kinds: Vec<Kind> = raw.iter().map(|line| kind_of(line)).collect();
    let bodies: Vec<Vec<char>> = raw
        .iter()
        .map(|line| expand_tabs(line.get(1..).unwrap_or("")).chars().collect())
        .collect();

    // Word-level ranges for runs of N removed lines followed by N added lines.
    let mut word: Vec<Option<(usize, usize)>> = vec![None; raw.len()];
    let mut i = 0;
    while i < raw.len() {
        if kinds[i] != Kind::Del {
            i += 1;
            continue;
        }
        let del_start = i;
        while i < raw.len() && kinds[i] == Kind::Del {
            i += 1;
        }
        let add_start = i;
        while i < raw.len() && kinds[i] == Kind::Add {
            i += 1;
        }
        let dels = add_start - del_start;
        let adds = i - add_start;
        if dels == adds {
            for offset in 0..dels {
                let (old, new) =
                    changed_range(&bodies[del_start + offset], &bodies[add_start + offset]);
                // Only highlight when the lines share a meaningful part.
                if old.0 + (bodies[del_start + offset].len() - old.1) > 2 {
                    word[del_start + offset] = Some(old);
                    word[add_start + offset] = Some(new);
                }
            }
        }
    }

    raw.iter()
        .enumerate()
        .map(|(index, line)| match kinds[index] {
            Kind::Header => {
                let (range, rest) = split_hunk_header(line);
                let mut spans = vec![Span::styled(
                    range,
                    Style::default().fg(if emphasis {
                        palette.accent
                    } else {
                        palette.faint
                    }),
                )];
                if !rest.is_empty() {
                    spans.push(Span::styled(rest, Style::default().fg(palette.muted)));
                }
                Line::from(spans)
            }
            Kind::Meta => Line::styled(line.to_string(), Style::default().fg(palette.faint)),
            kind => {
                let (sign, sign_color, bg, word_bg, base) = match kind {
                    Kind::Add => (
                        "+",
                        palette.add,
                        Some(palette.add_bg),
                        palette.add_word_bg,
                        palette.fg,
                    ),
                    Kind::Del => (
                        "-",
                        palette.del,
                        Some(palette.del_bg),
                        palette.del_word_bg,
                        palette.fg,
                    ),
                    _ => (
                        " ",
                        palette.faint,
                        None,
                        palette.selection,
                        if emphasis { palette.fg } else { palette.muted },
                    ),
                };
                let body = &bodies[index];
                let colors = syntax_colors(body, lang, palette, base);
                let mut spans = vec![Span::styled(
                    format!("{sign} "),
                    Style::default()
                        .fg(sign_color)
                        .bg(bg.unwrap_or(Color::Reset)),
                )];
                let mut current = String::new();
                let mut current_style: Option<Style> = None;
                for (pos, ch) in body.iter().enumerate() {
                    let mut style = Style::default().fg(colors[pos]);
                    if let Some(bg) = bg {
                        style = style.bg(bg);
                    }
                    if word[index].is_some_and(|(start, end)| pos >= start && pos < end) {
                        style = style.bg(word_bg).add_modifier(Modifier::BOLD);
                    }
                    if current_style != Some(style) {
                        if let Some(prev) = current_style {
                            spans.push(Span::styled(std::mem::take(&mut current), prev));
                        }
                        current_style = Some(style);
                    }
                    current.push(*ch);
                }
                if let Some(style) = current_style {
                    spans.push(Span::styled(current, style));
                }
                let mut rendered = Line::from(spans);
                if let Some(bg) = bg {
                    rendered = rendered.style(Style::default().bg(bg));
                }
                rendered
            }
        })
        .collect()
}

fn split_hunk_header(line: &str) -> (String, String) {
    // "@@ -1,3 +1,4 @@ fn context()" -> ("@@ -1,3 +1,4 @@", " fn context()")
    if let Some(end) = line.get(2..).and_then(|rest| rest.find("@@")) {
        let split = end + 4;
        (
            line[..split.min(line.len())].to_owned(),
            line.get(split..).unwrap_or("").to_owned(),
        )
    } else {
        (line.to_owned(), String::new())
    }
}

/// Short label for a hunk: its function context, or its first changed line.
pub fn hunk_label(text: &str) -> String {
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("");
    let (_, context) = split_hunk_header(header);
    let context = context.trim();
    if !context.is_empty() {
        return context.to_owned();
    }
    lines
        .find(|line| line.starts_with('+') || line.starts_with('-'))
        .map(|line| line[1..].trim().to_owned())
        .unwrap_or_default()
}

/// Added and removed line counts for a hunk.
pub fn hunk_stats(text: &str) -> (usize, usize) {
    text.lines()
        .skip(1)
        .fold((0, 0), |(adds, dels), line| match kind_of(line) {
            Kind::Add => (adds + 1, dels),
            Kind::Del => (adds, dels + 1),
            _ => (adds, dels),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{map::theme::load_theme, tui::theme::palette_from};

    fn palette() -> Palette {
        palette_from(load_theme("github-dark").unwrap(), true)
    }

    #[test]
    fn finds_the_changed_word() {
        let old: Vec<char> = "async fn generate_commit_message(&self)".chars().collect();
        let new: Vec<char> = "async fn complete(&self)".chars().collect();
        let (o, n) = changed_range(&old, &new);
        assert_eq!(
            old[o.0..o.1].iter().collect::<String>(),
            "generate_commit_message"
        );
        assert_eq!(new[n.0..n.1].iter().collect::<String>(), "complete");
    }

    #[test]
    fn highlights_keywords_strings_and_comments() {
        let palette = palette();
        let text: Vec<char> = "let x = \"hi\"; // note".chars().collect();
        let colors = syntax_colors(&text, Lang::Rust, &palette, palette.fg);
        assert_eq!(colors[0], palette.keyword);
        assert_eq!(colors[8], palette.string);
        assert_eq!(colors[14], palette.comment);
    }

    #[test]
    fn renders_one_line_per_diff_line_and_labels_hunks() {
        let text = "@@ -1,2 +1,2 @@ fn main() {\n context\n-old\n+new\n";
        let lines = hunk_lines(text, "src/main.rs", &palette(), true);
        assert_eq!(lines.len(), 4);
        assert_eq!(hunk_label(text), "fn main() {");
        assert_eq!(hunk_label("@@ -1 +1 @@\n-a\n+b\n"), "a");
        assert_eq!(hunk_stats(text), (1, 1));
    }
}
