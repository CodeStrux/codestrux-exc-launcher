use std::path::{Path, PathBuf};

use clap::ValueEnum;
use crossterm::style::Color;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum Theme {
    Default,
    Dark,
    Mono,
}

impl Theme {
    fn parse(name: &str) -> Option<Theme> {
        match name.to_ascii_lowercase().as_str() {
            "default" => Some(Theme::Default),
            "dark" => Some(Theme::Dark),
            "mono" => Some(Theme::Mono),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    pub accent: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub selected_bg: Color,
    /// Foreground used for the selected grid cell. Kept separate from
    /// `text` because `selected_bg` is usually a light/saturated color
    /// (via crossterm's "Dark*" -> bright-ANSI mapping in the picker), so
    /// the selected item needs a foreground chosen for contrast against
    /// *that* background specifically, not the theme's normal text color.
    pub selected_fg: Color,
}

impl ThemeColors {
    pub fn default_theme() -> Self {
        ThemeColors {
            accent: Color::Magenta,
            border: Color::DarkGrey,
            text: Color::White,
            muted: Color::Grey,
            selected_bg: Color::DarkMagenta,
            selected_fg: Color::Black,
        }
    }

    pub fn dark() -> Self {
        ThemeColors {
            accent: Color::Cyan,
            border: Color::DarkBlue,
            text: Color::Grey,
            muted: Color::DarkGrey,
            selected_bg: Color::DarkCyan,
            selected_fg: Color::Black,
        }
    }

    pub fn mono() -> Self {
        ThemeColors {
            accent: Color::White,
            border: Color::White,
            text: Color::White,
            muted: Color::Grey,
            selected_bg: Color::Grey,
            selected_fg: Color::Black,
        }
    }

    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Default => Self::default_theme(),
            Theme::Dark => Self::dark(),
            Theme::Mono => Self::mono(),
        }
    }
}

/// A user-defined palette, either inlined as a `[theme]` table in the main
/// config or loaded from a standalone file referenced by `[meta] theme_file`.
/// Any field left unset falls back to the corresponding color from the
/// built-in base theme (`[meta] theme`, or `default` if that's also unset).
#[derive(Debug, Deserialize, Default, Clone, PartialEq, Eq)]
pub struct CustomTheme {
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default)]
    pub border: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub muted: Option<String>,
    #[serde(default)]
    pub selected_bg: Option<String>,
    #[serde(default)]
    pub selected_fg: Option<String>,
}

impl CustomTheme {
    /// Merge this custom palette over `base`, keeping `base`'s color for any
    /// field that's unset or fails to parse.
    pub fn apply_over(&self, base: ThemeColors) -> ThemeColors {
        ThemeColors {
            accent: self.accent.as_deref().and_then(parse_color).unwrap_or(base.accent),
            border: self.border.as_deref().and_then(parse_color).unwrap_or(base.border),
            text: self.text.as_deref().and_then(parse_color).unwrap_or(base.text),
            muted: self.muted.as_deref().and_then(parse_color).unwrap_or(base.muted),
            selected_bg: self.selected_bg.as_deref().and_then(parse_color).unwrap_or(base.selected_bg),
            selected_fg: self.selected_fg.as_deref().and_then(parse_color).unwrap_or(base.selected_fg),
        }
    }

    /// Every set field parses as a valid color; used by `exc validate`.
    pub fn invalid_fields(&self) -> Vec<(&'static str, &str)> {
        let mut bad = Vec::new();
        for (field, value) in [
            ("accent", &self.accent),
            ("border", &self.border),
            ("text", &self.text),
            ("muted", &self.muted),
            ("selected_bg", &self.selected_bg),
            ("selected_fg", &self.selected_fg),
        ] {
            if let Some(v) = value
                && parse_color(v).is_none() {
                    bad.push((field, v.as_str()));
                }
        }
        bad
    }
}

/// Parses `#rrggbb` hex or a crossterm named color (case-insensitive).
pub fn parse_color(spec: &str) -> Option<Color> {
    let s = spec.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        return Some(Color::Rgb { r, g, b });
    }
    match s.to_ascii_lowercase().as_str() {
        "black" => Some(Color::Black),
        "darkgrey" | "darkgray" => Some(Color::DarkGrey),
        "red" => Some(Color::Red),
        "darkred" => Some(Color::DarkRed),
        "green" => Some(Color::Green),
        "darkgreen" => Some(Color::DarkGreen),
        "yellow" => Some(Color::Yellow),
        "darkyellow" => Some(Color::DarkYellow),
        "blue" => Some(Color::Blue),
        "darkblue" => Some(Color::DarkBlue),
        "magenta" => Some(Color::Magenta),
        "darkmagenta" => Some(Color::DarkMagenta),
        "cyan" => Some(Color::Cyan),
        "darkcyan" => Some(Color::DarkCyan),
        "white" => Some(Color::White),
        "grey" | "gray" => Some(Color::Grey),
        _ => None,
    }
}

