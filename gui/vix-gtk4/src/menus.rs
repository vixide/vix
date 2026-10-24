//! The native menu bar, from the tree in `crate::menu_tree`, as a GIO menu
//! model (`gtk::Application::set_menubar`). Every leaf targets one
//! application action, `app.run`, with its Vix action id as the string
//! parameter, so a click hands that id to `App::run_action`; **Open with
//! System Dialog…** targets `app.open-native`.

use gtk4::gio;
use gtk4::prelude::*;

use crate::menu_tree::{self as menu, Node};

/// The application action every leaf runs (parameter: the action id).
pub const RUN: &str = "run";

/// The application action behind **Open with System Dialog…**.
pub const OPEN_NATIVE: &str = "open-native";

/// GIO reads `_` as a mnemonic marker; show it literally instead.
fn label(s: &str) -> String {
    s.replace('_', "__")
}

/// The full menu bar, labels translated into the active locale.
#[must_use]
pub fn menubar() -> gio::Menu {
    let bar = gio::Menu::new();
    for m in menu::tree() {
        bar.append_submenu(Some(&label(&m.title)), &dropdown(m.items));
    }
    bar
}

/// One dropdown: its entries in sections, since GIO draws a separator
/// between sections rather than as an item.
fn dropdown(nodes: Vec<Node>) -> gio::Menu {
    let out = gio::Menu::new();
    let mut section = gio::Menu::new();
    for node in nodes {
        match node {
            Node::Action { label: l, id } => {
                let item = gio::MenuItem::new(Some(&label(&l)), None);
                item.set_action_and_target_value(
                    Some(&format!("app.{RUN}")),
                    Some(&id.to_variant()),
                );
                section.append_item(&item);
            }
            Node::OpenNative { label: l } => {
                section.append(Some(&label(&l)), Some(&format!("app.{OPEN_NATIVE}")));
            }
            Node::Separator => {
                if section.n_items() > 0 {
                    out.append_section(None, &section);
                    section = gio::Menu::new();
                }
            }
            Node::Submenu { label: l, items } => {
                section.append_submenu(Some(&label(&l)), &dropdown(items));
            }
        }
    }
    if section.n_items() > 0 {
        out.append_section(None, &section);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Leaves (actions, the native-dialog item, submenus) in a GIO menu,
    /// looking through its sections.
    fn count(model: &gio::MenuModel) -> usize {
        (0..model.n_items())
            .map(|i| {
                if let Some(sec) = model.item_link(i, gio::MENU_LINK_SECTION) {
                    count(&sec)
                } else if let Some(sub) = model.item_link(i, gio::MENU_LINK_SUBMENU) {
                    1 + count(&sub)
                } else {
                    1
                }
            })
            .sum()
    }

    fn count_nodes(nodes: &[Node]) -> usize {
        nodes
            .iter()
            .map(|n| match n {
                Node::Submenu { items, .. } => 1 + count_nodes(items),
                Node::Separator => 0,
                _ => 1,
            })
            .sum()
    }

    #[test]
    fn every_node_becomes_a_menu_item() {
        let bar = menubar();
        let tree = menu::tree();
        assert_eq!(usize::try_from(bar.n_items()).ok(), Some(tree.len()));
        for (i, m) in (0..bar.n_items()).zip(&tree) {
            let sub = bar.item_link(i, gio::MENU_LINK_SUBMENU).unwrap();
            assert_eq!(count(&sub), count_nodes(&m.items), "menu {}", m.title);
        }
    }

    #[test]
    fn underscores_are_not_mnemonics() {
        assert_eq!(label("snake_case"), "snake__case");
    }
}
