//! The Zed keymap's built-in bindings, from Zed's default Linux keymap
//! (on macOS Zed uses `Cmd` where Linux uses `Ctrl`; here `Ctrl` stands in
//! for both, as in the VS Code and Sublime keymaps).
//!
//! All-`Ctrl`, no chords: one flat `""` context, the same shape as VS Code
//! and Sublime, with Shift written out explicitly in each token (`"C-S-p"`)
//! because the dispatch reads the Shift modifier bit, not letter case.
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
];

/// Zed's chord contexts (a single top-level `""` context), wired into [`crate::TABLES`].
pub(crate) const CONTEXTS: &[ChordContext] = &[ChordContext {
    name: "",
    bindings: NORMAL,
}];
