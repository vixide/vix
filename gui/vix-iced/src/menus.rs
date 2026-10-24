//! The native menu bar (macOS), from the tree in `crate::menu_tree`, through
//! `muda` — iced has no native menus of its own. Each item's muda id is its
//! Vix action id; [`clicks`] delivers clicked ids as an iced subscription,
//! so a click wakes the event loop at once instead of waiting for a tick.

use std::sync::Mutex;

use crate::menu_tree::{self as menu, Node};
use iced::Subscription;
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

/// The muda id of **Open with System Dialog…**.
pub const OPEN_NATIVE: &str = "vix-gui.open-native";

/// The installed menu bar; must stay alive while it is shown.
pub struct NativeMenu {
    _menu: Menu,
}

/// muda reads `&` as a mnemonic marker; show it literally instead.
fn label(s: &str) -> String {
    s.replace('&', "&&")
}

/// Append one menu's nodes to a submenu (recursing into nested submenus).
fn fill(sub: &Submenu, nodes: Vec<Node>) {
    for node in nodes {
        let _ = match node {
            Node::Action { label: l, id } => {
                sub.append(&MenuItem::with_id(id, label(&l), true, None))
            }
            Node::OpenNative { label: l } => {
                sub.append(&MenuItem::with_id(OPEN_NATIVE, label(&l), true, None))
            }
            Node::Separator => sub.append(&PredefinedMenuItem::separator()),
            Node::Submenu { label: l, items } => {
                let nested = Submenu::new(label(&l), true);
                fill(&nested, items);
                sub.append(&nested)
            }
        };
    }
}

/// Build the menu bar and install it as the app's. Call once the
/// application (`NSApp`) exists — once the window has opened — so it
/// replaces winit's default menu rather than being replaced by it.
#[must_use]
pub fn install() -> NativeMenu {
    let bar = Menu::new();
    for m in menu::tree() {
        let sub = Submenu::new(label(&m.title), true);
        fill(&sub, m.items);
        let _ = bar.append(&sub);
    }
    bar.init_for_nsapp();
    NativeMenu { _menu: bar }
}

/// The ids of clicked menu items, as they are clicked.
pub fn clicks() -> Subscription<String> {
    Subscription::run(|| {
        iced::stream::channel(16, async |sender| {
            // muda calls the handler on the main thread for each click; the
            // channel hands the id to the iced runtime, which wakes the loop.
            let sender = Mutex::new(sender);
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                if let Ok(mut s) = sender.lock() {
                    let _ = s.try_send(event.id.0);
                }
            }));
            std::future::pending::<()>().await;
        })
    })
}
