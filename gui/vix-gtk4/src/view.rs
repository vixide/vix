//! The Vix window: owns the Vix `App`, renders it through `ui::draw` into the
//! in-memory cell grid on every `DrawingArea` draw, paints the grid with
//! Cairo and Pango (`crate::paint`), and feeds keys, input-method commits,
//! the mouse and the native menus back as `vix-input` events and action ids.
//!
//! GTK runs every handler on the main thread, so the state is one
//! `Rc<RefCell<State>>` shared by the closures. A handler never holds the
//! borrow across a call that can run another handler — the input method's
//! `filter_keypress` (which may emit `commit`), `Window::close` (which emits
//! `close-request`), or showing the file dialog.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gdk::{self, Key, ModifierType};
use gtk4::glib::{self, Propagation};
use gtk4::prelude::*;
use gtk4::{cairo, gio};
use ratatui::Terminal;
use vix::app::{App, AppFlags};
use vix_input::{KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::grid::GridBackend;
use crate::keys;
use crate::menus;
use crate::paint::{self, CellSize};
use crate::render::Palette;

/// Everything the window's handlers share.
pub struct State {
    app: App,
    term: Terminal<GridBackend>,
    palette: Palette,
    /// The Pango family list (primary, then Nerd Font fallbacks).
    family: String,
    /// The cell size the last frame was painted with.
    cell: CellSize,
    /// Fractional wheel lines not yet delivered.
    scroll_accum: f64,
    /// The button held down, so motion reports as a drag.
    pressed: Option<MouseButton>,
    /// The cell the last motion event was reported for.
    last_move_cell: Option<(u16, u16)>,
    /// The pointer's last position, for wheel events (which carry none).
    pointer: (f64, f64),
    /// The modifiers of the key press being filtered by the input method,
    /// for the text it commits.
    held: KeyModifiers,
    /// The App asked to quit and the window is closing for real.
    quitting: bool,
    /// `App::on_exit` has run.
    exited: bool,
    /// The open file dialog; GTK does not keep a native dialog alive.
    dialog: Option<gtk4::FileChooserNative>,
}

impl State {
    /// Size the grid to a `width` × `height` area of `cr`, draw the App into
    /// it, and paint it. Returns the text cursor's rectangle, if shown.
    pub fn render(
        &mut self,
        cr: &cairo::Context,
        width: f64,
        height: f64,
    ) -> Option<(f64, f64, f64, f64)> {
        let cell = paint::measure(cr, &self.family);
        self.cell = cell;
        let cols = cell_index(width / cell.width, u16::MAX).max(1);
        let rows = cell_index(height / cell.height, u16::MAX).max(1);
        self.term.backend_mut().resize(cols, rows);
        let app = &mut self.app;
        let _ = self.term.draw(|frame| vix::ui::draw(app, frame));
        paint::paint(
            cr,
            self.term.backend(),
            self.palette,
            &self.family,
            cell,
            (width, height),
        )
    }

    /// The cell under a position in the drawing area.
    fn cell_at(&self, (x, y): (f64, f64)) -> (u16, u16) {
        let area = self.term.backend().buffer().area;
        (
            cell_index(x / self.cell.width, area.width),
            cell_index(y / self.cell.height, area.height),
        )
    }

    /// Run `App::on_exit` once.
    pub fn exit(&mut self) {
        if !self.exited {
            self.exited = true;
            self.app.on_exit();
        }
    }

    /// Refresh git state; deferred until the window is up (T122).
    pub fn refresh_git(&mut self) {
        self.app.refresh_git();
    }
}

/// A fractional cell coordinate, clamped into `0..len`.
fn cell_index(v: f64, len: u16) -> u16 {
    let max = f64::from(len.saturating_sub(1));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped into 0..=u16 range first"
    )]
    let i = v.floor().clamp(0.0, max) as u16;
    i
}

/// A GDK button number as a `vix-input` button; `None` for back/forward.
fn mouse_button(n: u32) -> Option<MouseButton> {
    match n {
        gdk::BUTTON_PRIMARY => Some(MouseButton::Left),
        gdk::BUTTON_MIDDLE => Some(MouseButton::Middle),
        gdk::BUTTON_SECONDARY => Some(MouseButton::Right),
        _ => None,
    }
}

/// The window and its shared state; cheap to clone into handlers.
#[derive(Clone)]
pub struct View {
    state: Rc<RefCell<State>>,
    window: gtk4::ApplicationWindow,
    area: gtk4::DrawingArea,
    im: gtk4::IMMulticontext,
}

