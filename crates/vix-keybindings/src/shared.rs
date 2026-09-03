//! Keymap-agnostic bindings: `src/app.rs`'s `global_shared_key`, which
//! every keymap dispatches through identically (menu-bar mnemonics and
//! function keys). Unlike [`crate::TABLES`] (one table per keymap id),
//! this is a single flat list looked up via [`lookup_shared`] — these
//! bindings don't vary by keymap at all, so there's no `KeymapTable` to
//! key them under.
//!
//! Not every binding `global_shared_key`'s original dispatch defines is
//! here, and that's deliberate, not an oversight:
//! - The leading `Alt+<letter>` menu-mnemonic branch is a genuinely
//!   dynamic lookup into the live menu structure (`menu_index_for_alt`),
//!   not static data a table entry could stand for.
//! - Six arms are genuinely focus-gated (`Ctrl+Shift+Right`/`Left`,
//!   `Alt+Up`/`Down`, `Alt+n`/`p`) — only consumed while the editor pane
//!   is focused. `App::focus` is per-request runtime state, not something
//!   a keymap-keyed lookup table can express without changing behavior
//!   for every other pane (a fixed table entry would fire regardless of
//!   which pane is focused).
//!
//! Both stay host-side in `App::global_shared_key`, documented there.
//! Everything else — clean `(token, action_id)` pairs with no additional
//! runtime gating — lives in [`SHARED`] below.
//!
//! Tokens here go slightly beyond a plain `Ctrl`-chord (unlike every
//! per-keymap table so far): named keys (`Tab`, `BackTab`, `Left`,
//! `Right`, `F1`–`F12`) and `Ctrl+Space`. Only the `F`-key rows ever
//! distinguish `Shift` (`F3` vs `Shift+F3`) — every other row here
//! ignores the Shift bit entirely, matching the original dispatch's own
//! guards (or lack of one).

#![warn(clippy::pedantic)]

use crate::Binding;

/// Every keymap-agnostic binding, in no particular order.
pub const SHARED: &[Binding] = &[
    Binding {
        key_token: "C-Space",
        action_id: "lsp.complete",
    },
    Binding {
        key_token: "C-Tab",
        action_id: "tab.next",
    },
    Binding {
        key_token: "C-BackTab",
        action_id: "tab.prev",
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
        key_token: "F1",
        action_id: "help.shortcuts",
    },
    Binding {
        key_token: "F12",
        action_id: "nav.goto_definition",
    },
    Binding {
        key_token: "F2",
        action_id: "lsp.rename",
    },
    Binding {
        key_token: "F6",
        action_id: "view.focus_other_pane",
    },
    Binding {
        key_token: "F10",
        action_id: "view.toggle_menu",
    },
    Binding {
        key_token: "S-F3",
        action_id: "edit.find_prev",
    },
    Binding {
        key_token: "F3",
        action_id: "edit.find_next",
    },
    Binding {
        key_token: "A-j",
        action_id: "nav.recent_locations",
    },
];

/// The action bound to `token` in the keymap-agnostic [`SHARED`] table,
/// if any.
#[must_use]
pub fn lookup_shared(token: &str) -> Option<&'static str> {
    SHARED
        .iter()
        .find(|b| b.key_token == token)
        .map(|b| b.action_id)
}
