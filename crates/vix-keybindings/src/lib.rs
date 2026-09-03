//! An exhaustive, queryable registry of every built-in keybinding, and (once
//! T104h–T104j land) the user/script override layer built on top of it.
//!
//! See `spec/index.md` for the audit and design this implements. Status
//! (T104e): the registry API is real — [`Binding`], [`ChordContext`],
//! [`KeymapTable`], [`lookup`], [`shortcuts_for`], [`lookup_sequence`]
//! (T104b, for a leader-style multi-character sequence table) — and six
//! keymaps are fully converted: Emacs (`emacs`, T104a), Vi (`vi`) and
//! Spacemacs (`spacemacs`, T104b), VS Code (`vscode-macos`/
//! `vscode-windows`, T104c — one shared table, since VS Code's bindings
//! don't differ by host OS in a terminal), `IntelliJ` (`intellij-macos`/
//! `intellij-windows`, T104d — two genuinely different tables this time,
//! unlike VS Code's shared one), and Eclipse (`eclipse`, T104e). Their
//! dispatch functions (`vim_normal_key`, `spacemacs_leader_lookup`,
//! `vscode_ctrl_key`, `intellij_key`, `eclipse_key`) all now go through
//! [`TABLES`] instead of their own hardcoded `match`/const. The remaining
//! two keymap ids (`apple`, `sublime`) have an empty table each, filled in
//! one per task (T104f–T104g). Nothing outside the six converted ids is
//! queryable yet — `lookup`/`lookup_sequence`/`shortcuts_for` simply
//! return nothing for them, same as an unrecognized token would.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

