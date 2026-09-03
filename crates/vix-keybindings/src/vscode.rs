//! The VS Code keymap's built-in bindings (`src/app.rs`'s
//! `vscode_ctrl_key`), converted from that function's former hardcoded
//! `match`.
//!
//! One table serves both `vscode-macos` and `vscode-windows` — `App`'s
//! own doc comment already establishes why: "macOS and Windows share the
//! same `Ctrl`-based bindings in the terminal" (VS Code's `Cmd` shortcuts
//! map onto `Ctrl` either way in a terminal). This is the deliberate,
//! documented duplication `crates/vix-keybindings/spec/index.md`'s
//! "Why 10 keymap ids, not `App`'s private 9-variant enum" already calls
//! for.
//!
//! All-Ctrl, no chords, so there is exactly one context, `""` — the
//! simplest conversion of the three done so far (Emacs, Vi/Spacemacs,
//! this one). One real subtlety: every token that needs `Shift` encodes
//! it explicitly (`"C-S-p"`, not the bare uppercase `"C-P"`), matching
//! the original dispatch's own `Self::shift(&key)` modifier-bit check
//! rather than `vix_macros::encode_key`'s usual "Shift is implicit in an
//! uppercase char" rule — a terminal can report `Ctrl+Shift+p` as a
//! lowercase `p` with the Shift bit set rather than an uppercase `P`, and
//! the original code (and this table) must not silently collide that
//! with plain `Ctrl+p` if it does.

#![warn(clippy::pedantic)]

use crate::{Binding, ChordContext};

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
        key_token: "C-t",
        action_id: "nav.goto_workspace_symbol",
    },
    Binding {
        key_token: "C-S-p",
        action_id: "tools.palette",
    },
    Binding {
        key_token: "C-p",
        action_id: "file.open",
    },
    Binding {
        key_token: "C-S-o",
        action_id: "nav.goto_symbol",
    },
    Binding {
        key_token: "C-S-g",
        action_id: "edit.find_prev",
    },
    // `Ctrl+G` (Go to Line) reuses the same `nav.goto_line` action id
    // every other keymap's Go-to-Line binding already runs.
    Binding {
        key_token: "C-g",
        action_id: "nav.goto_line",
    },
    Binding {
        key_token: "C-S-e",
        action_id: "view.toggle_explorer_focus",
    },
    Binding {
        key_token: "C-b",
        action_id: "view.explorer",
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
        key_token: "C-r",
        action_id: "edit.replace",
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
    Binding {
        key_token: "C-]",
        action_id: "edit.match_bracket",
    },
    Binding {
        key_token: "C-S-l",
        action_id: "edit.select_all_occurrences",
    },
    Binding {
        key_token: "C-S-m",
        action_id: "lsp.diagnostics",
    },
    Binding {
        key_token: "C-S-k",
        action_id: "cut_line",
    },
    Binding {
        key_token: "C-S-\\",
        action_id: "edit.match_bracket",
    },
    Binding {
        key_token: "C-\\",
        action_id: "view.split_vertical",
    },
    Binding {
        key_token: "C-j",
        action_id: "view.bottom_dock",
    },
    Binding {
        key_token: "C-`",
        action_id: "tools.terminal",
    },
];

pub(crate) const CONTEXTS: &[ChordContext] = &[ChordContext {
    name: "",
    bindings: NORMAL,
}];