impl View {
    /// Open the window for a started `App` and wire up its input, menus and
    /// background timer.
    pub fn new(gtk_app: &gtk4::Application, app: App, dark: bool, title: &str) -> Self {
        let area = gtk4::DrawingArea::builder()
            .focusable(true)
            .hexpand(true)
            .vexpand(true)
            .build();
        let window = gtk4::ApplicationWindow::builder()
            .application(gtk_app)
            .title(title)
            .default_width(1200)
            .default_height(800)
            .show_menubar(true)
            .child(&area)
            .build();
        let im = gtk4::IMMulticontext::new();
        im.set_client_widget(Some(&area));
        // There is no preedit drawing in the grid; let the input method show
        // its composition itself.
        im.set_use_preedit(false);
        let state = State {
            app,
            term: Terminal::new(GridBackend::new(80, 24)).unwrap_or_else(|never| match never {}),
            palette: Palette::for_appearance(dark),
            family: paint::font_family(),
            cell: CellSize::default(),
            scroll_accum: 0.0,
            pressed: None,
            last_move_cell: None,
            pointer: (0.0, 0.0),
            held: KeyModifiers::NONE,
            quitting: false,
            exited: false,
            dialog: None,
        };
        let view = Self {
            state: Rc::new(RefCell::new(state)),
            window,
            area,
            im,
        };
        view.connect_drawing();
        view.connect_keys();
        view.connect_mouse();
        view.connect_actions(gtk_app);
        let v = view.clone();
        view.window.connect_close_request(move |_| v.on_close());
        view.window.present();
        view.area.grab_focus();
        view.schedule_tick();
        #[cfg(feature = "screenshot")]
        if let Some(script) = crate::startup::Script::from_env() {
            crate::screenshot::start(view.clone(), script);
        }
        view
    }

    /// The shared state.
    pub fn state(&self) -> &Rc<RefCell<State>> {
        &self.state
    }

    /// Bring the window to the front.
    pub fn present(&self) {
        self.window.present();
    }

