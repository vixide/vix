//! The Vix theme model and chooser state.
//!
//! The theme is strictly monochrome (see `spec/themes.md`): one foreground and
//! one background, with two modes that swap them — Dark (white on black, the
//! default) and Light (black on white). Emphasis comes from bold/dim intensity,
//! never from hue. The sole use of reversed video is selections ([`selected`]).
//!
//! Color names ([`Mode::label`]) are returned as i18n keys for the host to
//! translate, so this crate stays free of any localization dependency.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use ratatui_core::style::{Color, Modifier, Style};
use serde::Deserialize;

/// The two themes. Dark is the default.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// White foreground on black background.
    Dark,
    /// Black foreground on white background.
    Light,
}

/// All themes, in the order the chooser presents them.
pub const MODES: [Mode; 2] = [Mode::Dark, Mode::Light];

impl Mode {
    /// i18n key for this theme's display name (translated by the host).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Mode::Dark => "theme.dark",
            Mode::Light => "theme.light",
        }
    }

    /// Stable identifier persisted in settings.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Mode::Dark => "dark",
            Mode::Light => "light",
        }
    }

    /// Parse a persisted name, defaulting to Dark for anything unrecognized.
    #[must_use]
    pub fn from_name(s: &str) -> Mode {
        match s {
            "light" => Mode::Light,
            _ => Mode::Dark,
        }
    }
}

// Current mode as a process-global so the static style helpers can read it
// without threading a theme through every render function. false = Dark.
static LIGHT: AtomicBool = AtomicBool::new(false);

/// The currently active theme mode.
#[must_use]
pub fn mode() -> Mode {
    if LIGHT.load(Ordering::Relaxed) {
        Mode::Light
    } else {
        Mode::Dark
    }
}

/// Switch the active theme mode.
pub fn set_mode(m: Mode) {
    LIGHT.store(m == Mode::Light, Ordering::Relaxed);
}

/// Foreground color for the current mode.
#[must_use]
pub fn fg() -> Color {
    match mode() {
        Mode::Dark => Color::White,
        Mode::Light => Color::Black,
    }
}

/// Background color for the current mode.
#[must_use]
pub fn bg() -> Color {
    match mode() {
        Mode::Dark => Color::Black,
        Mode::Light => Color::White,
    }
}

/// The base style: theme foreground on theme background. Used to paint the whole
/// frame so every pane shares the same background.
#[must_use]
pub fn base() -> Style {
    Style::default().fg(fg()).bg(bg())
}

/// Style for panel titles/borders. The built-in themes use no bold, so a focused
/// title is plain (full intensity) and an unfocused one is dimmed.
#[must_use]
pub fn title(focused: bool) -> Style {
    let base = Style::default().fg(fg());
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
// Custom themes (per-region RGB), loaded from JSON. See spec/themes.md.
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
/// missing values fall back to the active built-in monochrome theme).
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
}

/// A custom theme loaded from a JSON file in `~/.config/vix/themes/`.
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

// The active custom theme, if any. When set, its per-region colors override the
// monochrome defaults; unset regions/channels still fall back to monochrome.
static CUSTOM: RwLock<Option<CustomTheme>> = RwLock::new(None);

/// Set (or clear) the active custom theme.
pub fn set_custom(theme: Option<CustomTheme>) {
    *CUSTOM.write().expect("theme lock") = theme;
}

/// Name of the active custom theme, if one is active.
#[must_use]
pub fn custom_name() -> Option<String> {
    CUSTOM.read().expect("theme lock").as_ref().map(|c| c.name.clone())
}

fn rgb(c: Rgb) -> Color {
    Color::Rgb(c[0], c[1], c[2])
}

/// Foreground color for `region`: the custom theme's, or the monochrome default.
#[must_use]
pub fn region_fg(region: Region) -> Color {
    if let Some(ct) = CUSTOM.read().expect("theme lock").as_ref() {
        if let Some(c) = ct.region_colors(region).foreground {
            return rgb(c);
        }
    }
    fg()
}

/// Background color for `region`: the custom theme's, or the monochrome default.
#[must_use]
pub fn region_bg(region: Region) -> Color {
    if let Some(ct) = CUSTOM.read().expect("theme lock").as_ref() {
        if let Some(c) = ct.region_colors(region).background {
            return rgb(c);
        }
    }
    bg()
}

