//! Paint a ratatui cell grid with GPUI: background runs as quads, text as
//! shaped lines positioned on the cell grid, then the text cursor.
//!
//! Text is shaped in *runs* — maximal stretches of one row sharing a style —
//! so a frame costs a few hundred `shape_line` calls rather than one per cell
//! (GPUI caches shaped lines across frames, so an unchanged row is nearly
//! free). Every run starts at its exact cell's x position, and any non-ASCII
//! cell (box drawing, symbols, CJK, emoji, often from a fallback font with a
//! different advance) is shaped on its own, so a fallback glyph can never
//! push the rest of its row off the grid.

use gpui_kit::{
    App, Bounds, FontStyle, FontWeight, Hsla, Pixels, Rgba, SharedString, TextAlign, TextRun,
    UnderlineStyle, Window, fill, point, px, size,
};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Position;
use ratatui::style::{Color, Modifier};

/// Metrics and defaults the painter needs for one frame.
#[derive(Clone)]
pub struct Metrics {
    /// Monospace font family.
    pub family: SharedString,
    /// Per-glyph fallback families (Nerd Font icon glyphs, chiefly).
    pub fallbacks: Option<gpui_kit::FontFallbacks>,
    /// Font size.
    pub font_size: Pixels,
    /// Width of one cell.
    pub cell_width: Pixels,
    /// Height of one cell (the line height).
    pub cell_height: Pixels,
    /// Foreground for `Color::Reset`.
    pub default_fg: Rgba,
    /// Background for `Color::Reset`.
    pub default_bg: Rgba,
}

/// A ratatui color as RGB, resolving `Reset` to `default` and the 16 named /
/// 256 indexed colors through the standard xterm palette.
#[must_use]
pub fn rgb(color: Color, default: Rgba) -> Rgba {
    let hex = match color {
        Color::Reset => return default,
        Color::Rgb(r, g, b) => return from_rgb(r, g, b),
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
    gpui_kit::rgb(hex)
}

/// An xterm-256 palette entry.
fn indexed(i: u8) -> Rgba {
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
        0..=15 => rgb(BASE[usize::from(i)], gpui_kit::rgb(0)),
        16..=231 => {
            let n = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            from_rgb(level(n / 36), level((n / 6) % 6), level(n % 6))
        }
        232..=255 => {
            let v = 8 + (i - 232) * 10;
            from_rgb(v, v, v)
        }
    }
}

fn from_rgb(r: u8, g: u8, b: u8) -> Rgba {
    gpui_kit::rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
}

/// A cell's effective (foreground, background), after `REVERSED` and `DIM`.
#[must_use]
pub fn cell_colors(cell: &Cell, m: &Metrics) -> (Rgba, Rgba) {
    let mut fg = rgb(cell.fg, m.default_fg);
    let mut bg = rgb(cell.bg, m.default_bg);
    if cell.modifier.contains(Modifier::REVERSED) {
        std::mem::swap(&mut fg, &mut bg);
    }
    if cell.modifier.contains(Modifier::DIM) {
        fg.a *= 0.6;
    }
    (fg, bg)
}

/// Whether two cells can share one shaped text run.
fn same_text_style(a: &Cell, b: &Cell) -> bool {
    a.fg == b.fg && a.bg == b.bg && a.modifier == b.modifier
}

/// Paint `buffer` (and the cursor, if any) into `bounds`.
pub fn paint_grid(
    buffer: &Buffer,
    cursor: Option<Position>,
    m: &Metrics,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    window.paint_quad(fill(bounds, m.default_bg));
    let area = buffer.area;
    for y in 0..area.height {
        let top = bounds.origin.y + m.cell_height * f32::from(y);
        paint_row_backgrounds(buffer, y, top, m, bounds, window);
        paint_row_text(buffer, y, top, m, bounds, window, cx);
    }
    if let Some(pos) = cursor
        && pos.x < area.width
        && pos.y < area.height
    {
        let (fg, _) = cell_colors(&buffer[(pos.x, pos.y)], m);
        let origin = point(
            bounds.origin.x + m.cell_width * f32::from(pos.x),
            bounds.origin.y + m.cell_height * f32::from(pos.y),
        );
        window.paint_quad(fill(Bounds::new(origin, size(px(2.), m.cell_height)), fg));
    }
}

