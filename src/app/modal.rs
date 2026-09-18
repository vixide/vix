//! `Settings::modal_engine`'s host wiring (`crates/vix-modal/spec/index.md`).
//! T112: Visual / Visual Line entry, exit, and cursor-extending movement.
//! T113: the numeric-count prefix and every pure motion in
//! `vix_modal::motion`, driving Normal-mode movement. T114: `d`/`c`/`y`
//! composing with any T113 motion (via
//! [`vix_modal::motion::MotionKind`]'s exclusive/inclusive/linewise
//! classification), `x` as sugar for `d` + one right motion, `dd`/`cc`/`yy`
//! as sugar for the whole current line, `p`/`P` reading a register, and
//! `"{a-z}` selecting a named register for the next operator or paste. T115
//! (this slice): `i`/`a` + `w`/a delimiter pair/a quote composing with
//! `d`/`c`/`y` the same way a motion does (`vix_modal::text_object`), and
//! `.` dot-repeat — keystroke replay of the last real change (`d{motion}`,
//! a text object, or `p`/`P`; `y` and `c` are excluded, see
//! [`App::apply_operator_to_range`]'s own doc for why `c` specifically).
//! `vim_key`/`spacemacs_key` call [`App::modal_key`] before falling through
//! to `vim_normal_key`'s existing table — everything this doesn't recognize
//! (Visual mode's own motion vocabulary, still just `h j k l` per T112) keeps
//! working exactly as it did before the engine existed.

use super::ModalPending;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vix_modal::motion::MotionKind::{self, Exclusive, Inclusive, Linewise};

use super::App;
use crate::editor_core::actions::InsertText;

impl App {
    /// The modal engine's own key handling, tried before `vim_normal_key`'s
    /// table. Returns `true` if `key` was consumed. A cheap no-op whenever
    /// `Settings::modal_engine` is off (the default) or the mode is
    /// [`vix_modal::Mode::Insert`] (that path is still `modal_insert`,
    /// unchanged, checked by the caller before this ever runs).
    pub(super) fn modal_key(&mut self, key: KeyEvent) -> bool {
        if !self.settings.subsystems.modal_engine {
            return false;
        }
        match self.modal_mode {
            vix_modal::Mode::Normal => self.modal_normal_key(key),
            vix_modal::Mode::Visual | vix_modal::Mode::VisualLine => self.modal_visual_key(key),
            vix_modal::Mode::Insert => false,
        }
    }

