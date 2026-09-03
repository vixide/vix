//! The `IntelliJ` IDEA keymap's built-in bindings (`src/app.rs`'s
//! `intellij_key`), converted from that function's former hardcoded
//! `match`.
//!
//! Unlike VS Code (T104c, one shared table for both host OSes),
//! `intellij-macos` and `intellij-windows` genuinely differ — `IntelliJ`'s
//! own "go to" family uses `Ctrl+O`/`Ctrl+Shift+O`/`Ctrl+L` on macOS but
//! `Ctrl+N`/`Ctrl+Shift+N`/`Ctrl+G` on Windows, plus a few other
//! platform-only bindings (`Ctrl+,` for Settings on macOS,
//! `Ctrl+Y` delete-line on Windows) — so this crate holds two real
//! tables, each self-contained (the ~13 bindings both platforms share are
//! duplicated across them, plain data, not worth a shared-slice
//! indirection for this few).
//!
//! No chords, all-Ctrl (plus one Ctrl+Alt pair, `Ctrl+Alt+L`/`Ctrl+Alt+O`
//! — a single keystroke's modifier combination, not a chord prefix, so it
//! lives in the same `""` context as everything else), so one context per
//! table, exactly like VS Code. Every token that needs `Shift` encodes it
//! explicitly (T104c's subtlety applies here too — the original dispatch
//! checks the Shift modifier bit, not letter case).
//!
//! One faithfully-preserved original quirk, not a bug this conversion
//! introduces: neither platform's `Ctrl+N` binding is guarded by Shift at
//! all in the source — macOS's plain `'n' if !win => file.new` fires for
//! `Ctrl+N` *and* `Ctrl+Shift+N` alike (no separate Shift-aware arm), and
//! Windows's `'g' if win => nav.goto_line` fires for `Ctrl+G` *and*
//! `Ctrl+Shift+G` alike. Both tables list the Shift variant as an
//! explicit second `Binding` row rather than silently dropping it.

#![warn(clippy::pedantic)]

use crate::{Binding, ChordContext};

const MACOS: &[Binding] = &[
    Binding {
        key_token: "C-A-l",
        action_id: "lsp.format",
    },
    Binding {
        key_token: "C-A-o",
        action_id: "nav.goto_workspace_symbol",
    },
    Binding {
        key_token: "C-S-a",
        action_id: "tools.palette",
    },
    Binding {
        key_token: "C-S-s",
        action_id: "file.save_as",
    },
    Binding {
        key_token: "C-s",
        action_id: "file.save",
    },
    Binding {
        key_token: "C-S-w",
        action_id: "file.close_all",
    },
    Binding {
        key_token: "C-w",
        action_id: "file.close",
    },
    Binding {
        key_token: "C-S-f",
        action_id: "search.workspace",
    },
    Binding {
        key_token: "C-f",
        action_id: "edit.find",
    },
    Binding {
        key_token: "C-S-r",
        action_id: "search.workspace_replace",
    },
    Binding {
        key_token: "C-r",
        action_id: "edit.replace",
    },
    Binding {
        key_token: "C-b",
        action_id: "nav.goto_definition",
    },
    Binding {
        key_token: "C-d",
        action_id: "edit.duplicate_line",
    },
    Binding {
        key_token: "C-/",
        action_id: "edit.toggle_comment",
    },
    Binding {
        key_token: "C-7",
        action_id: "edit.toggle_comment",
    },
    Binding {
        key_token: "C-_",
        action_id: "edit.toggle_comment",
    },
    Binding {
        key_token: "C-,",
        action_id: "vix.settings",
    },
    // Not Shift-guarded in the original — see the module doc's "one
    // faithfully-preserved quirk".
    Binding {
        key_token: "C-n",
        action_id: "file.new",
    },
    Binding {
        key_token: "C-S-n",
        action_id: "file.new",
    },
    Binding {
        key_token: "C-S-o",
        action_id: "file.open",
    },
    Binding {
        key_token: "C-o",
        action_id: "nav.goto_symbol",
    },
    Binding {
        key_token: "C-l",
        action_id: "nav.goto_line",
    },
    Binding {
        key_token: "C-S-g",
        action_id: "edit.find_prev",
    },
    Binding {
        key_token: "C-g",
        action_id: "edit.find_next",
    },
];

const WINDOWS: &[Binding] = &[
    Binding {
        key_token: "C-A-l",
        action_id: "lsp.format",
    },
    Binding {
        key_token: "C-A-o",
        action_id: "nav.goto_workspace_symbol",
    },
    Binding {
        key_token: "C-S-a",
        action_id: "tools.palette",
    },
    Binding {
        key_token: "C-S-s",
        action_id: "file.save_as",
    },
    Binding {
        key_token: "C-s",
        action_id: "file.save",
    },
    Binding {
        key_token: "C-S-w",
        action_id: "file.close_all",
    },
    Binding {
        key_token: "C-w",
        action_id: "file.close",
    },
    Binding {
        key_token: "C-S-f",
        action_id: "search.workspace",
    },
    Binding {
        key_token: "C-f",
        action_id: "edit.find",
    },
    Binding {
        key_token: "C-S-r",
        action_id: "search.workspace_replace",
    },
    Binding {
        key_token: "C-r",
        action_id: "edit.replace",
    },
    Binding {
        key_token: "C-b",
        action_id: "nav.goto_definition",
    },
    Binding {
        key_token: "C-d",
        action_id: "edit.duplicate_line",
    },
    Binding {
        key_token: "C-y",
        action_id: "edit.delete_line",
    },
    Binding {
        key_token: "C-/",
        action_id: "edit.toggle_comment",
    },
    Binding {
        key_token: "C-7",
        action_id: "edit.toggle_comment",
    },
    Binding {
        key_token: "C-_",
        action_id: "edit.toggle_comment",
    },
    Binding {
        key_token: "C-S-n",
        action_id: "file.open",
    },
    Binding {
        key_token: "C-n",
        action_id: "nav.goto_symbol",
    },
    // Not Shift-guarded in the original — see the module doc's "one
    // faithfully-preserved quirk".
    Binding {
        key_token: "C-g",
        action_id: "nav.goto_line",
    },
    Binding {
        key_token: "C-S-g",
        action_id: "nav.goto_line",
    },
];

pub(crate) const CONTEXTS_MACOS: &[ChordContext] = &[ChordContext {
    name: "",
    bindings: MACOS,
}];

pub(crate) const CONTEXTS_WINDOWS: &[ChordContext] = &[ChordContext {
    name: "",
    bindings: WINDOWS,
}];
