//! Glyphs for the terminal UI. With Nerd Fonts (`AIC_ICONS=nerd`, or
//! `auto` when the remote icon style already assumes Nerd Fonts) the UI
//! uses file-type, provider, and git icons; otherwise plain Unicode that
//! renders in any font.

use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Icons {
    pub nerd: bool,
    /// Use the newest brand logos (OpenAI, Claude), which need Nerd Fonts
    /// 3.5+. Terminals with older or built-in symbol fonts (Ghostty's
    /// default, for example) draw them as replacement boxes.
    pub logos: bool,
}

impl Icons {
    /// `AIC_ICONS`: `nerd`, `unicode`, or `auto` (follow
    /// `AIC_REMOTE_ICON_STYLE`, whose default already uses Nerd Fonts).
    pub fn from_config(icons: &str, remote_icon_style: &str) -> Self {
        match icons {
            "nerd" => Self {
                nerd: true,
                logos: true,
            },
            "unicode" => Self {
                nerd: false,
                logos: false,
            },
            _ => Self {
                nerd: matches!(remote_icon_style, "auto" | "nerd-font"),
                logos: false,
            },
        }
    }

    fn pick(self, nerd: &'static str, plain: &'static str) -> &'static str {
        if self.nerd { nerd } else { plain }
    }

    pub fn branch(self) -> &'static str {
        self.pick("\u{e725}", "⎇")
    }
    pub fn commit(self) -> &'static str {
        self.pick("\u{f417}", "●")
    }
    pub fn check(self) -> &'static str {
        self.pick("\u{f00c}", "✓")
    }
    pub fn cross(self) -> &'static str {
        self.pick("\u{f00d}", "✕")
    }
    pub fn push(self) -> &'static str {
        self.pick("\u{f0ee}", "↑")
    }
    pub fn warning(self) -> &'static str {
        self.pick("\u{f071}", "!")
    }
    pub fn settings(self) -> &'static str {
        self.pick("\u{f013}", "⚙")
    }
    pub fn split(self) -> &'static str {
        self.pick("\u{f126}", "⑂")
    }

    /// File-type icon and colour for a path (Nerd Fonts only).
    pub fn file(self, path: &str) -> Option<(&'static str, Color)> {
        if !self.nerd {
            return None;
        }
        let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
        let ext = name.rsplit_once('.').map_or("", |(_, ext)| ext);
        let (glyph, rgb) = match (name.as_str(), ext) {
            ("dockerfile", _) => ("\u{f308}", 0x2496ed),
            (".gitignore" | ".gitattributes" | ".gitmodules", _) => ("\u{e702}", 0xf05032),
            (_, "lock") => ("\u{f023}", 0x8b949e),
            (n, _) if n.ends_with("-lock.json") || n.ends_with(".lock.json") => {
                ("\u{f023}", 0x8b949e)
            }
            (_, "rs") => ("\u{e7a8}", 0xdea584),
            (_, "ts" | "tsx" | "mts" | "cts") => ("\u{e628}", 0x3178c6),
            (_, "js" | "jsx" | "mjs" | "cjs") => ("\u{e74e}", 0xf1e05a),
            (_, "md" | "mdx" | "markdown") => ("\u{e73e}", 0x8b949e),
            (_, "json") => ("\u{e60b}", 0xcbcb41),
            (_, "py") => ("\u{e73c}", 0x3572a5),
            (_, "go") => ("\u{e627}", 0x00add8),
            (_, "html" | "htm") => ("\u{e736}", 0xe34c26),
            (_, "css" | "scss" | "sass" | "less") => ("\u{e749}", 0x563d7c),
            (_, "swift") => ("\u{e755}", 0xf05138),
            (_, "java" | "kt") => ("\u{e738}", 0xb07219),
            (_, "c" | "h") => ("\u{e61e}", 0x555555),
            (_, "cpp" | "cc" | "hpp") => ("\u{e61d}", 0xf34b7d),
            (_, "toml" | "yaml" | "yml" | "ini" | "cfg" | "conf") => ("\u{e615}", 0x8b949e),
            (_, "sh" | "bash" | "zsh" | "fish") => ("\u{f489}", 0x89e051),
            (_, "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "ico") => ("\u{f1c5}", 0xa074c4),
            _ => ("\u{f15b}", 0x8b949e),
        };
        Some((glyph, rgb_color(rgb)))
    }
}