    /// Normal-mode dispatch: pending sub-states first (register-select,
    /// `gg`/`f`/`t`/`F`/`T`'s second key, an in-progress operator), then
    /// fresh keys — `"`, digit accumulation, `d`/`c`/`y`/`x`/`p`/`P`, the
    /// T113 motions (via [`Self::modal_try_motion_key`]), and `v`/`V`
    /// (T112). Everything else falls through to `vim_normal_key`.
    fn modal_normal_key(&mut self, key: KeyEvent) -> bool {
        // Dot-repeat (T115): keystroke replay, not semantic replay -- every
        // key seen while nothing is pending starts a fresh recording (the
        // previous one, if it never became a real change, is simply
        // discarded); every key seen with something already pending
        // extends the current one. `Self::apply_operator_to_range` commits
        // the recording to `modal_last_change` at the one point that
        // actually defines "a change" happened; nothing else needs to
        // touch `modal_last_change` -- including replay itself, which runs
        // back through this very function and so naturally re-records
        // whatever it just repeated as the new last change, matching real
        // Vim's own "`.` after `.`" behavior with no special-casing.
        if self.modal_pending == ModalPending::None
            && self.modal_pending_find.is_none()
            && self.modal_pending_text_object.is_none()
            && self.modal_pending_operator.is_none()
            && self.modal_active_register.is_none()
            && self.modal_count.is_empty()
            && self.modal_operator_count == 1
        {
            self.modal_recording.clear();
        }
        self.modal_recording.push(key);

        if self.modal_pending == ModalPending::RegisterSelect {
            self.modal_pending = ModalPending::None;
            if let KeyCode::Char(c @ 'a'..='z') = key.code {
                self.modal_active_register = Some(c);
            }
            return true;
        }
        // A pending `gg` or `f`/`t`/`F`/`T` always resolves (or silently
        // cancels, matching the old table's own `gg`/`dd`/`yy` convention)
        // on the very next key, modified or not — so this runs before the
        // ctrl/alt guard below, which only gates *fresh* dispatch. It also
        // runs regardless of whether an operator is pending underneath it
        // (`dgg`, `df.`, …) — [`Self::apply_modal_motion_fallible`], which
        // this eventually reaches, is what actually checks that.
        if self.modal_pending == ModalPending::G || self.modal_pending_find.is_some() {
            self.modal_resolve_pending(key);
            return true;
        }
        if let Some(ia) = self.modal_pending_text_object {
            self.modal_resolve_text_object(ia, key);
            return true;
        }
        if let Some(op) = self.modal_pending_operator {
            return self.modal_operator_key(op, key);
        }
        if Self::ctrl(&key) || Self::alt(&key) {
            return false;
        }
        if key.code == KeyCode::Char('"') {
            self.modal_pending = ModalPending::RegisterSelect;
            return true;
        }
        // A digit keeps accumulating the count prefix -- unless it's a
        // leading '0', which `Count::push_digit` itself refuses (that's the
        // `0` motion, not the start of a count).
        if let KeyCode::Char(c) = key.code
            && self.modal_count.push_digit(c)
        {
            return true;
        }
        if key.code == KeyCode::Char('.') {
            let override_count = if self.modal_count.is_empty() {
                None
            } else {
                Some(self.modal_count.value())
            };
            self.modal_reset_counts();
            self.modal_dot_repeat(override_count);
            return true;
        }
        if let KeyCode::Char(c @ ('d' | 'c' | 'y')) = key.code {
            self.modal_operator_count = self.modal_count.value();
            self.modal_count.reset();
            self.modal_pending_operator = Some(c);
            return true;
        }
        if key.code == KeyCode::Char('x') {
            // Real Vim: x IS d + one-char-forward motion (dl), not its own
            // code path -- reusing the very same operator machinery is the
            // point, not just matching its visible behavior.
            let count = self.modal_effective_count();
            self.modal_pending_operator = Some('d');
            self.apply_modal_motion(Exclusive, |t, p| vix_modal::motion::char_right(t, p, count));
            return true;
        }
        if let KeyCode::Char(c @ ('p' | 'P')) = key.code {
            self.modal_paste(c == 'P');
            self.modal_reset_counts();
            return true;
        }
        let count = self.modal_effective_count();
        if self.modal_try_motion_key(key, count) {
            return true;
        }
        match key.code {
            KeyCode::Char('v') => {
                self.modal_mode = vix_modal::Mode::Visual;
                self.modal_reset_counts();
                true
            }
            KeyCode::Char('V') => {
                self.modal_mode = vix_modal::Mode::VisualLine;
                let area = self.editor_view();
                self.editor.select_line(area);
                self.modal_reset_counts();
                true
            }
            _ => {
                self.modal_reset_counts();
                false
            }
        }
    }

    /// Keys while an operator (`d`/`c`/`y`, T114) is pending its motion. A
    /// doubled operator key (`dd`/`cc`/`yy`) means "the whole current
    /// line(s), linewise" — real Vim's own shorthand, implemented as sugar
    /// for the linewise `j`-with-`count - 1` motion so it reuses the exact
    /// same range math as every other operator+motion pair. Anything
    /// [`Self::modal_try_motion_key`] recognizes composes with the operator
    /// instead of moving the cursor (decided inside
    /// [`Self::apply_modal_motion_fallible`], not here). An unrecognized key
    /// cancels the pending operator without changing anything.
    fn modal_operator_key(&mut self, op: char, key: KeyEvent) -> bool {
        if Self::ctrl(&key) || Self::alt(&key) {
            self.modal_pending_operator = None;
            self.modal_reset_counts();
            return true;
        }
        if let KeyCode::Char(c) = key.code
            && self.modal_count.push_digit(c)
        {
            return true;
        }
        if key.code == KeyCode::Char(op) {
            let count = self.modal_effective_count();
            self.apply_modal_motion(Linewise, move |t, p| {
                vix_modal::motion::line_down(t, p, count.saturating_sub(1))
            });
            return true;
        }
        // `i`/`a` (T115): a text object is coming, not a motion -- wait for
        // its second key (`w`, `(`, `"`, …) instead of trying it as one.
        if let KeyCode::Char(c @ ('i' | 'a')) = key.code {
            self.modal_pending_text_object = Some(c);
            return true;
        }
        let count = self.modal_effective_count();
        if self.modal_try_motion_key(key, count) {
            return true;
        }
        self.modal_pending_operator = None;
        self.modal_reset_counts();
        true
    }

