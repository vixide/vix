//! The native menu bar, built from the same `vix-menu` model the terminal
//! menu bar draws. Every leaf dispatches [`RunAction`] carrying its action id,
//! which the window hands to `App::run_action` — one action, one
//! implementation, whichever menu bar it was chosen from.

use gpui_kit::{Menu, MenuItem};
use vix::menu::{Item, menus};

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
    menus()
        .iter()
        .map(|def| {
            let mut items = items(def.items);
            // The File menu (second, after the Vix application menu) gains the
            // native open dialog right after Vix's own Open….
            if def.name == "menu.file"
                && let Some(pos) = def.items.iter().position(|it| it.action == "file.open")
            {
                items.insert(
                    pos + 1,
                    MenuItem::action(t!("menu.item.file.open_native").to_string(), OpenNative),
                );
            }
            Menu {
                name: def.title().into(),
                items,
                disabled: false,
            }
        })
        .collect()
}

/// Convert one dropdown's entries (recursing into submenus).
fn items(entries: &[Item]) -> Vec<MenuItem> {
    entries
        .iter()
        .map(|it| {
            if it.is_separator() {
                MenuItem::separator()
            } else if let Some(sub) = it.submenu {
                MenuItem::submenu(Menu {
                    name: it.label().into(),
                    items: items(sub),
                    disabled: false,
                })
            } else {
                MenuItem::action(it.label(), RunAction { id: it.action })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf_ids(items: &[MenuItem], out: &mut Vec<&'static str>) {
        for it in items {
            match it {
                MenuItem::Action { action, .. } => {
                    if let Some(a) = action.as_any().downcast_ref::<RunAction>() {
                        out.push(a.id);
                    }
                }
                MenuItem::Submenu(m) => leaf_ids(&m.items, out),
                _ => {}
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
    fn every_model_leaf_becomes_a_native_item_in_order() {
        let native = native_menus();
        assert_eq!(native.len(), menus().len());
        for (n, def) in native.iter().zip(menus()) {
            let mut got = Vec::new();
            leaf_ids(&n.items, &mut got);
            let mut want = Vec::new();
            model_ids(def.items, &mut want);
            assert_eq!(got, want, "menu {}", def.name);
        }
    }

    #[test]
    fn file_menu_gains_the_native_open_dialog() {
        let native = native_menus();
        let (file, _) = native
            .iter()
            .zip(menus())
            .find(|(_, d)| d.name == "menu.file")
            .unwrap();
        assert!(file.items.iter().any(|it| matches!(
            it,
            MenuItem::Action { action, .. } if action.as_any().is::<OpenNative>()
        )));
    }
}
