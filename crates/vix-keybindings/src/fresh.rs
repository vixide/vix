//! The Fresh keymap's built-in bindings, from Fresh's default keymap
//! (`crates/fresh-editor-core/keymaps/default.json` in
//! <https://github.com/sinelaw/fresh>, read on 2026-11-10).
//!
//! All-`Ctrl`, no chords: one flat `""` context, the same shape as the
//! Sublime and Zed tables, with Shift written out explicitly in each token.
//! `Ctrl+D` (Fresh's add cursor at next match) is deliberately unbound so it
//! reaches the editor's own `Ctrl+D`, which does the same.

use crate::{Binding, ChordContext};

/// Fresh's Ctrl-key bindings (its only context), with Shift encoded explicitly in the token.
const NORMAL: &[Binding] = &[
    // Fresh's Quick Open (files; Fresh also takes `>` for commands).
    Binding {
        key_token: "C-p",
        action_id: "file.open",
    },
    Binding {
        key_token: "C-e",
        action_id: "view.toggle_explorer_focus",
    },
    Binding {
        key_token: "C-b",
        action_id: "view.explorer",
    },
    Binding {
        key_token: "C-n",
        action_id: "file.new",
    },
    Binding {
        key_token: "C-o",
        action_id: "file.open_path",
    },
    Binding {
        key_token: "C-s",
        action_id: "file.save",
    },
    Binding {
        key_token: "C-q",
        action_id: "file.quit",
    },
    Binding {
        key_token: "C-f",
        action_id: "edit.find",
    },
    Binding {
        key_token: "C-r",
        action_id: "edit.replace",
    },
    Binding {
        key_token: "C-S-n",
        action_id: "edit.find_prev",
    },
    Binding {
        key_token: "C-g",
        action_id: "nav.goto_line",
    },
    Binding {
        key_token: "C-]",
        action_id: "edit.match_bracket",
    },
    Binding {
        key_token: "C-l",
        action_id: "edit.select_line",
    },
    Binding {
        key_token: "C-t",
        action_id: "edit.transpose_chars",
    },
    Binding {
        key_token: "C-k",
        action_id: "cut_to_line_end",
    },
    Binding {
        key_token: "C-w",
        action_id: "select_word",
    },
    Binding {
        key_token: "C-h",
        action_id: "delete_word_left",
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
    // Alt with punctuation or an arrow (Alt+letter stays a menu mnemonic).
    Binding {
        key_token: "A-Up",
        action_id: "edit.move_line_up",
    },
    Binding {
        key_token: "A-Down",
        action_id: "edit.move_line_down",
    },
    Binding {
        key_token: "A-.",
        action_id: "lsp.code_action",
    },
    Binding {
        key_token: "A-Left",
        action_id: "nav.back",
    },
    Binding {
        key_token: "A-Right",
        action_id: "nav.forward",
    },
    Binding {
        key_token: "A-/",
        action_id: "search.workspace",
    },
];

/// Fresh's chord contexts (a single top-level `""` context), wired into [`crate::TABLES`].
pub(crate) const CONTEXTS: &[ChordContext] = &[ChordContext {
    name: "",
    bindings: NORMAL,
}];