/// One quad per maximal run of a non-default background.
fn paint_row_backgrounds(
    buffer: &Buffer,
    y: u16,
    top: Pixels,
    m: &Metrics,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let width = buffer.area.width;
    let mut x = 0;
    while x < width {
        let (_, bg) = cell_colors(&buffer[(x, y)], m);
        let start = x;
        while x < width && cell_colors(&buffer[(x, y)], m).1 == bg {
            x += 1;
        }
        if bg != m.default_bg {
            let origin = point(bounds.origin.x + m.cell_width * f32::from(start), top);
            window.paint_quad(fill(
                Bounds::new(
                    origin,
                    size(m.cell_width * f32::from(x - start), m.cell_height),
                ),
                bg,
            ));
        }
    }
}

/// Shape and paint one row's text in style runs (see the module docs).
fn paint_row_text(
    buffer: &Buffer,
    y: u16,
    top: Pixels,
    m: &Metrics,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let width = buffer.area.width;
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
        let run = text_run(first, text.len(), m);
        let shaped = window
            .text_system()
            .shape_line(text.into(), m.font_size, &[run], None);
        let origin = point(bounds.origin.x + m.cell_width * f32::from(start), top);
        let _ = shaped.paint(origin, m.cell_height, TextAlign::Left, None, window, cx);
    }
}

/// The GPUI text run for `len` bytes styled like `cell`.
fn text_run(cell: &Cell, len: usize, m: &Metrics) -> TextRun {
    let (fg, _) = cell_colors(cell, m);
    let fg: Hsla = fg.into();
    let mut font = gpui_kit::font(m.family.clone());
    font.fallbacks.clone_from(&m.fallbacks);
    if cell.modifier.contains(Modifier::BOLD) {
        font.weight = FontWeight::BOLD;
    }
    if cell.modifier.contains(Modifier::ITALIC) {
        font.style = FontStyle::Italic;
    }
    TextRun {
        len,
        font,
        color: fg,
        background_color: None,
        underline: cell
            .modifier
            .contains(Modifier::UNDERLINED)
            .then_some(UnderlineStyle {
                thickness: px(1.),
                color: Some(fg),
                wavy: false,
            }),
        strikethrough: cell.modifier.contains(Modifier::CROSSED_OUT).then_some(
            gpui_kit::StrikethroughStyle {
                thickness: px(1.),
                color: Some(fg),
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(c: Rgba) -> (u8, u8, u8) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "components are in 0.0..=1.0"
        )]
        let to = |v: f32| (v * 255.0).round() as u8;
        (to(c.r), to(c.g), to(c.b))
    }

    #[test]
    fn resolves_reset_rgb_and_indexed_colors() {
        let default = gpui_kit::rgb(0x12_3456);
        assert_eq!(hex(rgb(Color::Reset, default)), (0x12, 0x34, 0x56));
        assert_eq!(hex(rgb(Color::Rgb(1, 2, 3), default)), (1, 2, 3));
        assert_eq!(hex(rgb(Color::Indexed(1), default)), (0xcd, 0, 0));
        // 6×6×6 cube: index 16 is black, 231 is white.
        assert_eq!(hex(rgb(Color::Indexed(16), default)), (0, 0, 0));
        assert_eq!(hex(rgb(Color::Indexed(231), default)), (255, 255, 255));
        // Grayscale ramp.
        assert_eq!(hex(rgb(Color::Indexed(232), default)), (8, 8, 8));
    }

    #[test]
    fn reversed_swaps_and_dim_fades() {
        let m = Metrics {
            family: "Menlo".into(),
            fallbacks: None,
            font_size: px(14.),
            cell_width: px(8.),
            cell_height: px(18.),
            default_fg: gpui_kit::rgb(0xff_ffff),
            default_bg: gpui_kit::rgb(0x00_0000),
        };
        let mut cell = Cell::new("x");
        cell.set_fg(Color::Rgb(10, 20, 30));
        cell.modifier = Modifier::REVERSED;
        let (fg, bg) = cell_colors(&cell, &m);
        assert_eq!(hex(fg), (0, 0, 0));
        assert_eq!(hex(bg), (10, 20, 30));
        cell.modifier = Modifier::DIM;
        assert!(cell_colors(&cell, &m).0.a < 1.0);
    }
}
