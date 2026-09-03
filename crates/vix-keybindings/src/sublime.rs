//! The Sublime Text keymap's built-in bindings (`src/app.rs`'s
//! `sublime_key`), converted from that function's former hardcoded
//! `match`.
//!
//! All-`Ctrl`, no chords — one flat `""` context, same shape as VS Code,
//! `IntelliJ`, and Eclipse. The now-familiar Shift-bit-vs-char-case
//! subtlety (T104c/T104d/T104e) applies here too: the original dispatch
//! checks the Shift *modifier bit* via `Self::shift`, not letter case, so
//! every Shift-requiring token is written out explicitly (`"C-S-p"`, not
//! relying on an uppercase char).

#![warn(clippy::pedantic)]

use crate::{Binding, ChordContext};

const NORMAL: &[Binding] = &[
    Binding {
        key_token: "C-S-p",
        action_id: "tools.palette",
    },
    Binding {
        key_token: "C-p",
        action_id: "file.open",
    },
    Binding {
        key_token: "C-r",
        action_id: "nav.goto_symbol",
    },
    Binding {
        key_token: "C-g",
        action_id: "nav.goto_line",
    },
    Binding {
        key_token: "C-S-d",
        action_id: "edit.duplicate_line",
    },
    // Nearest to Sublime's add-next-occurrence: a caret on every match.
    Binding {
        key_token: "C-d",
        action_id: "edit.select_all_occurrences",
    },
    Binding {
        key_token: "C-l",
        action_id: "edit.select_line",
    },
    Binding {
        key_token: "C-j",
        action_id: "edit.join_lines",
    },
    Binding {
        key_token: "C-m",
        action_id: "edit.match_bracket",
    },
    Binding {
        key_token: "C-S-k",
        action_id: "cut_line",
    },
    Binding {
        key_token: "C-h",
        action_id: "edit.replace",
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
        key_token: "C-b",
        action_id: "tools.test",
    },
    Binding {
        key_token: "C-`",
        action_id: "tools.terminal",
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
        key_token: "C-S-t",
        action_id: "file.reopen_closed",
    },
    // Many terminals emit the same control byte (0x1F) for Ctrl+/,
    // Ctrl+7, and Ctrl+_, so accept all three for Comment.
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
