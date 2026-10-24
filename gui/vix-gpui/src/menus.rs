//! The native menu bar, from the shared tree in `vix_gui_common::menu`.

use gpui_kit::{Menu, MenuItem};
use vix_gui_common::menu::{self, Node};

/// Run one Vix action by id (`file.save`, `view.theme`, …).
#[derive(Clone, PartialEq, Eq, Debug, gpui_kit::Action)]
#[action(namespace = vix, no_json)]
pub struct RunAction {
    /// The `App::run_action` id.
    pub id: &'static str,
}

/// Open files through the platform's native file dialog (GUI only).
#[derive(Clone, PartialEq, Eq, Debug, Default, gpui_kit::Action)]
#[action(namespace = vix, no_json)]
pub struct OpenNative;

/// The full native menu bar, labels translated into the active locale.
#[must_use]
pub fn native_menus() -> Vec<Menu> {
    menu::tree()
        .into_iter()
        .map(|m| Menu {
            name: m.title.into(),
            items: items(m.items),
            disabled: false,
        })
        .collect()
}

/// Convert one menu's nodes (recursing into submenus).
fn items(nodes: Vec<Node>) -> Vec<MenuItem> {
    nodes
        .into_iter()
        .map(|n| match n {
            Node::Action { label, id } => MenuItem::action(label, RunAction { id }),
            Node::OpenNative { label } => MenuItem::action(label, OpenNative),
            Node::Separator => MenuItem::separator(),
            Node::Submenu { label, items: sub } => MenuItem::submenu(Menu {
                name: label.into(),
                items: items(sub),
                disabled: false,
            }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(items: &[MenuItem]) -> usize {
        items
            .iter()
            .map(|it| match it {
                MenuItem::Submenu(m) => 1 + count(&m.items),
                _ => 1,
            })
            .sum()
    }

    fn count_nodes(nodes: &[Node]) -> usize {
        nodes
            .iter()
            .map(|n| match n {
                Node::Submenu { items, .. } => 1 + count_nodes(items),
                _ => 1,
            })
            .sum()
    }

    #[test]
    fn every_node_becomes_a_native_item() {
        let native = native_menus();
        let tree = menu::tree();
        assert_eq!(native.len(), tree.len());
        for (n, m) in native.iter().zip(&tree) {
            assert_eq!(count(&n.items), count_nodes(&m.items), "menu {}", m.title);
        }
    }
}
