//! Single-color themes: the bundled `… Monochrome` themes imitate one-color
//! displays, so everything on screen is drawn in the theme's one color on its
//! background (see `spec/index.md`, "Single-color themes").
//!
//! A theme is single-color when all five regions set the same foreground and
//! the same background, and it sets no syntax and no cursor colors. The host
//! passes each finished frame through [`flatten`], which maps every cell color
//! that isn't the background to the foreground. That catches the colors drawn
//! outside the theme (gutter marks, spell-check underlines, diagnostics, the
//! terminal pane) in one place, for the terminal UI and every GUI front end.

use ratatui::buffer::Buffer;
use ratatui::style::Color;

use crate::{CustomTheme, Rgb, read, rgb};

/// Old names of renamed bundled themes and their current names, so a saved
/// setting that names the old one keeps working.
const RENAMED: &[(&str, &str)] = &[
    ("Phosphor Green", "Phosphor Green Monochrome"),
    ("Phosphor Amber", "Phosphor Amber Monochrome"),
    ("Safelight Red", "Safelight Red Monochrome"),
];

/// The current name for a saved theme `name`: its new name if the bundled
/// theme was renamed (matched case-insensitively), else `name` unchanged.
#[must_use]
pub fn current_name(name: &str) -> &str {
    RENAMED
        .iter()
        .find(|(old, _)| old.eq_ignore_ascii_case(name))
        .map_or(name, |&(_, new)| new)
}

impl CustomTheme {
    /// The `(foreground, background)` of a single-color theme, or `None` when
    /// this theme uses more than one color.
    #[must_use]
    pub fn single_color(&self) -> Option<(Rgb, Rgb)> {
        let s = &self.syntax;
        if self.editor.cursor.is_some()
            || s.keyword.is_some()
            || s.string.is_some()
            || s.comment.is_some()
            || s.number.is_some()
        {
            return None;
        }
        let pairs = [
            (self.menu_bar.foreground, self.menu_bar.background),
            (self.status_bar.foreground, self.status_bar.background),
            (self.left_dock.foreground, self.left_dock.background),
            (self.right_dock.foreground, self.right_dock.background),
            (self.editor.foreground, self.editor.background),
        ];
        let (Some(fg), Some(bg)) = pairs[0] else {
            return None;
        };
        pairs
            .iter()
            .all(|&p| p == (Some(fg), Some(bg)))
            .then_some((fg, bg))
    }
}

/// The active theme's `(foreground, background)` when it is single-color.
#[must_use]
pub fn active() -> Option<(Color, Color)> {
    let (fg, bg) = read().as_ref()?.single_color()?;
    Some((rgb(fg), rgb(bg)))
}

/// Within this distance (in RGB space) of the background, a color counts as
/// background. Kept small so dim gray hint text still shows, in the theme color.
const NEAR_BACKGROUND: u32 = 48;

/// Repaint every cell of `buf` in `fg` on `bg`: a color near `bg` (and a
/// `Reset` background) becomes `bg`, every other color (and a `Reset`
/// foreground) becomes `fg`. A cell that would end up `fg` on `fg` — a colored
/// highlight under colored text — gets `bg` text, so it reads as reverse video.
pub fn flatten(buf: &mut Buffer, fg: Color, bg: Color) {
    let bg_rgb = approx_rgb(bg).unwrap_or((0, 0, 0));
    let is_bg = |c: Color| approx_rgb(c).is_some_and(|c| distance(c, bg_rgb) <= NEAR_BACKGROUND);
    for cell in &mut buf.content {
        let lit_bg = cell.bg != Color::Reset && !is_bg(cell.bg);
        let lit_fg = cell.fg == Color::Reset || !is_bg(cell.fg);
        cell.bg = if lit_bg { fg } else { bg };
        cell.fg = if lit_fg && !lit_bg { fg } else { bg };
        if cell.underline_color != Color::Reset {
            cell.underline_color = fg;
        }
    }
}

/// Euclidean distance between two RGB colors, rounded down.
fn distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let d = |x: u8, y: u8| u32::from(x.abs_diff(y)).pow(2);
    (d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)).isqrt()
}

/// An approximate RGB value for `c` (the xterm palette for named and indexed
/// colors), or `None` for `Reset`.
fn approx_rgb(c: Color) -> Option<(u8, u8, u8)> {
    Some(match c {
        Color::Reset => return None,
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black | Color::Indexed(0 | 16) => (0, 0, 0),
        Color::Indexed(i @ 232..=255) => {
            let v = 8 + (i - 232) * 10;
            (v, v, v)
        }
        // Every other named or indexed color is bright enough to be text.
        _ => (128, 128, 128),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    const GREEN: Color = Color::Rgb(65, 255, 0);
    const BLACK: Color = Color::Rgb(0, 0, 0);

    fn one_cell(fg: Color, bg: Color) -> (Color, Color) {
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 1));
        buf.content[0].fg = fg;
        buf.content[0].bg = bg;
        flatten(&mut buf, GREEN, BLACK);
        (buf.content[0].fg, buf.content[0].bg)
    }

    #[test]
    fn colored_text_becomes_the_theme_color() {
        assert_eq!(one_cell(Color::Rgb(248, 81, 73), BLACK), (GREEN, BLACK));
        assert_eq!(one_cell(Color::Red, Color::Reset), (GREEN, BLACK));
        assert_eq!(one_cell(Color::Reset, Color::Reset), (GREEN, BLACK));
        // Dim gray hint text still shows.
        assert_eq!(one_cell(Color::DarkGray, BLACK), (GREEN, BLACK));
    }

    #[test]
    fn a_colored_highlight_reads_as_reverse_video() {
        assert_eq!(
            one_cell(Color::White, Color::Rgb(40, 80, 200)),
            (BLACK, GREEN)
        );
        assert_eq!(one_cell(Color::Black, Color::Yellow), (BLACK, GREEN));
    }

    #[test]
    fn near_black_is_background() {
        assert_eq!(one_cell(GREEN, Color::Rgb(20, 20, 20)), (GREEN, BLACK));
    }

    #[test]
    fn renamed_themes_resolve_to_their_new_names() {
        assert_eq!(current_name("Phosphor Green"), "Phosphor Green Monochrome");
        assert_eq!(current_name("phosphor amber"), "Phosphor Amber Monochrome");
        assert_eq!(current_name("Safelight Red"), "Safelight Red Monochrome");
        assert_eq!(current_name("Dark"), "Dark");
    }

    #[test]
    fn single_color_needs_one_color_everywhere() {
        let region = r#"{"foreground": [1, 2, 3], "background": [0, 0, 0]}"#;
        let json = |extra: &str| {
            format!(
                r#"{{"name": "T", "menu-bar": {region}, "status-bar": {region},
                "left-dock": {region}, "right-dock": {region}, "editor": {region}{extra}}}"#
            )
        };
        let theme = |extra: &str| crate::parse_theme(&json(extra)).expect("parses");
        assert_eq!(theme("").single_color(), Some(([1, 2, 3], [0, 0, 0])));
        assert_eq!(
            theme(r#", "syntax": {"keyword": [9, 9, 9]}"#).single_color(),
            None
        );
        let partial = crate::parse_theme(r#"{"name": "P", "editor": {"foreground": [1, 2, 3]}}"#)
            .expect("parses");
        assert_eq!(partial.single_color(), None, "every region must be set");
    }
}
