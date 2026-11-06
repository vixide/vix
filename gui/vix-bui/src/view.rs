//! The BUI window: one `canvas` filling it, painted from the cell grid, with
//! BUI's keyboard, mouse and wheel events forwarded to `crate::model::Model`.
//!
//! BUI (Blender's UI toolkit, `blenderui`) is retained-mode with a gpui-style
//! Rust API. Here only its window, event loop, input and `canvas` painter are
//! used; none of Blender's widgets are, because the whole Vix `App` is drawn
//! by its own `ui::draw` into a character grid, exactly as in the other front
//! ends.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use blenderui::prelude::*;
use blenderui::{Modifiers, MouseButton as BuiButton};
use vix_input::{KeyModifiers, MouseButton};

use crate::font_choice::FONT_SIZE;
use crate::model::Model;
use crate::render::Rgba8;
use crate::scene::Op;

/// What the window shares with its event handlers.
pub struct Shared {
    /// The Vix `App` and its grid.
    pub model: RefCell<Model>,
    /// Where the canvas's top-left is in the window, set while painting.
    origin: Cell<(f32, f32)>,
}

impl Shared {
    /// Wrap a model for a window.
    #[must_use]
    pub fn new(model: Model) -> Rc<Self> {
        Rc::new(Self {
            model: RefCell::new(model),
            origin: Cell::new((0.0, 0.0)),
        })
    }

    /// A window position as a position inside the grid.
    fn local(&self, x: f32, y: f32) -> (f32, f32) {
        let (ox, oy) = self.origin.get();
        (x - ox, y - oy)
    }

    /// Fit the grid to `bounds`, render the `App` into it and paint it.
    fn paint(&self, bounds: Bounds<f32>, painter: &mut Painter) {
        self.origin.set((bounds.origin.x, bounds.origin.y));
        let mut model = self.model.borrow_mut();
        model.fit(bounds.size.width, bounds.size.height);
        model.render();
        let palette = model.palette();
        painter.fill(bounds, color(palette.bg));
        replay(&model.scene(), bounds.origin.x, bounds.origin.y, painter);
    }
}

/// A grid color as BUI's.
fn color(c: Rgba8) -> Rgba {
    rgba((u32::from(c.r) << 24) | (u32::from(c.g) << 16) | (u32::from(c.b) << 8) | u32::from(c.a))
}

/// The rectangle at (`x`, `y`) of the given size, as BUI bounds.
fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<f32> {
    Bounds {
        origin: point(x, y),
        size: size(w, h),
    }
}

/// Draw `ops` onto `painter`, offset by (`ox`, `oy`).
///
/// BUI has no bold face to ask for, so bold text is the same glyph drawn a
/// half pixel to the right as well, which thickens its stems.
fn replay(ops: &[Op], ox: f32, oy: f32, painter: &mut Painter) {
    // Text sits a little below the cell top so the glyph box centers in it.
    const TEXT_DROP: f32 = 2.0;
    for op in ops {
        match op {
            Op::Rect {
                x,
                y,
                w,
                h,
                color: c,
            } => {
                painter.fill(rect(ox + x, oy + y, *w, *h), color(*c));
            }
            Op::Text {
                x,
                y,
                text,
                color: c,
                bold,
            } => {
                let (x, y) = (ox + x, oy + y + TEXT_DROP);
                painter.text(x, y, text, FONT_SIZE, color(*c));
                if *bold {
                    painter.text(x + 0.5, y, text, FONT_SIZE, color(*c));
                }
            }
        }
    }
}

/// The BUI button as `vix-input`'s.
fn button(b: BuiButton) -> MouseButton {
    match b {
        BuiButton::Left => MouseButton::Left,
        BuiButton::Right => MouseButton::Right,
        BuiButton::Middle => MouseButton::Middle,
    }
}

/// BUI's modifier flags as `vix-input`'s.
fn mods(m: Modifiers) -> KeyModifiers {
    crate::keys::modifiers(m.shift, m.control, m.alt)
}

/// After an input event: quit if the App asked to, otherwise redraw if the
/// event changed anything.
fn settle(shared: &Shared, cx: &mut App, redraw: bool) {
    if shared.model.borrow_mut().should_quit() {
        cx.quit();
    } else if redraw {
        cx.notify_all();
    }
}

/// The root view.
pub struct VixBui {
    shared: Rc<Shared>,
}

impl VixBui {
    /// A view over `shared`.
    #[must_use]
    pub const fn new(shared: Rc<Shared>) -> Self {
        Self { shared }
    }
}

