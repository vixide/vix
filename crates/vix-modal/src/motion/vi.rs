//! The Vi keymap's own character, word and column rules (T838), kept apart
//! from the shared scanner the rest of [`crate::motion`] uses (Helix's
//! `w`/`b`/`e` read that one and keep their behavior).
//!
//! A position here is a grapheme start: Vim moves over a character together
//! with its composing characters, so a decomposed `é` (`e` + U+0301) is
//! one stop, never two. Graphemes are computed over the current line only,
//! with `unicode-segmentation`. Each line also has one more stop after its
//! last grapheme, its line break (or the text's end) — Vim's `NUL`
//! position, where an operator's exclusive range may end but a Normal-mode
//! cursor never rests.
//!
//! The word scanner follows Vim's `fwd_word`, `bck_word` and `end_word`
//! (`search.c`) step for step, over three classes: blank (white space and
//! the line break), word (alphanumeric or `_`) and punctuation (anything
//! else); a run of one class is a word, and an empty line is a word too.

use super::{Text, current_line, next_line, off_the_lines, prev_line, row_at};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The grapheme stops of one line: each grapheme's start (a char offset),
/// then the line's end (its `NUL` stop), and the first character of each
/// grapheme. A line read through this costs the line's length.
struct Stops {
    /// The line's `(start, end)`, as [`super::current_line`] gives it.
    line: (usize, usize),
    /// Grapheme starts, then `line.1`; never empty.
    at: Vec<usize>,
    /// The first character of each grapheme (one fewer than `at`).
    first: Vec<char>,
}

impl Stops {
    /// The stops of the `(start, end)` line.
    fn of_line(text: &(impl Text + ?Sized), line: (usize, usize)) -> Self {
        let s = text.range_string(line.0, line.1);
        let mut at = Vec::new();
        let mut first = Vec::new();
        let mut off = line.0;
        for g in s.graphemes(true) {
            at.push(off);
            first.push(g.chars().next().unwrap_or(' '));
            off = off.saturating_add(g.chars().count());
        }
        at.push(line.1);
        Self { line, at, first }
    }

    /// The index of the last stop (the line's `NUL`).
    fn nul(&self) -> usize {
        self.at.len().saturating_sub(1)
    }

    /// The index of the stop holding `pos`: the last one at or before it.
    fn index_of(&self, pos: usize) -> usize {
        self.at.iter().rposition(|&s| s <= pos).unwrap_or(0)
    }

    /// The offset of stop `i` (the line's end past the last).
    fn offset(&self, i: usize) -> usize {
        self.at.get(i).copied().unwrap_or(self.line.1)
    }
}

/// Vim's character class (`cls()`): 0 blank, 1 punctuation, 2 word.
fn class_of(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        2
    } else {
        1
    }
}

/// A Vim cursor walking grapheme stops across lines (`inc_cursor` /
/// `dec_cursor`), reading one line at a time.
struct Walker<'a, T: Text + ?Sized> {
    /// The text walked.
    text: &'a T,
    /// The current line's stops.
    stops: Stops,
    /// The current stop's index in `stops`.
    idx: usize,
}

/// What a step did, as Vim's `inc()`/`dec()` report it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Moved within the line.
    Moved,
    /// Moved onto the line's `NUL` stop (forward only).
    OntoEnd,
    /// Moved to another line.
    NewLine,
    /// Could not move: the text's start or end.
    Stuck,
}

impl Step {
    /// Vim's `i >= 1`: the step reached a line's end or another line.
    fn crossed(self) -> bool {
        matches!(self, Self::OntoEnd | Self::NewLine)
    }
}

impl<'a, T: Text + ?Sized> Walker<'a, T> {
    /// A walker on the stop holding `pos` (a position on no line belongs to
    /// the last line's end).
    fn at(text: &'a T, pos: usize) -> Self {
        let stops = Stops::of_line(text, row_at(text, pos));
        let idx = stops.index_of(pos);
        Self { text, stops, idx }
    }

