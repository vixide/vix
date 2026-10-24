//! A ratatui [`Backend`] that draws into an in-memory cell grid instead of a
//! terminal. The App's unchanged `ui::draw` renders through a
//! `ratatui::Terminal<GridBackend>` each frame; the window then paints the
//! grid (see `crate::paint`). Ratatui's own diffing still applies: only changed
//! cells reach [`Backend::draw`], and the grid accumulates them.

use std::convert::Infallible;

use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect, Size};

/// The grid a frame was drawn into, plus where (and whether) the App asked
/// for the text cursor.
pub struct GridBackend {
    buffer: Buffer,
    cursor: Position,
    cursor_visible: bool,
}

impl GridBackend {
    /// An empty grid of `cols` × `rows` cells.
    #[must_use]
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            buffer: Buffer::empty(Rect::new(0, 0, cols, rows)),
            cursor: Position::ORIGIN,
            cursor_visible: false,
        }
    }

    /// Change the grid's size. The next `Terminal::draw` notices (via
    /// [`Backend::size`]), clears, and redraws everything.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        if self.buffer.area.width != cols || self.buffer.area.height != rows {
            self.buffer.resize(Rect::new(0, 0, cols, rows));
        }
    }

    /// The current cell contents.
    #[must_use]
    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// The text cursor's cell, when the last frame showed one.
    #[must_use]
    pub fn cursor(&self) -> Option<Position> {
        self.cursor_visible.then_some(self.cursor)
    }
}

impl Backend for GridBackend {
    type Error = Infallible;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let area = self.buffer.area;
        for (x, y, cell) in content {
            if x < area.width && y < area.height {
                self.buffer[(x, y)] = cell.clone();
            }
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.cursor_visible = false;
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.cursor_visible = true;
        Ok(())
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        Ok(self.cursor)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.cursor = position.into();
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.buffer.reset();
        Ok(())
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        let area = self.buffer.area;
        let (cx, cy) = (self.cursor.x, self.cursor.y);
        let cells: Box<dyn Iterator<Item = (u16, u16)>> = match clear_type {
            ClearType::All => {
                self.buffer.reset();
                return Ok(());
            }
            ClearType::AfterCursor => Box::new(
                (0..area.height)
                    .flat_map(move |y| (0..area.width).map(move |x| (x, y)))
                    .filter(move |&(x, y)| (y, x) > (cy, cx)),
            ),
            ClearType::BeforeCursor => Box::new(
                (0..area.height)
                    .flat_map(move |y| (0..area.width).map(move |x| (x, y)))
                    .filter(move |&(x, y)| (y, x) < (cy, cx)),
            ),
            ClearType::CurrentLine => Box::new((0..area.width).map(move |x| (x, cy))),
            ClearType::UntilNewLine => Box::new((cx..area.width).map(move |x| (x, cy))),
        };
        for (x, y) in cells {
            if y < area.height {
                self.buffer[(x, y)].reset();
            }
        }
        Ok(())
    }

    fn size(&self) -> Result<Size, Self::Error> {
        Ok(self.buffer.area.as_size())
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        Ok(WindowSize {
            columns_rows: self.buffer.area.as_size(),
            pixels: Size::default(),
        })
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::widgets::Paragraph;

    #[test]
    fn terminal_draws_into_the_grid_and_tracks_the_cursor() {
        let mut term = Terminal::new(GridBackend::new(10, 2)).unwrap();
        term.draw(|f| {
            f.render_widget(Paragraph::new("hello"), f.area());
            f.set_cursor_position((3, 1));
        })
        .unwrap();
        let grid = term.backend();
        assert_eq!(grid.buffer()[(0, 0)].symbol(), "h");
        assert_eq!(grid.buffer()[(4, 0)].symbol(), "o");
        assert_eq!(grid.cursor(), Some(Position::new(3, 1)));

        term.draw(|f| f.render_widget(Paragraph::new("x"), f.area()))
            .unwrap();
        assert_eq!(term.backend().cursor(), None);
        assert_eq!(term.backend().buffer()[(1, 0)].symbol(), " ");
    }

    #[test]
    fn resize_is_picked_up_by_the_next_draw() {
        let mut term = Terminal::new(GridBackend::new(4, 1)).unwrap();
        term.backend_mut().resize(8, 3);
        term.draw(|f| {
            assert_eq!(f.area(), Rect::new(0, 0, 8, 3));
            f.render_widget(Paragraph::new("abcdefgh"), f.area());
        })
        .unwrap();
        assert_eq!(term.backend().buffer()[(7, 0)].symbol(), "h");
    }
}
