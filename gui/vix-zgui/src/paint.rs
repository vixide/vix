//! The grid as a zgui custom element: it measures to the box CSS gives it,
//! fits the `App`'s cell grid into that box, renders the `App` with
//! `ui::draw`, and paints the cells as quads (backgrounds, underlines, the
//! cursor) and shaped glyph runs (text), on zgui's own quad and glyph
//! pipelines.
//!
//! Text is shaped through the window's own font system (`Fonts::shaper`), so
//! each run's face handle is one the renderer draws, fallback faces for
//! icons and CJK included. Spans come from `crate::render`, which starts
//! every span at its exact column and gives each non-ASCII cell a span of
//! its own, so a fallback glyph with a different advance cannot shift its
//! row. Shaped spans are cached, since most of a frame repeats the last one.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ratatui::style::Modifier;
use zgui::app::{LineRequest, Shaper};
use zgui::canvas::zgui_color::Color;
use zgui::custom::{CustomElement, CustomLayoutCx, CustomMeasured, PaintSlot, ScenePainter};
use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::text::ShapedRunOwned;
use zgui::view::Ident;

use crate::font_choice::{self, FONT_SIZE, LINE_HEIGHT};
use crate::model::{Metrics, Model};
use crate::render::{Rgba8, cell_colors, row_backgrounds, row_text};

/// Shaped spans kept before the cache starts over.
const CACHE_LIMIT: usize = 8192;

/// The key a shaped span is cached under: its text, bold, italic.
type SpanKey = (String, bool, bool);

/// The grid element. It shares the [`Model`] with the event handlers.
pub struct GridElement {
    model: Rc<RefCell<Model>>,
    shaper: Shaper,
    families: Vec<Ident>,
    /// Metrics for the scale they were measured at.
    metrics: Option<Metrics>,
    cache: HashMap<SpanKey, Vec<ShapedRunOwned>>,
}

impl GridElement {
    /// A grid painting `model`, shaping with `shaper`.
    #[must_use]
    pub fn new(model: Rc<RefCell<Model>>, shaper: Shaper) -> Self {
        let mut families = vec![Ident::new(&font_choice::primary_family())];
        families.extend(font_choice::NERD_FAMILIES.iter().map(|f| Ident::new(f)));
        families.push(Ident::new("monospace"));
        Self {
            model,
            shaper,
            families,
            metrics: None,
            cache: HashMap::new(),
        }
    }

    /// The line request for one span's style at `scale`.
    fn request(&self, bold: bool, italic: bool, scale: f32) -> LineRequest<'_> {
        LineRequest {
            families: &self.families,
            weight: if bold { 700 } else { 400 },
            italic,
            size_device_px: FONT_SIZE * scale,
            letter_spacing: 0.0,
            // Ligatures draw two characters as one glyph in one cell.
            ligatures: false,
        }
    }

    /// The cell geometry at `scale`, measured from the primary face.
    fn metrics(&mut self, scale: f32) -> Metrics {
        if let Some(m) = self.metrics
            && (m.scale - scale).abs() < f32::EPSILON
        {
            return m;
        }
        let cell_h = (FONT_SIZE * LINE_HEIGHT * scale).round().max(1.0);
        let request = self.request(false, false, scale);
        let m = self.shaper.line_metrics(&request).map_or_else(
            || Metrics {
                scale,
                cell_w: (FONT_SIZE * 0.6 * scale).max(1.0),
                cell_h,
                baseline: (cell_h * 0.78).round(),
            },
            |line| {
                let ascent = line.metrics.ascent.0;
                let descent = line.metrics.descent.0;
                Metrics {
                    scale,
                    cell_w: line.cell_advance.0.max(1.0),
                    cell_h,
                    baseline: ((cell_h - (ascent + descent)) / 2.0 + ascent).round(),
                }
            },
        );
        self.metrics = Some(m);
        self.cache.clear();
        m
    }

    /// The shaped runs for `text` in the given style, from the cache.
    fn shaped(&mut self, text: &str, bold: bool, italic: bool, scale: f32) -> &[ShapedRunOwned] {
        if self.cache.len() > CACHE_LIMIT {
            self.cache.clear();
        }
        let key = (text.to_string(), bold, italic);
        if !self.cache.contains_key(&key) {
            let request = LineRequest {
                families: &self.families,
                weight: if bold { 700 } else { 400 },
                italic,
                size_device_px: FONT_SIZE * scale,
                letter_spacing: 0.0,
                ligatures: false,
            };
            let runs = self.shaper.shape_line(text, &request);
            self.cache.insert(key.clone(), runs);
        }
        self.cache.get(&key).map_or(&[], Vec::as_slice)
    }
}