fn rgb_color(rgb: u32) -> Color {
    Color::Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// Display name, glyph, and brand colour for an AI provider.
///
/// Nerd Fonts 3.5 added real OpenAI (`cod-openai`, also used for Codex) and
/// Claude (`cod-claude`, for Anthropic and Claude Code) logos; those appear
/// only with `AIC_ICONS=nerd`, and `auto` uses long-standing glyphs instead.
/// Copilot, Azure, and Apple logos are older and always used. Ollama and
/// Groq have no Nerd Font logo.
pub fn provider(icons: Icons, provider: &str) -> (&'static str, &'static str, Color) {
    // (name, newest logo, long-standing Nerd Font glyph, plain, colour)
    let (name, logo, nerd, plain, rgb) = match provider {
        "openai" => ("OpenAI", "\u{ec81}", "\u{f06a9}", "◎", 0x10a37f),
        "azure-openai" => ("Azure OpenAI", "\u{ebd8}", "\u{ebd8}", "◎", 0x0078d4),
        "anthropic" => ("Anthropic", "\u{ec82}", "✻", "✻", 0xd97757),
        "claude-code" => ("Claude Code", "\u{ec82}", "✻", "✻", 0xd97757),
        "codex" => ("Codex", "\u{ec81}", "\u{f06a9}", "◎", 0x10a37f),
        "copilot" => ("GitHub Copilot", "\u{f4b8}", "\u{f4b8}", "◉", 0x8957e5),
        "groq" => ("Groq", "\u{f0e7}", "\u{f0e7}", "⚡", 0xf55036),
        "ollama" => ("Ollama", "\u{f233}", "\u{f233}", "◉", 0xc9d1d9),
        "apple" => ("Apple Intelligence", "\u{f179}", "\u{f179}", "◉", 0xa2aaad),
        "test" => ("Test", "\u{f0668}", "\u{f0668}", "◇", 0x8b949e),
        _ => ("Custom", "\u{f06a9}", "\u{f06a9}", "◎", 0x8b949e),
    };
    let glyph = match (icons.nerd, icons.logos) {
        (true, true) => logo,
        (true, false) => nerd,
        _ => plain,
    };
    (name, glyph, rgb_color(rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_follows_the_remote_icon_style() {
        assert!(Icons::from_config("auto", "auto").nerd);
        assert!(!Icons::from_config("auto", "auto").logos);
        assert!(Icons::from_config("nerd", "auto").logos);
        assert!(Icons::from_config("auto", "nerd-font").nerd);
        assert!(!Icons::from_config("auto", "emoji").nerd);
        assert!(!Icons::from_config("unicode", "auto").nerd);
        assert!(Icons::from_config("nerd", "label").nerd);
    }

    #[test]
    fn file_icons_only_with_nerd_fonts() {
        assert!(
            Icons {
                nerd: false,
                logos: false
            }
            .file("src/main.rs")
            .is_none()
        );
        let nerd = Icons {
            nerd: true,
            logos: false,
        };
        assert_eq!(nerd.file("src/main.rs").unwrap().0, "\u{e7a8}");
        assert_eq!(nerd.file("web/App.tsx").unwrap().0, "\u{e628}");
        assert_eq!(nerd.file("Cargo.lock").unwrap().0, "\u{f023}");
        assert_eq!(nerd.file("package-lock.json").unwrap().0, "\u{f023}");
        assert_eq!(nerd.file("README").unwrap().0, "\u{f15b}");
    }

    #[test]
    fn providers_have_names_and_fallback_glyphs() {
        let plain = Icons {
            nerd: false,
            logos: false,
        };
        let safe = Icons {
            nerd: true,
            logos: false,
        };
        let latest = Icons {
            nerd: true,
            logos: true,
        };
        assert_eq!(provider(latest, "openai").1, "\u{ec81}");
        assert_eq!(provider(latest, "anthropic").1, "\u{ec82}");
        assert_eq!(provider(safe, "openai").1, "\u{f06a9}");
        assert_eq!(provider(safe, "anthropic").1, "✻");
        assert_eq!(provider(plain, "anthropic").0, "Anthropic");
        assert_eq!(provider(plain, "openai").1, "◎");
        assert_eq!(provider(plain, "unknown").0, "Custom");
    }
}
