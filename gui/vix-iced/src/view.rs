//! The iced application: owns the Vix `App`, renders it through `ui::draw`
//! into the in-memory cell grid after every message (event, tick, resize),
//! paints the grid with a `canvas` (`crate::paint`), and feeds keyboard,
//! IME, mouse and menu input back as `vix-input` events and action ids.
//!
//! iced is Elm-shaped: runtime events reach [`VixIced::update`] as messages
//! through a subscription, and [`VixIced::view`] only borrows the grid the
//! last update drew. The App itself starts on the first message, once the
//! OS appearance (`system::theme`) is known, since `theme = "auto"` needs it.

use std::time::Duration;

use crate::fonts::Fonts;
use crate::grid::GridBackend;
use crate::ime::ImeAnchor;
use crate::paint::Grid;
use crate::render::Palette;
use crate::startup::Loaded;
use iced::widget::{canvas, stack};
use iced::{
    Element, Event, Length, Point, Rectangle, Size, Subscription, Task, Theme, event, keyboard,
    mouse, system, theme, time, window,
};
use ratatui::Terminal;
use vix::app::{App, AppFlags};
use vix::cli::Cli;
use vix_input::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

/// Wheel pixels per scrolled line, for trackpads reporting pixel deltas.
const PIXELS_PER_LINE: f32 = 20.0;

/// The window's messages.
#[derive(Clone, Debug)]
pub enum Message {
    /// The OS appearance, asked for at boot; starts the App.
    Appearance(theme::Mode),
    /// A runtime event (keyboard, mouse, IME, window) for a window.
    Event(window::Id, Event),
    /// Background work is due (`App::poll_interval`).
    Tick,
    /// A native menu item was clicked; its action id.
    #[cfg_attr(
        not(target_os = "macos"),
        expect(dead_code, reason = "no native menu bar here")
    )]
    Menu(String),
    /// Step the scripted render smoke test.
    #[cfg(feature = "screenshot")]
    Script,
    /// The scripted run's window capture.
    #[cfg(feature = "screenshot")]
    Captured(window::Screenshot),
}

/// The Vix window.
pub struct VixIced {
    /// Command line and settings, until the App starts.
    pending: Option<(Cli, Loaded)>,
    /// The App, once started.
    app: Option<App>,
    term: Terminal<GridBackend>,
    palette: Palette,
    fonts: Fonts,
    title: String,
    /// The window, once open, and its size in logical pixels.
    window: Option<(window::Id, Size)>,
    /// `App::refresh_git` has run (needs both the App and the window).
    git_refreshed: bool,
    /// Modifiers held, for mouse events (which carry none).
    mods: keyboard::Modifiers,
    /// The button held down, so motion reports as a drag.
    pressed: Option<MouseButton>,
    /// The cell under the pointer, as last reported to the App.
    pointer: Option<(u16, u16)>,
    /// Fractional wheel lines not yet delivered.
    scroll_accum: f32,
    #[cfg(target_os = "macos")]
    menu: Option<crate::menus::NativeMenu>,
    #[cfg(feature = "screenshot")]
    script: Option<crate::screenshot::Run>,
}

impl VixIced {
    /// The window before the App starts, and the task that asks for the OS
    /// appearance (whose answer starts it).
    pub fn boot(cli: Cli, loaded: Loaded, title: String) -> (Self, Task<Message>) {
        let window = Self {
            pending: Some((cli, loaded)),
            app: None,
            term: Terminal::new(GridBackend::new(80, 24)).unwrap_or_else(|never| match never {}),
            palette: Palette::for_appearance(true),
            fonts: Fonts::load(),
            title,
            window: None,
            git_refreshed: false,
            mods: keyboard::Modifiers::empty(),
            pressed: None,
            pointer: None,
            scroll_accum: 0.0,
            #[cfg(target_os = "macos")]
            menu: None,
            #[cfg(feature = "screenshot")]
            script: None,
        };
        (window, system::theme().map(Message::Appearance))
    }

    /// The window title.
    #[must_use]
    pub fn title(&self) -> String {
        self.title.clone()
    }

    /// The iced theme: only the window background before the first frame,
    /// following the OS appearance like the grid's default colors.
    #[must_use]
    pub fn theme(&self) -> Theme {
        if self.palette == Palette::for_appearance(false) {
            Theme::Light
        } else {
            Theme::Dark
        }
    }

    /// Runtime events, the `App::tick` timer, and (macOS) menu clicks.
    pub fn subscription(&self) -> Subscription<Message> {
        let interval = self
            .app
            .as_ref()
            .map_or(Duration::from_millis(100), App::poll_interval);
        let mut subs = vec![
            event::listen_with(|event, _status, id| {
                wanted(&event).then_some(Message::Event(id, event))
            }),
            time::every(interval).map(|_| Message::Tick),
        ];
        #[cfg(target_os = "macos")]
        subs.push(crate::menus::clicks().map(Message::Menu));
        #[cfg(feature = "screenshot")]
        if self.script.is_some() {
            subs.push(time::every(crate::screenshot::STEP).map(|_| Message::Script));
        }
        Subscription::batch(subs)
    }