    /// The current offset.
    fn pos(&self) -> usize {
        self.stops.offset(self.idx)
    }

    /// The class under the cursor; the `NUL` stop is blank.
    fn cls(&self) -> u8 {
        self.stops.first.get(self.idx).map_or(0, |&c| class_of(c))
    }

    /// Whether the cursor's line is the text's last.
    fn on_last_line(&self) -> bool {
        next_line(self.text, self.stops.line).is_none()
    }

    /// Whether the cursor is on an empty line.
    fn on_empty_line(&self) -> bool {
        self.stops.line.0 == self.stops.line.1
    }

    /// Vim's `inc_cursor()`.
    fn inc(&mut self) -> Step {
        let nul = self.stops.nul();
        if self.idx < nul {
            self.idx = self.idx.saturating_add(1);
            return if self.idx == nul {
                Step::OntoEnd
            } else {
                Step::Moved
            };
        }
        match next_line(self.text, self.stops.line) {
            Some(line) => {
                self.stops = Stops::of_line(self.text, line);
                self.idx = 0;
                Step::NewLine
            }
            None => Step::Stuck,
        }
    }

    /// Vim's `dec_cursor()`: back one stop, from a line's start onto the
    /// previous line's `NUL` stop.
    fn dec(&mut self) -> Step {
        if let Some(i) = self.idx.checked_sub(1) {
            self.idx = i;
            return Step::Moved;
        }
        match prev_line(self.text, self.stops.line) {
            Some(line) => {
                self.stops = Stops::of_line(self.text, line);
                self.idx = self.stops.nul();
                Step::NewLine
            }
            None => Step::Stuck,
        }
    }

    /// Vim's `skip_chars()`: step while the class is `class`; `true` when
    /// the text's start or end stopped it.
    fn skip(&mut self, class: u8, forward: bool) -> bool {
        while self.cls() == class {
            let step = if forward { self.inc() } else { self.dec() };
            if step == Step::Stuck {
                return true;
            }
        }
        false
    }
}

/// Vim's `fwd_word(count, FALSE, eol)`: `eol` is set when an operator is
/// pending, so the last word moved over stops at its line's end.
fn fwd_word(text: &(impl Text + ?Sized), pos: usize, count: usize, eol: bool) -> usize {
    let mut w = Walker::at(text, pos);
    let mut left = count.max(1);
    while let Some(rest) = left.checked_sub(1) {
        left = rest;
        let last = left == 0;
        let sclass = w.cls();
        let last_line = w.on_last_line();
        let step = w.inc();
        if step == Step::Stuck || (step.crossed() && last_line) || (step.crossed() && eol && last) {
            return w.pos();
        }
        if sclass != 0 {
            while w.cls() == sclass {
                let step = w.inc();
                if step == Step::Stuck || (step.crossed() && eol && last) {
                    return w.pos();
                }
            }
        }
        while w.cls() == 0 {
            if w.on_empty_line() {
                break;
            }
            let step = w.inc();
            if step == Step::Stuck || (step.crossed() && eol && last) {
                return w.pos();
            }
        }
    }
    w.pos()
}

/// Vim's `bck_word(count, FALSE, FALSE)`.
fn bck_word(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let mut w = Walker::at(text, pos);
    for _ in 0..count.max(1) {
        if w.dec() == Step::Stuck {
            return w.pos();
        }
        let mut finished = false;
        while w.cls() == 0 {
            if w.on_empty_line() {
                finished = true;
                break;
            }
            if w.dec() == Step::Stuck {
                return w.pos();
            }
        }
        if finished {
            continue;
        }
        let class = w.cls();
        if w.skip(class, false) {
            return w.pos();
        }
        // Overshot: forward one.
        w.inc();
    }
    w.pos()
}

