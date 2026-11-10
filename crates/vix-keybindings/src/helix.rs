//! The Helix keymap's built-in bindings (`tui/src/app/helix.rs`'s
//! `helix_normal_key`).
//!
//! Helix is selection-first: a motion (`w`, `x`, `%`, ...) selects, then an
//! action (`d`, `c`, `y`, ...) acts on the selection. The `helix.*` action
//! ids carry that model; everything else reuses an existing Vix action id.
//!
//! Four contexts: Normal mode's single keys (`""`), the `g` goto prefix
//! (`"g"`), the `m` match prefix (`"m"`), and the `Space` leader
//! (`"Space"`). Entering a prefix is a mode transition, not an action, so
//! -- like Vi's `g`/`d`/`y` -- the prefix keys themselves stay host-side.
//! So do the keys that take a character argument (`f`/`t`/`F`/`T`/`r`),
//! `:` (the command line), `Esc`, and Insert mode.

use crate::{Binding, ChordContext};

/// Normal (and Select) mode's single-key bindings.
const NORMAL: &[Binding] = &[
    Binding {
        key_token: "h",
        action_id: "helix.move_left",
    },
    Binding {
        key_token: "Left",
        action_id: "helix.move_left",
    },
    Binding {
        key_token: "j",
        action_id: "helix.move_down",
    },
    Binding {
        key_token: "Down",
        action_id: "helix.move_down",
    },
    Binding {
        key_token: "k",
        action_id: "helix.move_up",
    },
    Binding {
        key_token: "Up",
        action_id: "helix.move_up",
    },
    Binding {
        key_token: "l",
        action_id: "helix.move_right",
    },
    Binding {
        key_token: "Right",
        action_id: "helix.move_right",
    },
    Binding {
        key_token: "w",
        action_id: "helix.select_word_next",
    },
    Binding {
        key_token: "b",
        action_id: "helix.select_word_prev",
    },
    Binding {
        key_token: "e",
        action_id: "helix.select_word_end",
    },
    Binding {
        key_token: "W",
        action_id: "helix.select_long_word_next",
    },
    Binding {
        key_token: "B",
        action_id: "helix.select_long_word_prev",
    },
    Binding {
        key_token: "E",
        action_id: "helix.select_long_word_end",
    },
    Binding {
        key_token: "x",
        action_id: "helix.select_line",
    },
    Binding {
        key_token: "%",
        action_id: "edit.select_all",
    },
    Binding {
        key_token: "d",
        action_id: "helix.delete",
    },
    Binding {
        key_token: "c",
        action_id: "helix.change",
    },
    Binding {
        key_token: "y",
        action_id: "helix.yank",
    },
    Binding {
        key_token: "p",
        action_id: "helix.paste_after",
    },
    Binding {
        key_token: "P",
        action_id: "helix.paste_before",
    },
    Binding {
        key_token: "~",
        action_id: "helix.switch_case",
    },
    Binding {
        key_token: ">",
        action_id: "indent_selection",
    },
    Binding {
        key_token: "<",
        action_id: "outdent_selection",
    },
    Binding {
        key_token: "J",
        action_id: "edit.join_lines",
    },
    Binding {
        key_token: "u",
        action_id: "edit.undo",
    },
    Binding {
        key_token: "U",
        action_id: "edit.redo",
    },
    Binding {
        key_token: ";",
        action_id: "helix.collapse",
    },
    Binding {
        key_token: ",",
        action_id: "remove_all_multi_cursors",
    },
    Binding {
        key_token: "v",
        action_id: "helix.toggle_select",
    },
    Binding {
        key_token: "i",
        action_id: "helix.insert",
    },
    Binding {
        key_token: "a",
        action_id: "helix.append",
    },
    Binding {
        key_token: "I",
        action_id: "vim.insert_line_start",
    },
    Binding {
        key_token: "A",
        action_id: "vim.append_end",
    },
    Binding {
        key_token: "o",
        action_id: "vim.open_below",
    },
    Binding {
        key_token: "O",
        action_id: "vim.open_above",
    },
    Binding {
        key_token: "/",
        action_id: "edit.find",
    },
    Binding {
        key_token: "?",
        action_id: "edit.find",
    },
    Binding {
        key_token: "n",
        action_id: "edit.find_next",
    },
    Binding {
        key_token: "N",
        action_id: "edit.find_prev",
    },
    Binding {
        key_token: "*",
        action_id: "search.next_selection",
    },
    Binding {
        key_token: "s",
        action_id: "edit.select_regex_in_selection",
    },
    Binding {
        key_token: "C",
        action_id: "spawn_multi_cursor_down",
    },
];

/// The second key after `g` (goto mode).
const GOTO: &[Binding] = &[
    Binding {
        key_token: "g",
        action_id: "edit.go_first",
    },
    Binding {
        key_token: "e",
        action_id: "edit.go_last",
    },
    Binding {
        key_token: "h",
        action_id: "edit.line_start",
    },
    Binding {
        key_token: "l",
        action_id: "edit.line_end",
    },
    Binding {
        key_token: "s",
        action_id: "motion.home",
    },
    Binding {
        key_token: "d",
        action_id: "nav.goto_definition",
    },
    Binding {
        key_token: "D",
        action_id: "nav.goto_declaration",
    },
    Binding {
        key_token: "y",
        action_id: "nav.goto_type_definition",
    },
    Binding {
        key_token: "i",
        action_id: "nav.goto_implementation",
    },
    Binding {
        key_token: "r",
        action_id: "lsp.references",
    },
    Binding {
        key_token: "n",
        action_id: "tab.next",
    },
    Binding {
        key_token: "p",
        action_id: "tab.prev",
    },
];

/// The second key after `m` (match mode).
const MATCH: &[Binding] = &[Binding {
    key_token: "m",
    action_id: "edit.match_bracket",
}];

/// The second key after `Space` (the space-mode leader).
const SPACE: &[Binding] = &[
    Binding {
        key_token: "f",
        action_id: "file.open",
    },
    Binding {
        key_token: "b",
        action_id: "nav.switch_buffer",
    },
    Binding {
        key_token: "s",
        action_id: "nav.goto_symbol",
    },
    Binding {
        key_token: "S",
        action_id: "nav.goto_workspace_symbol",
    },
    Binding {
        key_token: "/",
        action_id: "search.workspace",
    },
    Binding {
        key_token: "k",
        action_id: "lsp.hover",
    },
    Binding {
        key_token: "r",
        action_id: "lsp.rename",
    },
    Binding {
        key_token: "a",
        action_id: "lsp.code_action",
    },
    Binding {
        key_token: "d",
        action_id: "lsp.diagnostics",
    },
    Binding {
        key_token: "p",
        action_id: "helix.paste_after",
    },
    Binding {
        key_token: "P",
        action_id: "helix.paste_before",
    },
    Binding {
        key_token: "y",
        action_id: "helix.yank",
    },
    Binding {
        key_token: "?",
        action_id: "tools.palette",
    },
];

/// The Helix chord contexts, wired into [`crate::TABLES`].
pub(crate) const CONTEXTS: &[ChordContext] = &[
    ChordContext {
        name: "",
        bindings: NORMAL,
    },
    ChordContext {
        name: "g",
        bindings: GOTO,
    },
    ChordContext {
        name: "m",
        bindings: MATCH,
    },
    ChordContext {
        name: "Space",
        bindings: SPACE,
    },
];
