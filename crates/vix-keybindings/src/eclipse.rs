//! The Eclipse keymap's built-in bindings (`src/app.rs`'s `eclipse_key`),
//! converted from that function's former hardcoded `match`.
//!
//! All-`Ctrl` (plus one `Alt`-only binding, word completion), no chords —
//! same "one flat `""` context" shape as VS Code and `IntelliJ`. The same
//! Shift-bit-vs-char-case subtlety from T104c/T104d applies here too: the
//! original dispatch checks the Shift *modifier bit* via `Self::shift`, not
//! letter case, so every Shift-requiring token is written out explicitly
//! (`"C-S-w"`, not relying on an uppercase char).
//!
//! `Alt+/` (word completion) is the one binding that isn't a `Ctrl` chord —
//! the original dispatch treats it as its own leading case, matched only
//! when `Ctrl` is *not* also held. `Ctrl+Alt+/` therefore falls through to
//! the `Ctrl` branch below and hits `edit.toggle_comment`, same as plain
//! `Ctrl+/` — `Alt` is simply not examined once `Ctrl` is present. This
//! table preserves that: `"A-/"` is a distinct row from `"C-/"`, and
//! `App::eclipse_token` builds the right one for the right modifier
//! combination.

#![warn(clippy::pedantic)]

use crate::{Binding, ChordContext};

const NORMAL: &[Binding] = &[
    Binding {
        key_token: "A-/",
        action_id: "autocomplete",
    },
    Binding {
        key_token: "C-n",
        action_id: "file.new",
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
        key_token: "C-S-s",
        action_id: "file.save_as",
    },
    Binding {
        key_token: "C-s",
        action_id: "file.save",
    },
    Binding {
        key_token: "C-y",
        action_id: "edit.redo",
    },
    Binding {
        key_token: "C-S-f",
        action_id: "lsp.format",
    },
    Binding {
        key_token: "C-f",
        action_id: "edit.find",
    },
    Binding {
        key_token: "C-S-k",
        action_id: "edit.find_prev",
    },
    Binding {
        key_token: "C-k",
        action_id: "edit.find_next",
    },
    Binding {
        key_token: "C-h",
        action_id: "search.workspace",
    },
    Binding {
        key_token: "C-l",
        action_id: "nav.goto_line",
    },
    Binding {
        key_token: "C-d",
        action_id: "edit.delete_line",
    },
    Binding {
        key_token: "C-o",
        action_id: "nav.goto_symbol",
    },
    Binding {
        key_token: "C-S-r",
        action_id: "file.open",
    },
    Binding {
        key_token: "C-r",
        action_id: "edit.replace",
    },
    Binding {
        key_token: "C-S-t",
        action_id: "nav.goto_workspace_symbol",
    },
    Binding {
        key_token: "C-S-b",
        action_id: "run.toggle_breakpoint",
    },
    Binding {
        key_token: "C-b",
        action_id: "tools.test",
    },
    Binding {
        key_token: "C-3",
        action_id: "tools.palette",
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
];

pub(crate) const CONTEXTS: &[ChordContext] = &[ChordContext {
    name: "",
    bindings: NORMAL,
}];
