//! The zgui window: one custom element (`crate::paint::GridElement`) filling
//! it, listeners that feed keys, the pointer and the wheel back into the
//! shared [`Model`], a timer for the `App`'s background work, the close box
//! routed through Vix's own quit flow, and (on macOS) the native menu bar.
//!
//! The element is a window shortcut, so it hears keys before anything has
//! focus, and focusable, so it keeps hearing them after a click.

use std::cell::RefCell;
use std::rc::Rc;

use vix_input::MouseButton;
use zgui::custom::{Custom, CustomHandle, custom};
use zgui::elements::Element;
use zgui::prelude::*;
use zgui::reactive::RenderEffect;
use zgui::view::WindowShortcut;

use crate::keys;
use crate::model::Model;
use crate::paint::GridElement;
use crate::render::{Palette, Rgba8};

/// The shared model, as every handler holds it.
type Shared = Rc<RefCell<Model>>;

/// The window's style sheet: the grid fills the window, on the palette's
/// background (so a resize never flashes another color).
fn sheet(palette: Palette) -> String {
    let Rgba8 { r, g, b, .. } = palette.bg;
    format!(
        ":root {{ background-color: rgb({r}, {g}, {b}); display: flex; }}
         .vix-grid {{ flex: 1 1 auto; width: 100%; height: 100%; outline: none; }}"
    )
}

/// Open the window around `model` and run until it closes.
///
/// # Errors
/// Whatever stopped zgui: no window could open, or no graphics device.
pub fn run(model: Model, title: String) -> Result<(), zgui::Error> {
    let palette = model.palette();
    let shared: Shared = Rc::new(RefCell::new(model));
    let exit = Rc::clone(&shared);
    let builder = app()
        .with_application_id("com.vixide.vix-zgui")
        .with_title(title)
        .with_size(1200.0, 800.0)
        .with_stylesheet(sheet(palette));
    #[cfg(target_os = "macos")]
    let builder = builder.with_menu(crate::menus::bar);
    let result = builder.run(move || window(&shared));
    // However the loop ended, the session is saved once.
    exit.borrow_mut().on_exit();
    result
}

/// After the `App` handled something: quit if it asked to, else repaint.
fn settle(shared: &Shared, handle: &CustomHandle<GridElement>) {
    let quit = shared.borrow_mut().should_quit();
    if quit {
        shared.borrow_mut().on_exit();
        use_windows().quit();
    } else {
        handle.repaint();
    }
}

/// A pointer button as Vix's, or `None` for one it has no use for.
fn button(b: Option<PointerButton>) -> Option<MouseButton> {
    match b? {
        PointerButton::Primary => Some(MouseButton::Left),
        PointerButton::Secondary => Some(MouseButton::Right),
        PointerButton::Middle => Some(MouseButton::Middle),
        _ => None,
    }
}

/// The window's content: the grid element and everything that drives it.
fn window(shared: &Shared) -> impl IntoView + use<> {
    let fonts = use_context::<Fonts>().unwrap_or_else(Fonts::system);
    let (element, handle) = custom(GridElement::new(Rc::clone(shared), fonts.shaper()));

    // Background work, as the terminal loop's tick.
    let ticker = {
        let (shared, handle) = (Rc::clone(shared), handle.clone());
        set_interval(Model::tick_every(), move || {
            let redraw = shared.borrow_mut().tick();
            if redraw {
                settle(&shared, &handle);
            }
        })
    };
    // The close box runs Vix's own quit flow (unsaved changes, session).
    let close = {
        let (shared, handle) = (Rc::clone(shared), handle.clone());
        on_close_request(move || {
            let mut model = shared.borrow_mut();
            if !model.should_quit() {
                model.run_action("file.quit");
            }
            if model.should_quit() {
                model.on_exit();
                CloseResponse::Close
            } else {
                drop(model);
                handle.repaint();
                CloseResponse::Veto
            }
        })
    };
    #[cfg(target_os = "macos")]
    let menu = {
        let (shared, handle) = (Rc::clone(shared), handle.clone());
        zgui::desktop::on_menu(move |id| {
            crate::menus::clicked(&mut shared.borrow_mut(), id.as_str());
            settle(&shared, &handle);
        })
    };
    // The element hears keys while nothing has focus, too.
    let anchor = NodeRef::new();
    let shortcut = RenderEffect::new(move |previous: Option<Option<WindowShortcut>>| {
        drop(previous);
        anchor.get();
        anchor.window_shortcut()
    });
    on_cleanup_local(move || {
        drop(ticker);
        drop(close);
        #[cfg(target_os = "macos")]
        drop(menu);
        drop(shortcut);
    });

    wired(element, shared, &handle)
        .class("vix-grid")
        .tabindex(Focus::Sequential)
        .node_ref(anchor)
        .into_view()
}

