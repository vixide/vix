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
//! a text object, or `p`/`P`; not `y`). T837: a change that goes on into
//! Insert mode (`c`, `i`, `o`, …) is recorded through the `Esc` that ends
//! it ([`InsertRecord`]) and folded into one undo step.
//! `vim_key`/`spacemacs_key` call [`App::modal_key`] before falling through
//! to `vim_normal_key`'s existing table — everything this doesn't recognize
//! (Visual mode's own motion vocabulary, still just `h j k l` per T112) keeps
//! working exactly as it did before the engine existed.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use super::{AppFlags, ModalPending};
use crate::geo::Geo;
use ropey::Rope;
use vix_input::{KeyCode, KeyEvent, KeyModifiers};
use vix_modal::motion::MotionKind::{self, Exclusive, Inclusive, Linewise};
use vix_modal::text::Text;

use super::App;
use crate::editor_core::actions::InsertText;

/// The cells a tab takes on screen, for the Vi keymap's display columns
/// (T838): the editor draws a tab as one cell (`vix-editor-core`'s render
/// and wrap layout), and `j`/`k` keep the column the user sees.
const VI_TAB_CELLS: usize = 1;
use crate::editor_core::textobject::{TextObjectDir, TextObjectKind};

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
            // Where this command's change starts in the undo tree (T837):
            // leaving the Insert mode it may go on into folds everything
            // since into one undo step.
            self.modal_recording_seq = self.modal_history_seq();
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
        if matches!(
            self.modal_pending,
            ModalPending::G | ModalPending::BracketNext | ModalPending::BracketPrev
        ) || self.modal_pending_find.is_some()
        {
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
            // `x` is `dl`, and `dl` takes `count` displayed characters up to
            // the line's end, so `$x` and `9x` reach the last one (T838).
            self.modal_pending_operator = Some('d');
            self.apply_modal_motion(Exclusive, |t, p| {
                vix_modal::motion::vi_chars_end(t, p, count)
            });
            return true;
        }
        if let KeyCode::Char(c @ ('p' | 'P')) = key.code {
            let count = self.modal_effective_count();
            self.modal_paste(c == 'P', count);
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
                self.modal_visual_exact = None;
                self.modal_reset_counts();
                true
            }
            KeyCode::Char('V') => {
                self.modal_mode = vix_modal::Mode::VisualLine;
                self.modal_visual_exact = None;
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
                vix_modal::motion::line_down(t, p, count.minus(1))
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
        let Some(text) = self
            .editor
            .active_tab_mut()
            .map(|t| t.editor.code_ref().rope().clone())
        else {
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
            // Tree-sitter textobjects (T616): function, class, parameter.
            'f' | 'c' | 'a' => {
                self.textobject_at_cursor(Self::textobject_kind(obj)?, ia == 'i', count)
            }
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
                    vix_modal::motion::vi_char_left(t, p, count)
                });
            }
            // With an operator, `l` may reach the line's end (`dl` is `x`);
            // as a cursor motion it stops on the last character (T838).
            KeyCode::Char('l') | KeyCode::Right if self.modal_pending_operator.is_some() => {
                self.apply_modal_motion(Exclusive, |t, p| {
                    vix_modal::motion::vi_chars_end(t, p, count)
                });
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.apply_modal_motion(Exclusive, |t, p| {
                    vix_modal::motion::vi_char_right(t, p, count)
                });
            }
            KeyCode::Char('j') | KeyCode::Down => self.modal_vertical(count, true),
            KeyCode::Char('k') | KeyCode::Up => self.modal_vertical(count, false),
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
                let moving = self.modal_pending_operator.is_none();
                self.apply_modal_motion(Inclusive, |t, p| vix_modal::motion::line_end(t, p, count));
                // After `$`, `j`/`k` keep to each line's end (Vim's
                // `curswant` of MAXCOL).
                if moving {
                    self.modal_curswant = self.editor.active_tab().map(|t| {
                        (
                            usize::MAX,
                            t.editor.get_cursor(),
                            t.editor.code_ref().len_chars(),
                        )
                    });
                }
            }
            // The Vi keymap's own word classes (T838): `pkg::run` is three
            // words. With an operator, the last word stops at its line's end.
            KeyCode::Char('w') if self.modal_pending_operator.is_some() => {
                self.apply_modal_motion(Exclusive, |t, p| {
                    vix_modal::motion::vi_word_forward_for_operator(t, p, count)
                });
            }
            KeyCode::Char('w') => {
                self.apply_modal_motion(Exclusive, |t, p| {
                    vix_modal::motion::vi_word_forward(t, p, count)
                });
            }
            KeyCode::Char('b') => {
                self.apply_modal_motion(Exclusive, |t, p| {
                    vix_modal::motion::vi_word_backward(t, p, count)
                });
            }
            KeyCode::Char('e') => {
                self.apply_modal_motion(Inclusive, |t, p| {
                    vix_modal::motion::vi_word_end(t, p, count)
                });
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
            // `]f`/`[f` and friends (T616): same "count survives to the
            // second key" rule as `gg`.
            KeyCode::Char(']') => self.modal_pending = ModalPending::BracketNext,
            KeyCode::Char('[') => self.modal_pending = ModalPending::BracketPrev,
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
    fn apply_modal_motion(&mut self, kind: MotionKind, f: impl FnOnce(&Rope, usize) -> usize) {
        self.apply_modal_motion_fallible(kind, |text, pos| Some(f(text, pos)));
    }

    /// Like [`Self::apply_modal_motion`], for a motion that can fail to find
    /// a target (`f`/`t`/`F`/`T`) — the cursor is left untouched, and any
    /// pending operator is cancelled without changing anything, on `None`.
    fn apply_modal_motion_fallible(
        &mut self,
        kind: MotionKind,
        f: impl FnOnce(&Rope, usize) -> Option<usize>,
    ) {
        // The buffer's rope, shared in O(1) rather than copied out (T843):
        // the motion walks only the text it needs from the cursor.
        let Some((pos, text)) = self
            .editor
            .active_tab_mut()
            .map(|t| (t.editor.get_cursor(), t.editor.code_ref().rope().clone()))
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
            // A Normal-mode cursor rests on a grapheme, never on a line's
            // end; an operator's range may end there (T838).
            tab.editor
                .set_cursor(vix_modal::motion::vi_normal_cursor(&text, new_pos));
        }
    }

    /// `j`/`k` (`down` picks which): with an operator, the line-wise motion;
    /// as a cursor motion, onto the preferred display column carried across a
    /// run of vertical motions (Vim's `curswant`, T838) — kept while the
    /// cursor is still where the last `j`/`k`/`$` left it in an unchanged
    /// buffer, otherwise the cursor's own column.
    fn modal_vertical(&mut self, count: usize, down: bool) {
        if self.modal_pending_operator.is_some() {
            self.apply_modal_motion(Linewise, |t, p| {
                if down {
                    vix_modal::motion::line_down(t, p, count)
                } else {
                    vix_modal::motion::line_up(t, p, count)
                }
            });
            return;
        }
        let stored = self.modal_curswant;
        let mut landed = None;
        self.apply_modal_motion(Linewise, |t, p| {
            let len = t.len_chars();
            let want = match stored {
                Some((want, at, was)) if at == p && was == len => want,
                _ => vix_modal::motion::display_column(t, p, VI_TAB_CELLS),
            };
            let to = if down {
                vix_modal::motion::line_down_to_column(t, p, count, want, VI_TAB_CELLS)
            } else {
                vix_modal::motion::line_up_to_column(t, p, count, want, VI_TAB_CELLS)
            };
            landed = Some((want, to, len));
            to
        });
        self.modal_curswant = landed;
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
        text: &Rope,
        before: usize,
        after: usize,
        kind: MotionKind,
    ) {
        // `cw` (T837): Vim changes to the end of the word and keeps the
        // white space after it. The `w` motion is the only exclusive one
        // whose key is `w` (`fw`/`tw` are inclusive), and the recording's
        // last key is the motion key that just resolved.
        let after = if op == 'c'
            && kind == Exclusive
            && self.modal_recording.last().map(|k| k.code) == Some(KeyCode::Char('w'))
        {
            vix_modal::operator::change_word_end(text, before, after)
        } else {
            after
        };
        let (start, end, reg_kind) =
            vix_modal::operator::vi_operator_range(text, before, after, kind);
        self.apply_operator_to_range(op, text, start, end, reg_kind);
    }

    /// The part of applying an operator that doesn't care where `(start,
    /// end)` came from — a T113 motion (via [`Self::run_modal_operator`]) or
    /// a T115 text object (via [`Self::modal_resolve_text_object`]), both of
    /// which only differ in how they compute the range in the first place.
    fn apply_operator_to_range<T: Text + ?Sized>(
        &mut self,
        op: char,
        text: &T,
        start: usize,
        end: usize,
        reg_kind: vix_modal::register::RegisterKind,
    ) {
        // An empty character-wise range (`x` on an empty line, `d$` there)
        // changes nothing and leaves the register alone, as in Vim; `c` still
        // goes on into Insert mode.
        if op != 'c' && start >= end && reg_kind == vix_modal::register::RegisterKind::Char {
            return;
        }
        let removed = text.range_string(start, end);
        // A line-wise change (`cc`, `Vc`) keeps one empty line to type
        // into, as Vim does: the register still gets the whole lines, but
        // the final line break stays in the buffer.
        let end = if op == 'c'
            && reg_kind == vix_modal::register::RegisterKind::Line
            && end > start
            && text.char_at(end.minus(1)) == Some('\n')
        {
            end.minus(1)
        } else {
            end
        };
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
                        let content = tab.editor.code_ref().rope();
                        let cursor = vix_modal::motion::line_first_non_blank(content, start);
                        tab.editor.set_cursor(cursor);
                    } else if op == 'd' {
                        // A character-wise delete that took a line's last
                        // characters leaves the cursor on the new last one,
                        // not on the line break (`$x`, `dw` on a last word).
                        let content = tab.editor.code_ref().rope();
                        let cursor = vix_modal::motion::vi_normal_cursor(content, start);
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
        // was never dot-repeatable there either). `c` is recorded when its
        // Insert-mode session ends (T837, `Self::modal_finish_insert_record`),
        // so `.` replays the typed text too.
        if op == 'd' {
            self.modal_last_change = self.modal_recording.clone();
            self.modal_last_change_insert.clear();
        }
    }

    /// Write `text` to the register T114's `"{a-z}` selected for this
    /// command, or the unnamed register (real `vix_clipboard`) when none
    /// was — consuming `modal_active_register` either way, since a register
    /// selection only ever applies to the very next operator or paste.
    pub(super) fn modal_write_register(
        &mut self,
        text: String,
        kind: vix_modal::register::RegisterKind,
    ) {
        if let Some(name) = self.modal_active_register.take() {
            self.modal_registers
                .set(name, vix_modal::register::RegisterValue { text, kind });
        } else {
            self.modal_unnamed_kind = kind;
            if let Some(tab) = self.editor.active_tab_mut() {
                tab.editor.copy_to_clipboard(&text);
            } else {
                // No tab to hold the in-memory fallback: use the bare helper.
                vix_clipboard::set(&text).ok();
            }
        }
    }

    /// `p`/`P`: read the selected (or unnamed) register and insert it
    /// `count` times (`3p`, T840) relative to the cursor, as one insertion
    /// and so one undo step — see [`vix_modal::operator::paste_plan`] for
    /// the char-wise-vs-line-wise placement rule. A no-op if the register
    /// is empty/unset, or there's no active tab.
    fn modal_paste(&mut self, before: bool, count: usize) {
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
        let Some((at, content, cursor)) =
            vix_modal::operator::paste_plan_counted(&text, pos, &value, before, count)
        else {
            self.messages.warn(t!("msg.vi_put_too_long").to_string());
            return;
        };
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        tab.editor.set_cursor(at);
        tab.editor.apply(InsertText { text: content });
        tab.editor.set_cursor(cursor);
        // Dot-repeat (T115): p/P is one of the three change kinds the spec
        // names outright.
        self.modal_last_change = self.modal_recording.clone();
        self.modal_last_change_insert.clear();
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
            // With no count recorded, the new one goes first -- after a
            // register prefix, which must stay first.
            let (start, len) = if let Some(start) = recorded.iter().position(is_digit) {
                let len = recorded
                    .get(start..)
                    .unwrap_or_default()
                    .iter()
                    .take_while(|k| is_digit(k))
                    .count();
                (start, len)
            } else {
                match recorded.as_slice() {
                    [q, _, ..] if q.code == KeyCode::Char('"') => (2, 0),
                    _ => (0, 0),
                }
            };
            let before = recorded.get(..start).unwrap_or_default().to_vec();
            let after = recorded.get(start.plus(len)..).unwrap_or_default().to_vec();
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
        let typed = self.modal_last_change_insert.clone();
        // The Normal-mode part runs through the same two layers `vim_key`
        // tries (`i`/`o`/`A`/… live in the old table), so a text object or
        // motion is recomputed where the cursor is now; then, only if that
        // put the editor back in Insert mode, the typed keys, through the
        // same Insert-mode path typing uses (T837).
        for key in keys {
            if !self.modal_key(key) {
                self.vim_normal_key(key);
            }
        }
        self.modal_after_normal_key();
        if self.flags.contains(AppFlags::MODAL_INSERT) {
            for key in typed {
                if !self.modal_insert_key(key) {
                    self.editor_key(key);
                }
            }
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
        } else if matches!(
            self.modal_pending,
            ModalPending::BracketNext | ModalPending::BracketPrev
        ) {
            let dir = if self.modal_pending == ModalPending::BracketNext {
                TextObjectDir::Next
            } else {
                TextObjectDir::Previous
            };
            self.modal_pending = ModalPending::None;
            if let KeyCode::Char(c) = key.code
                && unmodified
                && let Some(kind) = Self::textobject_kind(c)
            {
                self.modal_textobject_motion(kind, dir, count);
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
        self.modal_operator_count.times(self.modal_count.value())
    }

    /// Clear both count slots back to "nothing typed" — every
    /// mode-affecting key (a motion firing, an operator resolving or
    /// cancelling, `Esc`, entering Insert, …) resets them.
    fn modal_reset_counts(&mut self) {
        self.modal_count.reset();
        self.modal_operator_count = 1;
    }

    /// Visual / Visual Line mode keys: `Esc` returns to Normal (collapsing
    /// the selection to the cursor); `h j k l` and the arrow keys extend it;
    /// `i`/`a` select a text object; `"{a-z}` picks a register and
    /// `d`/`x`/`c`/`y` apply an operator to the selection (T840,
    /// [`Self::modal_visual_operator`]). Anything else falls through
    /// unconsumed.
    fn modal_visual_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Esc {
            self.modal_pending_text_object = None;
            self.modal_visual_exact = None;
            if self.modal_pending == ModalPending::RegisterSelect {
                self.modal_pending = ModalPending::None;
            }
            self.modal_mode = vix_modal::Mode::Normal;
            if let Some(tab) = self.editor.active_tab_mut() {
                tab.editor.clear_selection();
            }
            return true;
        }
        if let Some(ia) = self.modal_pending_text_object.take() {
            self.modal_visual_text_object(ia, key.code);
            return true;
        }
        if self.modal_pending == ModalPending::RegisterSelect {
            self.modal_pending = ModalPending::None;
            if let KeyCode::Char(c @ 'a'..='z') = key.code {
                self.modal_active_register = Some(c);
            }
            return true;
        }
        if Self::ctrl(&key) || Self::alt(&key) {
            return false;
        }
        if let KeyCode::Char(c @ ('i' | 'a')) = key.code {
            self.modal_pending_text_object = Some(c);
            return true;
        }
        match key.code {
            KeyCode::Char('"') => {
                self.modal_pending = ModalPending::RegisterSelect;
                return true;
            }
            // `x` is `d` in Visual mode, as in Vim.
            KeyCode::Char(c @ ('d' | 'x' | 'c' | 'y')) => {
                self.modal_visual_operator(if c == 'x' { 'd' } else { c });
                return true;
            }
            _ => {}
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
        if self.modal_mode == vix_modal::Mode::VisualLine
            && step == KeyCode::Down
            && self.extend_visual_line_onto_final_line()
        {
            return true;
        }
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

    /// The textobject kind a modal object letter names (T616): `f`
    /// function (`m` too after `]`/`[`, Vim's own method motions), `c`
    /// class, `a` parameter (argument).
    fn textobject_kind(c: char) -> Option<TextObjectKind> {
        match c {
            'f' | 'm' => Some(TextObjectKind::Function),
            'c' => Some(TextObjectKind::Class),
            'a' => Some(TextObjectKind::Parameter),
            _ => None,
        }
    }

    /// `]f`/`[f`/`]c`/`[c`/`]a`/`[a` (T616): jump to the `count`-th next or
    /// previous textobject start — or, with an operator pending, apply it
    /// up to there (an exclusive motion, like `w`).
    fn modal_textobject_motion(&mut self, kind: TextObjectKind, dir: TextObjectDir, count: usize) {
        match self.textobject_jump_target(kind, dir, count) {
            Ok((target, _)) => self.apply_modal_motion_fallible(Exclusive, |_, _| Some(target)),
            Err(miss) => {
                self.modal_reset_counts();
                self.modal_pending_operator = None;
                self.report_textobject_miss(miss);
            }
        }
    }

    /// Visual mode `i`/`a` + an object (T616): select it — for the
    /// Tree-sitter objects, growing to the next enclosing one when the
    /// selection already covers this one, so `vafaf` selects the outer
    /// function.
    fn modal_visual_text_object(&mut self, ia: char, code: KeyCode) {
        let range = match code {
            KeyCode::Char(c) if Self::textobject_kind(c).is_some() && c != 'm' => {
                let Some(kind) = Self::textobject_kind(c) else {
                    return;
                };
                match self.textobject_selection(kind, ia == 'i', 1) {
                    Ok(r) => Some(r),
                    Err(miss) => {
                        self.report_textobject_miss(miss);
                        None
                    }
                }
            }
            _ => self.modal_text_object_range(ia, code, 1),
        };
        if let Some((start, end)) = range {
            self.modal_mode = vix_modal::Mode::Visual;
            self.select_remembering(start, end);
            self.modal_visual_exact = Some((start, end));
        }
    }

    /// `d`/`c`/`y` (and `x`, passed here as `d`) in Visual or Visual Line
    /// mode (T840): apply the operator to the selection through the same
    /// [`Self::apply_operator_to_range`] a motion or text object uses, then
    /// return to Normal mode (`c` goes on into Insert). Character-wise Visual
    /// writes a character-wise register and Visual Line a line-wise one; see
    /// [`vix_modal::operator::visual_range`] for the inclusive range rule. A
    /// text object's own selection is used exactly as it was selected.
    /// Visual operators are not dot-repeatable yet, so a `d` or `c` here
    /// clears what `.` replays rather than leave an older change in place
    /// (T837); a `c` and the text typed after it are still one undo step.
    fn modal_visual_operator(&mut self, op: char) {
        let linewise = self.modal_mode == vix_modal::Mode::VisualLine;
        let seq = self.modal_history_seq();
        let exact = self.modal_visual_exact.take();
        self.modal_mode = vix_modal::Mode::Normal;
        self.modal_reset_counts();
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let text = tab.editor.get_content();
        let cursor = tab.editor.get_cursor();
        let selection = tab.editor.get_selection().map(|s| (s.start, s.end));
        tab.editor.clear_selection();
        let (start, end, kind) = match (exact, selection) {
            (Some(range), Some(sel)) if !linewise && range == sel => {
                (sel.0, sel.1, vix_modal::register::RegisterKind::Char)
            }
            _ => vix_modal::operator::visual_range(&text, selection, cursor, linewise),
        };
        let last_change = std::mem::take(&mut self.modal_last_change);
        self.apply_operator_to_range(op, &text, start, end, kind);
        if op == 'y' {
            self.modal_last_change = last_change;
        } else {
            self.modal_last_change.clear();
            self.modal_last_change_insert.clear();
            // No Normal-mode keys to replay: the Insert session `c` opens is
            // grouped for undo but not recorded for `.`.
            self.modal_recording.clear();
            self.modal_recording_seq = seq;
        }
    }

    /// `j` in Visual Line mode when the selection ends just above the buffer's
    /// final line: select that line too, and report whether it did.
    /// A whole-line selection's end is exclusive, so the cursor already sits at
    /// the start of the next line; when that line is the last one the native
    /// shift-down has no line below to move to and would leave it out. (With
    /// a line after it, the cursor moves onto that line and the snap includes
    /// this one.) The final line, with no line break after it, ends at the end
    /// of the buffer. An empty final line (the buffer ends in a newline) is
    /// left to the native motion, which reaches it.
    fn extend_visual_line_onto_final_line(&mut self) -> bool {
        let Some(tab) = self.editor.active_tab_mut() else {
            return false;
        };
        let Some(sel) = tab.editor.get_selection() else {
            return false;
        };
        let cursor = tab.editor.get_cursor();
        let code = tab.editor.code_ref();
        let final_start = code.line_to_char(code.len_lines().minus(1));
        let end = code.len_chars();
        if sel.end != final_start || final_start >= end || cursor != sel.end {
            return false;
        }
        tab.editor.set_selection_range(sel.start, end);
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
        let end_row = code.char_to_line(sel.end.minus(1).max(sel.start));
        let line_start = code.line_to_char(start_row);
        // The end of the last selected line: the start of the next line, or --
        // when the selection reaches the buffer's final line, which has no line
        // break after it -- the end of the buffer. (Summing the lines' lengths
        // instead left out the line breaks between them, so a selection of two
        // or more lines ending on the last line fell short.)
        let line_end = if end_row.plus(1) < code.len_lines() {
            code.line_to_char(end_row.plus(1))
        } else {
            code.len_chars()
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

/// An Insert-mode session being recorded as part of a Vi change (T837): the
/// Normal-mode keys that opened it (`ciw`, `cw`, `o`, `3A`, …), the keys
/// typed in it, and where in the undo tree the change began.
#[derive(Debug, Clone, Default)]
pub(super) struct InsertRecord {
    /// The Normal-mode keys of the command that entered Insert mode. Empty
    /// when there are none to replay (a Visual `c`).
    normal: Vec<KeyEvent>,
    /// The keys typed in Insert mode so far: text, `Enter`, `Tab`,
    /// `Backspace`, `Delete`.
    typed: Vec<KeyEvent>,
    /// The undo state when the change began (`History::current`).
    start_seq: Option<usize>,
    /// `.` can replay the session: no key or edit it cannot replay came in.
    replayable: bool,
    /// The session's edits fold into one undo step on `Esc` (a cursor key
    /// in Insert mode ends that, as it starts a new undo step in Vim).
    grouped: bool,
    /// The key being dispatched reached [`App::modal_insert_key`].
    noted: bool,
}

impl App {
    /// The active tab's current undo state (`History::current`), if any.
    fn modal_history_seq(&self) -> Option<usize> {
        self.editor
            .active_tab()
            .map(|t| t.editor.code_ref().history().current())
    }

    /// After a Vi/Spacemacs Normal-mode key (or a replayed change): when it
    /// entered Insert mode, start recording the session as part of the
    /// change the recording holds (T837). A no-op with the modal engine
    /// off, or when a session is already being recorded.
    pub(super) fn modal_after_normal_key(&mut self) {
        if !self.settings.subsystems.modal_engine
            || !self.flags.contains(AppFlags::MODAL_INSERT)
            || self.modal_insert_record.is_some()
        {
            return;
        }
        self.modal_insert_record = Some(InsertRecord {
            normal: self.modal_recording.clone(),
            typed: Vec::new(),
            start_seq: self.modal_recording_seq,
            replayable: true,
            grouped: true,
            noted: false,
        });
    }

    /// A key in Vi/Spacemacs Insert mode. `Esc` returns to Normal mode and
    /// ends the session's record (`true`: consumed); every other key is
    /// noted in the record and left for the editor to type (`false`).
    pub(super) fn modal_insert_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Esc {
            self.flags.remove(AppFlags::MODAL_INSERT);
            self.modal_mode = vix_modal::Mode::Normal;
            self.modal_finish_insert_record();
            return true;
        }
        let plain = !Self::ctrl(&key) && !Self::alt(&key);
        let Some(rec) = self.modal_insert_record.as_mut() else {
            return false;
        };
        rec.noted = true;
        match key.code {
            KeyCode::Char(_)
            | KeyCode::Enter
            | KeyCode::Tab
            | KeyCode::Backspace
            | KeyCode::Delete
                if plain =>
            {
                rec.typed.push(key);
            }
            // A key `.` cannot replay: a modified key (a shortcut), a
            // function key, or a cursor key, which in Vim also starts a new
            // undo step and a new insert.
            KeyCode::Char(_)
            | KeyCode::Enter
            | KeyCode::Tab
            | KeyCode::Backspace
            | KeyCode::Delete
            | KeyCode::F(_)
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown => {
                rec.replayable = false;
                rec.grouped = false;
            }
            // Anything else (a lone modifier, …) edits nothing; any edit it
            // does make is caught by `Self::modal_insert_unwatch`.
            _ => {}
        }
        false
    }

    /// Before a key is dispatched while a session is recorded: the undo
    /// state to compare against afterward (`None`: nothing recorded).
    pub(super) fn modal_insert_watch(&mut self) -> Option<usize> {
        let seq = self.modal_history_seq();
        let rec = self.modal_insert_record.as_mut()?;
        rec.noted = false;
        seq
    }

    /// After that key: an edit made by something other than Insert-mode
    /// typing (a completion popup, ghost text, an Org table key, …) is not
    /// in the record, so the session can no longer be replayed or grouped.
    pub(super) fn modal_insert_unwatch(&mut self, before: Option<usize>) {
        let Some(before) = before else {
            return;
        };
        let now = self.modal_history_seq();
        if let Some(rec) = self.modal_insert_record.as_mut()
            && !rec.noted
            && now != Some(before)
        {
            rec.replayable = false;
            rec.grouped = false;
        }
    }

    /// `Esc` ended a recorded Insert-mode session: fold the change's edits
    /// into one undo step and make it what `.` replays — or, when it
    /// cannot be replayed, clear what `.` replays so an older change is
    /// not repeated in its place. A session that changed nothing (`i` then
    /// `Esc`) leaves both alone.
    fn modal_finish_insert_record(&mut self) {
        let Some(rec) = self.modal_insert_record.take() else {
            return;
        };
        let now = self.modal_history_seq();
        if now.is_none() || now == rec.start_seq {
            return;
        }
        if rec.grouped
            && let Some(seq) = rec.start_seq
            && let Some(tab) = self.editor.active_tab_mut()
        {
            tab.editor.code_mut().squash_history_since(seq);
        }
        if rec.replayable && !rec.normal.is_empty() {
            self.modal_last_change = rec.normal;
            let mut typed = rec.typed;
            typed.push(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            self.modal_last_change_insert = typed;
        } else {
            self.modal_last_change.clear();
            self.modal_last_change_insert.clear();
        }
    }
}

#[cfg(test)]
#[path = "tests_r3_modal.rs"]
mod r3_tests;

#[cfg(test)]
#[path = "tests_t838_modal.rs"]
mod t838_tests;
