//! The Helix keymap (`crates/vix-keymap-model/spec/index.md`, § Helix).
//!
//! Helix is selection-first: in Normal mode a motion (`w`, `e`, `b`, `x`,
//! `f<char>`, …) *selects*, then an action (`d`, `c`, `y`, `p`, `r<char>`,
//! `~`, …) acts on the selection. `v` toggles Select mode, where motions
//! extend the selection instead of replacing it.
//!
//! This reuses the Vi keymap's machinery rather than running a second
//! engine: the `MODAL_INSERT` flag and `modal_mode` for the mode (Select
//! mode is [`vix_modal::Mode::Visual`]), `vim_cmd` and `run_vim_command`
//! for the `:` command line, `vim_pending` for the one pending prefix or
//! character-argument key, the `vim.*` insert-entry actions, the register
//! writer, and [`vix_modal::motion`]'s pure motions. The selection itself
//! is the editor's own `[anchor, cursor)` selection, and the
//! selection-shaped arithmetic lives in [`vix_modal::helix`]. Key → action
//! lookup goes through `vix-keybindings`' `"helix"` table.
//!
//! A count prefix (`3w`, `2x`, `5j`) accumulates in the Vi keymap's own
//! `modal_count` and is read by whatever the next key runs. `.` repeats
//! the last change semantically: [`HelixState`] remembers *what* changed
//! (`d`, `c` plus the typed text, `r<char>`, …), and `.` applies that again
//! to whatever is selected now.

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, AppFlags, Focus};
use crate::editor_core::actions::InsertText;

/// A buffer snapshot: its text, the cursor, and the sorted non-empty
/// selection, if any.
type Snapshot = (String, usize, Option<(usize, usize)>);

/// The most times a count (or `.`) repeats a paste, an undo, or a change:
/// a typed `999999999p` must not try to build gigabytes of text. Motions
/// need no cap; they stop as soon as they stop moving.
const MAX_REPEAT: usize = 10_000;

/// A change `.` can repeat, recorded semantically so it applies to the
/// selection `.` finds, not the one it was first made on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Edit {
    /// `d`.
    Delete,
    /// `c`, then the text typed before `Esc`.
    Change(String),
    /// `i` (or `a`, when `after`), then the text typed before `Esc`.
    Insert {
        /// `a` rather than `i`.
        after: bool,
        /// What was typed.
        text: String,
    },
    /// `r<char>`.
    Replace(char),
    /// `~`.
    SwitchCase,
    /// `>`.
    Indent,
    /// `<`.
    Outdent,
    /// `p` (or `P`, when `before`), `count` copies.
    Paste {
        /// `P` rather than `p`.
        before: bool,
        /// How many copies were pasted.
        count: usize,
    },
}

/// The Helix keymap's `.` state.
#[derive(Debug, Default)]
pub(super) struct HelixState {
    /// What `.` repeats; `None` until the first change.
    last_change: Option<Edit>,
    /// While Insert mode runs after `c`/`i`/`a`: where typing started, and
    /// the change to record (with the typed text) on `Esc`.
    insert: Option<(usize, Edit)>,
}

/// The `vix-keybindings` context a pending prefix key names, if it is one.
fn prefix_context(prefix: char) -> Option<&'static str> {
    match prefix {
        'g' => Some("g"),
        'm' => Some("m"),
        ' ' => Some("Space"),
        _ => None,
    }
}