impl Render for VixBui {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let s = &self.shared;
        let paint = Rc::clone(s);
        let key = Rc::clone(s);
        let down = Rc::clone(s);
        let up = Rc::clone(s);
        let motion = Rc::clone(s);
        let wheel = Rc::clone(s);
        body()
            .m_0()
            .css("height", "100vh")
            .on_key_down(move |e, _, cx| {
                let k = &e.keystroke;
                let m = k.modifiers;
                let redraw = key.model.borrow_mut().key_down(
                    &k.key,
                    k.key_char.as_deref(),
                    m.shift,
                    m.control,
                    m.alt,
                );
                settle(&key, cx, redraw);
            })
            .on_any_mouse_down(move |e, _, cx| {
                let (x, y) = down.local(e.position.x.0, e.position.y.0);
                down.model
                    .borrow_mut()
                    .mouse_down(x, y, button(e.button), mods(e.modifiers));
                settle(&down, cx, true);
            })
            .on_mouse_up(BuiButton::Left, up_handler(&up))
            .on_mouse_up(BuiButton::Right, up_handler(&up))
            .on_mouse_up(BuiButton::Middle, up_handler(&up))
            .on_mouse_move(move |e, _, cx| {
                let (x, y) = motion.local(e.position.x.0, e.position.y.0);
                let moved = motion
                    .model
                    .borrow_mut()
                    .mouse_move(x, y, mods(e.modifiers));
                settle(&motion, cx, moved);
            })
            .on_scroll_wheel(move |e, _, cx| {
                let (x, y) = wheel.local(e.position.x.0, e.position.y.0);
                let moved = wheel
                    .model
                    .borrow_mut()
                    .scroll(x, y, e.delta, mods(e.modifiers));
                settle(&wheel, cx, moved);
            })
            .child(
                canvas(move |bounds, painter| paint.paint(bounds, painter))
                    .id("vix-grid")
                    .w_full()
                    .h_full(),
            )
    }
}

/// The handler for a mouse-up of any button, in window coordinates.
fn up_handler(shared: &Rc<Shared>) -> impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static {
    let shared = Rc::clone(shared);
    move |e, _, cx| {
        let (x, y) = shared.local(e.position.x.0, e.position.y.0);
        shared
            .model
            .borrow_mut()
            .mouse_up(x, y, button(e.button), mods(e.modifiers));
        settle(&shared, cx, true);
    }
}

thread_local! {
    /// The window's shared state, for the timer job (which carries no data
    /// because it must be `Send`; it always runs on this, the UI, thread).
    static SHARED: RefCell<Option<Rc<Shared>>> = const { RefCell::new(None) };
}

/// The timer job: tick the `App` and redraw when it is due.
fn tick_job(cx: &mut App) {
    let Some(shared) = SHARED.with(|s| s.borrow().clone()) else {
        return;
    };
    let redraw = shared.model.borrow_mut().tick();
    settle(&shared, cx, redraw);
}

/// Open the window for `shared` with a logical size of `width` x `height`.
///
/// # Errors
/// Returns BUI's message when the window cannot be created.
pub fn open(
    cx: &mut App,
    shared: &Rc<Shared>,
    title: String,
    width: f32,
    height: f32,
) -> Result<WindowHandle<VixBui>, String> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(width), px(height)),
        })),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
        }),
        ..WindowOptions::default()
    };
    let view = Rc::clone(shared);
    cx.open_window(options, move |_, cx| cx.new(|_| VixBui::new(view)))
}

/// Open the window and run BUI's event loop until it closes, then run the
/// App's shutdown work.
pub fn run(model: Model, title: String, dark: bool) {
    let shared = Shared::new(model);
    SHARED.with(|s| *s.borrow_mut() = Some(Rc::clone(&shared)));
    let window = Rc::clone(&shared);
    Application::new()
        .theme(if dark { "dark" } else { "light" })
        .run(move |cx| {
            if let Err(e) = open(cx, &window, title, 1200.0, 800.0) {
                eprintln!("vix-bui: could not open a window: {e}");
                return;
            }
            // Closing the window with its own close box ends the program
            // (BUI offers no way to veto it; see the spec's limits).
            cx.on_window_closed(|_, cx| cx.quit());
            let sender = cx.ui_sender();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(Model::tick_every());
                    if !sender.send(tick_job) {
                        break;
                    }
                }
            });
        });
    shared.model.borrow_mut().on_exit();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_keep_their_channels() {
        let c = color(Rgba8 {
            r: 255,
            g: 0,
            b: 128,
            a: 255,
        });
        assert!((c.r - 1.0).abs() < 1e-6);
        assert!(c.g.abs() < 1e-6);
        assert!((c.b - 128.0 / 255.0).abs() < 1e-6);
        assert!((c.a - 1.0).abs() < 1e-6);
    }

    #[test]
    fn buttons_map_one_to_one() {
        assert_eq!(button(BuiButton::Left), MouseButton::Left);
        assert_eq!(button(BuiButton::Right), MouseButton::Right);
        assert_eq!(button(BuiButton::Middle), MouseButton::Middle);
    }

    #[test]
    fn modifiers_map_shift_control_alt() {
        let m = mods(Modifiers {
            shift: true,
            control: false,
            alt: true,
        });
        assert!(m.contains(KeyModifiers::SHIFT));
        assert!(m.contains(KeyModifiers::ALT));
        assert!(!m.contains(KeyModifiers::CONTROL));
    }
}
