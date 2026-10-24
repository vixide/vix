//! The window: owns the Vix `App`, renders it through `ui::draw` into the
//! in-memory cell grid after every input event and timer tick, updates the
//! window's grid models from it (`crate::paint`), and feeds keyboard, mouse
//! and menu input back as `vix-input` events and action ids.
//!
//! Slint calls back into Rust from its event loop, so the state lives in an
//! `Rc<RefCell<…>>` shared by the callbacks; each takes it with
//! `try_borrow_mut` and drops an event that arrives while it is busy (only
//! possible while a modal dialog runs a nested event loop).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::font_choice::cell_height;
use crate::fonts::Families;
use crate::grid::GridBackend;
use crate::keys::{self, Mods};
use crate::paint::{self, GridModel};
use crate::render::{Palette, cell_colors};
use ratatui::Terminal;
use slint::{CloseRequestResponse, ComponentHandle, Timer, TimerMode};
use slint_interpreter::{ComponentInstance, Value};
use vix::app::{App as VixApp, AppFlags};
use vix_input::{KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

/// Scroll pixels per scrolled line. Slint reports a mouse-wheel notch as 60
/// pixels (three lines here) and trackpads in pixels.
const PIXELS_PER_LINE: f32 = 20.0;

/// Whether Slint swaps Command and Control here (see `crate::keys`).
const APPLE: bool = cfg!(any(target_os = "macos", target_os = "ios"));

/// The window's state, shared by its callbacks.
pub struct State {
    app: VixApp,
    term: Terminal<GridBackend>,
    grid: GridModel,
    palette: Palette,
    fonts: Families,
    /// Fractional wheel lines not yet delivered.
    scroll_accum: f32,
    /// The button held down, so motion reports as a drag.
    pressed: Option<MouseButton>,
    /// The cell the last motion event was reported for.
    last_move_cell: Option<(u16, u16)>,
    /// The App asked to quit and the event loop is stopping.
    quitting: bool,
    #[cfg(feature = "screenshot")]
    script: Option<crate::screenshot::Run>,
}

/// A number property of the window (`0.0` if missing).
fn number(ui: &ComponentInstance, name: &str) -> f32 {
    match ui.get_property(name) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "window geometry is far inside f32 range"
        )]
        Ok(Value::Number(n)) => n as f32,
        _ => 0.0,
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

/// A callback argument as a number.
fn arg_number(args: &[Value], i: usize) -> f32 {
    match args.get(i) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "window geometry is far inside f32 range"
        )]
        Some(Value::Number(n)) => *n as f32,
        _ => 0.0,
    }
}

/// A callback argument as a small integer (one of the window's enum codes).
fn arg_int(args: &[Value], i: usize) -> i32 {
    match args.get(i) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the window passes small enum codes"
        )]
        Some(Value::Number(n)) => n.round() as i32,
        _ => -1,
    }
}

/// A callback argument as the window's `Mods` struct.
fn arg_mods(args: &[Value], i: usize) -> Mods {
    let Some(Value::Struct(s)) = args.get(i) else {
        return Mods::default();
    };
    let flag = |name: &str| matches!(s.get_field(name), Some(Value::Bool(true)));
    Mods {
        control: flag("control"),
        alt: flag("alt"),
        shift: flag("shift"),
        meta: flag("meta"),
    }
}

impl State {
    /// Wrap a started `App`. `dark` is the OS appearance.
    #[must_use]
    pub fn new(app: VixApp, dark: bool) -> Self {
        Self {
            app,
            term: Terminal::new(GridBackend::new(80, 24)).unwrap_or_else(|never| match never {}),
            grid: GridModel::new(),
            palette: Palette::for_appearance(dark),
            fonts: Families::load(),
            scroll_accum: 0.0,
            pressed: None,
            last_move_cell: None,
            quitting: false,
            #[cfg(feature = "screenshot")]
            script: crate::startup::Script::from_env().map(crate::screenshot::Run::new),
        }
    }

    /// The value for the window's `rows` property.
    #[must_use]
    pub fn rows(&self) -> Value {
        self.grid.rows()
    }

