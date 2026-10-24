//! Paint the cell grid on an iced `canvas`: the default background, each
//! row's background runs and text spans (planned by `crate::render`) at
//! their cell positions, underline and strikethrough rules, and the 2px
//! text-cursor bar.

use crate::font_choice::FONT_SIZE;
use crate::fonts::Fonts;
use crate::grid::GridBackend;
use crate::render::{Palette, Rgba8, cell_colors, row_backgrounds, row_text};
use iced::advanced::text::{Alignment, LineHeight, Shaping};
use iced::alignment::Vertical;
use iced::widget::canvas::{self, Frame, Geometry, Text};
use iced::{Color, Pixels, Point, Rectangle, Renderer, Size, Theme, mouse};
use ratatui::style::Modifier;

/// One frame's grid, borrowed from the window state for drawing.
pub struct Grid<'a> {
    /// The grid the App's last frame was drawn into.
    pub cells: &'a GridBackend,
    /// Colors for `Color::Reset`.
    pub palette: Palette,
    /// Fonts and cell size.
    pub fonts: Fonts,
}

impl Grid<'_> {
    /// The top-left corner of a cell.
    fn at(&self, col: u16, row: u16) -> Point {
        let cell = self.fonts.cell;
        Point::new(cell.width * f32::from(col), cell.height * f32::from(row))
    }
}

impl<Message> canvas::Program<Message> for Grid<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let p = self.palette;
        let cell = self.fonts.cell;
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), color(p.bg));
        let buffer = self.cells.buffer();
        for y in 0..buffer.area.height {
            for run in row_backgrounds(buffer, y, p) {
                let size = Size::new(cell.width * f32::from(run.len), cell.height);
                frame.fill_rectangle(self.at(run.col, y), size, color(run.color));
            }
            for span in row_text(buffer, y, p) {
                let m = span.style.modifier;
                let fg = color(span.style.fg);
                let at = self.at(span.col, y);
                let cells = span.text.chars().count();
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a span is at most one row of cells (a u16)"
                )]
                let width = cell.width * cells as f32;
                let font = self.fonts.for_span(
                    &span.text,
                    m.contains(Modifier::BOLD),
                    m.contains(Modifier::ITALIC),
                );
                frame.fill_text(Text {
                    content: span.text,
                    position: at,
                    color: fg,
                    size: Pixels(FONT_SIZE),
                    line_height: LineHeight::Absolute(Pixels(cell.height)),
                    font,
                    align_x: Alignment::Left,
                    align_y: Vertical::Top,
                    shaping: Shaping::Advanced,
                    max_width: f32::INFINITY,
                });
                if m.contains(Modifier::UNDERLINED) {
                    let rule = Point::new(at.x, at.y + cell.height - 2.0);
                    frame.fill_rectangle(rule, Size::new(width, 1.0), fg);
                }
                if m.contains(Modifier::CROSSED_OUT) {
                    let rule = Point::new(at.x, at.y + (cell.height / 2.0).round());
                    frame.fill_rectangle(rule, Size::new(width, 1.0), fg);
                }
            }
        }
        if let Some(pos) = self.cells.cursor()
            && pos.x < buffer.area.width
            && pos.y < buffer.area.height
        {
            let (fg, _) = cell_colors(&buffer[(pos.x, pos.y)], p);
            let bar = Size::new(2.0, cell.height);
            frame.fill_rectangle(self.at(pos.x, pos.y), bar, color(fg));
        }
        vec![frame.into_geometry()]
    }
}

/// A planned color as an iced one.
#[must_use]
pub fn color(c: Rgba8) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, f32::from(c.a) / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_keep_their_channels_and_alpha() {
        let c = color(Rgba8 {
            r: 255,
            g: 0,
            b: 51,
            a: 153,
        });
        assert!((c.r - 1.0).abs() < 1e-6);
        assert!(c.g.abs() < 1e-6);
        assert!((c.b - 0.2).abs() < 1e-6);
        assert!((c.a - 0.6).abs() < 1e-6);
    }
}
