//! The Vix theme model.
//!
//! Every theme is a JSON [`CustomTheme`] with per-region colors and font
//! attributes (see `spec/index.md`). There is always exactly one active theme;
//! the bundled `Dark` and `Light` themes (from `themes/dark.json` /
//! `themes/light.json`) are just ordinary themes the host ships. The style
//! helpers ([`fg`], [`bg`], [`region_base`], …) read the active theme.
//! [`theme_names`] produces the de-duplicated, sorted list for the View → Theme
//! submenu.
//!
//! Theme names are plain strings (also the value persisted in settings), so this
//! crate stays free of any localization dependency.

#![warn(clippy::pedantic)]

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::path::Path;
use std::sync::RwLock;

use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

// Ultimate fallbacks used only before a theme is loaded, or for a theme that
// leaves the editor foreground/background unset. They match the bundled `Dark`.
const FALLBACK_FG: Color = Color::Rgb(215, 215, 215);
const FALLBACK_BG: Color = Color::Rgb(40, 40, 40);

/// Primary foreground: the active theme's editor foreground (or the dark default).
///
/// # Panics
/// Panics if the active-theme lock is poisoned.
#[must_use]
pub fn fg() -> Color {
    CUSTOM
        .read()
        .expect("theme lock")
        .as_ref()
        .and_then(|c| c.editor.foreground)
        .map_or(FALLBACK_FG, rgb)
}

/// Primary background: the active theme's editor background (or the dark default).
///
/// # Panics
/// Panics if the active-theme lock is poisoned.
#[must_use]
pub fn bg() -> Color {
    CUSTOM
        .read()
        .expect("theme lock")
        .as_ref()
        .and_then(|c| c.editor.background)
        .map_or(FALLBACK_BG, rgb)
}

/// The base style: theme foreground on theme background. Used to paint the whole
/// frame so every pane shares the same background.
#[must_use]
pub fn base() -> Style {
    Style::default().fg(fg()).bg(bg())
}

/// Style for panel titles/borders. A focused title is plain (full intensity) and
/// an unfocused one is dimmed.
#[must_use]
pub fn title(focused: bool) -> Style {
    let base = Style::default().fg(fg());
    if focused {
        base
    } else {
        base.add_modifier(Modifier::DIM)
    }
}

/// Style for a `region`'s panel title/border: the region's foreground, dimmed
/// when unfocused. Keeps borders matching panes whose colors differ from the
/// editor's.
#[must_use]
pub fn region_title(region: Region, focused: bool) -> Style {
    let base = Style::default().fg(region_fg(region));
    if focused {
        base
    } else {
        base.add_modifier(Modifier::DIM)
    }
}

/// Style for the active selection (reversed video; the one allowed exception to
/// the no-reversed-text rule).
#[must_use]
pub fn selected() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}

/// Dimmed foreground for secondary/hint text.
#[must_use]
pub fn dim() -> Style {
    Style::default().fg(fg()).add_modifier(Modifier::DIM)
}

// ===========================================================================
// Theme model (per-region RGB), loaded from JSON. See spec/index.md.
// ===========================================================================

/// An `[R, G, B]` color, 0-255 per channel.
pub type Rgb = [u8; 3];

/// A themeable UI region.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Region {
    /// The top menu bar.
    MenuBar,
    /// The bottom status bar.
    StatusBar,
    /// The left dock (file explorer).
    LeftDock,
    /// The right dock (message drawer).
    RightDock,
    /// The center editor.
    Editor,
}

/// Foreground/background colors and font attributes for a region (each optional;
/// a missing value falls back to the primary editor color).
#[derive(Deserialize, Clone, Default)]
pub struct RegionColors {
    /// Foreground color.
    pub foreground: Option<Rgb>,
    /// Background color.
    pub background: Option<Rgb>,
    /// `"normal"` (default) or `"italic"`.
    #[serde(rename = "font-style", default)]
    pub font_style: Option<String>,
    /// `"normal"` (default) or `"bold"`.
    #[serde(rename = "font-weight", default)]
    pub font_weight: Option<String>,
}

/// Editor colors, including an optional cursor color and font attributes.
#[derive(Deserialize, Clone, Default)]
pub struct EditorColors {
    /// Text foreground.
    pub foreground: Option<Rgb>,
    /// Editor background.
    pub background: Option<Rgb>,
    /// Cursor color.
    pub cursor: Option<Rgb>,
    /// `"normal"` (default) or `"italic"`.
    #[serde(rename = "font-style", default)]
    pub font_style: Option<String>,
    /// `"normal"` (default) or `"bold"`.
    #[serde(rename = "font-weight", default)]
    pub font_weight: Option<String>,
}

/// Optional syntax-highlight colors.
#[derive(Deserialize, Clone, Default)]
pub struct SyntaxColors {
    /// Keywords.
    pub keyword: Option<Rgb>,
    /// String literals.
    pub string: Option<Rgb>,
    /// Comments.
    pub comment: Option<Rgb>,
    /// Numeric literals.
    pub number: Option<Rgb>,
}

