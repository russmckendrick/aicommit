use svg::Document;
use svg::node::element::{Group, Rectangle, Style, Text as SvgText};

pub fn new_document(width: f64, height: f64) -> Document {
    Document::new()
        .set("viewBox", (0.0, 0.0, width, height))
        .set("width", width)
        .set("height", height)
        .set("xmlns", "http://www.w3.org/2000/svg")
        .add(base_style())
}

pub fn rect(x: f64, y: f64, w: f64, h: f64, fill: &str) -> Rectangle {
    Rectangle::new()
        .set("x", x)
        .set("y", y)
        .set("width", w)
        .set("height", h)
        .set("fill", fill)
}

pub fn rounded_rect(x: f64, y: f64, w: f64, h: f64, fill: &str, rx: f64) -> Rectangle {
    rect(x, y, w, h, fill).set("rx", rx).set("ry", rx)
}

pub fn text(x: f64, y: f64, content: &str, size: f64) -> SvgText {
    SvgText::new(content)
        .set("x", x)
        .set("y", y)
        .set("font-size", size)
        .set("fill", "#333333")
}

pub fn text_with_colour(x: f64, y: f64, content: &str, size: f64, fill: &str) -> SvgText {
    text(x, y, content, size).set("fill", fill)
}

pub fn group() -> Group {
    Group::new()
}

pub fn title_text(x: f64, y: f64, content: &str) -> SvgText {
    text(x, y, content, 18.0)
        .set("font-weight", "bold")
        .set("fill", "#1a1a1a")
}

pub fn subtitle_text(x: f64, y: f64, content: &str) -> SvgText {
    text(x, y, content, 12.0).set("fill", "#666666")
}

fn base_style() -> Style {
    Style::new(
        "text { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; }",
    )
}

/// Truncate a string to fit within an approximate pixel width at a given font size.
pub fn truncate_to_width(s: &str, max_width: f64, font_size: f64) -> String {
    let char_width = font_size * 0.6;
    let max_chars = (max_width / char_width).floor() as usize;
    if s.len() <= max_chars {
        s.to_owned()
    } else if max_chars > 3 {
        format!("{}...", &s[..max_chars - 3])
    } else {
        s.chars().take(max_chars).collect()
    }
}
