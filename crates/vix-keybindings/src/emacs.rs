//! The Emacs keymap's built-in bindings (`src/app.rs`'s `emacs_key` and its
//! five chord-continuation handlers), converted from that file's former
//! hardcoded `match` arms plus five `EMACS_CTRL_*` consts into this crate's
//! shared schema (`SPACEMACS_LEADER` stays Spacemacs's own table, T104b).
//!
//! Fixed one real drift along the way: the old `EMACS_CTRL_X` const (used
//! only for the which-key popup and the F1 help overlay, never for actual
//! dispatch — the real `C-x` chord handler was a second, separate
//! hardcoded `match`) disagreed with that handler in two ways: it claimed
//! `b` ran a `"buffers"` action that didn't exist anywhere in
//! `App::run_action`, and it was missing the `C-b`/`0` bindings the real
//! handler accepted. This table is the single source now, so display and
//! dispatch can't drift apart again; `nav.switch_buffer` is the real,
//! now-existing action id both `C-x b` and `C-x C-b` run.

#![warn(clippy::pedantic)]

use crate::{Binding, ChordContext};

/// The top level: every Ctrl-letter binding `emacs_key` used to hardcode,
/// plus every Meta (Alt) binding `emacs_meta_key` used to hardcode — a
/// Meta binding is a single keystroke, not a chord prefix, so it belongs
/// here rather than in a context of its own.
const TOP_LEVEL: &[Binding] = &[
    Binding {
        key_token: "C-s",
        action_id: "edit.find",
    },
    Binding {
        key_token: "C-f",
        action_id: "motion.char_right",
    },
    Binding {
        key_token: "C-b",
        action_id: "motion.char_left",
    },
    Binding {
        key_token: "C-n",
        action_id: "motion.line_down",
    },
    Binding {
        key_token: "C-p",
        action_id: "motion.line_up",
    },
    Binding {
        key_token: "C-a",
        action_id: "motion.home",
    },
    Binding {
        key_token: "C-e",
        action_id: "motion.end",
    },
    Binding {
        key_token: "C-v",
        action_id: "motion.page_down",
    },
    Binding {
        key_token: "C-d",
        action_id: "motion.delete_forward",
    },
    Binding {
        key_token: "C-w",
        action_id: "edit.cut",
    },
    Binding {
        key_token: "C-y",
        action_id: "edit.paste",
    },
    Binding {
        key_token: "C-k",
        action_id: "cut_line",
    },
    Binding {
        key_token: "C-t",
        action_id: "edit.transpose_chars",
    },
    Binding {
        key_token: "C-g",
        action_id: "edit.keyboard_quit",
    },
    Binding {
        key_token: "C-/",
        action_id: "edit.undo",
    },
    Binding {
        key_token: "C-7",
        action_id: "edit.undo",
    },
    Binding {
        key_token: "C-_",
        action_id: "edit.undo",
    },
    Binding {
        key_token: "A-x",
        action_id: "tools.palette",
    },
    Binding {
        key_token: "A-f",
        action_id: "nav.word_next",
    },
    Binding {
        key_token: "A-b",
        action_id: "nav.word_prev",
    },
    Binding {
        key_token: "A-v",
        action_id: "motion.page_up",
    },
    Binding {
        key_token: "A-w",
        action_id: "edit.copy",
    },
    Binding {
        key_token: "A-t",
        action_id: "edit.transpose_words",
    },
    Binding {
        key_token: "A-<",
        action_id: "edit.go_first",
    },
    Binding {
        key_token: "A->",
        action_id: "edit.go_last",
    },
];

/// The `Ctrl+X …` chord (the old `EMACS_CTRL_X`, corrected — see the module
/// doc's "Fixed one real drift" note).
const CTRL_X: &[Binding] = &[
    Binding {
        key_token: "C-f",
        action_id: "file.open",
    },
    Binding {
        key_token: "C-s",
        action_id: "file.save",
    },
    Binding {
        key_token: "C-c",
        action_id: "file.quit",
    },
    Binding {
        key_token: "C-b",
        action_id: "nav.switch_buffer",
    },
    Binding {
        key_token: "k",
        action_id: "file.close",
    },
    Binding {
        key_token: "b",
        action_id: "nav.switch_buffer",
    },
    Binding {
        key_token: "o",
        action_id: "view.focus_other_pane",
    },
    Binding {
        key_token: "2",
        action_id: "view.split_horizontal",
    },
    Binding {
        key_token: "3",
        action_id: "view.split_vertical",
    },
    Binding {
        key_token: "0",
        action_id: "view.unsplit",
    },
    Binding {
        key_token: "1",
        action_id: "view.unsplit",
    },
];