/// A theme loaded from a JSON file (bundled in the binary or installed in
/// `~/.config/vix/themes/`).
#[derive(Deserialize, Clone)]
pub struct CustomTheme {
    /// Display name (also the value persisted in settings).
    pub name: String,
    /// Menu-bar colors.
    #[serde(rename = "menu-bar", default)]
    pub menu_bar: RegionColors,
    /// Status-bar colors.
    #[serde(rename = "status-bar", default)]
    pub status_bar: RegionColors,
    /// Left-dock (explorer) colors.
    #[serde(rename = "left-dock", default)]
    pub left_dock: RegionColors,
    /// Right-dock (messages) colors.
    #[serde(rename = "right-dock", default)]
    pub right_dock: RegionColors,
    /// Editor colors.
    #[serde(default)]
    pub editor: EditorColors,
    /// Syntax colors.
    #[serde(default)]
    pub syntax: SyntaxColors,
}

impl CustomTheme {
    /// The foreground/background for `region`.
    fn region_colors(&self, region: Region) -> RegionColors {
        match region {
            Region::MenuBar => self.menu_bar.clone(),
            Region::StatusBar => self.status_bar.clone(),
            Region::LeftDock => self.left_dock.clone(),
            Region::RightDock => self.right_dock.clone(),
            Region::Editor => RegionColors {
                foreground: self.editor.foreground,
                background: self.editor.background,
                font_style: self.editor.font_style.clone(),
                font_weight: self.editor.font_weight.clone(),
            },
        }
    }
}

impl RegionColors {
    /// Font attributes (`ITALIC` / `BOLD`) requested by this region, if any.
    fn modifiers(&self) -> Modifier {
        let mut m = Modifier::empty();
        if self.font_style.as_deref() == Some("italic") {
            m |= Modifier::ITALIC;
        }
        if self.font_weight.as_deref() == Some("bold") {
            m |= Modifier::BOLD;
        }
        m
    }
}

// The active theme. Always `Some` after the host applies one at startup; the
// fallbacks above cover the brief window before that.
static CUSTOM: RwLock<Option<CustomTheme>> = RwLock::new(None);

/// Set (or clear) the active theme.
///
/// # Panics
/// Panics if the active-theme lock is poisoned.
pub fn set_custom(theme: Option<CustomTheme>) {
    *CUSTOM.write().expect("theme lock") = theme;
}

/// Name of the active theme, if one is active.
///
/// # Panics
/// Panics if the active-theme lock is poisoned.
#[must_use]
pub fn custom_name() -> Option<String> {
    CUSTOM.read().expect("theme lock").as_ref().map(|c| c.name.clone())
}

fn rgb(c: Rgb) -> Color {
    Color::Rgb(c[0], c[1], c[2])
}

/// Foreground color for `region`: the theme's, or the primary editor foreground.
///
/// # Panics
/// Panics if the active-theme lock is poisoned.
#[must_use]
pub fn region_fg(region: Region) -> Color {
    if let Some(ct) = CUSTOM.read().expect("theme lock").as_ref()
        && let Some(c) = ct.region_colors(region).foreground {
            return rgb(c);
        }
    fg()
}

/// Background color for `region`: the theme's, or the primary editor background.
///
/// # Panics
/// Panics if the active-theme lock is poisoned.
#[must_use]
pub fn region_bg(region: Region) -> Color {
    if let Some(ct) = CUSTOM.read().expect("theme lock").as_ref()
        && let Some(c) = ct.region_colors(region).background {
            return rgb(c);
        }
    bg()
}

/// Font attributes (`ITALIC` / `BOLD`) the active theme requests for `region`.
///
/// # Panics
/// Panics if the active-theme lock is poisoned.
#[must_use]
pub fn region_modifiers(region: Region) -> Modifier {
    CUSTOM
        .read()
        .expect("theme lock")
        .as_ref().map_or_else(Modifier::empty, |ct| ct.region_colors(region).modifiers())
}

/// Base style (fg on bg, plus any custom font attributes) for `region`.
#[must_use]
pub fn region_base(region: Region) -> Style {
    Style::default()
        .fg(region_fg(region))
        .bg(region_bg(region))
        .add_modifier(region_modifiers(region))
}

/// Cursor color from the active theme, if one specifies it.
///
/// # Panics
/// Panics if the active-theme lock is poisoned.
#[must_use]
pub fn editor_cursor() -> Option<Color> {
    CUSTOM
        .read()
        .expect("theme lock")
        .as_ref()
        .and_then(|c| c.editor.cursor)
        .map(rgb)
}