/// `element` with the listeners that feed keys, IME commits, the pointer and
/// the wheel to the model.
fn wired(
    element: Element<Custom>,
    shared: &Shared,
    handle: &CustomHandle<GridElement>,
) -> Element<Custom> {
    let on_key = {
        let (shared, handle) = (Rc::clone(shared), handle.clone());
        move |ev: &mut EventCx<'_, events::KeyDown>| {
            let Some(key) = keys::key_event(&ev.key, &ev.key_without_modifiers, ev.modifiers)
            else {
                return;
            };
            // Tab is Vix's, not a focus move; nothing else should act on it.
            ev.prevent_default();
            ev.stop_propagation();
            shared.borrow_mut().key(key);
            settle(&shared, &handle);
        }
    };
    let on_commit = {
        let (shared, handle) = (Rc::clone(shared), handle.clone());
        move |ev: &mut EventCx<'_, events::ImeCommit>| {
            if let zgui::vocab::ImeEvent::Commit(text) = &**ev {
                shared.borrow_mut().commit(text.as_str());
                settle(&shared, &handle);
            }
        }
    };
    let on_down = {
        let (shared, handle) = (Rc::clone(shared), handle.clone());
        move |ev: &mut EventCx<'_, events::PointerDown>| {
            let Some(b) = button(ev.button) else { return };
            ev.capture_pointer();
            let mods = keys::modifiers(ev.modifiers);
            let (x, y) = (ev.position.x.0, ev.position.y.0);
            shared.borrow_mut().mouse_down(x, y, b, mods);
            settle(&shared, &handle);
        }
    };
    let on_up = {
        let (shared, handle) = (Rc::clone(shared), handle.clone());
        move |ev: &mut EventCx<'_, events::PointerUp>| {
            let Some(b) = button(ev.button) else { return };
            ev.release_pointer();
            let mods = keys::modifiers(ev.modifiers);
            let (x, y) = (ev.position.x.0, ev.position.y.0);
            shared.borrow_mut().mouse_up(x, y, b, mods);
            settle(&shared, &handle);
        }
    };
    let on_move = {
        let (shared, handle) = (Rc::clone(shared), handle.clone());
        move |ev: &mut EventCx<'_, events::PointerMove>| {
            let mods = keys::modifiers(ev.modifiers);
            let (x, y) = (ev.position.x.0, ev.position.y.0);
            let moved = shared.borrow_mut().mouse_move(x, y, mods);
            if moved {
                settle(&shared, &handle);
            }
        }
    };
    let on_wheel = {
        let (shared, handle) = (Rc::clone(shared), handle.clone());
        move |ev: &mut EventCx<'_, events::Wheel>| {
            let mods = keys::modifiers(ev.modifiers);
            let (x, y) = (ev.position.x.0, ev.position.y.0);
            let moved = match ev.delta {
                ScrollDelta::Lines { y: lines, .. } => {
                    shared.borrow_mut().scroll_lines(x, y, lines, mods)
                }
                ScrollDelta::Pixels(size) => {
                    shared.borrow_mut().scroll_pixels(x, y, size.height.0, mods)
                }
                _ => false,
            };
            ev.prevent_default();
            if moved {
                settle(&shared, &handle);
            }
        }
    };

    element
        .on(events::KEY_DOWN, on_key)
        .on(events::IME_COMMIT, on_commit)
        .on(events::POINTER_DOWN, on_down)
        .on(events::POINTER_UP, on_up)
        .on(events::POINTER_MOVE, on_move)
        .on(events::WHEEL, on_wheel)
}