    /// The default background, for the window's `grid-background`.
    #[must_use]
    pub fn background(&self) -> Value {
        Value::Brush(slint::Brush::SolidColor(paint::color(self.palette.bg)))
    }

    /// The primary font family.
    #[must_use]
    pub fn family(&self) -> &str {
        &self.fonts.primary
    }

    /// Refresh git state; deferred until the window is up (T122).
    pub fn refresh_git(&mut self) {
        self.app.refresh_git();
    }

    /// Run the App's exit work (session save, …) once the window has closed.
    pub fn on_exit(&mut self) {
        self.app.on_exit();
    }

    /// The cell size in logical pixels: the measured width of the font's
    /// `M`, and the fixed line height.
    fn cell(ui: &ComponentInstance) -> (f32, f32) {
        (number(ui, "cell-w").max(1.0), cell_height())
    }

    /// The cell under a grid position.
    fn cell_at(&self, ui: &ComponentInstance, x: f32, y: f32) -> (u16, u16) {
        let (w, h) = Self::cell(ui);
        let area = self.term.backend().buffer().area;
        (
            cell_index(x / w, area.width),
            cell_index(y / h, area.height),
        )
    }

    /// Quit when the App asked to; drop a suspend request (no meaning here).
    fn check_quit(&mut self) {
        self.app.flags.remove(AppFlags::SUSPEND_REQUESTED);
        if self.app.flags.contains(AppFlags::SHOULD_QUIT) && !self.quitting {
            self.quitting = true;
            let _ = slint::quit_event_loop();
        }
    }

    /// Draw the App into the grid and update the window from it.
    pub fn render(&mut self, ui: &ComponentInstance) {
        let (w, h) = Self::cell(ui);
        let cols = cell_index(number(ui, "grid-width") / w, u16::MAX).max(1);
        let rows = cell_index(number(ui, "grid-height") / h, u16::MAX).max(1);
        self.term.backend_mut().resize(cols, rows);
        let app = &mut self.app;
        let _ = self.term.draw(|frame| vix::ui::draw(app, frame));
        let buffer = self.term.backend().buffer();
        self.grid.update(buffer, self.palette, &self.fonts);
        let cursor = self
            .term
            .backend()
            .cursor()
            .filter(|c| c.x < buffer.area.width && c.y < buffer.area.height);
        let _ = ui.set_property("cursor-visible", Value::Bool(cursor.is_some()));
        if let Some(c) = cursor {
            let (fg, _) = cell_colors(&buffer[(c.x, c.y)], self.palette);
            let _ = ui.set_property("cursor-col", Value::Number(f64::from(c.x)));
            let _ = ui.set_property("cursor-row", Value::Number(f64::from(c.y)));
            let _ = ui.set_property(
                "cursor-color",
                Value::Brush(slint::Brush::SolidColor(paint::color(fg))),
            );
        }
    }

    /// Feed one key press to the App.
    fn key(&mut self, key: KeyEvent) {
        self.app.on_key(key);
        self.check_quit();
    }

    /// A Slint `key-pressed` event; true when it was a key Vix uses.
    pub fn slint_key(&mut self, text: &str, mods: Mods) -> bool {
        match keys::key_event(text, mods, APPLE) {
            Some(k) => {
                self.key(k);
                true
            }
            None => false,
        }
    }

    fn mouse(&mut self, kind: MouseEventKind, cell: (u16, u16), mods: KeyModifiers) {
        self.app.on_mouse(MouseEvent {
            kind,
            column: cell.0,
            row: cell.1,
            modifiers: mods,
        });
        self.check_quit();
    }

    /// A pointer event from the window's `TouchArea`.
    fn pointer(&mut self, ui: &ComponentInstance, args: &[Value]) {
        let button = match arg_int(args, 1) {
            0 => MouseButton::Left,
            1 => MouseButton::Right,
            2 => MouseButton::Middle,
            _ => return self.pointer_moved(ui, args),
        };
        let cell = self.cell_at(ui, arg_number(args, 2), arg_number(args, 3));
        let mods = keys::modifiers(arg_mods(args, 4), APPLE);
        match arg_int(args, 0) {
            0 => {
                self.pressed = Some(button);
                self.mouse(MouseEventKind::Down(button), cell, mods);
            }
            1 => {
                self.pressed = None;
                self.mouse(MouseEventKind::Up(button), cell, mods);
            }
            _ => self.pointer_moved(ui, args),
        }
    }