    fn connect_drawing(&self) {
        let state = Rc::clone(&self.state);
        let im = self.im.clone();
        self.area.set_draw_func(move |_, cr, w, h| {
            let cursor = state.borrow_mut().render(cr, f64::from(w), f64::from(h));
            // Keep IME candidate windows next to the text cursor.
            if let Some((x, y, cw, ch)) = cursor {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "window coordinates fit in i32"
                )]
                let rect = gdk::Rectangle::new(x as i32, y as i32, cw.ceil() as i32, ch as i32);
                im.set_cursor_location(&rect);
            }
        });
    }

    fn connect_keys(&self) {
        let keys = gtk4::EventControllerKey::new();
        let v = self.clone();
        keys.connect_key_pressed(move |ctrl, keyval, keycode, state| {
            v.on_key_pressed(ctrl, keyval, keycode, state)
        });
        let v = self.clone();
        keys.connect_key_released(move |ctrl, _, _, state| {
            // Some input methods act on releases too.
            if !keys::is_chord(keys::modifiers(state))
                && let Some(ev) = ctrl.current_event()
            {
                v.im.filter_keypress(&ev);
            }
        });
        self.area.add_controller(keys);

        let v = self.clone();
        self.im.connect_commit(move |_, text| {
            let held = v.state.borrow().held;
            v.send_keys(keys::committed(text, held));
        });
        let focus = gtk4::EventControllerFocus::new();
        let im = self.im.clone();
        focus.connect_enter(move |_| im.focus_in());
        let im = self.im.clone();
        focus.connect_leave(move |_| im.focus_out());
        self.area.add_controller(focus);
    }

    /// One key press: plain typing goes through the input method first
    /// (its text arrives as `commit`); named keys and chords go straight to
    /// the key rules.
    fn on_key_pressed(
        &self,
        ctrl: &gtk4::EventControllerKey,
        keyval: Key,
        keycode: u32,
        state: ModifierType,
    ) -> Propagation {
        let mods = keys::modifiers(state);
        let mut keyval = keyval;
        if keys::is_chord(mods) {
            if mods.contains(KeyModifiers::ALT) {
                keyval = without_alt(ctrl, keyval, keycode, state);
            }
        } else {
            self.state.borrow_mut().held = mods;
            if let Some(ev) = ctrl.current_event()
                && self.im.filter_keypress(&ev)
            {
                self.after_input();
                return Propagation::Stop;
            }
        }
        let name = keyval.name().map(|n| n.to_string()).unwrap_or_default();
        let Some(key) = keys::key_press(&name, keyval.to_unicode(), state) else {
            return Propagation::Proceed;
        };
        self.send_keys([key]);
        Propagation::Stop
    }

    /// Hand key presses to the App, then redraw.
    pub fn send_keys(&self, keys: impl IntoIterator<Item = KeyEvent>) {
        {
            let mut s = self.state.borrow_mut();
            for k in keys {
                s.app.on_key(k);
            }
        }
        self.after_input();
    }

    fn connect_mouse(&self) {
        let click = gtk4::GestureClick::new();
        click.set_button(0);
        let v = self.clone();
        click.connect_pressed(move |g, _, x, y| {
            if let Some(b) = mouse_button(g.current_button()) {
                v.state.borrow_mut().pressed = Some(b);
                v.mouse(MouseEventKind::Down(b), (x, y), g.current_event_state());
            }
        });
        let v = self.clone();
        click.connect_released(move |g, _, x, y| {
            if let Some(b) = mouse_button(g.current_button()) {
                v.state.borrow_mut().pressed = None;
                v.mouse(MouseEventKind::Up(b), (x, y), g.current_event_state());
            }
        });
        self.area.add_controller(click);

        let motion = gtk4::EventControllerMotion::new();
        let v = self.clone();
        motion.connect_motion(move |c, x, y| {
            // A terminal reports motion per cell, not per pixel; so does this.
            let (kind, cell) = {
                let mut s = v.state.borrow_mut();
                s.pointer = (x, y);
                let cell = s.cell_at((x, y));
                if s.last_move_cell == Some(cell) {
                    return;
                }
                s.last_move_cell = Some(cell);
                (
                    s.pressed
                        .map_or(MouseEventKind::Moved, MouseEventKind::Drag),
                    cell,
                )
            };
            v.mouse_at(kind, cell, c.current_event_state());
        });
        self.area.add_controller(motion);

        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        let v = self.clone();
        scroll.connect_scroll(move |c, _, dy| {
            let mods = keys::modifiers(c.current_event_state());
            {
                let mut s = v.state.borrow_mut();
                // Wheel clicks arrive as whole lines; smooth (trackpad)
                // deltas as fractions of one, accumulated here.
                s.scroll_accum += dy;
                let cell = s.cell_at(s.pointer);
                // Positive dy scrolls the view down.
                while s.scroll_accum.abs() >= 1.0 {
                    let kind = if s.scroll_accum > 0.0 {
                        s.scroll_accum -= 1.0;
                        MouseEventKind::ScrollDown
                    } else {
                        s.scroll_accum += 1.0;
                        MouseEventKind::ScrollUp
                    };
                    s.app.on_mouse(MouseEvent {
                        kind,
                        column: cell.0,
                        row: cell.1,
                        modifiers: mods,
                    });
                }
            }
            v.after_input();
            Propagation::Stop
        });
        self.area.add_controller(scroll);
    }

    /// A mouse event at a position in the drawing area.
    fn mouse(&self, kind: MouseEventKind, pos: (f64, f64), state: ModifierType) {
        let cell = self.state.borrow().cell_at(pos);
        self.mouse_at(kind, cell, state);
    }

    /// A mouse event at a cell.
    fn mouse_at(&self, kind: MouseEventKind, cell: (u16, u16), state: ModifierType) {
        self.state.borrow_mut().app.on_mouse(MouseEvent {
            kind,
            column: cell.0,
            row: cell.1,
            modifiers: keys::modifiers(state),
        });
        self.after_input();
    }

    /// The native menus' actions: `app.run(id)` and `app.open-native`.
    fn connect_actions(&self, gtk_app: &gtk4::Application) {
        let run = gio::SimpleAction::new(menus::RUN, Some(glib::VariantTy::STRING));
        let v = self.clone();
        run.connect_activate(move |_, param| {
            if let Some(id) = param.and_then(|p| p.str()) {
                v.run_action(id);
            }
        });
        gtk_app.add_action(&run);
        let open = gio::SimpleAction::new(menus::OPEN_NATIVE, None);
        let v = self.clone();
        open.connect_activate(move |_, _| v.open_native());
        gtk_app.add_action(&open);
    }

    /// Run one action id, as a menu does.
    pub fn run_action(&self, id: &str) {
        self.state.borrow_mut().app.run_action(id);
        self.after_input();
    }

    /// Open files through the platform's file dialog.
    fn open_native(&self) {
        let dialog = gtk4::FileChooserNative::new(
            Some(&t!("menu.item.file.open_native")),
            Some(&self.window),
            gtk4::FileChooserAction::Open,
            None,
            None,
        );
        dialog.set_modal(true);
        dialog.set_select_multiple(true);
        let v = self.clone();
        dialog.connect_response(move |d, response| {
            let mut paths: Vec<PathBuf> = Vec::new();
            if response == gtk4::ResponseType::Accept {
                let files = d.files();
                paths = (0..files.n_items())
                    .filter_map(|i| files.item(i).and_downcast::<gio::File>())
                    .filter_map(|f| f.path())
                    .collect();
            }
            // Dropped after this handler: the signal holds its own reference.
            let _ours = v.state.borrow_mut().dialog.take();
            {
                let mut s = v.state.borrow_mut();
                for p in &paths {
                    s.app.open_initial(p);
                }
            }
            v.after_input();
        });
        self.state.borrow_mut().dialog = Some(dialog.clone());
        // Not under a borrow: a native dialog may run a nested main loop.
        dialog.show();
    }

    /// Background work at `App::poll_interval`, exactly as the terminal
    /// loop's `App::tick`; reschedules itself until the window closes.
    fn schedule_tick(&self) {
        let interval = self.state.borrow().app.poll_interval();
        let v = self.clone();
        glib::timeout_add_local_once(interval, move || {
            if v.state.borrow().quitting {
                return;
            }
            v.state.borrow_mut().app.tick();
            v.after_input();
            v.schedule_tick();
        });
    }

    /// After anything that may have changed the App: quit if it asked to,
    /// and redraw.
    fn after_input(&self) {
        let close = {
            let mut s = self.state.borrow_mut();
            // Suspend-to-shell has no meaning in a window.
            s.app.flags.remove(AppFlags::SUSPEND_REQUESTED);
            let quit = s.app.flags.contains(AppFlags::SHOULD_QUIT) && !s.quitting;
            if quit {
                s.quitting = true;
            }
            quit
        };
        if close {
            self.window.close();
        } else {
            self.area.queue_draw();
        }
    }

    /// The close button runs Vix's own quit flow (unsaved changes, session)
    /// instead of closing outright.
    fn on_close(&self) -> Propagation {
        let mut s = self.state.borrow_mut();
        if !s.quitting {
            s.app.run_action("file.quit");
            s.app.flags.remove(AppFlags::SUSPEND_REQUESTED);
            if !s.app.flags.contains(AppFlags::SHOULD_QUIT) {
                drop(s);
                self.area.queue_draw();
                return Propagation::Stop;
            }
            s.quitting = true;
        }
        s.exit();
        Propagation::Proceed
    }

    /// Close the window now, without Vix's quit flow (the scripted run).
    #[cfg(feature = "screenshot")]
    pub fn close_now(&self) {
        self.state.borrow_mut().quitting = true;
        self.window.close();
    }

    /// The drawing area's size in logical pixels and its scale factor.
    #[cfg(feature = "screenshot")]
    pub fn area_size(&self) -> (i32, i32, i32) {
        (
            self.area.width(),
            self.area.height(),
            self.area.scale_factor(),
        )
    }
}