/// Vim's `end_word(count, FALSE, FALSE, FALSE)`.
fn end_word(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let mut w = Walker::at(text, pos);
    for _ in 0..count.max(1) {
        let sclass = w.cls();
        if w.inc() == Step::Stuck {
            return w.pos();
        }
        if w.cls() == sclass && sclass != 0 {
            if w.skip(sclass, true) {
                return w.pos();
            }
        } else {
            while w.cls() == 0 {
                if w.inc() == Step::Stuck {
                    return w.pos();
                }
            }
            let class = w.cls();
            if w.skip(class, true) {
                return w.pos();
            }
        }
        // Overshot: back one.
        w.dec();
    }
    w.pos()
}

/// Where a Normal-mode cursor rests for a motion that landed on `pos`
/// (Vim's `adjust_cursor`): the start of the grapheme holding it, and
/// never on a line's end — a landing there moves back onto the line's last
/// grapheme (an empty line keeps its one position). A position on no line
/// (the very end of a text ending in a line break) is kept as it is. Reads
/// only `pos`'s line.
#[must_use]
pub fn vi_normal_cursor(text: &(impl Text + ?Sized), pos: usize) -> usize {
    if off_the_lines(text, pos) {
        return pos;
    }
    let line = current_line(text, pos);
    let stops = Stops::of_line(text, line);
    let i = stops.index_of(pos);
    let i = if i >= stops.nul() {
        stops.nul().saturating_sub(1)
    } else {
        i
    };
    stops.offset(i)
}

/// `h` for the Vi keymap: `count` graphemes left, not past the line's
/// start.
#[must_use]
pub fn vi_char_left(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let stops = Stops::of_line(text, current_line(text, pos));
    let i = stops.index_of(pos);
    stops.offset(i.saturating_sub(count))
}

/// `l` for the Vi keymap: `count` graphemes right, not past the line's
/// last grapheme (a Normal-mode cursor never rests on the line's end).
#[must_use]
pub fn vi_char_right(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let stops = Stops::of_line(text, current_line(text, pos));
    let i = stops.index_of(pos);
    let last = stops.nul().saturating_sub(1);
    stops.offset(i.saturating_add(count).min(last))
}

/// The exclusive end of what `{count}x` (`{count}dl`) acts on from `pos`:
/// `count` graphemes (at least one) forward, stopping at the line's end, so
/// `$x` and `9x` take the line's last character and never its line
/// break. Equal to `pos` only on an empty line.
#[must_use]
pub fn vi_chars_end(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let stops = Stops::of_line(text, current_line(text, pos));
    let i = stops.index_of(pos);
    stops.offset(i.saturating_add(count.max(1)).min(stops.nul()))
}

/// The end of the grapheme starting at or holding `pos` (one past its last
/// character), on its line; `pos` itself at a line's end.
pub(crate) fn grapheme_end(text: &(impl Text + ?Sized), pos: usize) -> usize {
    if off_the_lines(text, pos) {
        return pos;
    }
    let stops = Stops::of_line(text, current_line(text, pos));
    let i = stops.index_of(pos);
    if i >= stops.nul() {
        pos
    } else {
        stops.offset(i.saturating_add(1))
    }
}

/// `w` for the Vi keymap (Normal mode): the start of the `count`-th next
/// word, where a word is a run of word characters, a run of punctuation, or
/// an empty line (`pkg::run` is three words: `pkg`, `::`, `run`). Past
/// the last word it rests on the text's last character.
#[must_use]
pub fn vi_word_forward(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    vi_normal_cursor(text, fwd_word(text, pos, count, false))
}

/// `w` as an operator's motion (`dw`, `yw`, `cw`): the exclusive end of
/// the range. Like [`vi_word_forward`], except that when the last word moved
/// over ends its line the range stops at that line's end instead of taking
/// the line break and the next line's indent (`:help w`'s special case).
#[must_use]
pub fn vi_word_forward_for_operator(
    text: &(impl Text + ?Sized),
    pos: usize,
    count: usize,
) -> usize {
    fwd_word(text, pos, count, true)
}