    /// Handle one message, then draw the App's next frame into the grid.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = match message {
            Message::Appearance(mode) => self.start(mode != theme::Mode::Light),
            Message::Event(id, event) => {
                self.handle(id, &event);
                Task::none()
            }
            Message::Tick => {
                if let Some(app) = &mut self.app {
                    // Background work (streamed output, LSP, file watching,
                    // …), exactly as the terminal loop's `App::tick`.
                    app.tick();
                }
                Task::none()
            }
            Message::Menu(id) => {
                self.run_menu_action(&id);
                Task::none()
            }
            #[cfg(feature = "screenshot")]
            Message::Script => self.step_script(),
            #[cfg(feature = "screenshot")]
            Message::Captured(shot) => {
                if let Some(run) = &self.script {
                    run.save(&shot);
                }
                if let Some(app) = &mut self.app {
                    app.flags.insert(AppFlags::SHOULD_QUIT);
                }
                Task::none()
            }
        };
        self.after_window_up();
        if let Some(quit) = self.check_quit() {
            return quit;
        }
        self.render();
        task
    }

    /// The grid's canvas, with the IME anchor at the text cursor above it.
    #[must_use]
    pub fn view(&self) -> Element<'_, Message> {
        let cell = self.fonts.cell;
        let cursor = self.term.backend().cursor().map_or(Point::ORIGIN, |c| {
            Point::new(cell.width * f32::from(c.x), cell.height * f32::from(c.y))
        });
        let grid = canvas(Grid {
            cells: self.term.backend(),
            palette: self.palette,
            fonts: self.fonts,
        })
        .width(Length::Fill)
        .height(Length::Fill);
        stack![grid, ImeAnchor::new(Rectangle::new(cursor, cell))].into()
    }

    /// Start the App for the OS appearance `dark`.
    fn start(&mut self, dark: bool) -> Task<Message> {
        let Some((cli, loaded)) = self.pending.take() else {
            return Task::none();
        };
        self.palette = Palette::for_appearance(dark);
        match crate::startup::start(&cli, loaded, dark) {
            Ok((app, _title)) => {
                self.app = Some(app);
                #[cfg(feature = "screenshot")]
                {
                    self.script =
                        crate::startup::Script::from_env().map(crate::screenshot::Run::new);
                }
                Task::none()
            }
            Err(e) => {
                eprintln!("vix-iced: {e}");
                iced::exit()
            }
        }
    }

    /// Once both the App and the window exist: install the native menu bar
    /// (macOS) and refresh git state — deferred until the window is up, as
    /// in the terminal binary (T122).
    fn after_window_up(&mut self) {
        if self.git_refreshed || self.window.is_none() {
            return;
        }
        let Some(app) = &mut self.app else {
            return;
        };
        self.git_refreshed = true;
        app.refresh_git();
        #[cfg(target_os = "macos")]
        {
            self.menu = Some(crate::menus::install());
        }
    }

    /// Quit when the App asked to (after its own exit work); drop a suspend
    /// request (no meaning in a window).
    fn check_quit(&mut self) -> Option<Task<Message>> {
        let app = self.app.as_mut()?;
        app.flags.remove(AppFlags::SUSPEND_REQUESTED);
        if !app.flags.contains(AppFlags::SHOULD_QUIT) {
            return None;
        }
        app.on_exit();
        self.app = None;
        Some(iced::exit())
    }

    /// Run one action id from a menu.
    fn run_menu_action(&mut self, id: &str) {
        let Some(app) = &mut self.app else {
            return;
        };
        #[cfg(target_os = "macos")]
        if id == crate::menus::OPEN_NATIVE {
            if let Some(paths) = rfd::FileDialog::new().pick_files() {
                for p in paths {
                    app.open_initial(&p);
                }
            }
            return;
        }
        app.run_action(id);
    }

    /// Draw the App's next frame into the grid, sized to the window.
    fn render(&mut self) {
        let (Some(app), Some((_, size))) = (&mut self.app, self.window) else {
            return;
        };
        let cell = self.fonts.cell;
        let cols = cell_index(size.width / cell.width, u16::MAX).max(1);
        let rows = cell_index(size.height / cell.height, u16::MAX).max(1);
        self.term.backend_mut().resize(cols, rows);
        let _ = self.term.draw(|frame| vix::ui::draw(app, frame));
    }

    /// Feed one runtime event to the App.
    fn handle(&mut self, id: window::Id, event: &Event) {
        match event {
            Event::Window(window::Event::Opened { size, .. } | window::Event::Resized(size)) => {
                self.window = Some((id, *size));
            }
            Event::Window(window::Event::CloseRequested) => {
                // The close button runs Vix's own quit flow (unsaved
                // changes, session) instead of closing outright.
                self.run_menu_action("file.quit");
            }
            Event::Keyboard(k) => {
                if let keyboard::Event::KeyPressed { modifiers, .. }
                | keyboard::Event::ModifiersChanged(modifiers) = k
                {
                    self.mods = *modifiers;
                }
                if let (Some(app), Some(key)) = (&mut self.app, crate::keys::key_event(k)) {
                    app.on_key(key);
                }
            }
            Event::InputMethod(iced::advanced::input_method::Event::Commit(text)) => {
                if let Some(app) = &mut self.app {
                    for key in crate::keys::commit_events(text) {
                        app.on_key(key);
                    }
                }
            }
            Event::Mouse(m) => self.handle_mouse(m),
            _ => {}
        }
    }

    /// Feed one mouse event to the App.
    fn handle_mouse(&mut self, event: &mouse::Event) {
        let mods = crate::keys::modifiers(self.mods);
        match event {
            mouse::Event::CursorMoved { position } => {
                // A terminal reports motion per cell, not per pixel; so does this.
                let cell = self.cell_at(*position);
                if self.pointer != Some(cell) {
                    self.pointer = Some(cell);
                    let kind = self
                        .pressed
                        .map_or(MouseEventKind::Moved, MouseEventKind::Drag);
                    self.mouse(kind, cell, mods);
                }
            }
            mouse::Event::ButtonPressed(b) | mouse::Event::ButtonReleased(b) => {
                let b = match b {
                    mouse::Button::Left => MouseButton::Left,
                    mouse::Button::Right => MouseButton::Right,
                    mouse::Button::Middle => MouseButton::Middle,
                    _ => return,
                };
                let kind = if matches!(event, mouse::Event::ButtonPressed(_)) {
                    self.pressed = Some(b);
                    MouseEventKind::Down(b)
                } else {
                    self.pressed = None;
                    MouseEventKind::Up(b)
                };
                self.mouse(kind, self.pointer.unwrap_or((0, 0)), mods);
            }
            mouse::Event::WheelScrolled { delta } => {
                self.scroll_accum += match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => y / PIXELS_PER_LINE,
                };
                let cell = self.pointer.unwrap_or((0, 0));
                // Positive y moves the content down: the view scrolls up.
                while self.scroll_accum.abs() >= 1.0 {
                    let kind = if self.scroll_accum > 0.0 {
                        self.scroll_accum -= 1.0;
                        MouseEventKind::ScrollUp
                    } else {
                        self.scroll_accum += 1.0;
                        MouseEventKind::ScrollDown
                    };
                    self.mouse(kind, cell, mods);
                }
            }
            _ => {}
        }
    }

    fn mouse(&mut self, kind: MouseEventKind, cell: (u16, u16), mods: KeyModifiers) {
        if let Some(app) = &mut self.app {
            app.on_mouse(MouseEvent {
                kind,
                column: cell.0,
                row: cell.1,
                modifiers: mods,
            });
        }
    }

    /// The cell under a window position.
    fn cell_at(&self, pos: Point) -> (u16, u16) {
        let cell = self.fonts.cell;
        let area = self.term.backend().buffer().area;
        (
            cell_index(pos.x / cell.width, area.width),
            cell_index(pos.y / cell.height, area.height),
        )
    }

    /// Advance the scripted run: type its keys through [`Self::handle`],
    /// capture the window, or nothing yet.
    #[cfg(feature = "screenshot")]
    fn step_script(&mut self) -> Task<Message> {
        use crate::screenshot::Step;
        let (Some(run), Some(app)) = (&mut self.script, &mut self.app) else {
            return Task::none();
        };
        match run.step(app) {
            Step::Wait => Task::none(),
            Step::Type(keys) => {
                let id = self.window.map_or_else(window::Id::unique, |(id, _)| id);
                for key in keys {
                    self.handle(id, &Event::Keyboard(key));
                }
                Task::none()
            }
            Step::Capture => self.window.map_or_else(Task::none, |(id, _)| {
                window::screenshot(id).map(Message::Captured)
            }),
        }
    }
}

