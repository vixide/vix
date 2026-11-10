//! The Zed keymap's built-in bindings, from Zed's default Linux keymap
//! (on macOS Zed uses `Cmd` where Linux uses `Ctrl`; here `Ctrl` stands in
//! for both, as in the VS Code and Sublime keymaps).
//!
//! `Ctrl` keys in a `""` context, with Shift written out explicitly in each
//! token (`"C-S-p"`) because the dispatch reads the Shift modifier bit, not
//! letter case, plus a `"C-k"` context for Zed's two-key `Ctrl+K …` chords
//! (from `assets/keymaps/default-linux.json`, read on 2026-11-10). Chords
//! whose second key is `Ctrl+[`, `Ctrl+]`, `Ctrl+I` or an arrow are left
//! out: terminals send those as Esc, Esc, Tab and escape sequences.
//! `Ctrl+D` (Zed's select next occurrence) is deliberately unbound here so
//! it reaches the editor's own `Ctrl+D`, which adds the next occurrence.

use crate::{Binding, ChordContext};

/// Zed's Ctrl-key bindings (its only context), with Shift encoded explicitly in the token.
const NORMAL: &[Binding] = &[
    Binding {
        key_token: "C-q",
        action_id: "file.quit",
    },
    Binding {
        key_token: "C-n",
        action_id: "file.new",
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
        key_token: "C-S-t",
        action_id: "file.reopen_closed",
    },
    Binding {
        key_token: "C-S-p",
        action_id: "tools.palette",
    },
    // Zed's file finder; Ctrl+E opens it too on Linux.
    Binding {
        key_token: "C-p",
        action_id: "file.open",
    },
    Binding {
        key_token: "C-e",
        action_id: "file.open",
    },
    // Zed's outline (symbols in this file).
    Binding {
        key_token: "C-S-o",
        action_id: "nav.goto_symbol",
    },
    // Zed's project symbols.
    Binding {
        key_token: "C-t",
        action_id: "nav.goto_workspace_symbol",
    },
    Binding {
        key_token: "C-g",
        action_id: "nav.goto_line",
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
        key_token: "C-h",
        action_id: "edit.replace",
    },
    Binding {
        key_token: "C-S-l",
        action_id: "edit.select_all_occurrences",
    },
    Binding {
        key_token: "C-S-d",
        action_id: "edit.duplicate_line",
    },
    Binding {
        key_token: "C-S-k",
        action_id: "cut_line",
    },
    Binding {
        key_token: "C-l",
        action_id: "edit.select_line",
    },
    Binding {
        key_token: "C-m",
        action_id: "edit.match_bracket",
    },
    Binding {
        key_token: "C-S-m",
        action_id: "lsp.diagnostics",
    },
    Binding {
        key_token: "C-.",
        action_id: "lsp.code_action",
    },
    // Zed's left dock, which holds its project panel.
    Binding {
        key_token: "C-b",
        action_id: "view.explorer",
    },
    Binding {
        key_token: "C-S-e",
        action_id: "view.toggle_explorer_focus",
    },
    Binding {
        key_token: "C-j",
        action_id: "view.bottom_dock",
    },
    Binding {
        key_token: "C-`",
        action_id: "tools.terminal",
    },
    // Zed's Git panel.
    Binding {
        key_token: "C-S-g",
        action_id: "git.changes",
    },
    Binding {
        key_token: "C-,",
        action_id: "vix.settings",
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
        action_id: "git.diff_next",
    },
    Binding {
        key_token: "A-,",
        action_id: "git.diff_prev",
    },
];

/// The second key of Zed's `Ctrl+K …` chords that have a Vix action.
const CTRL_K: &[Binding] = &[
    Binding {
        key_token: "C-l",
        action_id: "editor.fold_toggle",
    },
    Binding {
        key_token: "C-0",
        action_id: "editor.fold_all",
    },
    Binding {
        key_token: "C-j",
        action_id: "editor.unfold_all",
    },
    Binding {
        key_token: "C-c",
        action_id: "edit.toggle_comment",
    },
    Binding {
        key_token: "C-s",
        action_id: "help.shortcuts",
    },
    Binding {
        key_token: "C-w",
        action_id: "file.close_all",
    },
    Binding {
        key_token: "C-z",
        action_id: "view.soft_wrap",
    },
    Binding {
        key_token: "C-o",
        action_id: "file.open_path",
    },
    Binding {
        key_token: "v",
        action_id: "tools.markdown_preview",
    },
];

/// Zed's chord contexts (the top level and `Ctrl+K`), wired into [`crate::TABLES`].
pub(crate) const CONTEXTS: &[ChordContext] = &[
    ChordContext {
        name: "",
        bindings: NORMAL,
    },
    ChordContext {
        name: "C-k",
        bindings: CTRL_K,
    },
];
