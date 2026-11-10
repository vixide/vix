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

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, AppFlags, Focus};
use crate::editor_core::actions::InsertText;

/// A buffer snapshot: its text, the cursor, and the sorted non-empty
/// selection, if any.
type Snapshot = (String, usize, Option<(usize, usize)>);

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
        match key.code {
            // Esc leaves Select mode; the selection stays, as in Helix.
            KeyCode::Esc => self.modal_mode = vix_modal::Mode::Normal,
            KeyCode::Char(c @ ('g' | 'm' | ' ' | 'f' | 't' | 'F' | 'T' | 'r')) => {
                self.vim_pending = Some(c);
            }
            _ => {
                let token = crate::macros::encode_key(key);
                if let Some(action) = vix_keybindings::lookup("helix", "", &token) {
                    self.run_action(action);
                }
            }
        }
        // Swallow every other Normal-mode key so it never types.
        true
    }

    /// The key after a pending `g`/`m`/`Space` prefix (a table lookup in
    /// that context) or `f`/`t`/`F`/`T`/`r` (its character argument). `Esc`
    /// or a miss cancels silently, like Vi's pending operators.
    fn helix_pending_key(&mut self, pending: char, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            return;
        }
        if let Some(context) = prefix_context(pending) {
            let token = crate::macros::encode_key(key);
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
        } else {
            self.helix_find(pending, target);
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

    /// Dispatch a `helix.*` action. Returns whether `action` was one.
    pub(super) fn run_helix_action(&mut self, action: &str) -> bool {
        match action {
            "helix.move_left" => self.helix_move(KeyCode::Left),
            "helix.move_right" => self.helix_move(KeyCode::Right),
            "helix.move_up" => self.helix_move(KeyCode::Up),
            "helix.move_down" => self.helix_move(KeyCode::Down),
            "helix.select_word_next" => {
                self.helix_select(false, |t, p| vix_modal::motion::word_forward(t, p, 1));
            }
            "helix.select_word_prev" => {
                self.helix_select(false, |t, p| vix_modal::motion::word_backward(t, p, 1));
            }
            "helix.select_word_end" => {
                self.helix_select(true, |t, p| vix_modal::motion::word_end(t, p, 1));
            }
            "helix.select_long_word_next" => {
                self.helix_select(false, |t, p| vix_modal::helix::long_word_forward(t, p, 1));
            }
            "helix.select_long_word_prev" => {
                self.helix_select(false, |t, p| vix_modal::helix::long_word_backward(t, p, 1));
            }
            "helix.select_long_word_end" => {
                self.helix_select(true, |t, p| vix_modal::helix::long_word_end(t, p, 1));
            }
            "helix.select_line" => self.helix_select_line(),
            "helix.delete" => self.helix_delete(false),
            "helix.change" => self.helix_delete(true),
            "helix.yank" => self.helix_yank(),
            "helix.paste_after" => self.helix_paste(false),
            "helix.paste_before" => self.helix_paste(true),
            "helix.switch_case" => self.helix_rewrite(vix_modal::helix::swap_case),
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
            "helix.insert" => self.helix_insert(false),
            "helix.append" => self.helix_insert(true),
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

    /// `h`/`j`/`k`/`l`: move the cursor. Normal mode drops the selection
    /// first; Select mode extends it instead (the editor's own
    /// shift-motion keeps the anchor, as Vi's Visual mode does).
    fn helix_move(&mut self, code: KeyCode) {
        if self.helix_extending() {
            self.editor_key(KeyEvent::new(code, KeyModifiers::SHIFT));
            return;
        }
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.clear_selection();
        }
        self.editor_motion(code);
    }

    /// A selecting motion: select from the cursor to where `f` lands (one
    /// past it when `inclusive`). In Select mode the existing anchor stays
    /// put and only the cursor end moves.
    fn helix_select(&mut self, inclusive: bool, f: impl FnOnce(&str, usize) -> usize) {
        let extend = self.helix_extending();
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let text = tab.editor.get_content();
        let pos = tab.editor.get_cursor();
        let target = f(&text, pos);
        let cursor = if inclusive {
            (target + 1).min(text.chars().count())
        } else {
            target
        };
        let anchor = if extend {
            tab.editor.selection_anchor()
        } else {
            pos
        };
        tab.editor.set_selection_range(anchor, cursor);
    }

    /// `f`/`t`/`F`/`T` + `target`: select up to (`t`/`T`) or through
    /// (`f`/`F`) the next or previous `target` on the line. A miss leaves
    /// everything as it was.
    fn helix_find(&mut self, find: char, target: char) {
        let Some((text, pos, _)) = self.helix_state() else {
            return;
        };
        let hit = match find {
            'f' => vix_modal::motion::find_char_forward(&text, pos, target, 1),
            't' => vix_modal::motion::till_char_forward(&text, pos, target, 1),
            'F' => vix_modal::motion::find_char_backward(&text, pos, target, 1),
            _ => vix_modal::motion::till_char_backward(&text, pos, target, 1),
        };
        let Some(hit) = hit else {
            return;
        };
        self.helix_select(matches!(find, 'f' | 't'), |_, _| hit);
    }

    /// `x`: select the whole line(s) under the selection, or grow a
    /// whole-line selection by one more line.
    fn helix_select_line(&mut self) {
        let Some((text, pos, sel)) = self.helix_state() else {
            return;
        };
        let (start, end) = sel.unwrap_or((pos, pos));
        let (a, b) = vix_modal::helix::select_lines(&text, start, end);
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
        if self.helix_blocked() {
            return;
        }
        let Some((start, end, removed)) = self.helix_effective() else {
            return;
        };
        if start < end {
            self.helix_write_register(removed);
            if let Some(tab) = self.editor.active_tab_mut() {
                tab.editor.set_selection_range(start, end);
                tab.editor.apply(InsertText {
                    text: String::new(),
                });
            }
            self.mark_active_dirty();
        }
        self.modal_mode = vix_modal::Mode::Normal;
        if change {
            self.vim_enter_insert();
        }
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

    /// `p`/`P` (`before`): paste the register after or before the
    /// effective range, then select what was pasted.
    fn helix_paste(&mut self, before: bool) {
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
        let Some((_, _, sel)) = self.helix_state() else {
            self.vim_enter_insert();
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
        self.modal_mode = vix_modal::Mode::Normal;
        self.vim_enter_insert();
    }
}