impl App {
    /// Helix keymap dispatch: Insert mode passes typing through, the `:`
    /// command line takes every key while open, a pending prefix (`g`,
    /// `m`, `Space`) or character-argument key (`f`/`t`/`F`/`T`/`r`)
    /// resolves on the next key, and every other Normal-mode key is looked
    /// up in the `"helix"` table. Returns true if the key was consumed.
    pub(super) fn helix_key(&mut self, key: KeyEvent) -> bool {
        if self.vim_cmd.is_some() {
            self.vim_cmd_key(key);
            return true;
        }
        if self.flags.contains(AppFlags::MODAL_INSERT) {
            if key.code == KeyCode::Esc {
                self.flags.remove(AppFlags::MODAL_INSERT);
                self.modal_mode = vix_modal::Mode::Normal;
                self.helix_finish_insert();
                return true;
            }
            return false;
        }
        if let Some(pending) = self.vim_pending.take() {
            self.helix_pending_key(pending, key);
            return true;
        }
        if Self::ctrl(&key) || Self::alt(&key) || matches!(key.code, KeyCode::F(_)) {
            return false;
        }
        if key.code == KeyCode::Char(':') {
            self.vim_cmd = Some(String::new());
            return true;
        }
        if self.focus != Focus::Editor {
            return false;
        }
        // `1`-`9` start a count and `0` continues one (a leading `0` is no
        // count, and unbound in Helix).
        if let KeyCode::Char(c) = key.code
            && self.modal_count.push_digit(c)
        {
            return true;
        }
        match key.code {
            // Esc leaves Select mode; the selection stays, as in Helix.
            KeyCode::Esc => {
                self.modal_mode = vix_modal::Mode::Normal;
                self.modal_count.reset();
            }
            // The count waits with the prefix for the key that completes it.
            KeyCode::Char(c @ ('g' | 'm' | ' ' | 'f' | 't' | 'F' | 'T' | 'r')) => {
                self.vim_pending = Some(c);
            }
            _ => {
                let token = crate::macros::encode_key(key);
                if let Some(action) = vix_keybindings::lookup("helix", "", &token) {
                    self.helix_run_counted(action);
                }
                self.modal_count.reset();
            }
        }
        // Swallow every other Normal-mode key so it never types.
        true
    }

    /// Run a Normal-mode table action under the pending count. `helix.*`
    /// actions read the count themselves; the reused Vix actions that
    /// Helix also counts (undo, redo, indent, outdent) simply run that many
    /// times, and `>`/`<` are remembered for `.`.
    fn helix_run_counted(&mut self, action: &str) {
        let count = self.modal_count.value().min(MAX_REPEAT);
        match action {
            "edit.undo" | "edit.redo" | "indent_selection" | "outdent_selection" => {
                for _ in 0..count {
                    self.run_action(action);
                }
                match action {
                    "indent_selection" => self.helix_record(Edit::Indent),
                    "outdent_selection" => self.helix_record(Edit::Outdent),
                    _ => {}
                }
            }
            _ => self.run_action(action),
        }
    }

    /// The key after a pending `g`/`m`/`Space` prefix (a table lookup in
    /// that context) or `f`/`t`/`F`/`T`/`r` (its character argument). `Esc`
    /// or a miss cancels silently, like Vi's pending operators.
    ///
    /// The pending count applies to what the key completes (`3fx`, `2 Space
    /// p`), and `{N}gg` goes to line `N`, as in Helix. Either way the count
    /// ends here.
    fn helix_pending_key(&mut self, pending: char, key: KeyEvent) {
        self.helix_resolve_pending(pending, key);
        self.modal_count.reset();
    }

