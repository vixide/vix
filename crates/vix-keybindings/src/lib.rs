//! An exhaustive, queryable registry of every built-in keybinding, and (once
//! T104h–T104j land) the user/script override layer built on top of it.
//!
//! See `spec/index.md` for the audit and design this implements. Status
//! (T104a): the registry API is real — [`Binding`], [`ChordContext`],
//! [`KeymapTable`], [`lookup`], [`shortcuts_for`] — and one keymap, Emacs
//! (`emacs`), is fully converted: `emacs_key` and its five
//! chord-continuation handlers now dispatch through [`TABLES`] instead of
//! their own hardcoded `match`. The other nine keymap ids have an empty
//! table each, filled in one per task (T104b–T104g). Nothing outside
//! `emacs` is queryable yet — `lookup`/`shortcuts_for` simply return
//! nothing for them, same as an unrecognized token would.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

mod emacs;

/// One key binding within a single chord context: a
/// [`vix-macros`](https://docs.rs/vix-macros) token (`C-`/`A-`/`S-`
/// prefixes, e.g. `C-c`, `S-Tab`, `Enter`, `a`) and the action id it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    /// The key token, in `vix-macros`' grammar.
    pub key_token: &'static str,
    /// The `App::run_action`-dispatchable id this key runs.
    pub action_id: &'static str,
}

/// One dispatch depth within a keymap: the top level (`""`), or a specific
/// chord prefix already typed, named by the key tokens typed to reach it,
/// space-joined (e.g. `"C-x"`, `"C-c C-x"`). A non-chorded keymap (most of
/// them) has exactly one context, `""`.
#[derive(Debug, Clone, Copy)]
pub struct ChordContext {
    /// This context's name (`""` for the top level).
    pub name: &'static str,
    /// Every binding reachable at this depth, in no particular order.
    pub bindings: &'static [Binding],
}

/// Every built-in binding for one keymap, keyed on
/// `vix_keymap_model::Keymap::id` (10 values — `vscode-macos` and
/// `vscode-windows` get identical tables, since VS Code's bindings don't
/// differ by host OS in a terminal; deliberate duplication over inventing a
/// second, coarser keymap enum just to avoid it).
#[derive(Debug, Clone, Copy)]
pub struct KeymapTable {
    /// The keymap this table belongs to (`vix_keymap_model::Keymap::id`).
    pub keymap_id: &'static str,
    /// Every chord context this keymap defines.
    pub contexts: &'static [ChordContext],
}

/// Every keymap's table. One entry per `vix_keymap_model::KEYMAPS` id;
/// still-empty tables are filled in by their own task (see each variant's
/// comment) and are simply never matched by [`lookup`]/[`shortcuts_for`]
/// in the meantime.
pub const TABLES: &[KeymapTable] = &[
    KeymapTable {
        keymap_id: "apple",
        contexts: &[], // T104g
    },
    KeymapTable {
        keymap_id: "vscode-macos",
        contexts: &[], // T104c
    },
    KeymapTable {
        keymap_id: "vscode-windows",
        contexts: &[], // T104c
    },
    KeymapTable {
        keymap_id: "emacs",
        contexts: emacs::CONTEXTS,
    },
    KeymapTable {
        keymap_id: "vi",
        contexts: &[], // T104b
    },
    KeymapTable {
        keymap_id: "spacemacs",
        contexts: &[], // T104b
    },
    KeymapTable {
        keymap_id: "intellij-macos",
        contexts: &[], // T104d
    },
    KeymapTable {
        keymap_id: "intellij-windows",
        contexts: &[], // T104d
    },
    KeymapTable {
        keymap_id: "eclipse",
        contexts: &[], // T104e
    },
    KeymapTable {
        keymap_id: "sublime",
        contexts: &[], // T104f
    },
];

/// The action bound to `token` in `keymap_id`'s `context` (top level =
/// `""`), if any.
#[must_use]
pub fn lookup(keymap_id: &str, context: &str, token: &str) -> Option<&'static str> {
    TABLES
        .iter()
        .find(|t| t.keymap_id == keymap_id)
        .and_then(|t| t.contexts.iter().find(|c| c.name == context))
        .and_then(|c| c.bindings.iter().find(|b| b.key_token == token))
        .map(|b| b.action_id)
}

/// Every `(keymap_id, context, key_token)` bound to `action_id`, across
/// every keymap — feeds a help overlay in place of hand-walking each
/// keymap's own tables.
#[must_use]
pub fn shortcuts_for(action_id: &str) -> Vec<(&'static str, &'static str, &'static str)> {
    let mut out = Vec::new();
    for table in TABLES {
        for ctx in table.contexts {
            for b in ctx.bindings {
                if b.action_id == action_id {
                    out.push((table.keymap_id, ctx.name, b.key_token));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_keymap_id_has_exactly_one_table() {
        let ids = [
            "apple",
            "vscode-macos",
            "vscode-windows",
            "emacs",
            "vi",
            "spacemacs",
            "intellij-macos",
            "intellij-windows",
            "eclipse",
            "sublime",
        ];
        for id in ids {
            assert_eq!(
                TABLES.iter().filter(|t| t.keymap_id == id).count(),
                1,
                "keymap id {id} should have exactly one table"
            );
        }
    }

    #[test]
    fn lookup_finds_a_top_level_emacs_binding() {
        assert_eq!(lookup("emacs", "", "C-s"), Some("edit.find"));
    }

    #[test]
    fn lookup_distinguishes_contexts() {
        // "b" means different things at the top level (nothing) vs. inside
        // the C-x chord (switch buffer).
        assert_eq!(lookup("emacs", "", "b"), None);
        assert_eq!(lookup("emacs", "C-x", "b"), Some("nav.switch_buffer"));
    }

    #[test]
    fn lookup_returns_none_for_an_unknown_keymap_or_token() {
        assert_eq!(lookup("does-not-exist", "", "C-s"), None);
        assert_eq!(lookup("emacs", "", "C-does-not-exist"), None);
        assert_eq!(lookup("emacs", "no-such-context", "C-s"), None);
    }

    #[test]
    fn shortcuts_for_finds_every_binding_of_an_action() {
        // file.quit is bound in the emacs "C-x" context (C-c) only.
        let hits = shortcuts_for("file.quit");
        assert_eq!(hits, vec![("emacs", "C-x", "C-c")]);
    }

    #[test]
    fn shortcuts_for_an_unbound_action_is_empty() {
        assert!(shortcuts_for("no.such.action").is_empty());
    }

    #[test]
    fn unpopulated_keymaps_return_nothing() {
        assert_eq!(lookup("vi", "", "h"), None);
        assert!(
            shortcuts_for("edit.find")
                .iter()
                .all(|(id, ..)| *id != "vi")
        );
    }
}
