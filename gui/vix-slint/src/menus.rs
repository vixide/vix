//! The native menu bar, from the tree in `crate::menu_tree`, as Slint
//! markup. Slint menus are declared in `.slint` rather than built through an
//! API, and the window's markup is compiled at startup anyway (`view.rs`),
//! so the menu bar is generated as literal `Menu` / `MenuItem` /
//! `MenuSeparator` elements in the model's exact order. Every leaf calls the
//! window's `menu-activated` callback with its Vix action id, which goes to
//! `App::run_action`.
//!
//! Slint shows a `MenuBar` natively on macOS and Windows (through its own
//! `muda` integration); elsewhere it would draw one inside the window above
//! the grid, duplicating Vix's own menu bar, so there the window's
//! `native-menus` property is false and the menu bar is left out.

use std::fmt::Write as _;

use crate::menu_tree::{self, Node};

/// The action id of **Open with System Dialog…**.
pub const OPEN_NATIVE: &str = "vix-gui.open-native";

/// Whether this platform shows a Slint `MenuBar` natively.
pub const NATIVE: bool = cfg!(any(target_os = "macos", target_os = "windows"));

/// `s` as a Slint string literal. Slint strings interpolate `\{…}`, so only
/// the backslash, the quote and newlines need escaping.
#[must_use]
pub fn literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A menu label as a Slint literal. The native menus go through `muda`,
/// which reads `&` as a mnemonic marker; double it to show it literally.
fn label(s: &str) -> String {
    literal(&s.replace('&', "&&"))
}

/// One leaf: a `MenuItem` that hands `id` to `menu-activated`.
fn item(out: &mut String, text: &str, id: &str) {
    let _ = writeln!(
        out,
        "MenuItem {{ title: {}; activated => {{ root.menu-activated({}); }} }}",
        label(text),
        literal(id)
    );
}

/// A menu's entries (recursing into submenus).
fn entries(out: &mut String, nodes: &[Node]) {
    for node in nodes {
        match node {
            Node::Action { label: text, id } => item(out, text, id),
            Node::OpenNative { label: text } => item(out, text, OPEN_NATIVE),
            Node::Separator => out.push_str("MenuSeparator {}\n"),
            Node::Submenu { label: text, items } => {
                let _ = writeln!(out, "Menu {{ title: {};", label(text));
                entries(out, items);
                out.push_str("}\n");
            }
        }
    }
}

/// The `MenuBar` element for the window, labels translated into the active
/// locale (call after the locale is set). Shown only when the window's
/// `native-menus` property is true.
#[must_use]
pub fn markup() -> String {
    let mut out = String::from("if root.native-menus: MenuBar {\n");
    for menu in menu_tree::tree() {
        let _ = writeln!(out, "Menu {{ title: {};", label(&menu.title));
        entries(&mut out, &menu.items);
        out.push_str("}\n");
    }
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf_ids(items: &[Node], out: &mut Vec<String>) {
        for it in items {
            match it {
                Node::Action { id, .. } => out.push(literal(id)),
                Node::OpenNative { .. } => out.push(literal(OPEN_NATIVE)),
                Node::Submenu { items, .. } => leaf_ids(items, out),
                Node::Separator => {}
            }
        }
    }

    #[test]
    fn literals_escape_quotes_backslashes_and_newlines() {
        assert_eq!(literal("Open…"), "\"Open…\"");
        assert_eq!(literal(r#"say "hi" \{x}"#), r#""say \"hi\" \\{x}""#);
        assert_eq!(literal("a\nb"), "\"a\\nb\"");
    }

    #[test]
    fn every_leaf_dispatches_its_action_in_order() {
        let markup = markup();
        let got: Vec<&str> = markup
            .split("root.menu-activated(")
            .skip(1)
            .filter_map(|s| s.split_once(");").map(|(id, _)| id))
            .collect();
        let mut want = Vec::new();
        for m in menu_tree::tree() {
            leaf_ids(&m.items, &mut want);
        }
        assert_eq!(got, want);
        assert!(got.contains(&literal(OPEN_NATIVE).as_str()));
    }
}