    /// Resolve a pending text object (T115: `modal_pending_text_object` is
    /// `i` or `a`) now that its second key has arrived. A miss (an
    /// unrecognized `i`/`a` + key combination, or no active tab) cancels
    /// cleanly, same convention as every other pending-key miss.
    fn modal_resolve_text_object(&mut self, ia: char, key: KeyEvent) {
        self.modal_pending_text_object = None;
        let count = self.modal_effective_count();
        self.modal_reset_counts();
        let range = self.modal_text_object_range(ia, key.code, count);
        let Some((start, end)) = range else {
            self.modal_pending_operator = None;
            return;
        };
        let Some(op) = self.modal_pending_operator.take() else {
            return;
        };
        let Some(text) = self.editor.active_tab_mut().map(|t| t.editor.get_content()) else {
            return;
        };
        // Text objects are always a plain character-wise range -- there's
        // no "before/after the cursor" pair to sort or extend the way a
        // motion's landing position needs (`vix_modal::motion::MotionKind`
        // doesn't apply here).
        self.apply_operator_to_range(
            op,
            &text,
            start,
            end,
            vix_modal::register::RegisterKind::Char,
        );
    }

    /// The actual `i`/`a` + object dispatch, kept separate from
    /// [`Self::modal_resolve_text_object`] so that function's borrow of
    /// `self.editor` (for the operator that follows) doesn't overlap this
    /// one's.
    fn modal_text_object_range(
        &self,
        ia: char,
        code: KeyCode,
        count: usize,
    ) -> Option<(usize, usize)> {
        let KeyCode::Char(obj) = code else {
            return None;
        };
        let tab = self.editor.active_tab()?;
        let pos = tab.editor.get_cursor();
        let text = tab.editor.get_content();
        let pair = |open, close| {
            if ia == 'i' {
                vix_modal::text_object::inner_pair(&text, pos, count, open, close)
            } else {
                vix_modal::text_object::around_pair(&text, pos, count, open, close)
            }
        };
        let quote = |q| {
            if ia == 'i' {
                vix_modal::text_object::inner_quote(&text, pos, q)
            } else {
                vix_modal::text_object::around_quote(&text, pos, q)
            }
        };
        match obj {
            'w' if ia == 'i' => vix_modal::text_object::inner_word(&text, pos, count),
            'w' => vix_modal::text_object::around_word(&text, pos, count),
            '(' | ')' | 'b' => pair('(', ')'),
            '{' | '}' | 'B' => pair('{', '}'),
            '[' | ']' => pair('[', ']'),
            '<' | '>' => pair('<', '>'),
            '"' => quote('"'),
            '\'' => quote('\''),
            '`' => quote('`'),
            _ => None,
        }
    }