/// Resolves `theme_file` (from `[meta] theme_file`) against `config_dir`,
/// expanding a leading `~` to the home directory.
pub fn resolve_theme_file_path(theme_file: &str, config_dir: &Path) -> PathBuf {
    if let Some(rest) = theme_file.strip_prefix("~/")
        && let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    let path = PathBuf::from(theme_file);
    if path.is_absolute() {
        path
    } else {
        config_dir.join(path)
    }
}

pub fn load_theme_file(path: &Path) -> Option<CustomTheme> {
    let raw = std::fs::read_to_string(path).ok()?;
    toml::from_str(&raw).ok()
}

/// Precedence: `--theme` CLI flag (built-in name) > inline `[theme]` table in
/// the config > `[meta] theme_file` (external palette file) > `[meta] theme`
/// built-in name > built-in default. `--no-color` / `NO_COLOR` forces mono
/// regardless of everything else.
pub fn resolve_theme(
    cli_theme: Option<&str>,
    meta_theme: Option<&str>,
    inline_theme: Option<&CustomTheme>,
    theme_file: Option<&Path>,
    no_color: bool,
) -> ThemeColors {
    if no_color || std::env::var_os("NO_COLOR").is_some() {
        return ThemeColors::mono();
    }

    if let Some(name) = cli_theme.and_then(Theme::parse) {
        return ThemeColors::for_theme(name);
    }

    let base = ThemeColors::for_theme(meta_theme.and_then(Theme::parse).unwrap_or(Theme::Default));

    if let Some(custom) = inline_theme {
        return custom.apply_over(base);
    }

    if let Some(path) = theme_file
        && let Some(custom) = load_theme_file(path) {
            return custom.apply_over(base);
        }

    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_themes_pair_a_dark_selected_fg_with_their_light_selected_bg() {
        // Regression: selected_fg used to reuse the theme's normal `text`
        // color (light grey/white), which read as low-contrast against the
        // light/saturated selected_bg colors below once crossterm's
        // "Dark*" -> bright-ANSI mapping applied in the picker.
        for theme in [ThemeColors::default_theme(), ThemeColors::dark(), ThemeColors::mono()] {
            assert_eq!(theme.selected_fg, Color::Black);
            assert_ne!(theme.selected_fg, theme.selected_bg);
        }
    }

    #[test]
    fn no_color_forces_mono() {
        let c = resolve_theme(Some("dark"), None, None, None, true);
        assert!(matches!(c.accent, Color::White));
    }

    #[test]
    fn cli_takes_precedence_over_meta() {
        let cli = resolve_theme(Some("dark"), Some("mono"), None, None, false);
        let meta = resolve_theme(None, Some("dark"), None, None, false);
        assert!(matches!(cli.accent, Color::Cyan));
        assert!(matches!(meta.accent, Color::Cyan));
    }

    #[test]
    fn unknown_theme_falls_back_to_default() {
        let c = resolve_theme(Some("nope"), None, None, None, false);
        assert!(matches!(c.accent, Color::Magenta));
    }

    #[test]
    fn hex_color_parses() {
        assert_eq!(parse_color("#ff8800"), Some(Color::Rgb { r: 0xff, g: 0x88, b: 0x00 }));
    }

    #[test]
    fn named_color_parses_case_insensitively() {
        assert_eq!(parse_color("Cyan"), Some(Color::Cyan));
        assert_eq!(parse_color("DARKGREY"), Some(Color::DarkGrey));
    }

    #[test]
    fn invalid_color_returns_none() {
        assert_eq!(parse_color("not-a-color"), None);
        assert_eq!(parse_color("#zzz"), None);
    }

    #[test]
    fn inline_custom_theme_overrides_only_set_fields() {
        let custom = CustomTheme {
            accent: Some("#00ff00".to_string()),
            ..Default::default()
        };
        let resolved = resolve_theme(None, Some("dark"), Some(&custom), None, false);
        assert_eq!(resolved.accent, Color::Rgb { r: 0, g: 255, b: 0 });
        // unset fields fall back to the "dark" base theme, not "default"
        assert!(matches!(resolved.border, Color::DarkBlue));
    }

    #[test]
    fn invalid_fields_reports_bad_color_names() {
        let custom = CustomTheme {
            accent: Some("nonsense".to_string()),
            border: Some("cyan".to_string()),
            ..Default::default()
        };
        let bad = custom.invalid_fields();
        assert_eq!(bad, vec![("accent", "nonsense")]);
    }
}