/// `b` for the Vi keymap: the start of the `count`-th previous word, with
/// [`vi_word_forward`]'s word classes.
#[must_use]
pub fn vi_word_backward(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    vi_normal_cursor(text, bck_word(text, pos, count))
}

/// `e` for the Vi keymap: the last grapheme of the `count`-th next word,
/// with [`vi_word_forward`]'s word classes (an inclusive motion).
#[must_use]
pub fn vi_word_end(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    vi_normal_cursor(text, end_word(text, pos, count))
}

/// The cells a grapheme `g` takes when it starts at display column `col`:
/// a tab reaches the next multiple of `tab_width` (at least 1), anything
/// else its Unicode width.
fn cells(g: &str, col: usize, tab_width: usize) -> usize {
    if g.starts_with('\t') {
        let tw = tab_width.max(1);
        tw.saturating_sub(col.checked_rem(tw).unwrap_or(0))
    } else {
        UnicodeWidthStr::width(g)
    }
}

/// The Normal-mode display column of the cursor at `pos` (Vim's
/// `w_virtcol`, the column `j`/`k` try to keep): the cells before its
/// grapheme on its line, plus — on a tab, where Vim draws a Normal-mode
/// cursor at the tab's last cell — the tab's width less one. Reads only
/// `pos`'s line.
#[must_use]
pub fn display_column(text: &(impl Text + ?Sized), pos: usize, tab_width: usize) -> usize {
    let (start, end) = current_line(text, pos);
    let s = text.range_string(start, end);
    let mut col = 0usize;
    let mut off = start;
    for g in s.graphemes(true) {
        let w = cells(g, col, tab_width);
        let next = off.saturating_add(g.chars().count());
        if next > pos {
            return if g.starts_with('\t') {
                col.saturating_add(w.saturating_sub(1))
            } else {
                col
            };
        }
        col = col.saturating_add(w);
        off = next;
    }
    col
}

/// The position on `line` for display column `column` (Vim's
/// `coladvance`): the grapheme covering that column (a tab or a wide
/// character spanning it included), or the line's last grapheme when the
/// line is shorter.
fn at_column(
    text: &(impl Text + ?Sized),
    line: (usize, usize),
    column: usize,
    tab_width: usize,
) -> usize {
    let s = text.range_string(line.0, line.1);
    let mut col = 0usize;
    let mut off = line.0;
    let mut last = line.0;
    for g in s.graphemes(true) {
        let next_col = col.saturating_add(cells(g, col, tab_width));
        if next_col > column {
            return off;
        }
        last = off;
        col = next_col;
        off = off.saturating_add(g.chars().count());
    }
    last
}

/// `j` for the Vi keymap: `count` lines down, onto display column
/// `column` — the preferred column the caller carries across a run of
/// vertical motions (Vim's `curswant`; [`display_column`] of where the run
/// started, or `usize::MAX` after `$` for "the line's end"), so `4ljj`
/// over a short line comes back to column 4. Reads the lines crossed.
#[must_use]
pub fn line_down_to_column(
    text: &(impl Text + ?Sized),
    pos: usize,
    count: usize,
    column: usize,
    tab_width: usize,
) -> usize {
    let mut row = row_at(text, pos);
    for _ in 0..count {
        match next_line(text, row) {
            Some(next) => row = next,
            None => break,
        }
    }
    at_column(text, row, column, tab_width)
}