/// A device-pixel rectangle.
fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(w), DevicePx(h)),
    )
}

/// A planned color as a zgui one.
fn color(c: Rgba8) -> Color {
    Color::srgb(
        f32::from(c.r) / 255.0,
        f32::from(c.g) / 255.0,
        f32::from(c.b) / 255.0,
        f32::from(c.a) / 255.0,
    )
}

impl CustomElement for GridElement {
    /// Take the whole box CSS gives the element (the window).
    fn layout(&mut self, cx: &mut CustomLayoutCx<'_>) -> CustomMeasured {
        CustomMeasured {
            width: cx.known_width.unwrap_or(1200.0 * cx.scale),
            height: cx.known_height.unwrap_or(800.0 * cx.scale),
            ..CustomMeasured::default()
        }
    }

    /// Fit the grid to the box, render the `App`, and paint the cells.
    fn paint(&mut self, painter: &mut ScenePainter<'_>) {
        let size = painter.size();
        let metrics = self.metrics(painter.scale());
        let model = Rc::clone(&self.model);
        let mut model = model.borrow_mut();
        model.fit(size.width.0, size.height.0, metrics);
        model.render();

        let Metrics {
            cell_w,
            cell_h,
            baseline,
            scale,
        } = metrics;
        let p = model.palette();
        // Whole-pixel cell edges, so neighboring fills meet without seams.
        let x_at = |col: u16| (f32::from(col) * cell_w).round();
        let y_at = |row: u16| f32::from(row) * cell_h;
        let line = scale.round().max(1.0);

        painter.fill(
            rect(0.0, 0.0, size.width.0, size.height.0),
            0.0,
            color(p.bg),
        );
        let buffer = model.buffer();
        for y in 0..buffer.area.height {
            let top = y_at(y);
            for run in row_backgrounds(buffer, y, p) {
                let x0 = x_at(run.col);
                let x1 = x_at(run.col + run.len);
                painter.fill(rect(x0, top, x1 - x0, cell_h), 0.0, color(run.color));
            }
            for span in row_text(buffer, y, p) {
                let m = span.style.modifier;
                let fg = color(span.style.fg);
                let bold = m.contains(Modifier::BOLD);
                let italic = m.contains(Modifier::ITALIC);
                let x0 = x_at(span.col);
                for run in self.shaped(&span.text, bold, italic, scale) {
                    let Some(first) = run.glyphs.first() else {
                        continue;
                    };
                    // Every run's baseline on the cell's, whatever face it
                    // fell back to.
                    let origin = Point::new(DevicePx(x0), DevicePx(top + baseline - first.y));
                    painter.glyphs(&run.as_run(PaintSlot(0)), origin, fg);
                }
                let cells = u16::try_from(span.text.chars().count()).unwrap_or(u16::MAX);
                let width = x_at(span.col.saturating_add(cells)) - x0;
                if m.contains(Modifier::UNDERLINED) {
                    painter.fill(rect(x0, top + baseline + line, width, line), 0.0, fg);
                }
                if m.contains(Modifier::CROSSED_OUT) {
                    let mid = top + (baseline * 0.65).round();
                    painter.fill(rect(x0, mid, width, line), 0.0, fg);
                }
            }
        }
        if let Some(pos) = model.cursor()
            && pos.x < buffer.area.width
            && pos.y < buffer.area.height
        {
            let (fg, _) = cell_colors(&buffer[(pos.x, pos.y)], p);
            painter.fill(
                rect(x_at(pos.x), y_at(pos.y), 2.0 * line, cell_h),
                0.0,
                color(fg),
            );
        }
    }
}
