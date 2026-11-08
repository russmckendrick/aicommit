//! Terminal UI colours, derived from the `aic map` themes so screenshots,
//! SVG maps, and the TUI look like one product.
//!
//! The main background is left as the terminal's own (`Color::Reset`), so
//! transparent and custom terminal backgrounds keep working; the theme
//! background is only used to blend diff and selection tints.

use ratatui::style::Color;

use crate::map::theme::{Theme, load_theme};

#[derive(Debug, Clone)]
pub struct Palette {
    pub name: String,
    pub dark: bool,
    pub fg: Color,
    pub bright: Color,
    pub muted: Color,
    pub faint: Color,
    pub border: Color,
    pub accent: Color,
    pub selection: Color,
    pub add: Color,
    pub del: Color,
    pub warn: Color,
    pub add_bg: Color,
    pub del_bg: Color,
    pub add_word_bg: Color,
    pub del_word_bg: Color,
    pub keyword: Color,
    pub string: Color,
    pub comment: Color,
    pub ty: Color,
    pub key_bg: Color,
}

type Rgb = (u8, u8, u8);

fn hex(value: &str) -> Rgb {
    let value = value.trim().trim_start_matches('#');
    let parse = |range: std::ops::Range<usize>| {
        value
            .get(range)
            .and_then(|part| u8::from_str_radix(part, 16).ok())
            .unwrap_or(128)
    };
    (parse(0..2), parse(2..4), parse(4..6))
}

fn blend(top: Rgb, bottom: Rgb, alpha: f32) -> Rgb {
    let mix = |a: u8, b: u8| (f32::from(a) * alpha + f32::from(b) * (1.0 - alpha)).round() as u8;
    (
        mix(top.0, bottom.0),
        mix(top.1, bottom.1),
        mix(top.2, bottom.2),
    )
}

/// Whether the terminal advertises 24-bit colour.
pub fn truecolor_supported() -> bool {
    std::env::var("COLORTERM")
        .map(|value| {
            let value = value.to_lowercase();
            value.contains("truecolor") || value.contains("24bit")
        })
        .unwrap_or(false)
        || matches!(
            std::env::var("TERM_PROGRAM").as_deref(),
            Ok("iTerm.app" | "WezTerm" | "ghostty" | "vscode")
        )
}

/// Nearest xterm-256 colour for terminals without truecolor.
pub fn to_ansi256(rgb: Rgb) -> u8 {
    let (r, g, b) = rgb;
    // Greyscale ramp (232-255) when the colour is close to grey.
    if r.abs_diff(g) < 10 && g.abs_diff(b) < 10 {
        let grey = (u16::from(r) + u16::from(g) + u16::from(b)) / 3;
        if grey < 8 {
            return 16;
        }
        if grey > 238 {
            return 231;
        }
        return 232 + ((grey - 8) * 24 / 231) as u8;
    }
    let level = |v: u8| -> u8 {
        if v < 48 {
            0
        } else if v < 115 {
            1
        } else {
            (v - 35) / 40
        }
    };
    16 + 36 * level(r) + 6 * level(g) + level(b)
}

struct Builder {
    truecolor: bool,
}

impl Builder {
    fn color(&self, rgb: Rgb) -> Color {
        if self.truecolor {
            Color::Rgb(rgb.0, rgb.1, rgb.2)
        } else {
            Color::Indexed(to_ansi256(rgb))
        }
    }
}

/// `auto` → github-dark or github-light from the terminal background
/// (`COLORFGBG`), defaulting to dark.
pub fn resolve_theme_name(configured: &str) -> String {
    if configured != "auto" {
        return configured.to_owned();
    }
    if terminal_is_light() {
        "github-light".to_owned()
    } else {
        "github-dark".to_owned()
    }
}

fn terminal_is_light() -> bool {
    // COLORFGBG is "fg;bg" (sometimes "fg;default;bg"); ANSI 7 and 9-15 are light.
    std::env::var("COLORFGBG")
        .ok()
        .and_then(|value| value.rsplit(';').next()?.parse::<u8>().ok())
        .is_some_and(|bg| bg == 7 || (9..=15).contains(&bg))
}

pub fn palette(configured: &str) -> Palette {
    let name = resolve_theme_name(configured);
    let theme = load_theme(&name)
        .or_else(|_| load_theme("github-dark"))
        .expect("github-dark theme is embedded");
    palette_from(theme, truecolor_supported())
}

pub fn palette_from(theme: &Theme, truecolor: bool) -> Palette {
    let b = Builder { truecolor };
    let dark = theme.variant.eq_ignore_ascii_case("dark");
    let background = hex(&theme.background);
    let (add, del, warn, keyword, string, ty) = if dark {
        (
            hex("#3fb950"),
            hex("#f85149"),
            hex("#d29922"),
            hex("#ff7b72"),
            hex("#a5d6ff"),
            hex("#ffa657"),
        )
    } else {
        (
            hex("#1a7f37"),
            hex("#cf222e"),
            hex("#9a6700"),
            hex("#cf222e"),
            hex("#0a3069"),
            hex("#953800"),
        )
    };
    let accent = hex(&theme.accent);
    let primary = hex(&theme.primary_text);
    let bright = if dark { hex("#f0f6fc") } else { hex("#0d1117") };

    Palette {
        name: theme.name.clone(),
        dark,
        fg: b.color(primary),
        bright: b.color(blend(bright, primary, 0.6)),
        muted: b.color(hex(&theme.secondary_text)),
        faint: b.color(hex(&theme.tertiary_text)),
        border: b.color(hex(&theme.border)),
        accent: b.color(accent),
        selection: b.color(blend(accent, background, 0.22)),
        add: b.color(add),
        del: b.color(del),
        warn: b.color(warn),
        add_bg: b.color(blend(add, background, 0.14)),
        del_bg: b.color(blend(del, background, 0.12)),
        add_word_bg: b.color(blend(add, background, 0.42)),
        del_word_bg: b.color(blend(del, background, 0.38)),
        keyword: b.color(keyword),
        string: b.color(string),
        comment: b.color(hex(&theme.tertiary_text)),
        ty: b.color(ty),
        key_bg: b.color(hex(&theme.surface)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blends_and_parses_hex() {
        assert_eq!(hex("#ff8000"), (255, 128, 0));
        assert_eq!(blend((255, 255, 255), (0, 0, 0), 0.5), (128, 128, 128));
    }

    #[test]
    fn maps_to_ansi256() {
        assert_eq!(to_ansi256((0, 0, 0)), 16);
        assert_eq!(to_ansi256((255, 255, 255)), 231);
        assert_eq!(to_ansi256((255, 0, 0)), 196);
        assert!((232..=255).contains(&to_ansi256((128, 128, 128))));
    }

    #[test]
    fn every_map_theme_builds_a_palette() {
        for name in crate::map::theme::available_theme_names() {
            let theme = load_theme(&name).unwrap();
            let palette = palette_from(theme, false);
            assert_eq!(palette.dark, theme.variant.eq_ignore_ascii_case("dark"));
        }
    }

    #[test]
    fn explicit_theme_is_kept() {
        assert_eq!(resolve_theme_name("dracula"), "dracula");
    }
}