/// Font attributes (`ITALIC` / `BOLD`) the active custom theme requests for
/// `region`. Empty for the built-in themes (which use no italic or bold).
#[must_use]
pub fn region_modifiers(region: Region) -> Modifier {
    CUSTOM
        .read()
        .expect("theme lock")
        .as_ref()
        .map(|ct| ct.region_colors(region).modifiers())
        .unwrap_or_else(Modifier::empty)
}

/// Base style (fg on bg, plus any custom font attributes) for `region`.
#[must_use]
pub fn region_base(region: Region) -> Style {
    Style::default()
        .fg(region_fg(region))
        .bg(region_bg(region))
        .add_modifier(region_modifiers(region))
}

/// Cursor color from the active custom theme, if one specifies it.
#[must_use]
pub fn editor_cursor() -> Option<Color> {
    CUSTOM
        .read()
        .expect("theme lock")
        .as_ref()
        .and_then(|c| c.editor.cursor)
        .map(rgb)
}

/// Syntax-highlight colors as `(token, "#rrggbb")` pairs from the active custom
/// theme. Empty when no custom theme is active (so the editor stays monochrome).
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
    ] {
        if let Some([r, g, b]) = color {
            out.push((token, format!("#{r:02x}{g:02x}{b:02x}")));
        }
    }
    out
}

/// Parse a single custom theme from JSON, returning `None` if it doesn't parse.
#[must_use]
pub fn parse_theme(json: &str) -> Option<CustomTheme> {
    serde_json::from_str(json).ok()
}

