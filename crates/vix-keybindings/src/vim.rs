//! The Vi (Vim) keymap's built-in bindings (`src/app.rs`'s `vim_normal_key`),
//! converted from that function's former hardcoded `match`.
//!
//! Shared with Spacemacs: `spacemacs_key` delegates to the very same
//! `vim_normal_key` for its Normal-mode vocabulary (confirmed in
//! `crates/vix-modal/spec/index.md`'s audit — "Spacemacs Normal mode is not
//! a second implementation; it is the Vi one"), so this table alone covers
//! both keymaps' Normal mode. Spacemacs's own `SPC`-leader is a separate,
//! sequence-matched table under its own keymap id (`spacemacs.rs`).

#![warn(clippy::pedantic)]

use crate::{Binding, ChordContext};

/// Normal mode's single-key bindings. `g`/`d`/`y` (starting a pending
/// two-key operator) are a mode transition, not a dispatchable action, so
/// — like Emacs's `C-x`/`C-c` prefix-entry (T104a) — they stay host-side
/// special-cased rather than living here.
const NORMAL: &[Binding] = &[
    Binding {
        key_token: "h",
        action_id: "motion.char_left",
    },
    Binding {
        key_token: "Left",
        action_id: "motion.char_left",
    },
    Binding {
        key_token: "j",
        action_id: "motion.line_down",
    },
    Binding {
        key_token: "Down",
        action_id: "motion.line_down",
    },
    Binding {
        key_token: "k",
        action_id: "motion.line_up",
    },
    Binding {
        key_token: "Up",
        action_id: "motion.line_up",
    },
    Binding {
        key_token: "l",
        action_id: "motion.char_right",
    },
    Binding {
        key_token: "Right",
        action_id: "motion.char_right",
    },
    // `^` rides smart Home (first non-blank, then column 0) via the same
    // action as `0` — both were `editor_motion(KeyCode::Home)` already.
    Binding {
        key_token: "0",
        action_id: "motion.home",
    },
    Binding {
        key_token: "^",
        action_id: "motion.home",
    },
    Binding {
        key_token: "$",
        action_id: "motion.end",
    },
    Binding {
        key_token: "w",
        action_id: "nav.word_next",
    },
    Binding {
        key_token: "b",
        action_id: "nav.word_prev",
    },
    Binding {
        key_token: "G",
        action_id: "edit.go_last",
    },
    Binding {
        key_token: "x",
        action_id: "motion.delete_forward",
    },
    Binding {
        key_token: "p",
        action_id: "edit.paste",
    },
    Binding {
        key_token: "u",
        action_id: "edit.undo",
    },
    Binding {
        key_token: "/",
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
        key_token: "%",
        action_id: "edit.match_bracket",
    },
    Binding {
        key_token: "i",
        action_id: "vim.insert",
    },
    Binding {
        key_token: "a",
        action_id: "vim.append",
    },
    Binding {
        key_token: "A",
        action_id: "vim.append_end",
    },
    Binding {
        key_token: "I",
        action_id: "vim.insert_line_start",
    },
    Binding {
        key_token: "o",
        action_id: "vim.open_below",
    },
    Binding {
        key_token: "O",
        action_id: "vim.open_above",
    },
];

/// The second key of a pending `g` operator: only `gg` continues it
/// (anything else silently cancels, matching the original dispatch, which
/// had no fallback/error status for a miss here).
const PENDING_G: &[Binding] = &[Binding {
    key_token: "g",
    action_id: "edit.go_first",
}];

/// The second key of a pending `d` operator: only `dd` continues it.
const PENDING_D: &[Binding] = &[Binding {
    key_token: "d",
    action_id: "cut_line",
}];

/// The second key of a pending `y` operator: only `yy` continues it.
const PENDING_Y: &[Binding] = &[Binding {
    key_token: "y",
    action_id: "copy_line",
}];

pub(crate) const CONTEXTS: &[ChordContext] = &[
    ChordContext {
        name: "",
        bindings: NORMAL,
    },
    ChordContext {
        name: "g",
        bindings: PENDING_G,
    },
    ChordContext {
        name: "d",
        bindings: PENDING_D,
    },
    ChordContext {
        name: "y",
        bindings: PENDING_Y,
    },
];
