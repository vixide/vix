//! The window-independent half of the front end: the Vix `App`, the cell grid
//! it renders into, and how window input becomes `App` calls.
//!
//! Nothing here touches zgui, so the tests drive it directly; `crate::view`
//! forwards zgui's events to it and paints [`Model::buffer`] with
//! `crate::paint`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use vix::app::{App, AppFlags};
use vix_input::{KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::grid::GridBackend;
use crate::render::Palette;

/// CSS pixels a continuous (trackpad) scroll moves per line.
const PIXELS_PER_LINE: f32 = 20.0;

/// The grid's geometry, in device pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Device pixels per CSS pixel.
    pub scale: f32,
    /// One cell's width.
    pub cell_w: f32,
    /// One cell's height.
    pub cell_h: f32,
    /// Distance from a cell's top down to the text baseline.
    pub baseline: f32,
}

impl Metrics {
    /// A placeholder geometry (a 14px monospace face at scale 1) until the
    /// window measures its font.
    #[must_use]
    pub const fn placeholder() -> Self {
        Self {
            scale: 1.0,
            cell_w: 8.0,
            cell_h: 18.0,
            baseline: 14.0,
        }
    }
}

/// The Vix `App` and everything needed to show and drive it in a window.
pub struct Model {
    app: App,
    term: Terminal<GridBackend>,
    palette: Palette,
    metrics: Metrics,
    /// Fractional wheel lines not yet delivered.
    scroll_accum: f32,
    /// The button held down, so motion reports as a drag.
    pressed: Option<MouseButton>,
    /// The cell the last motion event was reported for.
    last_move_cell: Option<(u16, u16)>,
    /// When the window was last redrawn because of the timer.
    last_redraw: Instant,
    /// `App::refresh_git` has not run yet (it waits for the first frame).
    git_pending: bool,
    /// `App::on_exit` has run.
    exited: bool,
}

impl Model {
    /// Wrap a started `App`. `dark` picks the default palette for
    /// `Color::Reset` cells.
    #[must_use]
    pub fn new(app: App, dark: bool) -> Self {
        Self {
            app,
            term: Terminal::new(GridBackend::new(80, 24)).unwrap_or_else(|never| match never {}),
            palette: Palette::for_appearance(dark),
            metrics: Metrics::placeholder(),
            scroll_accum: 0.0,
            pressed: None,
            last_move_cell: None,
            last_redraw: Instant::now(),
            git_pending: true,
            exited: false,
        }
    }

    /// The palette for cells that use the terminal default colors.
    #[must_use]
    pub const fn palette(&self) -> Palette {
        self.palette
    }

    /// The grid size, in cells.
    #[must_use]
    pub fn grid_size(&self) -> (u16, u16) {
        let area = self.term.backend().buffer().area;
        (area.width, area.height)
    }

    /// The cells as last rendered.
    #[must_use]
    pub fn buffer(&self) -> &Buffer {
        self.term.backend().buffer()
    }

    /// The text cursor's cell, when the last frame showed one.
    #[must_use]
    pub fn cursor(&self) -> Option<Position> {
        self.term.backend().cursor()
    }

    /// Resize the grid to fill `width` x `height` device pixels with cells
    /// of `metrics`.
    pub fn fit(&mut self, width: f32, height: f32, metrics: Metrics) {
        self.metrics = metrics;
        let cols = cell_count(width / metrics.cell_w);
        let rows = cell_count(height / metrics.cell_h);
        self.term.backend_mut().resize(cols, rows);
    }

    /// Render the `App` into the grid with the terminal build's own
    /// `ui::draw`.
    pub fn render(&mut self) {
        let app = &mut self.app;
        let _ = self.term.draw(|frame| vix::ui::draw(app, frame));
    }

