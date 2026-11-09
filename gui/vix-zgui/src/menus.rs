//! The native menu bar (macOS), from the tree in `crate::menu_tree`, through
//! zgui's own application menu (`App::with_menu`, built with muda inside
//! zgui). Each item's id is its Vix action id, so a click comes back through
//! `zgui::desktop::on_menu` as the id to hand to `App::run_action`.

use std::path::PathBuf;

use zgui::desktop::{AppMenu, MenuAction, MenuEntry, Submenu};

use crate::menu_tree::{self as menu, Node};
use crate::model::Model;

/// The id of **Open with System Dialog…**.
pub const OPEN_NATIVE: &str = "vix-gui.open-native";

/// One menu's nodes as zgui entries (recursing into nested submenus).
fn fill(mut sub: Submenu, nodes: Vec<Node>) -> Submenu {
    for node in nodes {
        sub = match node {
            Node::Action { label, id } => sub.with(MenuEntry::Action(MenuAction::new(id, label))),
            Node::OpenNative { label } => {
                sub.with(MenuEntry::Action(MenuAction::new(OPEN_NATIVE, label)))
            }
            Node::Separator => sub.separator(),
            Node::Submenu { label, items } => {
                sub.with(MenuEntry::Submenu(fill(Submenu::new(label), items)))
            }
        };
    }
    sub
}

/// The menu bar: one zgui submenu per Vix menu.
#[must_use]
pub fn bar() -> AppMenu {
    menu::tree().into_iter().fold(AppMenu::new(), |bar, m| {
        bar.with(fill(Submenu::new(m.title), m.items))
    })
}

/// Act on a clicked item: the system file dialog, or the action id.
pub fn clicked(model: &mut Model, id: &str) {
    if id == OPEN_NATIVE {
        let paths: Vec<PathBuf> = rfd::FileDialog::new().pick_files().unwrap_or_default();
        model.open_files(&paths);
        return;
    }
    model.run_action(id);
}