/// The key a chord with `Alt` held would be without `Alt` ("Option as
/// Meta": `Option+S` is `s`, not `ß`), from the event's hardware keycode
/// and keyboard layout; `keyval` itself if that cannot be resolved.
fn without_alt(
    ctrl: &gtk4::EventControllerKey,
    keyval: Key,
    keycode: u32,
    state: ModifierType,
) -> Key {
    let Some(ev) = ctrl
        .current_event()
        .and_then(|e| e.downcast::<gdk::KeyEvent>().ok())
    else {
        return keyval;
    };
    let group = i32::try_from(ev.layout()).unwrap_or(0);
    ev.display()
        .and_then(|d| d.translate_key(keycode, state - ModifierType::ALT_MASK, group))
        .map_or(keyval, |(k, ..)| k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_index_clamps() {
        assert_eq!(cell_index(-3.0, 10), 0);
        assert_eq!(cell_index(4.7, 10), 4);
        assert_eq!(cell_index(99.0, 10), 9);
    }

    #[test]
    fn mouse_buttons() {
        assert_eq!(mouse_button(1), Some(MouseButton::Left));
        assert_eq!(mouse_button(2), Some(MouseButton::Middle));
        assert_eq!(mouse_button(3), Some(MouseButton::Right));
        assert_eq!(mouse_button(8), None);
    }
}
