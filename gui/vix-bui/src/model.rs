//! The window-independent half of the front end: the Vix `App`, the cell grid
//! it renders into, and how BUI input becomes `App` calls.
//!
//! Nothing here touches BUI, so it is driven directly by the tests and by the
//! scripted screenshot run; `crate::view` only forwards BUI's events to it and
//! replays [`Model::scene`] onto BUI's painter.

use std::time::{Duration, Instant};

use ratatui::Terminal;
use vix::app::{App, AppFlags};
use vix_input::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::fonts::Fonts;
use crate::grid::GridBackend;
use crate::keys;
use crate::render::Palette;
use crate::scene::{self, Metrics, Op};

/// Wheel lines BUI reports per notch are already whole lines; trackpads may
/// report fractions, which accumulate here until they make a line.
const LINES_PER_STEP: f32 = 1.0;

/// The Vix `App` and everything needed to show and drive it in a window.
pub struct Model {
    app: App,
    term: Terminal<GridBackend>,
    palette: Palette,
    fonts: Fonts,
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
}

impl Model {
    /// Wrap a started `App`. `dark` picks the default palette for
    /// `Color::Reset` cells; `fonts` sets the cell size.
    #[must_use]
    pub fn new(app: App, dark: bool, fonts: Fonts) -> Self {
        let metrics = Self::metrics_for(&fonts, 1.0);
        Self {
            app,
            term: Terminal::new(GridBackend::new(80, 24)).unwrap_or_else(|never| match never {}),
            palette: Palette::for_appearance(dark),
            fonts,
            metrics,
            scroll_accum: 0.0,
            pressed: None,
            last_move_cell: None,
            last_redraw: Instant::now(),
            git_pending: true,
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

    /// The grid geometry for `fonts` at `scale` device pixels per logical one.
    fn metrics_for(fonts: &Fonts, scale: f32) -> Metrics {
        fonts
            .metrics()
            .map_or_else(Metrics::embedded, |font| Metrics::monospace(font, scale))
    }

    /// The grid geometry in use.
    #[must_use]
    pub const fn metrics(&self) -> Metrics {
        self.metrics
    }

    /// Give BUI this window's fonts; see [`Fonts::install`].
    pub fn install_fonts(&self) {
        self.fonts.install();
    }

    /// Resize the grid to fill a `width` x `height` logical-pixel area at
    /// `scale` device pixels per logical pixel.
    pub fn fit(&mut self, width: f32, height: f32, scale: f32) {
        self.metrics = Self::metrics_for(&self.fonts, scale);
        let cols = cell_count(width / self.metrics.cell_w);
        let rows = cell_count(height / self.metrics.cell_h);
        self.term.backend_mut().resize(cols, rows);
    }

    /// Render the `App` into the grid with the terminal build's own
    /// `ui::draw`.
    pub fn render(&mut self) {
        let app = &mut self.app;
        let _ = self.term.draw(|frame| vix::ui::draw(app, frame));
    }

    /// The drawing operations for the grid as last rendered. Runs inside
    /// `Application::run`: a character's first use may register its fallback
    /// font with BUI.
    #[must_use]
    pub fn scene(&mut self) -> Vec<Op> {
        let fonts = &mut self.fonts;
        scene::build(
            self.term.backend().buffer(),
            self.term.backend().cursor(),
            self.palette,
            self.metrics,
            &mut |ch| fonts.route(ch),
        )
    }

    /// The grid's text, one string per row (trailing blanks trimmed).
    #[cfg(test)]
    #[must_use]
    pub fn grid_lines(&self) -> Vec<String> {
        let buffer = self.term.backend().buffer();
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

    /// The cell under a pixel position (relative to the grid's top-left),
    /// clamped into the grid.
    #[must_use]
    pub fn cell_at(&self, x: f32, y: f32) -> (u16, u16) {
        let (cols, rows) = self.grid_size();
        (
            cell_index(x / self.metrics.cell_w, cols),
            cell_index(y / self.metrics.cell_h, rows),
        )
    }

    /// One BUI key-down: the keysym, the text the layout typed, and the
    /// Shift / Control (Command on macOS) / Alt flags. Returns whether the
    /// window should redraw.
    pub fn key_down(
        &mut self,
        keysym: &str,
        text: Option<&str>,
        shift: bool,
        control: bool,
        alt: bool,
    ) -> bool {
        match keys::key_event(keysym, text, shift, control, alt) {
            Some(key) => {
                self.app.on_key(key);
                true
            }
            None => false,
        }
    }

    /// A mouse button went down at pixel (`x`, `y`) in the grid.
    pub fn mouse_down(&mut self, x: f32, y: f32, button: MouseButton, mods: KeyModifiers) {
        self.pressed = Some(button);
        let cell = self.cell_at(x, y);
        self.mouse(MouseEventKind::Down(button), cell, mods);
    }

    /// A mouse button went up at pixel (`x`, `y`) in the grid.
    pub fn mouse_up(&mut self, x: f32, y: f32, button: MouseButton, mods: KeyModifiers) {
        self.pressed = None;
        let cell = self.cell_at(x, y);
        self.mouse(MouseEventKind::Up(button), cell, mods);
    }

    /// The pointer moved to pixel (`x`, `y`). Like a terminal, this reports
    /// per cell, not per pixel. Returns whether the cell changed.
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

    /// The wheel turned by `lines` (positive scrolls the view up) at pixel
    /// (`x`, `y`). Returns whether any whole line was delivered.
    pub fn scroll(&mut self, x: f32, y: f32, lines: f32, mods: KeyModifiers) -> bool {
        self.scroll_accum += lines;
        let cell = self.cell_at(x, y);
        let mut moved = false;
        while self.scroll_accum.abs() >= LINES_PER_STEP {
            let kind = if self.scroll_accum > 0.0 {
                self.scroll_accum -= LINES_PER_STEP;
                MouseEventKind::ScrollUp
            } else {
                self.scroll_accum += LINES_PER_STEP;
                MouseEventKind::ScrollDown
            };
            self.mouse(kind, cell, mods);
            moved = true;
        }
        moved
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

    /// Shutdown work (session save), once the window is gone.
    pub fn on_exit(&mut self) {
        self.app.on_exit();
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