    /// The T113 motions, tried by both fresh Normal-mode dispatch and
    /// operator-pending dispatch (T114) — the motion itself behaves
    /// identically either way; only [`Self::apply_modal_motion_fallible`]
    /// decides whether the result moves the cursor or feeds an operator.
    /// `count` is the resolved effective count (operator count × motion
    /// count, `1` when neither was typed). Returns `false` for any key that
    /// isn't one of these motions or a motion's own pending second key —
    /// callers decide what that means.
    fn modal_try_motion_key(&mut self, key: KeyEvent, count: usize) -> bool {
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.apply_modal_motion(Exclusive, |t, p| {
                    vix_modal::motion::char_left(t, p, count)
                });
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.apply_modal_motion(Exclusive, |t, p| {
                    vix_modal::motion::char_right(t, p, count)
                });
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.apply_modal_motion(Linewise, |t, p| vix_modal::motion::line_down(t, p, count));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.apply_modal_motion(Linewise, |t, p| vix_modal::motion::line_up(t, p, count));
            }
            KeyCode::Char('0') => {
                self.apply_modal_motion(Exclusive, vix_modal::motion::line_start);
            }
            KeyCode::Char('^') => {
                self.apply_modal_motion(Exclusive, vix_modal::motion::line_first_non_blank);
            }
            KeyCode::Char('$') => {
                // Real Vim's own exception: $ is exclusive for plain
                // movement but inclusive as an operator's motion (`d$`
                // deletes through the line's last character, like `D`).
                self.apply_modal_motion(Inclusive, |t, p| vix_modal::motion::line_end(t, p, count));
            }
            KeyCode::Char('w') => {
                self.apply_modal_motion(Exclusive, |t, p| {
                    vix_modal::motion::word_forward(t, p, count)
                });
            }
            KeyCode::Char('b') => {
                self.apply_modal_motion(Exclusive, |t, p| {
                    vix_modal::motion::word_backward(t, p, count)
                });
            }
            KeyCode::Char('e') => {
                self.apply_modal_motion(Inclusive, |t, p| vix_modal::motion::word_end(t, p, count));
            }
            KeyCode::Char('{') => {
                self.apply_modal_motion(Exclusive, |t, p| {
                    vix_modal::motion::paragraph_backward(t, p, count)
                });
            }
            KeyCode::Char('}') => {
                self.apply_modal_motion(Exclusive, |t, p| {
                    vix_modal::motion::paragraph_forward(t, p, count)
                });
            }
            KeyCode::Char('(') => {
                self.apply_modal_motion(Exclusive, |t, p| {
                    vix_modal::motion::sentence_backward(t, p, count)
                });
            }
            KeyCode::Char(')') => {
                self.apply_modal_motion(Exclusive, |t, p| {
                    vix_modal::motion::sentence_forward(t, p, count)
                });
            }
            // `gg`: a count typed before either `g` applies to the pair as a
            // whole (`5gg` = `gg` with count 5), so it must survive past
            // this first `g` -- only the pending-resolution arm (or an
            // unrelated key cancelling it) resets the counts.
            KeyCode::Char('g') => {
                self.modal_pending = ModalPending::G;
            }
            KeyCode::Char('G') => {
                let line = if self.modal_count.is_empty() && self.modal_operator_count == 1 {
                    None
                } else {
                    Some(count)
                };
                self.modal_goto_line(line);
            }
            // `f`/`t`/`F`/`T`: same "count survives to the second key" rule
            // as `gg`.
            KeyCode::Char(c @ ('f' | 't' | 'F' | 'T')) => {
                self.modal_pending_find = Some(c);
            }
            _ => return false,
        }
        true
    }

    /// Run a pure motion (`fn(text, pos) -> usize`, `count` already applied
    /// by the caller) against the active tab's buffer, moving its cursor —
    /// or, when an operator is pending (T114), feeding `kind` and the
    /// motion's landing position to it instead. A no-op with no active tab.
    fn apply_modal_motion(&mut self, kind: MotionKind, f: impl FnOnce(&str, usize) -> usize) {
        self.apply_modal_motion_fallible(kind, |text, pos| Some(f(text, pos)));
    }

    /// Like [`Self::apply_modal_motion`], for a motion that can fail to find
    /// a target (`f`/`t`/`F`/`T`) — the cursor is left untouched, and any
    /// pending operator is cancelled without changing anything, on `None`.
    fn apply_modal_motion_fallible(
        &mut self,
        kind: MotionKind,
        f: impl FnOnce(&str, usize) -> Option<usize>,
    ) {
        let Some((pos, text)) = self
            .editor
            .active_tab_mut()
            .map(|t| (t.editor.get_cursor(), t.editor.get_content()))
        else {
            return;
        };
        let result = f(&text, pos);
        self.modal_reset_counts();
        let Some(new_pos) = result else {
            self.modal_pending_operator = None;
            return;
        };
        if let Some(op) = self.modal_pending_operator.take() {
            self.run_modal_operator(op, &text, pos, new_pos, kind);
        } else if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_cursor(new_pos);
        }
    }

    /// Apply operator `op` (`d`/`c`/`y`) to the range between `before` (the
    /// cursor when the operator started) and `after` (the motion's landing
    /// position), classified by `kind`. Deletion goes through the same
    /// selection + [`InsertText`] path every other edit uses (undo and
    /// highlighting stay consistent with typing), not a raw buffer
    /// rewrite — `vix_modal::operator`'s own functions only ever compute
    /// positions and extracted text, never touch a live buffer themselves.
    fn run_modal_operator(
        &mut self,
        op: char,
        text: &str,
        before: usize,
        after: usize,
        kind: MotionKind,
    ) {
        let (start, end, reg_kind) = vix_modal::operator::operator_range(text, before, after, kind);
        self.apply_operator_to_range(op, text, start, end, reg_kind);
    }

    /// The part of applying an operator that doesn't care where `(start,
    /// end)` came from — a T113 motion (via [`Self::run_modal_operator`]) or
    /// a T115 text object (via [`Self::modal_resolve_text_object`]), both of
    /// which only differ in how they compute the range in the first place.
    fn apply_operator_to_range(
        &mut self,
        op: char,
        text: &str,
        start: usize,
        end: usize,
        reg_kind: vix_modal::register::RegisterKind,
    ) {
        let (_, removed) = vix_modal::operator::delete_range(text, (start, end));
        match op {
            'y' => {
                self.modal_write_register(removed, reg_kind);
                let cursor = if reg_kind == vix_modal::register::RegisterKind::Line {
                    vix_modal::motion::line_first_non_blank(text, start)
                } else {
                    start
                };
                if let Some(tab) = self.editor.active_tab_mut() {
                    tab.editor.set_cursor(cursor);
                }
            }
            'd' | 'c' => {
                self.modal_write_register(removed, reg_kind);
                if let Some(tab) = self.editor.active_tab_mut() {
                    tab.editor.set_selection_range(start, end);
                    tab.editor.apply(InsertText {
                        text: String::new(),
                    });
                    // A linewise delete (not change -- `c` starts typing
                    // right at the cut, indentation-preservation is a
                    // deliberately unimplemented nuance for v1) lands on the
                    // new current line's first non-blank, matching real
                    // Vim's `dd`/`dj`/`dgg`/... rather than wherever the
                    // raw deletion point happens to fall.
                    if op == 'd' && reg_kind == vix_modal::register::RegisterKind::Line {
                        let content = tab.editor.get_content();
                        let cursor = vix_modal::motion::line_first_non_blank(&content, start);
                        tab.editor.set_cursor(cursor);
                    }
                }
                if op == 'c' {
                    self.vim_enter_insert();
                }
            }
            _ => {}
        }
        // Dot-repeat (T115): only a real buffer mutation is a "change" --
        // `y` never touches the buffer, matching real Vim's own `.` (yank
        // was never dot-repeatable there either). `c` is excluded too, for
        // now: replaying it would need to replay the Insert-mode session
        // that follows, which needs its own recording hook outside this
        // file (the typed keys never reach `App::modal_key` at all, per
        // T112's own Insert-mode-passthrough design) -- a deliberately
        // scoped-out follow-on, not in the spec's own cut list.
        if op == 'd' {
            self.modal_last_change = self.modal_recording.clone();
        }
    }

    /// Write `text` to the register T114's `"{a-z}` selected for this
    /// command, or the unnamed register (real `vix_clipboard`) when none
    /// was — consuming `modal_active_register` either way, since a register
    /// selection only ever applies to the very next operator or paste.
    fn modal_write_register(&mut self, text: String, kind: vix_modal::register::RegisterKind) {
        if let Some(name) = self.modal_active_register.take() {
            self.modal_registers
                .set(name, vix_modal::register::RegisterValue { text, kind });
        } else {
            self.modal_unnamed_kind = kind;
            let _ = vix_clipboard::set(&text);
        }
    }

    /// `p`/`P`: read the selected (or unnamed) register and insert it
    /// relative to the cursor — see [`vix_modal::operator::paste_plan`] for
    /// the char-wise-vs-line-wise placement rule. A no-op if the register
    /// is empty/unset, or there's no active tab.
    fn modal_paste(&mut self, before: bool) {
        let value = if let Some(name) = self.modal_active_register.take() {
            self.modal_registers.get(name).cloned()
        } else {
            vix_clipboard::get()
                .ok()
                .map(|text| vix_modal::register::RegisterValue {
                    text,
                    kind: self.modal_unnamed_kind,
                })
        };
        let Some(value) = value else {
            return;
        };
        let Some((pos, text)) = self
            .editor
            .active_tab_mut()
            .map(|t| (t.editor.get_cursor(), t.editor.get_content()))
        else {
            return;
        };
        let (at, content, cursor) = vix_modal::operator::paste_plan(&text, pos, &value, before);
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        tab.editor.set_cursor(at);
        tab.editor.apply(InsertText { text: content });
        tab.editor.set_cursor(cursor);
        // Dot-repeat (T115): p/P is one of the three change kinds the spec
        // names outright.
        self.modal_last_change = self.modal_recording.clone();
    }

    /// `.` (T115): replay the last recorded change (`modal_last_change`) —
    /// an operator+motion/text-object that actually mutated the buffer, or
    /// `p`/`P` — a no-op when nothing has been recorded yet.
    /// `override_count`, when given (`{count}.`), replaces whatever count
    /// the change was originally recorded with, real Vim's own rule;
    /// `None` replays it exactly as it happened, count included.
    fn modal_dot_repeat(&mut self, override_count: Option<usize>) {
        if self.modal_last_change.is_empty() {
            return;
        }
        let recorded = self.modal_last_change.clone();
        let keys = if let Some(n) = override_count {
            // Splice the override in where the *first* run of digit keys
            // was (after any register prefix, before the operator, or
            // between the operator and the motion — wherever it actually
            // fell), rather than just prepending it — a register prefix
            // must stay first, or replay re-selects the wrong thing. A
            // command recorded with two separate counts (`2d3w`) only has
            // its first one replaced this way; a documented, narrow limit,
            // not silent corruption (an f/t/F/T target can itself be a
            // digit, but always comes after the motion key that starts it,
            // so it can never be mistaken for this first run).
            let is_digit = |k: &KeyEvent| matches!(k.code, KeyCode::Char(c) if c.is_ascii_digit());
            let (before, after) = match recorded.iter().position(is_digit) {
                Some(start) => {
                    let len = recorded[start..].iter().take_while(|k| is_digit(k)).count();
                    (recorded[..start].to_vec(), recorded[start + len..].to_vec())
                }
                None => (recorded.clone(), Vec::new()),
            };
            let mut keys = before;
            keys.extend(
                n.to_string()
                    .chars()
                    .map(|c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)),
            );
            keys.extend(after);
            keys
        } else {
            recorded
        };
        for key in keys {
            self.modal_key(key);
        }
    }

    /// `gg`/`G`: see [`vix_modal::motion::goto_line`] for what `line` means.
    fn modal_goto_line(&mut self, line: Option<usize>) {
        self.apply_modal_motion(Linewise, |t, _pos| vix_modal::motion::goto_line(t, line));
    }

    /// Resolve a pending `f`/`t`/`F`/`T` (`find_key`) now that its target
    /// character has arrived.
    fn modal_find_char(&mut self, find_key: char, target: char, count: usize) {
        self.apply_modal_motion_fallible(Inclusive, |t, p| match find_key {
            'f' => vix_modal::motion::find_char_forward(t, p, target, count),
            't' => vix_modal::motion::till_char_forward(t, p, target, count),
            'F' => vix_modal::motion::find_char_backward(t, p, target, count),
            'T' => vix_modal::motion::till_char_backward(t, p, target, count),
            _ => None,
        });
    }

    /// Resolve a pending `gg` (`ModalPending::G`) or `f`/`t`/`F`/`T`
    /// (`modal_pending_find`) now that its second key has arrived. A miss
    /// (an unexpected key, or one held with ctrl/alt) silently cancels —
    /// both the pending motion and any operator waiting on it — matching
    /// the old table's own convention for `gg`/`dd`/`yy`.
    fn modal_resolve_pending(&mut self, key: KeyEvent) {
        let count = self.modal_effective_count();
        let unmodified = !Self::ctrl(&key) && !Self::alt(&key);
        if self.modal_pending == ModalPending::G {
            self.modal_pending = ModalPending::None;
            if unmodified && key.code == KeyCode::Char('g') {
                self.modal_goto_line(Some(count));
                return;
            }
        } else if let Some(find_key) = self.modal_pending_find.take()
            && unmodified
            && let KeyCode::Char(target) = key.code
        {
            self.modal_find_char(find_key, target, count);
            return;
        }
        self.modal_reset_counts();
        self.modal_pending_operator = None;
    }

    /// The effective count for the next motion: the operator's own count
    /// (`2` in `2dw`, `1` when no operator is pending or it had none)
    /// multiplied by whatever count was typed after it (`3` in `2d3w`) —
    /// the spec's own `{count1}{operator}{count2}{motion}` rule.
    fn modal_effective_count(&self) -> usize {
        self.modal_operator_count
            .saturating_mul(self.modal_count.value())
    }

    /// Clear both count slots back to "nothing typed" — every
    /// mode-affecting key (a motion firing, an operator resolving or
    /// cancelling, `Esc`, entering Insert, …) resets them.
    fn modal_reset_counts(&mut self) {
        self.modal_count.reset();
        self.modal_operator_count = 1;
    }

    /// Visual / Visual Line mode keys: `Esc` returns to Normal (collapsing
    /// the selection to the cursor); `h j k l` and the arrow keys extend it.
    /// Anything else falls through unconsumed — T114's operators don't yet
    /// reach into Visual mode (they compose with T113 motions and
    /// `dd`/`cc`/`yy`/`x` only); a future slice is where a Visual selection
    /// becomes a third thing `d`/`c`/`y` can act on.
    fn modal_visual_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Esc {
            self.modal_mode = vix_modal::Mode::Normal;
            if let Some(tab) = self.editor.active_tab_mut() {
                tab.editor.clear_selection();
            }
            return true;
        }
        if Self::ctrl(&key) || Self::alt(&key) {
            return false;
        }
        let step = match key.code {
            KeyCode::Char('h') | KeyCode::Left => Some(KeyCode::Left),
            KeyCode::Char('j') | KeyCode::Down => Some(KeyCode::Down),
            KeyCode::Char('k') | KeyCode::Up => Some(KeyCode::Up),
            KeyCode::Char('l') | KeyCode::Right => Some(KeyCode::Right),
            _ => None,
        };
        let Some(step) = step else {
            return false;
        };
        // The real shift-extend mechanism: the editor's own selection model
        // already tracks a stable anchor across repeated extends/shrinks
        // (`Ctrl+Shift+Right`/`Left` already relies on the same plumbing —
        // `spec/index.md`'s audit confirms it's end-to-end already), so
        // there's no need for the modal engine to track its own anchor.
        self.editor_key(KeyEvent::new(step, KeyModifiers::SHIFT));
        if self.modal_mode == vix_modal::Mode::VisualLine {
            self.snap_visual_selection_to_whole_lines();
        }
        true
    }

    /// Re-snap the active tab's selection to whole lines, keeping the
    /// cursor on whichever end the selection is currently growing toward.
    /// `Editor::select_line` can't do this itself — it always selects just
    /// the cursor's current line fresh, with no memory of how far a
    /// multi-line Visual Line selection has already grown.
    fn snap_visual_selection_to_whole_lines(&mut self) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let cursor = tab.editor.get_cursor();
        let Some(sel) = tab.editor.get_selection() else {
            return;
        };
        let code = tab.editor.code_ref();
        let start_row = code.char_to_line(sel.start);
        // `sel.end` is exclusive, so back it up one character before asking
        // which line it's on -- otherwise a selection ending right at a
        // line break would misreport itself as reaching into the next line.
        let end_row = code.char_to_line(sel.end.saturating_sub(1).max(sel.start));
        let line_start = code.line_to_char(start_row);
        let line_end = if end_row + 1 < code.len_lines() {
            code.line_to_char(end_row + 1)
        } else {
            line_start
                + (start_row..=end_row)
                    .map(|r| code.line_len(r))
                    .sum::<usize>()
        };
        // Keep the cursor on the same end the native shift-motion just left
        // it on, so growing the selection downward keeps the caret visually
        // at the bottom (and upward, at the top) instead of snapping it to
        // a fixed end regardless of direction.
        if code.char_to_line(cursor) <= start_row {
            tab.editor.set_selection_range(line_end, line_start);
        } else {
            tab.editor.set_selection_range(line_start, line_end);
        }
    }
}