    /// The body of [`Self::helix_pending_key`], before the count resets.
    fn helix_resolve_pending(&mut self, pending: char, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            return;
        }
        if let Some(context) = prefix_context(pending) {
            let token = crate::macros::encode_key(key);
            if pending == 'g' && token == "g" && !self.modal_count.is_empty() {
                let line = self.modal_count.value();
                self.run_vim_command(&line.to_string());
                return;
            }
            if let Some(action) = vix_keybindings::lookup("helix", context, &token) {
                self.run_action(action);
            }
            return;
        }
        let KeyCode::Char(target) = key.code else {
            return;
        };
        if Self::ctrl(&key) || Self::alt(&key) {
            return;
        }
        if pending == 'r' {
            self.helix_rewrite(|s| vix_modal::helix::replace_chars(s, target));
            self.helix_record(Edit::Replace(target));
        } else {
            let count = self.modal_count.value();
            self.helix_find(pending, target, count);
        }
    }

    /// The which-key rows for a pending Helix prefix: `(title, [(key,
    /// action)])`, or `None` when no prefix is pending.
    pub(super) fn helix_which_key(&self) -> Option<(String, Vec<(String, String)>)> {
        let context = prefix_context(self.vim_pending?)?;
        let rows = vix_keybindings::TABLES
            .iter()
            .find(|t| t.keymap_id == "helix")
            .into_iter()
            .flat_map(|t| t.contexts)
            .find(|c| c.name == context)
            .map(|c| {
                c.bindings
                    .iter()
                    .map(|b| (super::display_key(b.key_token), b.action_id.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let title = if context == "Space" { "SPC" } else { context };
        Some((title.to_string(), rows))
    }

    /// Run a Helix `:` command. Helix's long names and `:o`/`:open` map
    /// onto the Vi command line's own commands; everything else goes to
    /// [`Self::run_vim_command`] unchanged (`:N`, `:q!`, …).
    pub(super) fn run_helix_command(&mut self, cmd: &str) {
        let (name, arg) = cmd
            .split_once(' ')
            .map_or((cmd, ""), |(n, a)| (n, a.trim()));
        match name {
            "o" | "open" | "e" | "edit" if !arg.is_empty() => {
                self.run_vim_command(&format!("e {arg}"));
            }
            "o" | "open" | "e" | "edit" => self.run_action("file.open"),
            "write" => self.run_vim_command("w"),
            "quit" => self.run_vim_command("q"),
            "quit!" => self.run_vim_command("q!"),
            "write-quit" => self.run_vim_command("wq"),
            _ => self.run_vim_command(cmd),
        }
    }

    /// Dispatch a `helix.*` action under the pending count (`modal_count`,
    /// 1 when none). Returns whether `action` was one.
    pub(super) fn run_helix_action(&mut self, action: &str) -> bool {
        let count = self.modal_count.value();
        self.run_helix_motion(action, count) || self.run_helix_edit(action, count)
    }

    /// The selecting and moving `helix.*` actions, each repeated `count`
    /// times. Returns whether `action` was one.
    fn run_helix_motion(&mut self, action: &str, count: usize) -> bool {
        use vix_modal::helix as hx;
        use vix_modal::motion as mo;
        match action {
            "helix.move_left" => self.helix_move(KeyCode::Left, count),
            "helix.move_right" => self.helix_move(KeyCode::Right, count),
            "helix.move_up" => self.helix_move(KeyCode::Up, count),
            "helix.move_down" => self.helix_move(KeyCode::Down, count),
            "helix.select_word_next" => {
                self.helix_select(false, count, |t, p| mo::word_forward(t, p, 1));
            }
            "helix.select_word_prev" => {
                self.helix_select(false, count, |t, p| mo::word_backward(t, p, 1));
            }
            "helix.select_word_end" => {
                self.helix_select(true, count, |t, p| mo::word_end(t, p, 1));
            }
            "helix.select_long_word_next" => {
                self.helix_select(false, count, |t, p| hx::long_word_forward(t, p, 1));
            }
            "helix.select_long_word_prev" => {
                self.helix_select(false, count, |t, p| hx::long_word_backward(t, p, 1));
            }
            "helix.select_long_word_end" => {
                self.helix_select(true, count, |t, p| hx::long_word_end(t, p, 1));
            }
            "helix.select_line" => self.helix_select_line(count),
            "helix.collapse" => {
                if let Some(tab) = self.editor.active_tab_mut() {
                    tab.editor.clear_selection();
                }
            }
            "helix.toggle_select" => {
                self.modal_mode = if self.modal_mode == vix_modal::Mode::Visual {
                    vix_modal::Mode::Normal
                } else {
                    vix_modal::Mode::Visual
                };
            }
            _ => return false,
        }
        true
    }

    /// The editing `helix.*` actions, and `.`. Each change is recorded for
    /// `.` (`c`/`i`/`a` once Insert mode ends). Returns whether `action`
    /// was one.
    fn run_helix_edit(&mut self, action: &str, count: usize) -> bool {
        match action {
            "helix.delete" => {
                self.helix_delete(false);
                self.helix_record(Edit::Delete);
            }
            "helix.change" => {
                self.helix_delete(true);
                self.helix_begin_insert(Edit::Change(String::new()));
            }
            "helix.yank" => self.helix_yank(),
            "helix.paste_after" | "helix.paste_before" => {
                let before = action == "helix.paste_before";
                let count = count.min(MAX_REPEAT);
                self.helix_paste(before, count);
                self.helix_record(Edit::Paste { before, count });
            }
            "helix.switch_case" => {
                self.helix_rewrite(vix_modal::helix::swap_case);
                self.helix_record(Edit::SwitchCase);
            }
            "helix.insert" | "helix.append" => {
                let after = action == "helix.append";
                self.helix_insert(after);
                self.helix_begin_insert(Edit::Insert {
                    after,
                    text: String::new(),
                });
            }
            "helix.repeat_last_change" => self.helix_repeat(count),
            _ => return false,
        }
        true
    }

    /// Whether Select mode (`v`) is on, so motions extend.
    fn helix_extending(&self) -> bool {
        self.modal_mode == vix_modal::Mode::Visual
    }

    /// The active tab's text, cursor, and sorted non-empty selection.
    fn helix_state(&mut self) -> Option<Snapshot> {
        let tab = self.editor.active_tab_mut()?;
        let sel = tab
            .editor
            .get_selection()
            .filter(|s| !s.is_empty())
            .map(|s| s.sorted());
        Some((tab.editor.get_content(), tab.editor.get_cursor(), sel))
    }

    /// `h`/`j`/`k`/`l`: move the cursor `count` times. Normal mode drops
    /// the selection first; Select mode extends it instead (the editor's
    /// own shift-motion keeps the anchor, as Vi's Visual mode does). The
    /// count stops mattering past the buffer's length, so it is capped
    /// there.
    fn helix_move(&mut self, code: KeyCode, count: usize) {
        let Some((text, _, _)) = self.helix_state() else {
            return;
        };
        let steps = count.min(text.chars().count().max(1));
        if self.helix_extending() {
            for _ in 0..steps {
                self.editor_key(KeyEvent::new(code, KeyModifiers::SHIFT));
            }
            return;
        }
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.clear_selection();
        }
        for _ in 0..steps {
            self.editor_motion(code);
        }
    }

    /// A selecting motion: select from the cursor to where `f` lands (one
    /// past it when `inclusive`). In Select mode the existing anchor stays
    /// put and only the cursor end moves.
    ///
    /// With a count, `f` runs `count` times and, as in Helix, each step
    /// starts a fresh selection where the last one ended, so `3w` selects
    /// the third word (in Select mode, everything up to it). A motion that
    /// stops moving (at the buffer's end) ends the count early.
    fn helix_select(&mut self, inclusive: bool, count: usize, f: impl Fn(&str, usize) -> usize) {
        let extend = self.helix_extending();
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let text = tab.editor.get_content();
        let len = text.chars().count();
        let pos = tab.editor.get_cursor();
        let (mut prev, mut target) = (pos, f(&text, pos));
        for _ in 1..count {
            let next = f(&text, target);
            if next == target {
                break;
            }
            (prev, target) = (target, next);
        }
        let past = |p: usize| if inclusive { (p + 1).min(len) } else { p };
        let anchor = if extend {
            tab.editor.selection_anchor()
        } else if prev == pos {
            pos
        } else {
            past(prev)
        };
        tab.editor.set_selection_range(anchor, past(target));
    }

    /// `f`/`t`/`F`/`T` + `target`: select up to (`t`/`T`) or through
    /// (`f`/`F`) the next or previous `target` on the line. A miss leaves
    /// everything as it was.
    /// With a count, the `count`th `target` (`3fx`).
    fn helix_find(&mut self, find: char, target: char, count: usize) {
        let Some((text, pos, _)) = self.helix_state() else {
            return;
        };
        let hit = match find {
            'f' => vix_modal::motion::find_char_forward(&text, pos, target, count),
            't' => vix_modal::motion::till_char_forward(&text, pos, target, count),
            'F' => vix_modal::motion::find_char_backward(&text, pos, target, count),
            _ => vix_modal::motion::till_char_backward(&text, pos, target, count),
        };
        let Some(hit) = hit else {
            return;
        };
        self.helix_select(matches!(find, 'f' | 't'), 1, |_, _| hit);
    }

    /// `x`: select the whole line(s) under the selection, or grow a
    /// whole-line selection by one more line; a count adds that many lines
    /// (`2x` on a bare cursor selects two).
    fn helix_select_line(&mut self, count: usize) {
        let Some((text, pos, sel)) = self.helix_state() else {
            return;
        };
        let (mut a, mut b) = sel.unwrap_or((pos, pos));
        for _ in 0..count {
            let next = vix_modal::helix::select_lines(&text, a, b);
            if next == (a, b) {
                break;
            }
            (a, b) = next;
        }
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_selection_range(a, b);
        }
    }

    /// Refuse an edit on a read-only buffer, with the usual status note.
    fn helix_blocked(&mut self) -> bool {
        if self.active_read_only() {
            self.status = t!("status.read_only_blocked").to_string();
            return true;
        }
        false
    }

    /// The text of the effective range (the selection, or the character
    /// under the cursor), with its bounds.
    fn helix_effective(&mut self) -> Option<(usize, usize, String)> {
        let (text, pos, sel) = self.helix_state()?;
        let (start, end) = vix_modal::helix::effective(&text, sel, pos);
        let picked: String = text.chars().skip(start).take(end - start).collect();
        Some((start, end, picked))
    }

    /// Write `text` to the unnamed register (the clipboard), remembering
    /// whether it was a whole-line yank so a Vi paste places it the same
    /// way.
    fn helix_write_register(&mut self, text: String) {
        let kind = if text.ends_with('\n') {
            vix_modal::register::RegisterKind::Line
        } else {
            vix_modal::register::RegisterKind::Char
        };
        self.modal_write_register(text, kind);
    }

    /// `d` (and `c`, when `change`): cut the effective range to the
    /// register, then — for `c` — enter Insert mode where it was.
    fn helix_delete(&mut self, change: bool) {
        if self.helix_cut("") && change {
            self.vim_enter_insert();
        }
    }

    /// Cut the effective range to the register and put `with` in its place
    /// in one edit (so `.` of a `c` is one undo step), leaving the cursor
    /// after it and Normal mode on. Returns false on a read-only buffer.
    fn helix_cut(&mut self, with: &str) -> bool {
        if self.helix_blocked() {
            return false;
        }
        let Some((start, end, removed)) = self.helix_effective() else {
            return true;
        };
        if start < end || !with.is_empty() {
            if start < end {
                self.helix_write_register(removed);
            }
            if let Some(tab) = self.editor.active_tab_mut() {
                tab.editor.set_selection_range(start, end);
                tab.editor.apply(InsertText {
                    text: with.to_string(),
                });
            }
            self.mark_active_dirty();
        }
        self.modal_mode = vix_modal::Mode::Normal;
        true
    }

    /// `y`: copy the effective range to the register; the selection stays.
    fn helix_yank(&mut self) {
        let Some((start, end, picked)) = self.helix_effective() else {
            return;
        };
        if start < end {
            self.helix_write_register(picked);
        }
        self.modal_mode = vix_modal::Mode::Normal;
    }

    /// `p`/`P` (`before`): paste the register (`count` copies of it) after
    /// or before the effective range, then select what was pasted.
    fn helix_paste(&mut self, before: bool, count: usize) {
        if self.helix_blocked() {
            return;
        }
        let Ok(clip) = vix_clipboard::get() else {
            return;
        };
        if clip.is_empty() {
            return;
        }
        let Some((text, pos, sel)) = self.helix_state() else {
            return;
        };
        let (start, end) = vix_modal::helix::effective(&text, sel, pos);
        let clip = clip.repeat(count.max(1));
        let plan = vix_modal::helix::paste(&text, start, end, &clip, before);
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.clear_selection();
            tab.editor.set_cursor(plan.at);
            tab.editor.apply(InsertText { text: plan.text });
            tab.editor.set_selection_range(plan.select.0, plan.select.1);
        }
        self.mark_active_dirty();
        self.modal_mode = vix_modal::Mode::Normal;
    }

    /// `r<char>` and `~`: replace the effective range with `f` of its text,
    /// keeping the result selected.
    fn helix_rewrite(&mut self, f: impl FnOnce(&str) -> String) {
        if self.helix_blocked() {
            return;
        }
        let Some((start, end, picked)) = self.helix_effective() else {
            return;
        };
        if start == end {
            return;
        }
        let new = f(&picked);
        let len = new.chars().count();
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_selection_range(start, end);
            tab.editor.apply(InsertText { text: new });
            tab.editor.set_selection_range(start, start + len);
        }
        self.mark_active_dirty();
    }

    /// `i` (or `a`, when `after`): enter Insert mode at the selection's
    /// start (or end). With nothing selected `a` steps past the cursor's
    /// character, like Vi's `a`.
    fn helix_insert(&mut self, after: bool) {
        self.helix_insert_at(after);
        self.modal_mode = vix_modal::Mode::Normal;
        self.vim_enter_insert();
    }

    /// Where `i` (or `a`, when `after`) starts typing: the selection's
    /// start (or end), the selection dropped.
    fn helix_insert_at(&mut self, after: bool) {
        let Some((_, _, sel)) = self.helix_state() else {
            return;
        };
        match sel {
            Some((start, end)) => {
                if let Some(tab) = self.editor.active_tab_mut() {
                    tab.editor.clear_selection();
                    tab.editor.set_cursor(if after { end } else { start });
                }
            }
            None if after => self.editor_motion(KeyCode::Right),
            None => {}
        }
    }

    /// Remember `change` as what `.` repeats.
    fn helix_record(&mut self, change: Edit) {
        self.helix.last_change = Some(change);
    }

    /// Insert mode just opened for `change` (`c`, `i` or `a`): note where
    /// typing starts, so `Esc` can record what was typed.
    fn helix_begin_insert(&mut self, change: Edit) {
        if !self.flags.contains(AppFlags::MODAL_INSERT) {
            return;
        }
        let at = self
            .editor
            .active_tab()
            .map_or(0, |tab| tab.editor.get_cursor());
        self.helix.insert = Some((at, change));
    }

    /// `Esc` ended an Insert-mode session `c`/`i`/`a` opened: record it,
    /// with the text from where typing started to the cursor, as what `.`
    /// repeats. An `i`/`a` that typed nothing changed nothing and is not
    /// recorded.
    fn helix_finish_insert(&mut self) {
        let Some((start, change)) = self.helix.insert.take() else {
            return;
        };
        let Some((text, cursor, _)) = self.helix_state() else {
            return;
        };
        let typed: String = text
            .chars()
            .skip(start)
            .take(cursor.saturating_sub(start))
            .collect();
        match change {
            Edit::Change(_) => self.helix_record(Edit::Change(typed)),
            Edit::Insert { after, .. } if !typed.is_empty() => {
                self.helix_record(Edit::Insert { after, text: typed });
            }
            _ => {}
        }
    }

    /// `.`: apply the last change again to the current selection (or the
    /// character under the cursor), `count` times.
    fn helix_repeat(&mut self, count: usize) {
        let Some(change) = self.helix.last_change.clone() else {
            return;
        };
        for _ in 0..count.min(MAX_REPEAT) {
            self.helix_apply(&change);
        }
        self.modal_mode = vix_modal::Mode::Normal;
    }

    /// Apply one recorded change, without recording it again.
    fn helix_apply(&mut self, change: &Edit) {
        match change {
            Edit::Delete => self.helix_delete(false),
            Edit::Change(text) => {
                self.helix_cut(text);
            }
            Edit::Insert { after, text } => {
                self.helix_insert_at(*after);
                self.helix_type(text);
            }
            Edit::Replace(with) => {
                let with = *with;
                self.helix_rewrite(|s| vix_modal::helix::replace_chars(s, with));
            }
            Edit::SwitchCase => self.helix_rewrite(vix_modal::helix::swap_case),
            Edit::Indent => self.run_action("indent_selection"),
            Edit::Outdent => self.run_action("outdent_selection"),
            Edit::Paste { before, count } => self.helix_paste(*before, *count),
        }
    }

    /// Type `text` at the cursor, as Insert mode would have.
    fn helix_type(&mut self, text: &str) {
        if text.is_empty() || self.helix_blocked() {
            return;
        }
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.clear_selection();
            tab.editor.apply(InsertText {
                text: text.to_string(),
            });
        }
        self.mark_active_dirty();
    }
}
