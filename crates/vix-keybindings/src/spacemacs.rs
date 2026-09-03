//! Spacemacs's own `SPC`-leader bindings (`src/app.rs`'s
//! `spacemacs_leader_key`/`spacemacs_leader_lookup`), converted from that
//! file's former `SPACEMACS_LEADER` const.
//!
//! Spacemacs's shared Normal-mode vocabulary (motions, `i`/`a`/…) lives
//! under the `"vi"` keymap id instead (`vim.rs`) — `spacemacs_key`
//! delegates to the very same `vim_normal_key` Vi uses, so it is not
//! duplicated here.
//!
//! This is the one context in the whole registry queried with
//! [`crate::lookup_sequence`] rather than [`crate::lookup`]: a leader
//! `key_token` is the **whole typed sequence** (e.g. `"ff"`, `"gs"`), not
//! one keypress, because Spacemacs's leader is a prefix search over
//! multi-character sequences of plain letters (never a modifier), not a
//! series of fixed chord depths the way Emacs's `C-x`/`C-c` families are —
//! discovered only once the actual matching algorithm
//! (`spacemacs_leader_lookup`'s exact-match / valid-prefix / neither) was
//! read closely enough to convert it for real, the same way T104a found
//! Emacs needed [`crate::ChordContext`] in the first place.

#![warn(clippy::pedantic)]

use crate::{Binding, ChordContext};

const LEADER: &[Binding] = &[
    Binding {
        key_token: " ", // SPC SPC — M-x / command palette
        action_id: "tools.palette",
    },
    Binding {
        key_token: "ff", // find file
        action_id: "file.open",
    },
    Binding {
        key_token: "fr",
        action_id: "file.open_recent",
    },
    Binding {
        key_token: "fs",
        action_id: "file.save",
    },
    Binding {
        key_token: "fp",
        action_id: "file.switch_project",
    },
    Binding {
        key_token: "bn", // buffers
        action_id: "tab.next",
    },
    Binding {
        key_token: "bp",
        action_id: "tab.prev",
    },
    Binding {
        key_token: "bd",
        action_id: "file.close",
    },
    Binding {
        key_token: "pf", // project: find/command
        action_id: "tools.palette",
    },
    Binding {
        key_token: "pp",
        action_id: "file.switch_project",
    },
    Binding {
        key_token: "pt", // project tree
        action_id: "view.explorer",
    },
    Binding {
        key_token: "gs", // git status
        action_id: "git.changes",
    },
    Binding {
        key_token: "gg",
        action_id: "git.status",
    },
    Binding {
        key_token: "gb",
        action_id: "git.blame",
    },
    Binding {
        key_token: "w/",
        action_id: "view.split_vertical",
    },
    Binding {
        key_token: "w-",
        action_id: "view.split_horizontal",
    },
    Binding {
        key_token: "wd",
        action_id: "view.unsplit",
    },
    Binding {
        key_token: "ww",
        action_id: "view.focus_other_pane",
    },
    Binding {
        key_token: "ss", // search
        action_id: "edit.find",
    },
    Binding {
        key_token: "sp",
        action_id: "search.workspace",
    },
    Binding {
        key_token: "tn", // toggles
        action_id: "view.line_numbers",
    },
    Binding {
        key_token: "tw",
        action_id: "view.whitespace",
    },
    Binding {
        key_token: "qq",
        action_id: "file.quit",
    },
    Binding {
        key_token: ";",
        action_id: "edit.toggle_comment",
    },
];

pub(crate) const CONTEXTS: &[ChordContext] = &[ChordContext {
    name: "",
    bindings: LEADER,
}];
