//! The native menu bar (macOS), from the tree in `crate::menu_tree`, through
//! `muda` — BUI has no menus an app can fill. Each item's muda id is its Vix
//! action id, so a click comes back from [`clicked`] as the id to run.

use crate::menu_tree::{self as menu, Node};
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

/// Build the menu bar and install it as the app's. Call once the window has
/// opened, so `NSApp` exists and this replaces BUI's small default bar rather
/// than being replaced by it.
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

/// The ids of menu items clicked since the last call.
#[must_use]
pub fn clicked() -> Vec<String> {
    MenuEvent::receiver().try_iter().map(|e| e.id.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ampersand_shows_literally() {
        assert_eq!(label("Find & Replace"), "Find && Replace");
    }
}
