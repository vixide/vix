//! Plan how to paint a ratatui cell grid, independent of any GUI toolkit:
//! resolve colors to plain RGBA, and split each row into background runs and
//! styled text spans. A toolkit front end then only fills rectangles and draws
//! shaped text at cell positions.
//!
//! Text is planned in *spans* — maximal stretches of one row sharing a style —
//! so a frame costs a few hundred text-shaping calls rather than one per
//! cell. Every span starts at its exact cell column, and any non-ASCII cell
//! (box drawing, Nerd Font icons, CJK, emoji — often from a fallback font with
//! a different advance) is a span of its own, so a fallback glyph can never
//! push the rest of its row off the grid.

use ratatui::buffer::{Buffer, Cell};
use ratatui::style::{Color, Modifier};

/// An sRGB color with alpha, 8 bits per channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba8 {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha (255 = opaque).
    pub a: u8,
}

impl Rgba8 {
    /// An opaque color from a `0xRRGGBB` value.
    #[must_use]
    pub const fn hex(v: u32) -> Self {
        let [_, r, g, b] = v.to_be_bytes();
        Self { r, g, b, a: 255 }
    }

    /// An opaque color from components.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

/// Default colors for `Color::Reset`, picked from the OS appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// Foreground for `Color::Reset`.
    pub fg: Rgba8,
    /// Background for `Color::Reset`.
    pub bg: Rgba8,
}

impl Palette {
    /// The dark or light default palette.
    #[must_use]
    pub const fn for_appearance(dark: bool) -> Self {
        if dark {
            Self {
                fg: Rgba8::hex(0xd4_d4d4),
                bg: Rgba8::hex(0x1e_1e1e),
            }
        } else {
            Self {
                fg: Rgba8::hex(0x1e_1e1e),
                bg: Rgba8::hex(0xff_ffff),
            }
        }
    }
}

/// A ratatui color as RGBA, resolving `Reset` to `default` and the 16 named /
/// 256 indexed colors through the standard xterm palette.
#[must_use]
pub fn resolve(color: Color, default: Rgba8) -> Rgba8 {
    let hex = match color {
        Color::Reset => return default,
        Color::Rgb(r, g, b) => return Rgba8::rgb(r, g, b),
        Color::Indexed(i) => return indexed(i),
        Color::Black => 0x00_0000,
        Color::Red => 0xcd_0000,
        Color::Green => 0x00_cd00,
        Color::Yellow => 0xcd_cd00,
        Color::Blue => 0x00_00ee,
        Color::Magenta => 0xcd_00cd,
        Color::Cyan => 0x00_cdcd,
        Color::Gray => 0xe5_e5e5,
        Color::DarkGray => 0x7f_7f7f,
        Color::LightRed => 0xff_0000,
        Color::LightGreen => 0x00_ff00,
        Color::LightYellow => 0xff_ff00,
        Color::LightBlue => 0x5c_5cff,
        Color::LightMagenta => 0xff_00ff,
        Color::LightCyan => 0x00_ffff,
        Color::White => 0xff_ffff,
    };
    Rgba8::hex(hex)
}

/// An xterm-256 palette entry.
fn indexed(i: u8) -> Rgba8 {
    const BASE: [Color; 16] = [
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::Gray,
        Color::DarkGray,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
        Color::White,
    ];
    match i {
        0..=15 => resolve(BASE[usize::from(i)], Rgba8::hex(0)),
        16..=231 => {
            let n = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Rgba8::rgb(level(n / 36), level((n / 6) % 6), level(n % 6))
        }
        232..=255 => {
            let v = 8 + (i - 232) * 10;
            Rgba8::rgb(v, v, v)
        }
    }
}

/// A cell's effective (foreground, background), after `REVERSED` and `DIM`.
#[must_use]
pub fn cell_colors(cell: &Cell, p: Palette) -> (Rgba8, Rgba8) {
    let mut fg = resolve(cell.fg, p.fg);
    let mut bg = resolve(cell.bg, p.bg);
    if cell.modifier.contains(Modifier::REVERSED) {
        std::mem::swap(&mut fg, &mut bg);
    }
    if cell.modifier.contains(Modifier::DIM) {
        fg.a = 153; // 60%
    }
    (fg, bg)
}

/// A stretch of one row with a non-default background.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BgRun {
    /// First column.
    pub col: u16,
    /// Number of cells.
    pub len: u16,
    /// Fill color.
    pub color: Rgba8,
}

/// How a text span is drawn: its color, plus the cell's ratatui modifiers
/// (`BOLD`, `ITALIC`, `UNDERLINED`, `CROSSED_OUT` are the ones a front end
/// draws; `REVERSED`/`DIM` are already folded into `fg`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanStyle {
    /// Text color.
    pub fg: Rgba8,
    /// The cell's modifiers.
    pub modifier: Modifier,
}