/// Load all custom themes (`*.json`) from `dir`, sorted by name. Unreadable or
/// malformed files are skipped.
#[must_use]
pub fn load_custom_themes(dir: &Path) -> Vec<CustomTheme> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Some(theme) = parse_theme(&text) {
                    out.push(theme);
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// One entry in the theme chooser: a built-in mode or a loaded custom theme.
/// The custom theme is boxed because it is much larger than a [`Mode`].
#[derive(Clone)]
pub enum Choice {
    /// A built-in monochrome theme.
    Builtin(Mode),
    /// A custom JSON theme.
    Custom(Box<CustomTheme>),
}

impl Choice {
    /// The value persisted in settings (`"dark"`, `"light"`, or a custom name).
    #[must_use]
    pub fn id(&self) -> String {
        match self {
            Choice::Builtin(m) => m.name().to_string(),
            Choice::Custom(c) => c.name.clone(),
        }
    }

    /// The built-in mode, if this is a built-in choice.
    #[must_use]
    pub fn builtin(&self) -> Option<Mode> {
        match self {
            Choice::Builtin(m) => Some(*m),
            Choice::Custom(_) => None,
        }
    }

    /// The custom-theme name, if this is a custom choice.
    #[must_use]
    pub fn custom_name(&self) -> Option<&str> {
        match self {
            Choice::Custom(c) => Some(&c.name),
            Choice::Builtin(_) => None,
        }
    }

    /// Lower-cased canonical (English) name used to order the chooser. Built-ins
    /// use their stable id (`"dark"`/`"light"`) so the order is locale-stable.
    fn sort_key(&self) -> String {
        match self {
            Choice::Builtin(m) => m.name().to_string(),
            Choice::Custom(c) => c.name.to_lowercase(),
        }
    }
}

/// Apply a choice to the global theme state: built-ins set the mode and clear
/// any custom theme; custom themes are installed (keeping the current mode as a
/// fallback for unspecified regions).
pub fn apply(choice: &Choice) {
    match choice {
        Choice::Builtin(m) => {
            set_mode(*m);
            set_custom(None);
        }
        Choice::Custom(c) => set_custom(Some((**c).clone())),
    }
}

/// Selection state for the theme chooser overlay. Lists the two built-in themes
/// followed by any custom themes. Moving the selection previews the theme live;
/// the host commits or reverts.
pub struct Chooser {
    /// All choices, in display order.
    pub choices: Vec<Choice>,
    /// Index of the highlighted choice.
    pub selected: usize,
    /// Index of the choice active when the chooser opened, restored on cancel.
    pub original: usize,
}

impl Chooser {
    /// Open the chooser listing the built-in modes and `customs`, highlighting
    /// the currently active theme.
    ///
    /// Every choice — built-in modes included — is sorted alphabetically by its
    /// canonical (English, case-insensitive) name, e.g. Dark, Darker, Darkest,
    /// Dracula, …, Light, Lighter, …
    ///
    /// Custom themes are de-duplicated by name (case-insensitively): the first of
    /// any repeated name wins (so user-installed themes shadow bundled ones), and
    /// a custom theme that shadows a built-in mode name (`Dark`/`Light`) is
    /// dropped in favor of the built-in.
    #[must_use]
    pub fn open(customs: Vec<CustomTheme>) -> Self {
        // De-duplicate, preserving the caller's order so precedence (user over
        // bundled) is honored.
        let mut seen: Vec<String> = MODES.iter().map(|m| m.name().to_lowercase()).collect();
        let mut deduped: Vec<CustomTheme> = Vec::new();
        for theme in customs {
            let key = theme.name.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            deduped.push(theme);
        }

        // Built-ins + customs, all sorted alphabetically by canonical name.
        let mut choices: Vec<Choice> = MODES.iter().map(|m| Choice::Builtin(*m)).collect();
        choices.extend(deduped.into_iter().map(|t| Choice::Custom(Box::new(t))));
        choices.sort_by_key(Choice::sort_key);

        let active = custom_name();
        let selected = choices
            .iter()
            .position(|c| match (&active, c) {
                (Some(name), Choice::Custom(ct)) => &ct.name == name,
                (None, Choice::Builtin(m)) => *m == mode(),
                _ => false,
            })
            .unwrap_or(0);
        Chooser { choices, selected, original: selected }
    }

    /// Highlight the previous choice, wrapping around.
    pub fn up(&mut self) {
        let n = self.choices.len();
        self.selected = (self.selected + n - 1) % n;
    }

    /// Highlight the next choice, wrapping around.
    pub fn down(&mut self) {
        self.selected = (self.selected + 1) % self.choices.len();
    }

    /// The highlighted choice.
    #[must_use]
    pub fn selected_choice(&self) -> &Choice {
        &self.choices[self.selected]
    }

    /// The choice active when the chooser opened.
    #[must_use]
    pub fn original_choice(&self) -> &Choice {
        &self.choices[self.original]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test to keep the process-global theme state sequential.
    #[test]
    fn custom_theme_parsing_regions_and_chooser() {
        let json = r#"{
            "name": "test",
            "menu-bar": { "foreground": [1, 2, 3], "background": [4, 5, 6] },
            "editor": { "foreground": [7, 8, 9] }
        }"#;
        let theme: CustomTheme = serde_json::from_str(json).unwrap();
        assert_eq!(theme.name, "test");

        set_mode(Mode::Dark);
        set_custom(Some(theme));
        assert_eq!(region_fg(Region::MenuBar), Color::Rgb(1, 2, 3));
        assert_eq!(region_bg(Region::MenuBar), Color::Rgb(4, 5, 6));
        assert_eq!(region_fg(Region::Editor), Color::Rgb(7, 8, 9));
        // An unspecified channel falls back to the monochrome theme.
        assert_eq!(region_bg(Region::Editor), Color::Black);
        assert_eq!(custom_name().as_deref(), Some("test"));

        // The chooser lists the built-ins first, then customs.
        let custom: CustomTheme = serde_json::from_str(r#"{ "name": "solar" }"#).unwrap();
        let ch = Chooser::open(vec![custom]);
        assert_eq!(ch.choices.len(), 3);
        assert!(matches!(ch.choices[0], Choice::Builtin(Mode::Dark)));
        assert!(matches!(ch.choices[1], Choice::Builtin(Mode::Light)));
        assert_eq!(ch.choices[2].custom_name(), Some("solar"));

        // Applying a built-in clears the custom theme.
        apply(&Choice::Builtin(Mode::Light));
        assert_eq!(custom_name(), None);
        assert_eq!(region_fg(Region::MenuBar), Color::Black); // light fg
        set_mode(Mode::Dark);
    }
}