/// The `Ctrl+C …` chord — the Org command family (the old `EMACS_CTRL_C`,
/// unchanged: this one was already wired to the real dispatch, just
/// relocated). `C-u C-c C-t` (close with a note) and `C-u C-c C-c` (force
/// `#+TBLFM:` recalc) are universal-argument variants handled as host logic
/// before the table lookup, not listed here as separate bindings.
const CTRL_C: &[Binding] = &[
    Binding {
        key_token: "C-t",
        action_id: "org.cycle_todo",
    },
    Binding {
        key_token: "C-c",
        action_id: "org.ctrl_c_ctrl_c",
    },
    Binding {
        key_token: "C-s",
        action_id: "org.schedule",
    },
    Binding {
        key_token: "C-d",
        action_id: "org.deadline",
    },
    Binding {
        key_token: "C-w",
        action_id: "org.refile",
    },
    Binding {
        key_token: "C-q",
        action_id: "org.set_tags",
    },
    Binding {
        key_token: "C-o",
        action_id: "org.link.follow",
    },
    Binding {
        key_token: "C-l",
        action_id: "org.link.insert",
    },
    Binding {
        key_token: "l",
        action_id: "org.link.store",
    },
    Binding {
        key_token: "a",
        action_id: "org.agenda",
    },
    Binding {
        key_token: ".",
        action_id: "org.timestamp",
    },
    Binding {
        key_token: "!",
        action_id: "org.timestamp_inactive",
    },
    Binding {
        key_token: "'",
        action_id: "org.edit_src",
    },
    Binding {
        key_token: "/",
        action_id: "org.sparse.match",
    },
    Binding {
        key_token: "-",
        action_id: "org.table.insert_hline",
    },
    Binding {
        key_token: "^",
        action_id: "org.table.sort",
    },
    Binding {
        key_token: "+",
        action_id: "org.table.sum_column",
    },
    Binding {
        key_token: "|",
        action_id: "org.table.create_from_region",
    },
];

/// The `Ctrl+C Ctrl+X …` chord — the extended Org family (the old
/// `EMACS_CTRL_C_X`, unchanged, just relocated).
const CTRL_C_X: &[Binding] = &[
    Binding {
        key_token: "f",
        action_id: "org.footnote",
    },
    Binding {
        key_token: "a",
        action_id: "org.archive.tag",
    },
    Binding {
        key_token: "<",
        action_id: "org.agenda.lock",
    },
    Binding {
        key_token: ">",
        action_id: "org.agenda.unlock",
    },
    Binding {
        key_token: "C-s",
        action_id: "org.archive.subtree",
    },
    Binding {
        key_token: "C-c",
        action_id: "org.column_view",
    },
    Binding {
        key_token: "C-u",
        action_id: "org.columns.update_dblock",
    },
    Binding {
        key_token: "C-i",
        action_id: "org.clock_in",
    },
    Binding {
        key_token: "C-o",
        action_id: "org.clock_out",
    },
    Binding {
        key_token: "C-w",
        action_id: "org.subtree.cut",
    },
    Binding {
        key_token: "C-y",
        action_id: "org.subtree.paste",
    },
];

/// The `Ctrl+C p c …` chord — the `project.*` family (the old
/// `EMACS_CTRL_C_P_C`, unchanged, just relocated). `m` continues into
/// [`CTRL_C_P_C_M`], handled as host logic, so it is not listed here.
const CTRL_C_P_C: &[Binding] = &[
    Binding {
        key_token: "o",
        action_id: "project.configure",
    },
    Binding {
        key_token: "c",
        action_id: "project.compile",
    },
    Binding {
        key_token: "t",
        action_id: "project.test",
    },
    Binding {
        key_token: ".",
        action_id: "project.test_at_point",
    },
    Binding {
        key_token: "i",
        action_id: "project.install",
    },
    Binding {
        key_token: "p",
        action_id: "project.package",
    },
    Binding {
        key_token: "r",
        action_id: "project.run",
    },
    Binding {
        key_token: "x",
        action_id: "project.run_task",
    },
    Binding {
        key_token: "X",
        action_id: "project.repeat_last_task",
    },
];

/// The `Ctrl+C p c m …` chord — the `project.subproject.*` family (the old
/// `EMACS_CTRL_C_P_C_M`, unchanged, just relocated).
const CTRL_C_P_C_M: &[Binding] = &[
    Binding {
        key_token: "f",
        action_id: "project.subproject.find_file",
    },
    Binding {
        key_token: "o",
        action_id: "project.subproject.configure",
    },
    Binding {
        key_token: "c",
        action_id: "project.subproject.compile",
    },
    Binding {
        key_token: "t",
        action_id: "project.subproject.test",
    },
    Binding {
        key_token: "i",
        action_id: "project.subproject.install",
    },
    Binding {
        key_token: "p",
        action_id: "project.subproject.package",
    },
    Binding {
        key_token: "r",
        action_id: "project.subproject.run",
    },
];

pub(crate) const CONTEXTS: &[ChordContext] = &[
    ChordContext {
        name: "",
        bindings: TOP_LEVEL,
    },
    ChordContext {
        name: "C-x",
        bindings: CTRL_X,
    },
    ChordContext {
        name: "C-c",
        bindings: CTRL_C,
    },
    ChordContext {
        name: "C-c C-x",
        bindings: CTRL_C_X,
    },
    ChordContext {
        name: "C-c p c",
        bindings: CTRL_C_P_C,
    },
    ChordContext {
        name: "C-c p c m",
        bindings: CTRL_C_P_C_M,
    },
];
