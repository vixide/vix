//! The Apple keymap's built-in bindings (`src/app.rs`'s `apple_ctrl_key`),
//! converted from that function's former hardcoded `match`.
//!
//! Unlike VS Code/`IntelliJ`/Eclipse/Sublime, Apple's original dispatch is
//! a genuine *mix*: several letters have an explicit `if
//! Self::shift(&key)` guard branching to a **different** action
//! (`o`/`s`/`w`/`t`/`b`/`f`/`g`), while the rest simply never examine the
//! Shift bit at all — the same action fires whether or not Shift is held
//! (`q`/`n`/`p`/`e`/`r`/`/`/`7`/`_`/`]`/`;`). To keep one uniform,
//! Shift-bit-explicit token function (`App::apple_ctrl_token`, same shape
//! as T104c–f's) rather than special-casing letters inside it, the
//! "doesn't care" letters get an explicit duplicate `"C-S-…"` row with the
//! identical action id — the same "faithfully preserve an unguarded
//! quirk" technique T104d used for `IntelliJ`'s `Ctrl+Shift+N`/`Ctrl+
//! Shift+G`, just applied more broadly here since more letters need it.
//!
//! Two bindings are deliberately **not** table rows, kept host-side in
//! `App::apple_ctrl_key` instead (see its doc comment):
//! - `Ctrl+Alt+R` (query replace) — the only binding here that keys off
//!   `Alt`, which this table's token function otherwise never encodes
//!   (every other row is Alt-agnostic, matching the original: nothing
//!   else in `apple_ctrl_key` ever checked `Self::alt`).
//! - `Ctrl+D` (forward delete) — genuinely focus-gated (only claims the
//!   key while the editor pane is focused; elsewhere it's left unclaimed
//!   so other panes keep their own `Ctrl+D` handling), which a
//!   keymap-keyed static table can't express.

#![warn(clippy::pedantic)]

use crate::{Binding, ChordContext};

const NORMAL: &[Binding] = &[
    // Shift-guarded: a genuinely different action per Shift state.
    Binding {
        key_token: "C-o",
        action_id: "file.open",
    },
    Binding {
        key_token: "C-S-o",
        action_id: "file.open_recent",
    },
    Binding {
        key_token: "C-s",
        action_id: "file.save",
    },
    Binding {
        key_token: "C-S-s",
        action_id: "file.save_as",
    },
    Binding {
        key_token: "C-w",
        action_id: "file.close",
    },
    Binding {
        key_token: "C-S-w",
        action_id: "file.close_all",
    },
    Binding {
        key_token: "C-t",
        action_id: "nav.goto_workspace_symbol",
    },
    Binding {
        key_token: "C-S-t",
        action_id: "file.reopen_closed",
    },
    Binding {
        key_token: "C-b",
        action_id: "view.explorer",
    },
    Binding {
        key_token: "C-S-b",
        action_id: "nav.outline",
    },
    Binding {
        key_token: "C-f",
        action_id: "edit.find",
    },
    Binding {
        key_token: "C-S-f",
        action_id: "search.workspace",
    },
    Binding {
        key_token: "C-g",
        action_id: "edit.find_next",
    },
    Binding {
        key_token: "C-S-g",
        action_id: "edit.find_prev",
    },
    // Shift-agnostic: the original never checked the Shift bit, so the
    // Shift variant is an explicit duplicate row with the same action.
    Binding {
        key_token: "C-q",
        action_id: "file.quit",
    },
    Binding {
        key_token: "C-S-q",
        action_id: "file.quit",
    },
    Binding {
        key_token: "C-n",
        action_id: "file.new",
    },
    Binding {
        key_token: "C-S-n",
        action_id: "file.new",
    },
    Binding {
        key_token: "C-p",
        action_id: "tools.palette",
    },
    Binding {
        key_token: "C-S-p",
        action_id: "tools.palette",
    },
    Binding {
        key_token: "C-e",
        action_id: "view.toggle_explorer_focus",
    },
    Binding {
        key_token: "C-S-e",
        action_id: "view.toggle_explorer_focus",
    },
    Binding {
        key_token: "C-r",
        action_id: "edit.replace",
    },
    Binding {
        key_token: "C-S-r",
        action_id: "edit.replace",
    },
    // Many terminals emit the same control byte (0x1F) for Ctrl+/,
    // Ctrl+7, and Ctrl+_, so accept all three for Comment.
    Binding {
        key_token: "C-/",
        action_id: "edit.toggle_comment",
    },
    Binding {
        key_token: "C-S-/",
        action_id: "edit.toggle_comment",
    },
    Binding {
        key_token: "C-7",
        action_id: "edit.toggle_comment",
    },
    Binding {
        key_token: "C-S-7",
        action_id: "edit.toggle_comment",
    },
    Binding {
        key_token: "C-_",
        action_id: "edit.toggle_comment",
    },
    Binding {
        key_token: "C-S-_",
        action_id: "edit.toggle_comment",
    },
    Binding {
        key_token: "C-]",
        action_id: "edit.match_bracket",
    },
    Binding {
        key_token: "C-S-]",
        action_id: "edit.match_bracket",
    },
    Binding {
        key_token: "C-;",
        action_id: "spell.suggest",
    },
    Binding {
        key_token: "C-S-;",
        action_id: "spell.suggest",
    },
];

pub(crate) const CONTEXTS: &[ChordContext] = &[ChordContext {
    name: "",
    bindings: NORMAL,
}];
