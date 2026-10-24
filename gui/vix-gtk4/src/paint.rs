//! Paint the cell grid onto a Cairo context with Pango text: the background,
//! each row's background runs and text spans from `crate::render`, and the
//! text cursor. The same function paints the window's `DrawingArea` and the
//! scripted smoke test's PNG.
//!
//! Text is laid out with Pango, so glyphs the primary font lacks (Nerd Font
//! icons, CJK, emoji) fall back through fontconfig / CoreText / DirectWrite
//! per glyph; the Nerd Font families chosen in `crate::font_choice` are
//! listed after the primary family so icons prefer them.

use gtk4::cairo;
use gtk4::pango::{self, prelude::*};
use pangocairo::functions::{create_layout, show_layout};
use ratatui::style::Modifier;

use crate::font_choice::{self, FONT_SIZE, cell_height};
use crate::grid::GridBackend;
use crate::render::{Palette, Rgba8, cell_colors, row_backgrounds, row_text};

/// One cell's size, plus the height of a line of text in it, in logical
/// pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellSize {
    /// Width of one cell (the font's advance).
    pub width: f64,
    /// Height of one cell (a row).
    pub height: f64,
    /// Height of the font's line, centered in the row.
    pub text: f64,
}

impl Default for CellSize {
    fn default() -> Self {
        let h = f64::from(cell_height());
        Self {
            width: f64::from(FONT_SIZE) * 0.6,
            height: h,
            text: h,
        }
    }
}

/// The Pango family list the window draws with: the primary monospace
/// family, then any installed Nerd Font fallbacks.
#[must_use]
pub fn font_family() -> String {
    let installed: Vec<String> = pangocairo::FontMap::default()
        .list_families()
        .iter()
        .map(|f| f.name().to_string())
        .collect();
    let mut families = vec![font_choice::primary_family()];
    families.extend(font_choice::nerd_fallbacks(&installed));
    families.join(",")
}

/// The regular font for `family` at [`FONT_SIZE`].
fn font(family: &str) -> pango::FontDescription {
    let mut fd = pango::FontDescription::new();
    fd.set_family(family);
    fd.set_absolute_size(f64::from(FONT_SIZE) * f64::from(pango::SCALE));
    fd
}

/// A Pango length in logical pixels.
fn px(units: i32) -> f64 {
    f64::from(units) / f64::from(pango::SCALE)
}

/// Measure the cell for `family` on `cr`: the advance of `M`, and the line
/// height (the row is at least [`cell_height`], so rows breathe the same as
/// the other front ends').
#[must_use]
pub fn measure(cr: &cairo::Context, family: &str) -> CellSize {
    let layout = create_layout(cr);
    layout.set_font_description(Some(&font(family)));
    layout.set_text("M");
    let (_, logical) = layout.extents();
    let width = px(logical.width());
    let text = px(logical.height());
    if width <= 0.0 {
        return CellSize::default();
    }
    CellSize {
        width,
        height: f64::from(cell_height()).max(text.ceil()),
        text,
    }
}

/// Set `cr`'s source to a planned color.
fn source(cr: &cairo::Context, c: Rgba8) {
    cr.set_source_rgba(
        f64::from(c.r) / 255.0,
        f64::from(c.g) / 255.0,
        f64::from(c.b) / 255.0,
        f64::from(c.a) / 255.0,
    );
}

/// Fill a rectangle with a planned color. A failed fill (an errored Cairo
/// context) leaves the frame as it is; there is nothing better to draw.
fn fill(cr: &cairo::Context, color: Rgba8, (x, y, width, height): (f64, f64, f64, f64)) {
    source(cr, color);
    cr.rectangle(x, y, width, height);
    let _ = cr.fill();
}

/// Paint the grid at the top-left of a `width` × `height` area. Returns the
/// text cursor's rectangle (x, y, width, height), when the frame shows one.
pub fn paint(
    cr: &cairo::Context,
    grid: &GridBackend,
    p: Palette,
    family: &str,
    cell: CellSize,
    (width, height): (f64, f64),
) -> Option<(f64, f64, f64, f64)> {
    fill(cr, p.bg, (0.0, 0.0, width, height));
    let buffer = grid.buffer();
    let at = |col: u16, row: u16| (cell.width * f64::from(col), cell.height * f64::from(row));
    let regular = font(family);
    let layout = create_layout(cr);
    let text_dy = (cell.height - cell.text) / 2.0;
    for y in 0..buffer.area.height {
        for run in row_backgrounds(buffer, y, p) {
            let (x0, y0) = at(run.col, y);
            fill(
                cr,
                run.color,
                (x0, y0, cell.width * f64::from(run.len), cell.height),
            );
        }
        for span in row_text(buffer, y, p) {
            let m = span.style.modifier;
            let mut fd = regular.clone();
            if m.contains(Modifier::BOLD) {
                fd.set_weight(pango::Weight::Bold);
            }
            if m.contains(Modifier::ITALIC) {
                fd.set_style(pango::Style::Italic);
            }
            layout.set_font_description(Some(&fd));
            let attrs = pango::AttrList::new();
            if m.contains(Modifier::UNDERLINED) {
                attrs.insert(pango::AttrInt::new_underline(pango::Underline::Single));
            }
            if m.contains(Modifier::CROSSED_OUT) {
                attrs.insert(pango::AttrInt::new_strikethrough(true));
            }
            layout.set_attributes(Some(&attrs));
            layout.set_text(&span.text);
            let (x0, y0) = at(span.col, y);
            source(cr, span.style.fg);
            cr.move_to(x0, y0 + text_dy);
            show_layout(cr, &layout);
        }
    }
    let pos = grid.cursor()?;
    if pos.x >= buffer.area.width || pos.y >= buffer.area.height {
        return None;
    }
    let (fg, _) = cell_colors(&buffer[(pos.x, pos.y)], p);
    let (x0, y0) = at(pos.x, pos.y);
    fill(cr, fg, (x0, y0, 2.0, cell.height));
    Some((x0, y0, cell.width, cell.height))
}