/// `k` for the Vi keymap: `count` lines up, onto display column `column`
/// (see [`line_down_to_column`]).
#[must_use]
pub fn line_up_to_column(
    text: &(impl Text + ?Sized),
    pos: usize,
    count: usize,
    column: usize,
    tab_width: usize,
) -> usize {
    let mut row = row_at(text, pos);
    for _ in 0..count {
        match prev_line(text, row) {
            Some(prev) => row = prev,
            None => break,
        }
    }
    at_column(text, row, column, tab_width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    fn ws(s: &str, from: usize, n: usize) -> Vec<usize> {
        let rope = Rope::from_str(s);
        let mut out = Vec::new();
        let mut p = from;
        for _ in 0..n {
            let next = vi_word_forward(s, p, 1);
            assert_eq!(
                next,
                vi_word_forward(&rope, p, 1),
                "rope agrees on {s:?} at {p}"
            );
            out.push(next);
            p = next;
        }
        out
    }

    #[test]
    fn w_treats_identifiers_and_punctuation_runs_as_separate_words() {
        // p k g : : r u n ( a r g ) ;
        assert_eq!(ws("pkg::run(arg);", 0, 6), [3, 5, 8, 9, 12, 13]);
        assert_eq!(ws("obj.field", 0, 3), [3, 4, 8]);
        assert_eq!(ws("a  b\n", 0, 2), [3, 3]);
        assert_eq!(vi_word_forward("pkg::run(arg);", 0, 3), 8, "a count");
        // An empty line is a word; the next line's indent is skipped.
        assert_eq!(ws("foo\n\n  bar", 0, 2), [4, 7]);
    }

    #[test]
    fn b_and_e_use_the_same_classes() {
        let s = "pkg::run(arg);";
        assert_eq!(vi_word_backward(s, 13, 1), 12);
        assert_eq!(vi_word_backward(s, 12, 1), 9);
        assert_eq!(vi_word_backward(s, 5, 1), 3);
        assert_eq!(vi_word_backward(s, 3, 2), 0);
        assert_eq!(vi_word_end(s, 0, 1), 2);
        assert_eq!(vi_word_end(s, 2, 1), 4);
        assert_eq!(vi_word_end(s, 4, 1), 7);
        assert_eq!(vi_word_end(s, 0, 4), 8);
        assert_eq!(vi_word_end(s, 12, 1), 13);
        assert_eq!(vi_word_end(s, 13, 1), 13, "nothing after the last word");
        assert_eq!(
            vi_word_end("obj.field", 2, 1),
            3,
            "'.' is a word of its own"
        );
        let rope = Rope::from_str(s);
        for p in 0..=14 {
            assert_eq!(vi_word_backward(s, p, 1), vi_word_backward(&rope, p, 1));
            assert_eq!(vi_word_end(s, p, 1), vi_word_end(&rope, p, 1));
        }
    }

    #[test]
    fn w_for_an_operator_stops_at_the_end_of_the_last_word_s_line() {
        // `dw` on the line's last word leaves the line break alone.
        assert_eq!(vi_word_forward_for_operator("foo\n  bar", 0, 1), 3);
        assert_eq!(vi_word_forward_for_operator("a\nb\n", 0, 1), 1);
        assert_eq!(vi_word_forward_for_operator("foo bar", 4, 1), 7);
        assert_eq!(vi_word_forward_for_operator("foo   \nbar", 3, 1), 6);
        // Not the last word moved over: it goes on to the next line.
        assert_eq!(vi_word_forward_for_operator("foo\n  bar baz", 0, 2), 10);
        // As a cursor motion, past the last word it rests on the last char.
        assert_eq!(vi_word_forward("foo bar", 4, 1), 6);
        assert_eq!(vi_word_forward("ab", 1, 1), 1);
        assert_eq!(vi_word_forward("foo bar baz\n", 4, 2), 10);
    }

    #[test]
    fn character_motions_step_over_whole_graphemes() {
        // c a f e U+0301 ' ' b a r
        let s = "cafe\u{301} bar";
        assert_eq!(vi_char_right(s, 3, 1), 5);
        assert_eq!(vi_char_left(s, 5, 1), 3);
        assert_eq!(vi_char_right(s, 0, 99), 8, "clamped to the last character");
        assert_eq!(vi_char_left(s, 8, 99), 0);
        assert_eq!(vi_chars_end(s, 3, 1), 5, "x takes e and its accent");
        assert_eq!(vi_chars_end(s, 2, 3), 6);
        assert_eq!(vi_chars_end(s, 8, 9), 9, "up to the line's end");
        assert_eq!(
            vi_chars_end("ab\n\ncd", 3, 1),
            3,
            "nothing on an empty line"
        );
        assert_eq!(vi_chars_end("abc\nd", 0, 9), 3, "never the line break");
        assert_eq!(
            vi_word_forward(s, 0, 1),
            6,
            "the accent is part of the word"
        );
        assert_eq!(
            vi_word_end(s, 0, 1),
            3,
            "e lands on the accented letter's start"
        );
    }

    #[test]
    fn a_normal_cursor_never_rests_on_a_line_end_or_inside_a_grapheme() {
        assert_eq!(vi_normal_cursor("abc\ndef", 3), 2);
        assert_eq!(vi_normal_cursor("abc", 3), 2);
        assert_eq!(vi_normal_cursor("cafe\u{301}", 4), 3);
        assert_eq!(vi_normal_cursor("cafe\u{301}", 5), 3);
        assert_eq!(
            vi_normal_cursor("a\n\nb", 2),
            2,
            "an empty line keeps its place"
        );
        assert_eq!(
            vi_normal_cursor("abc\n", 4),
            4,
            "past a final line break: untouched"
        );
        assert_eq!(vi_normal_cursor("abc", 1), 1);
    }

    #[test]
    fn display_columns_count_tabs_and_wide_characters() {
        assert_eq!(display_column("ab\tc", 3, 8), 8);
        assert_eq!(display_column("ab\tc", 2, 8), 7, "on a tab: its last cell");
        assert_eq!(display_column("ab\tc", 2, 1), 2);
        // a b 漢 字 c: 漢 is columns 2-3, 字 4-5.
        assert_eq!(display_column("ab\u{6f22}\u{5b57}c", 3, 8), 4);
        assert_eq!(display_column("ab\u{6f22}\u{5b57}c", 4, 8), 6);
        assert_eq!(display_column("x\ncafe\u{301}!", 7, 8), 4);
    }

    #[test]
    fn vertical_motions_keep_the_preferred_column() {
        let s = "abcdefgh\nab\nabcdefgh\n";
        let rope = Rope::from_str(s);
        let want = display_column(s, 4, 8);
        let short = line_down_to_column(s, 4, 1, want, 8);
        assert_eq!(short, 10, "onto the short line's last character");
        assert_eq!(
            line_down_to_column(&rope, short, 1, want, 8),
            16,
            "back to column 4"
        );
        assert_eq!(line_down_to_column(s, 4, 2, want, 8), 16);
        assert_eq!(line_up_to_column(s, 16, 2, want, 8), 4);
        // A tab spanning the column holds the cursor; the next line gets
        // the column back.
        let t = "0123456789\n\tx\n0123456789\n";
        assert_eq!(line_down_to_column(t, 3, 1, 3, 8), 11, "on the tab");
        assert_eq!(line_down_to_column(t, 11, 1, 3, 8), 17);
        assert_eq!(line_down_to_column(t, 3, 1, 8, 8), 12, "past the tab: x");
        // A wide character spanning the column.
        let w = "0123456789\nab\u{6f22}\u{5b57}cd\n0123456789\n";
        assert_eq!(line_down_to_column(w, 5, 1, 5, 8), 14, "column 5 is in 字");
        assert_eq!(line_down_to_column(w, 3, 1, 3, 8), 13, "column 3 is in 漢");
        assert_eq!(line_down_to_column(w, 14, 1, 5, 8), 23);
        // `$`'s preferred column: each line's last character.
        assert_eq!(line_down_to_column(s, 7, 1, usize::MAX, 8), 10);
        assert_eq!(line_down_to_column(s, 10, 1, usize::MAX, 8), 19);
        assert_eq!(
            line_down_to_column("ab\n\ncd", 1, 1, 1, 8),
            3,
            "an empty line"
        );
        assert_eq!(
            line_up_to_column(s, 2, 3, 2, 8),
            2,
            "no line above: the first"
        );
    }
}
