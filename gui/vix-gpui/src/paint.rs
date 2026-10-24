//! Paint the cell grid with GPUI, following the plan in
//! `crate::render`: background runs as quads, text spans as shaped
//! lines positioned on the cell grid, then the text cursor.

use crate::render::{Palette, Rgba8, SpanStyle, cell_colors, row_backgrounds, row_text};
use gpui_kit::{
    App, Bounds, FontFallbacks, FontStyle, FontWeight, Hsla, Pixels, Rgba, SharedString,
    StrikethroughStyle, TextAlign, TextRun, UnderlineStyle, Window, fill, point, px, size,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use ratatui::style::Modifier;

/// Fonts, cell metrics and default colors for painting.
#[derive(Clone)]
pub struct Metrics {
    /// Monospace font family.
    pub family: SharedString,
    /// Per-glyph fallback families (Nerd Font icon glyphs, chiefly).
    pub fallbacks: Option<FontFallbacks>,
    /// Font size.
    pub font_size: Pixels,
    /// Width of one cell.
    pub cell_width: Pixels,
    /// Height of one cell (the line height).
    pub cell_height: Pixels,
    /// Colors for `Color::Reset`.
    pub palette: Palette,
}

/// A planned color as a GPUI one.
pub fn rgba(c: Rgba8) -> Rgba {
    Rgba {
        r: f32::from(c.r) / 255.0,
        g: f32::from(c.g) / 255.0,
        b: f32::from(c.b) / 255.0,
        a: f32::from(c.a) / 255.0,
    }
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
    let origin = bounds.origin;
    window.paint_quad(fill(bounds, rgba(m.palette.bg)));
    for y in 0..buffer.area.height {
        let top = origin.y + m.cell_height * f32::from(y);
        for run in row_backgrounds(buffer, y, m.palette) {
            let at = point(origin.x + m.cell_width * f32::from(run.col), top);
            window.paint_quad(fill(
                Bounds::new(at, size(m.cell_width * f32::from(run.len), m.cell_height)),
                rgba(run.color),
            ));
        }
        for span in row_text(buffer, y, m.palette) {
            let run = text_run(span.style, span.text.len(), m);
            let shaped =
                window
                    .text_system()
                    .shape_line(span.text.into(), m.font_size, &[run], None);
            let at = point(origin.x + m.cell_width * f32::from(span.col), top);
            let _ = shaped.paint(at, m.cell_height, TextAlign::Left, None, window, cx);
        }
    }
    let area = buffer.area;
    if let Some(pos) = cursor
        && pos.x < area.width
        && pos.y < area.height
    {
        let (fg, _) = cell_colors(&buffer[(pos.x, pos.y)], m.palette);
        let at = point(
            origin.x + m.cell_width * f32::from(pos.x),
            origin.y + m.cell_height * f32::from(pos.y),
        );
        window.paint_quad(fill(Bounds::new(at, size(px(2.), m.cell_height)), rgba(fg)));
    }
}

/// The GPUI text run for `len` bytes in `style`.
fn text_run(style: SpanStyle, len: usize, m: &Metrics) -> TextRun {
    let fg: Hsla = rgba(style.fg).into();
    let mut font = gpui_kit::font(m.family.clone());
    font.fallbacks.clone_from(&m.fallbacks);
    if style.modifier.contains(Modifier::BOLD) {
        font.weight = FontWeight::BOLD;
    }
    if style.modifier.contains(Modifier::ITALIC) {
        font.style = FontStyle::Italic;
    }
    TextRun {
        len,
        font,
        color: fg,
        background_color: None,
        underline: style
            .modifier
            .contains(Modifier::UNDERLINED)
            .then_some(UnderlineStyle {
                thickness: px(1.),
                color: Some(fg),
                wavy: false,
            }),
        strikethrough: style.modifier.contains(Modifier::CROSSED_OUT).then_some(
            StrikethroughStyle {
                thickness: px(1.),
                color: Some(fg),
            },
        ),
    }
}