    /// The grid's text, one string per row (trailing blanks trimmed).
    #[cfg(test)]
    #[must_use]
    pub fn grid_lines(&self) -> Vec<String> {
        let buffer = self.buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// The cell under a position in CSS pixels from the grid's top-left,
    /// clamped into the grid.
    #[must_use]
    pub fn cell_at(&self, x: f32, y: f32) -> (u16, u16) {
        let (cols, rows) = self.grid_size();
        let m = self.metrics;
        (
            cell_index(x * m.scale / m.cell_w, cols),
            cell_index(y * m.scale / m.cell_h, rows),
        )
    }

    /// One key press, already translated (`crate::keys`).
    pub fn key(&mut self, key: KeyEvent) {
        self.app.on_key(key);
    }

    /// Text an input method committed: each character as a typed key.
    pub fn commit(&mut self, text: &str) {
        for c in text.chars().filter(|c| !c.is_control()) {
            self.app.on_key(KeyEvent::new(
                vix_input::KeyCode::Char(c),
                KeyModifiers::NONE,
            ));
        }
    }

    /// A mouse button went down at (`x`, `y`) CSS pixels in the grid.
    pub fn mouse_down(&mut self, x: f32, y: f32, button: MouseButton, mods: KeyModifiers) {
        self.pressed = Some(button);
        let cell = self.cell_at(x, y);
        self.mouse(MouseEventKind::Down(button), cell, mods);
    }

    /// A mouse button went up at (`x`, `y`) CSS pixels in the grid.
    pub fn mouse_up(&mut self, x: f32, y: f32, button: MouseButton, mods: KeyModifiers) {
        self.pressed = None;
        let cell = self.cell_at(x, y);
        self.mouse(MouseEventKind::Up(button), cell, mods);
    }

    /// The pointer moved to (`x`, `y`) CSS pixels. Like a terminal, this
    /// reports per cell, not per pixel. Returns whether the cell changed.
    pub fn mouse_move(&mut self, x: f32, y: f32, mods: KeyModifiers) -> bool {
        let cell = self.cell_at(x, y);
        if self.last_move_cell == Some(cell) {
            return false;
        }
        self.last_move_cell = Some(cell);
        let kind = self
            .pressed
            .map_or(MouseEventKind::Moved, MouseEventKind::Drag);
        self.mouse(kind, cell, mods);
        true
    }

    /// The wheel turned at (`x`, `y`) CSS pixels: `lines` whole or partial
    /// lines, positive moving the content up (the view down, as zgui reports
    /// it). Returns whether any whole line was delivered.
    pub fn scroll_lines(&mut self, x: f32, y: f32, lines: f32, mods: KeyModifiers) -> bool {
        self.scroll_accum += lines;
        let cell = self.cell_at(x, y);
        let mut moved = false;
        while self.scroll_accum.abs() >= 1.0 {
            let kind = if self.scroll_accum > 0.0 {
                self.scroll_accum -= 1.0;
                MouseEventKind::ScrollDown
            } else {
                self.scroll_accum += 1.0;
                MouseEventKind::ScrollUp
            };
            self.mouse(kind, cell, mods);
            moved = true;
        }
        moved
    }

    /// A continuous scroll of `pixels` CSS pixels (see
    /// [`Model::scroll_lines`]).
    pub fn scroll_pixels(&mut self, x: f32, y: f32, pixels: f32, mods: KeyModifiers) -> bool {
        self.scroll_lines(x, y, pixels / PIXELS_PER_LINE, mods)
    }

    /// Forward a mouse event at grid cell `cell` to the `App`, the same entry
    /// point the terminal's mouse events use.
    fn mouse(&mut self, kind: MouseEventKind, cell: (u16, u16), mods: KeyModifiers) {
        self.app.on_mouse(MouseEvent {
            kind,
            column: cell.0,
            row: cell.1,
            modifiers: mods,
        });
    }

    /// Run one action id, as a menu leaf would.
    pub fn run_action(&mut self, id: &str) {
        self.app.run_action(id);
    }

    /// Open `paths` in tabs, as if given on the command line.
    #[cfg(any(target_os = "macos", test))]
    pub fn open_files(&mut self, paths: &[PathBuf]) {
        for path in paths {
            self.app.open_initial(path);
        }
    }

    /// Background work, as the terminal loop's `App::tick`. Returns whether
    /// the window should redraw now: the first call (which also refreshes
    /// git state, deferred until a frame exists), then once per
    /// `App::poll_interval`.
    pub fn tick(&mut self) -> bool {
        self.app.tick();
        if self.git_pending {
            self.git_pending = false;
            self.app.refresh_git();
            self.last_redraw = Instant::now();
            return true;
        }
        if self.last_redraw.elapsed() >= self.app.poll_interval() {
            self.last_redraw = Instant::now();
            return true;
        }
        false
    }

    /// How often the window should call [`Model::tick`].
    #[must_use]
    pub const fn tick_every() -> Duration {
        Duration::from_millis(50)
    }

    /// Whether the App asked to quit. A suspend request (`Ctrl+Z`) has no
    /// meaning in a window and is dropped.
    pub fn should_quit(&mut self) -> bool {
        self.app.flags.remove(AppFlags::SUSPEND_REQUESTED);
        self.app.flags.contains(AppFlags::SHOULD_QUIT)
    }

    /// Shutdown work (session save), once.
    pub fn on_exit(&mut self) {
        if !self.exited {
            self.exited = true;
            self.app.on_exit();
        }
    }
}

/// A fractional cell count as a whole number of cells, at least one.
fn cell_count(v: f32) -> u16 {
    cell_index(v, u16::MAX).max(1)
}

/// A fractional cell coordinate, clamped into `0..len`.
fn cell_index(v: f32, len: u16) -> u16 {
    let max = f32::from(len.saturating_sub(1));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped into 0..=u16 range first"
    )]
    let i = v.floor().clamp(0.0, max) as u16;
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_index_clamps_into_the_grid() {
        assert_eq!(cell_index(-3.0, 10), 0);
        assert_eq!(cell_index(0.9, 10), 0);
        assert_eq!(cell_index(4.2, 10), 4);
        assert_eq!(cell_index(99.0, 10), 9);
        assert_eq!(cell_index(5.0, 0), 0);
    }

    #[test]
    fn cell_count_is_at_least_one() {
        assert_eq!(cell_count(0.0), 1);
        assert_eq!(cell_count(-5.0), 1);
        assert_eq!(cell_count(80.7), 80);
    }
}