    /// Pointer motion: reported per cell, not per pixel, as a terminal does.
    fn pointer_moved(&mut self, ui: &ComponentInstance, args: &[Value]) {
        if arg_int(args, 0) == 3 {
            self.pressed = None; // cancelled (the pointer left mid-press)
            return;
        }
        let cell = self.cell_at(ui, arg_number(args, 2), arg_number(args, 3));
        if self.last_move_cell != Some(cell) {
            self.last_move_cell = Some(cell);
            let kind = self
                .pressed
                .map_or(MouseEventKind::Moved, MouseEventKind::Drag);
            self.mouse(kind, cell, keys::modifiers(arg_mods(args, 4), APPLE));
        }
    }

    /// A wheel/trackpad scroll: whole lines, the remainder carried over.
    fn scroll(&mut self, ui: &ComponentInstance, args: &[Value]) {
        self.scroll_accum += arg_number(args, 1) / PIXELS_PER_LINE;
        let cell = self.cell_at(ui, arg_number(args, 2), arg_number(args, 3));
        let mods = keys::modifiers(arg_mods(args, 4), APPLE);
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

    /// Background work, the scripted run's next step, and a redraw.
    fn tick(&mut self, ui: &ComponentInstance, timer: &Timer) {
        // Background work (streamed output, LSP, file watching, …), exactly
        // as the terminal loop's `App::tick`.
        self.app.tick();
        self.check_quit();
        #[cfg(not(feature = "screenshot"))]
        let interval = self.app.poll_interval();
        #[cfg(feature = "screenshot")]
        let interval = self.script_step(ui);
        self.render(ui);
        if timer.interval() != interval {
            timer.set_interval(interval);
        }
    }

    /// Advance the scripted run, if any: its keystrokes go through the same
    /// handling as real ones, and its capture ends the run. Returns the timer
    /// interval (short while a run is in progress).
    #[cfg(feature = "screenshot")]
    fn script_step(&mut self, ui: &ComponentInstance) -> Duration {
        let interval = self.app.poll_interval();
        let Some(mut run) = self.script.take() else {
            return interval;
        };
        let step = run.step(&mut self.app);
        for (text, mods) in step.keys {
            self.slint_key(&text, mods);
        }
        if step.capture {
            // A steady-state frame: `ui::draw` plus diffing every row model.
            let at = std::time::Instant::now();
            self.render(ui);
            eprintln!("vix-slint: draw and model update took {:?}", at.elapsed());
            run.capture(ui, self.grid.span_count());
            self.quitting = true;
            let _ = slint::quit_event_loop();
        } else {
            self.script = Some(run);
        }
        interval.min(Duration::from_millis(50))
    }
}

/// Run one menu action id: the system file dialog for **Open with System
/// Dialog…**, `App::run_action` for everything else. The dialog runs before
/// the state is borrowed, since it spins a nested event loop.
fn menu_action(state: &Rc<RefCell<State>>, ui: &ComponentInstance, id: &str) {
    #[cfg(target_os = "macos")]
    if id == crate::menus::OPEN_NATIVE {
        let picked = rfd::FileDialog::new().pick_files().unwrap_or_default();
        if let Ok(mut s) = state.try_borrow_mut() {
            for p in picked {
                s.app.open_initial(&p);
            }
            s.render(ui);
        }
        return;
    }
    if let Ok(mut s) = state.try_borrow_mut() {
        s.app.run_action(id);
        s.check_quit();
        s.render(ui);
    }
}

/// Wire the window's callbacks to `state`, start the background timer, and
/// draw the first frame. Keep the returned timer alive while the window runs.
pub fn attach(ui: &ComponentInstance, state: &Rc<RefCell<State>>) -> Rc<Timer> {
    let weak = ui.as_weak();

    // Each callback: borrow the state, handle the event, redraw.
    macro_rules! on {
        ($name:literal, |$s:ident, $ui:ident, $args:ident| $body:expr) => {{
            let state = Rc::clone(state);
            let weak = weak.clone();
            let _ = ui.set_callback($name, move |$args: &[Value]| {
                let Some($ui) = weak.upgrade() else {
                    return Value::Void;
                };
                let Ok(mut $s) = state.try_borrow_mut() else {
                    return Value::Void;
                };
                $body
            });
        }};
    }

    on!("key", |s, ui, args| {
        let text = match args.first() {
            Some(Value::String(t)) => t.to_string(),
            _ => String::new(),
        };
        let used = s.slint_key(&text, arg_mods(args, 1));
        s.render(&ui);
        Value::Bool(used)
    });
    on!("pointer", |s, ui, args| {
        s.pointer(&ui, args);
        s.render(&ui);
        Value::Void
    });
    on!("scroll", |s, ui, args| {
        s.scroll(&ui, args);
        s.render(&ui);
        Value::Void
    });
    on!("resized", |s, ui, _args| {
        s.render(&ui);
        Value::Void
    });
    {
        let state = Rc::clone(state);
        let weak = weak.clone();
        let _ = ui.set_callback("menu-activated", move |args: &[Value]| {
            if let (Some(ui), Some(Value::String(id))) = (weak.upgrade(), args.first()) {
                menu_action(&state, &ui, id.as_str());
            }
            Value::Void
        });
    }

    // The close button runs Vix's own quit flow (unsaved changes, session)
    // instead of closing outright.
    {
        let state = Rc::clone(state);
        let weak = weak.clone();
        ui.window().on_close_requested(move || {
            let Ok(mut s) = state.try_borrow_mut() else {
                return CloseRequestResponse::KeepWindowShown;
            };
            if s.quitting {
                return CloseRequestResponse::HideWindow;
            }
            s.app.run_action("file.quit");
            s.check_quit();
            if let Some(ui) = weak.upgrade() {
                s.render(&ui);
            }
            CloseRequestResponse::KeepWindowShown
        });
    }

    #[cfg(target_os = "macos")]
    option_as_meta(ui, state);

    if let Ok(mut s) = state.try_borrow_mut() {
        s.render(ui);
    }
    // The timer reschedules itself to `App::poll_interval`, so its callback
    // holds it weakly (the caller owns it).
    let timer = Rc::new(Timer::default());
    let me = Rc::downgrade(&timer);
    let state = Rc::clone(state);
    timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        if let (Some(ui), Some(t), Ok(mut s)) =
            (weak.upgrade(), me.upgrade(), state.try_borrow_mut())
        {
            s.tick(&ui, &t);
        }
    });
    timer
}