/// Syntax-highlight colors as `(token, "#rrggbb")` pairs from the active theme.
/// Empty when the theme specifies no token colors (so the editor stays plain).
///
/// # Panics
/// Panics if the active-theme lock is poisoned.
#[must_use]
pub fn syntax_theme() -> Vec<(&'static str, String)> {
    let guard = CUSTOM.read().expect("theme lock");
    let Some(ct) = guard.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (token, color) in [
        ("keyword", ct.syntax.keyword),
        ("string", ct.syntax.string),
        ("comment", ct.syntax.comment),
        ("number", ct.syntax.number),
    ] {
        if let Some([r, g, b]) = color {
            out.push((token, format!("#{r:02x}{g:02x}{b:02x}")));
        }
    }
    out
}

/// The active theme's color for one syntax token (`"keyword"`, `"string"`,
/// `"comment"`, `"number"`); `None` when no theme is active, the token is
/// unknown, or the theme leaves it unset.
///
/// # Panics
/// Panics if the active-theme lock is poisoned.
#[must_use]
pub fn syntax_color(token: &str) -> Option<Color> {
    let guard = CUSTOM.read().expect("theme lock");
    let ct = guard.as_ref()?;
    let color = match token {
        "keyword" => ct.syntax.keyword,
        "string" => ct.syntax.string,
        "comment" => ct.syntax.comment,
        "number" => ct.syntax.number,
        _ => None,
    }?;
    Some(rgb(color))
}

/// Parse a single theme from JSON, returning `None` if it doesn't parse.
#[must_use]
pub fn parse_theme(json: &str) -> Option<CustomTheme> {
    serde_json::from_str(json).ok()
}

/// Load all themes (`*.json`) from `dir`, sorted by name. Unreadable or malformed
/// files are skipped.
#[must_use]
pub fn load_custom_themes(dir: &Path) -> Vec<CustomTheme> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Some(theme) = parse_theme(&text) {
                    out.push(theme);
                }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Apply `theme` as the active theme.
pub fn apply(theme: &CustomTheme) {
    set_custom(Some(theme.clone()));
}

/// The display names for a list of `themes`, for the View → Theme submenu:
/// de-duplicated by name (case-insensitively; the first of any repeated name
/// wins, so user-installed themes shadow bundled ones) and sorted by name.
#[must_use]
pub fn theme_names(themes: &[CustomTheme]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for theme in themes {
        let key = theme.name.to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        names.push(theme.name.clone());
    }
    names.sort_by_key(|n| n.to_lowercase());
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(json: &str) -> CustomTheme {
        serde_json::from_str(json).unwrap()
    }

    // One test to keep the process-global theme state sequential.
    #[test]
    fn parsing_regions_and_names() {
        let t = theme(
            r#"{
                "name": "test",
                "menu-bar": { "foreground": [1, 2, 3], "background": [4, 5, 6] },
                "editor": { "foreground": [7, 8, 9], "background": [10, 11, 12] }
            }"#,
        );
        assert_eq!(t.name, "test");

        set_custom(Some(t));
        assert_eq!(region_fg(Region::MenuBar), Color::Rgb(1, 2, 3));
        assert_eq!(region_bg(Region::MenuBar), Color::Rgb(4, 5, 6));
        assert_eq!(region_fg(Region::Editor), Color::Rgb(7, 8, 9));
        // An unspecified region channel falls back to the primary editor color.
        assert_eq!(region_bg(Region::LeftDock), Color::Rgb(10, 11, 12));
        assert_eq!(custom_name().as_deref(), Some("test"));

        // theme_names de-dups (case-insensitively) and sorts by name.
        let names = theme_names(&[
            theme(r#"{ "name": "Nord" }"#),
            theme(r#"{ "name": "Dark" }"#),
            theme(r#"{ "name": "dark" }"#), // duplicate of "Dark"
        ]);
        assert_eq!(names, vec!["Dark".to_string(), "Nord".to_string()]);

        // Applying a theme makes it active.
        apply(&theme(r#"{ "name": "Light", "editor": { "foreground": [40, 40, 40] } }"#));
        assert_eq!(custom_name().as_deref(), Some("Light"));
        assert_eq!(fg(), Color::Rgb(40, 40, 40));

        // Syntax colors: set tokens resolve, unset and unknown ones are None.
        apply(&theme(
            r#"{
                "name": "Syn",
                "syntax": { "keyword": [1, 1, 1], "string": [2, 2, 2], "number": [4, 4, 4] }
            }"#,
        ));
        assert_eq!(syntax_color("keyword"), Some(Color::Rgb(1, 1, 1)));
        assert_eq!(syntax_color("number"), Some(Color::Rgb(4, 4, 4)));
        assert_eq!(syntax_color("comment"), None, "unset token");
        assert_eq!(syntax_color("nonsense"), None, "unknown token");
        // And the editor-facing pairs include the number token.
        let pairs = syntax_theme();
        assert!(pairs.contains(&("number", "#040404".to_string())), "{pairs:?}");
    }
}
