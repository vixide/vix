//! The native menu bar's contents, built from the same `vix-menu` model the
//! terminal menu bar draws, as a toolkit-neutral tree. Each front end turns
//! it into its own native menu type; every leaf runs its action id through
//! `App::run_action` — one action, one implementation, whichever menu bar it
//! was chosen from.

use vix::menu::{Item, menus};

/// One entry of a native menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// Runs `App::run_action(id)`.
    Action {
        /// Translated label.
        label: String,
        /// The action id.
        id: &'static str,
    },
    /// Opens the platform's file dialog (GUI only).
    OpenNative {
        /// Translated label.
        label: String,
    },
    /// A divider.
    Separator,
    /// A nested menu.
    Submenu {
        /// Translated label.
        label: String,
        /// Its entries.
        items: Vec<Node>,
    },
}

/// One top-level menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    /// Translated title.
    pub title: String,
    /// Its entries.
    pub items: Vec<Node>,
}

/// The full native menu bar, labels translated into the active locale. The
/// File menu gains **Open with System Dialog…** right after Vix's own Open….
#[must_use]
pub fn tree() -> Vec<Menu> {
    menus()
        .iter()
        .map(|def| {
            let mut items = nodes(def.items);
            if def.name == "menu.file"
                && let Some(pos) = def.items.iter().position(|it| it.action == "file.open")
            {
                items.insert(
                    pos + 1,
                    Node::OpenNative {
                        label: t!("menu.item.file.open_native").to_string(),
                    },
                );
            }
            Menu {
                title: def.title(),
                items,
            }
        })
        .collect()
}

/// Convert one dropdown's entries (recursing into submenus).
fn nodes(entries: &[Item]) -> Vec<Node> {
    entries
        .iter()
        .map(|it| {
            if it.is_separator() {
                Node::Separator
            } else if let Some(sub) = it.submenu {
                Node::Submenu {
                    label: it.label(),
                    items: nodes(sub),
                }
            } else {
                Node::Action {
                    label: it.label(),
                    id: it.action,
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf_ids(items: &[Node], out: &mut Vec<&'static str>) {
        for it in items {
            match it {
                Node::Action { id, .. } => out.push(id),
                Node::Submenu { items, .. } => leaf_ids(items, out),
                Node::OpenNative { .. } | Node::Separator => {}
            }
        }
    }

    fn model_ids(items: &[Item], out: &mut Vec<&'static str>) {
        for it in items {
            if let Some(sub) = it.submenu {
                model_ids(sub, out);
            } else if !it.is_separator() {
                out.push(it.action);
            }
        }
    }

    #[test]
    fn every_model_leaf_becomes_a_node_in_order() {
        let tree = tree();
        assert_eq!(tree.len(), menus().len());
        for (m, def) in tree.iter().zip(menus()) {
            let mut got = Vec::new();
            leaf_ids(&m.items, &mut got);
            let mut want = Vec::new();
            model_ids(def.items, &mut want);
            assert_eq!(got, want, "menu {}", def.name);
        }
    }

    #[test]
    fn file_menu_gains_the_native_open_dialog() {
        let tree = tree();
        let (file, _) = tree
            .iter()
            .zip(menus())
            .find(|(_, d)| d.name == "menu.file")
            .unwrap();
        assert!(
            file.items
                .iter()
                .any(|n| matches!(n, Node::OpenNative { .. }))
        );
    }
}