/// "Option as Meta" on macOS: take `Option`+key presses from winit, whose
/// key-without-modifiers Slint's own key event lacks (it carries `ß` for
/// `Option+S`, and nothing for a dead key), and hand them to the App as
/// `Alt`+key. Everything else goes on to Slint.
#[cfg(target_os = "macos")]
fn option_as_meta(ui: &ComponentInstance, state: &Rc<RefCell<State>>) {
    use slint::winit_030::winit::event::{ElementState, WindowEvent};
    use slint::winit_030::winit::keyboard::{Key, ModifiersState};
    use slint::winit_030::winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
    use slint::winit_030::{EventResult, WinitWindowAccessor};

    let state = Rc::clone(state);
    let weak = ui.as_weak();
    let mut held = ModifiersState::empty();
    ui.window().on_winit_window_event(move |_, event| {
        match event {
            WindowEvent::ModifiersChanged(m) => held = m.state(),
            WindowEvent::KeyboardInput { event, .. }
                if held.alt_key() && event.state == ElementState::Pressed =>
            {
                if let Key::Character(c) = event.key_without_modifiers()
                    && let Some(key) = keys::option_chord(
                        &c,
                        held.shift_key(),
                        held.control_key(),
                        held.super_key(),
                    )
                    && let (Some(ui), Ok(mut s)) = (weak.upgrade(), state.try_borrow_mut())
                {
                    s.key(key);
                    s.render(&ui);
                    return EventResult::PreventDefault;
                }
            }
            _ => {}
        }
        EventResult::Propagate
    });
}
