//! The ttt keymap's built-in bindings, from ttt's default keybindings
//! (`config/keybindings.json` in <https://github.com/eugenioenko/ttt>,
//! read on 2026-11-10).
//!
//! A flat `""` context of `Ctrl` keys, plus two chord contexts for ttt's
//! two-key `Ctrl+K …` and `Ctrl+L …` (language) families. The host
//! (`tui/src/app/keymap.rs`'s `table_ctrl_key`) treats `Ctrl+K`/`Ctrl+L` as
//! a prefix because those contexts exist, then looks the next key up in
//! the matching context. `Ctrl+D` (ttt's select next match) is
//! deliberately unbound so it reaches the editor's own `Ctrl+D`.

use crate::{Binding, ChordContext};

/// ttt's top-level `Ctrl` bindings, with Shift encoded explicitly in the token.
const NORMAL: &[Binding] = &[
    // ttt's Ctrl+P is its command palette; files are Ctrl+K P.
    Binding {
        key_token: "C-p",
        action_id: "tools.palette",
    },
    Binding {
        key_token: "C-b",
        action_id: "view.explorer",
    },
    Binding {
        key_token: "C-0",
        action_id: "view.toggle_explorer_focus",
    },
    Binding {
        key_token: "C-n",
        action_id: "file.new",
    },
    Binding {
        key_token: "C-s",
        action_id: "file.save",
    },
    Binding {
        key_token: "C-w",
        action_id: "file.close",
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
        key_token: "C-g",
        action_id: "nav.goto_line",
    },
    Binding {
        key_token: "C-u",
        action_id: "lsp.complete",
    },
    Binding {
        key_token: "C-.",
        action_id: "lsp.code_action",
    },
    Binding {
        key_token: "C-t",
        action_id: "tools.terminal",
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

/// The second key of ttt's `Ctrl+K …` chords.
const CTRL_K: &[Binding] = &[
    Binding {
        key_token: "p",
        action_id: "file.open",
    },
    Binding {
        key_token: "e",
        action_id: "view.explorer",
    },
    Binding {
        key_token: "f",
        action_id: "search.workspace",
    },
    Binding {
        key_token: "r",
        action_id: "search.workspace_replace",
    },
    Binding {
        key_token: "c",
        action_id: "git.changes",
    },
    Binding {
        key_token: "t",
        action_id: "tools.terminal",
    },
    Binding {
        key_token: "s",
        action_id: "file.save_as",
    },
    Binding {
        key_token: "i",
        action_id: "lsp.hover",
    },
    Binding {
        key_token: "l",
        action_id: "edit.select_all_occurrences",
    },
    Binding {
        key_token: "k",
        action_id: "edit.delete_line",
    },
    Binding {
        key_token: "o",
        action_id: "edit.sort_lines",
    },
    Binding {
        key_token: "m",
        action_id: "edit.match_bracket",
    },
    Binding {
        key_token: "b",
        action_id: "view.bottom_dock",
    },
    Binding {
        key_token: "j",
        action_id: "edit.join_lines",
    },
    Binding {
        key_token: "y",
        action_id: "help.shortcuts",
    },
    Binding {
        key_token: ",",
        action_id: "vix.settings",
    },
];

/// The second key of ttt's `Ctrl+L …` (language) chords.
const CTRL_L: &[Binding] = &[
    Binding {
        key_token: "i",
        action_id: "nav.goto_implementation",
    },
    Binding {
        key_token: "f",
        action_id: "lsp.format",
    },
    Binding {
        key_token: "r",
        action_id: "lsp.references",
    },
    Binding {
        key_token: "t",
        action_id: "nav.goto_type_definition",
    },
    Binding {
        key_token: "o",
        action_id: "lsp.organize_imports",
    },
];

/// ttt's chord contexts, wired into [`crate::TABLES`].
pub(crate) const CONTEXTS: &[ChordContext] = &[
    ChordContext {
        name: "",
        bindings: NORMAL,
    },
    ChordContext {
        name: "C-k",
        bindings: CTRL_K,
    },
    ChordContext {
        name: "C-l",
        bindings: CTRL_L,
    },
];