/// Whether the window acts on a runtime event (the rest are dropped before
/// they become messages).
fn wanted(event: &Event) -> bool {
    match event {
        Event::Keyboard(k) => matches!(
            k,
            keyboard::Event::KeyPressed { .. } | keyboard::Event::ModifiersChanged(_)
        ),
        Event::Mouse(m) => matches!(
            m,
            mouse::Event::CursorMoved { .. }
                | mouse::Event::ButtonPressed(_)
                | mouse::Event::ButtonReleased(_)
                | mouse::Event::WheelScrolled { .. }
        ),
        Event::Window(w) => matches!(
            w,
            window::Event::Opened { .. }
                | window::Event::Resized(_)
                | window::Event::CloseRequested
        ),
        Event::InputMethod(iced::advanced::input_method::Event::Commit(_)) => true,
        _ => false,
    }
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
        assert_eq!(cell_index(4.9, 10), 4);
        assert_eq!(cell_index(42.0, 10), 9);
        assert_eq!(cell_index(5.0, 0), 0);
    }

    #[test]
    fn only_handled_events_become_messages() {
        assert!(wanted(&Event::Window(window::Event::CloseRequested)));
        assert!(wanted(&Event::Mouse(mouse::Event::CursorMoved {
            position: Point::ORIGIN
        })));
        assert!(!wanted(&Event::Window(window::Event::Focused)));
        assert!(!wanted(&Event::Mouse(mouse::Event::CursorEntered)));
    }
}