mod eclipse;
mod emacs;
mod intellij;
mod spacemacs;
mod vim;
mod vscode;

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
        contexts: vscode::CONTEXTS,
    },
    KeymapTable {
        keymap_id: "vscode-windows",
        contexts: vscode::CONTEXTS,
    },
    KeymapTable {
        keymap_id: "emacs",
        contexts: emacs::CONTEXTS,
    },
    KeymapTable {
        keymap_id: "vi",
        contexts: vim::CONTEXTS,
    },
    KeymapTable {
        keymap_id: "spacemacs",
        contexts: spacemacs::CONTEXTS,
    },
    KeymapTable {
        keymap_id: "intellij-macos",
        contexts: intellij::CONTEXTS_MACOS,
    },
    KeymapTable {
        keymap_id: "intellij-windows",
        contexts: intellij::CONTEXTS_WINDOWS,
    },
    KeymapTable {
        keymap_id: "eclipse",
        contexts: eclipse::CONTEXTS,
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

/// The result of matching a growing, multi-character sequence against a
/// context's bindings, for a keymap whose `key_token`s are whole typed
/// sequences (e.g. Spacemacs's `SPC`-leader — `spacemacs.rs`) rather than
/// one keypress each, so [`lookup`]'s exact match alone isn't the right
/// shape: the caller needs to know whether to keep accumulating too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceMatch {
    /// `seq` exactly matches one binding's `key_token` — run this action.
    Action(&'static str),
    /// `seq` is a strict prefix of at least one binding's `key_token` —
    /// not an action by itself, keep accumulating.
    Prefix,
    /// `seq` doesn't match or prefix anything in this context.
    None,
}

/// Match `seq` against `keymap_id`'s `context` the leader way: exact,
/// valid-prefix, or neither. Unlike [`lookup`], a miss on an unknown
/// keymap/context is indistinguishable from `SequenceMatch::None` — both
/// mean "nothing here", which is the right answer either way.
#[must_use]
pub fn lookup_sequence(keymap_id: &str, context: &str, seq: &str) -> SequenceMatch {
    let Some(bindings) = TABLES
        .iter()
        .find(|t| t.keymap_id == keymap_id)
        .and_then(|t| t.contexts.iter().find(|c| c.name == context))
        .map(|c| c.bindings)
    else {
        return SequenceMatch::None;
    };
    if let Some(b) = bindings.iter().find(|b| b.key_token == seq) {
        SequenceMatch::Action(b.action_id)
    } else if bindings.iter().any(|b| b.key_token.starts_with(seq)) {
        SequenceMatch::Prefix
    } else {
        SequenceMatch::None
    }
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
        // org.timestamp is bound in the emacs "C-c" context (.) only — no
        // Vim/Spacemacs binding does anything Org-specific.
        let hits = shortcuts_for("org.timestamp");
        assert_eq!(hits, vec![("emacs", "C-c", ".")]);
    }

    #[test]
    fn shortcuts_for_an_unbound_action_is_empty() {
        assert!(shortcuts_for("no.such.action").is_empty());
    }

    #[test]
    fn unpopulated_keymaps_return_nothing() {
        assert_eq!(lookup("sublime", "", "h"), None);
        assert!(
            shortcuts_for("edit.find")
                .iter()
                .all(|(id, ..)| *id != "sublime")
        );
    }

    #[test]
    fn lookup_finds_a_vim_normal_mode_binding() {
        assert_eq!(lookup("vi", "", "h"), Some("motion.char_left"));
    }

    #[test]
    fn lookup_distinguishes_vim_pending_operator_contexts() {
        // "g" alone (top level) starts a pending operator, handled host-side
        // — not a table entry there — but the *second* "g" of "gg" is.
        assert_eq!(lookup("vi", "", "g"), None);
        assert_eq!(lookup("vi", "g", "g"), Some("edit.go_first"));
        assert_eq!(lookup("vi", "d", "d"), Some("cut_line"));
        assert_eq!(lookup("vi", "y", "y"), Some("copy_line"));
        // A pending operator followed by anything else is simply not bound.
        assert_eq!(lookup("vi", "d", "x"), None);
    }

    #[test]
    fn spacemacs_has_no_normal_mode_table_of_its_own() {
        // Spacemacs delegates to the same vim_normal_key dispatch as Vi, so
        // its Normal-mode vocabulary lives under the "vi" id, not its own —
        // "spacemacs" only ever has its leader context (verified separately
        // by `lookup_sequence_matches_exactly_prefixes_or_neither`).
        assert_eq!(lookup("spacemacs", "", "h"), None);
    }

    #[test]
    fn lookup_finds_a_vscode_binding() {
        assert_eq!(lookup("vscode-macos", "", "C-p"), Some("file.open"));
    }

    #[test]
    fn vscode_distinguishes_ctrl_from_ctrl_shift_explicitly() {
        // "C-p" (Quick Open) and "C-S-p" (Command Palette) must not
        // collide, even though a terminal can report Ctrl+Shift+p as a
        // *lowercase* 'p' with the Shift bit set rather than an uppercase
        // 'P' — the reason this table encodes Shift explicitly instead of
        // relying on `vix_macros::encode_key`'s usual "implicit in an
        // uppercase char" rule (see `vscode.rs`'s module doc).
        assert_eq!(lookup("vscode-macos", "", "C-p"), Some("file.open"));
        assert_eq!(lookup("vscode-macos", "", "C-S-p"), Some("tools.palette"));
    }

    #[test]
    fn vscode_macos_and_windows_share_one_table() {
        assert_eq!(
            lookup("vscode-macos", "", "C-s"),
            lookup("vscode-windows", "", "C-s")
        );
    }

    #[test]
    fn intellij_macos_and_windows_are_genuinely_different_tables() {
        // Unlike VS Code, IntelliJ's "go to" family really differs by
        // platform: Ctrl+O/Ctrl+L on macOS, Ctrl+N/Ctrl+G on Windows.
        assert_eq!(lookup("intellij-macos", "", "C-o"), Some("nav.goto_symbol"));
        assert_eq!(lookup("intellij-windows", "", "C-o"), None);
        assert_eq!(
            lookup("intellij-windows", "", "C-n"),
            Some("nav.goto_symbol")
        );
        assert_eq!(lookup("intellij-macos", "", "C-n"), Some("file.new"));
        // But both share the platform-independent bindings.
        assert_eq!(
            lookup("intellij-macos", "", "C-A-l"),
            lookup("intellij-windows", "", "C-A-l")
        );
    }

    #[test]
    fn intellij_preserves_the_original_unguarded_shift_quirk() {
        // Neither the original macOS Ctrl+N arm nor the original Windows
        // Ctrl+G arm was Shift-guarded, so the Shift variant does the same
        // thing as the plain one on each platform (see `intellij.rs`'s
        // module doc) — not a bug this conversion introduces.
        assert_eq!(lookup("intellij-macos", "", "C-n"), Some("file.new"));
        assert_eq!(lookup("intellij-macos", "", "C-S-n"), Some("file.new"));
        assert_eq!(lookup("intellij-windows", "", "C-g"), Some("nav.goto_line"));
        assert_eq!(
            lookup("intellij-windows", "", "C-S-g"),
            Some("nav.goto_line")
        );
    }

    #[test]
    fn lookup_sequence_matches_exactly_prefixes_or_neither() {
        assert_eq!(
            lookup_sequence("spacemacs", "", "ff"),
            SequenceMatch::Action("file.open")
        );
        assert_eq!(lookup_sequence("spacemacs", "", "f"), SequenceMatch::Prefix);
        assert_eq!(lookup_sequence("spacemacs", "", "zz"), SequenceMatch::None);
    }

    #[test]
    fn lookup_sequence_on_an_unknown_keymap_or_context_is_none() {
        assert_eq!(
            lookup_sequence("does-not-exist", "", "ff"),
            SequenceMatch::None
        );
        assert_eq!(
            lookup_sequence("spacemacs", "no-such-context", "ff"),
            SequenceMatch::None
        );
    }

    #[test]
    fn lookup_finds_an_eclipse_ctrl_binding() {
        assert_eq!(lookup("eclipse", "", "C-f"), Some("edit.find"));
    }

    #[test]
    fn eclipse_distinguishes_ctrl_from_ctrl_shift_explicitly() {
        assert_eq!(lookup("eclipse", "", "C-w"), Some("file.close"));
        assert_eq!(lookup("eclipse", "", "C-S-w"), Some("file.close_all"));
    }

    #[test]
    fn eclipse_alt_slash_is_distinct_from_ctrl_slash() {
        // Alt+/ (word completion) and Ctrl+/ (toggle comment) are separate
        // rows — Alt is only examined when Ctrl is absent (see the module
        // doc's note on Ctrl+Alt+/ falling through to the Ctrl branch).
        assert_eq!(lookup("eclipse", "", "A-/"), Some("autocomplete"));
        assert_eq!(lookup("eclipse", "", "C-/"), Some("edit.toggle_comment"));
    }
}