/// A stretch of one row's text sharing a style, starting at `col`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextSpan {
    /// First column.
    pub col: u16,
    /// The text (one symbol per cell).
    pub text: String,
    /// How to draw it.
    pub style: SpanStyle,
}

/// Row `y`'s background runs, omitting the default background (the caller
/// fills the whole grid with it first).
#[must_use]
pub fn row_backgrounds(buffer: &Buffer, y: u16, p: Palette) -> Vec<BgRun> {
    let width = buffer.area.width;
    let mut out = Vec::new();
    let mut x = 0;
    while x < width {
        let (_, bg) = cell_colors(&buffer[(x, y)], p);
        let start = x;
        while x < width && cell_colors(&buffer[(x, y)], p).1 == bg {
            x += 1;
        }
        if bg != p.bg {
            out.push(BgRun {
                col: start,
                len: x - start,
                color: bg,
            });
        }
    }
    out
}

/// Whether two cells can share one text span.
fn same_text_style(a: &Cell, b: &Cell) -> bool {
    a.fg == b.fg && a.bg == b.bg && a.modifier == b.modifier
}

/// Row `y`'s visible text spans (see the module docs). Blank and `HIDDEN`
/// spans are omitted.
#[must_use]
pub fn row_text(buffer: &Buffer, y: u16, p: Palette) -> Vec<TextSpan> {
    let width = buffer.area.width;
    let mut out = Vec::new();
    let mut x = 0;
    while x < width {
        let first = &buffer[(x, y)];
        let start = x;
        let mut text = String::from(first.symbol());
        x += 1;
        if first.symbol().is_ascii() {
            while x < width {
                let c = &buffer[(x, y)];
                if !c.symbol().is_ascii() || !same_text_style(first, c) {
                    break;
                }
                text.push_str(c.symbol());
                x += 1;
            }
        }
        if first.modifier.contains(Modifier::HIDDEN) || text.trim().is_empty() {
            continue;
        }
        out.push(TextSpan {
            col: start,
            text,
            style: SpanStyle {
                fg: cell_colors(first, p).0,
                modifier: first.modifier,
            },
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use ratatui::style::Style;

    const DARK: Palette = Palette::for_appearance(true);

    #[test]
    fn resolves_reset_rgb_and_indexed_colors() {
        let default = Rgba8::hex(0x12_3456);
        assert_eq!(resolve(Color::Reset, default), default);
        assert_eq!(resolve(Color::Rgb(1, 2, 3), default), Rgba8::rgb(1, 2, 3));
        assert_eq!(resolve(Color::Indexed(1), default), Rgba8::hex(0xcd_0000));
        // 6×6×6 cube: index 16 is black, 231 is white.
        assert_eq!(resolve(Color::Indexed(16), default), Rgba8::rgb(0, 0, 0));
        assert_eq!(
            resolve(Color::Indexed(231), default),
            Rgba8::rgb(255, 255, 255)
        );
        // Grayscale ramp.
        assert_eq!(resolve(Color::Indexed(232), default), Rgba8::rgb(8, 8, 8));
    }

    #[test]
    fn reversed_swaps_and_dim_fades() {
        let mut cell = Cell::new("x");
        cell.set_fg(Color::Rgb(10, 20, 30));
        cell.modifier = Modifier::REVERSED;
        let (fg, bg) = cell_colors(&cell, DARK);
        assert_eq!(fg, DARK.bg);
        assert_eq!(bg, Rgba8::rgb(10, 20, 30));
        cell.modifier = Modifier::DIM;
        assert!(cell_colors(&cell, DARK).0.a < 255);
    }

    #[test]
    fn rows_split_into_styled_spans_and_isolate_non_ascii() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 12, 1));
        buf.set_string(0, 0, "ab", Style::default());
        buf.set_string(2, 0, "cd", Style::default().fg(Color::Red));
        buf.set_string(5, 0, "x\u{f07b}y", Style::default());
        let spans = row_text(&buf, 0, DARK);
        let got: Vec<(u16, &str)> = spans.iter().map(|s| (s.col, s.text.as_str())).collect();
        // "x" carries the trailing blank up to the icon; the icon stands alone;
        // the run after it resumes at the icon's next column.
        assert_eq!(
            got,
            [
                (0, "ab"),
                (2, "cd"),
                (4, " x"),
                (6, "\u{f07b}"),
                (7, "y    ")
            ]
        );
        assert_eq!(spans[1].style.fg, Rgba8::hex(0xcd_0000));
    }

    #[test]
    fn background_runs_skip_the_default() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 6, 1));
        buf.set_style(Rect::new(2, 0, 3, 1), Style::default().bg(Color::Blue));
        assert_eq!(
            row_backgrounds(&buf, 0, DARK),
            [BgRun {
                col: 2,
                len: 3,
                color: Rgba8::hex(0x00_00ee)
            }]
        );
    }
}
