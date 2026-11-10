//! Pure helpers for the Helix keymap's selection-first model
//! (`spec/index.md`, § Helix selections). Helix inverts Vi's grammar: a
//! motion *selects* first, then an action (`d`, `c`, `y`, `r`, `~`, …)
//! acts on whatever is selected. The host (`tui/src/app/helix.rs`) keeps
//! the selection in the editor's own `[anchor, cursor)` selection, so
//! everything here is plain arithmetic over **character offsets** — the
//! same convention as [`crate::motion`], which supplies the `w`/`b`/`e`
//! and `f`/`t` motions these build on.
//!
//! A selection here is half-open `[start, end)`. With nothing selected,
//! Helix still acts on the one character under the cursor; [`effective`]
//! is that rule.

/// The range an action acts on: the selection `sel` (sorted `(start,
/// end)`), or, when it is empty or absent, the single character under
/// `cursor` — Helix never acts on nothing. At the very end of the buffer
/// the result is the empty range `(len, len)`.
///
/// # Examples
/// ```
/// use vix_modal::helix::effective;
/// assert_eq!(effective("abc", Some((0, 2)), 2), (0, 2));
/// assert_eq!(effective("abc", None, 1), (1, 2));
/// assert_eq!(effective("abc", None, 3), (3, 3));
/// ```
#[must_use]
pub fn effective(text: &str, sel: Option<(usize, usize)>, cursor: usize) -> (usize, usize) {
    match sel {
        Some((s, e)) if s != e => (s.min(e), s.max(e)),
        _ => {
            let len = text.chars().count();
            let at = cursor.min(len);
            (at, (at + 1).min(len))
        }
    }
}

/// The start of the line holding char offset `pos`.
fn line_start(chars: &[char], pos: usize) -> usize {
    let pos = pos.min(chars.len());
    chars[..pos]
        .iter()
        .rposition(|&c| c == '\n')
        .map_or(0, |i| i + 1)
}

/// One past the newline that ends the line holding `pos` (the next line's
/// start), or the buffer end on a final line with no trailing newline.
fn line_after(chars: &[char], pos: usize) -> usize {
    let pos = pos.min(chars.len());
    chars[pos..]
        .iter()
        .position(|&c| c == '\n')
        .map_or(chars.len(), |i| pos + i + 1)
}

/// `x`: select whole lines. The result covers every line the range
/// `(start, end)` touches, newline included. When the range already is
/// exactly that — a repeated `x` — it grows by the next line instead, so
/// `x x x` selects three lines, as in Helix.
///
/// # Examples
/// ```
/// use vix_modal::helix::select_lines;
/// let text = "one\ntwo\nthree";
/// assert_eq!(select_lines(text, 1, 1), (0, 4));
/// assert_eq!(select_lines(text, 0, 4), (0, 8));
/// assert_eq!(select_lines(text, 0, 8), (0, 13));
/// ```
#[must_use]
pub fn select_lines(text: &str, start: usize, end: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    let (start, end) = (start.min(end), start.max(end));
    let first = line_start(&chars, start);
    let last = if end > start { end - 1 } else { start };
    let after = line_after(&chars, last);
    if first == start && after == end && end < chars.len() {
        return (first, line_after(&chars, end));
    }
    (first, after)
}

/// Whether `c` belongs to a `WORD` (`W`/`B`/`E`): anything but whitespace.
fn is_long_word(c: char) -> bool {
    !c.is_whitespace()
}

/// The `(start, end)` runs of non-whitespace — Helix's (and Vim's) `WORD`.
fn long_word_units(chars: &[char]) -> Vec<(usize, usize)> {
    let mut units = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if is_long_word(chars[i]) {
            let start = i;
            while i < chars.len() && is_long_word(chars[i]) {
                i += 1;
            }
            units.push((start, i));
        } else {
            i += 1;
        }
    }
    units
}

/// `W`: the start of the `count`-th next `WORD` (a run of non-whitespace),
/// or the buffer end once there are no more.
///
/// # Examples
/// ```
/// assert_eq!(vix_modal::helix::long_word_forward("a.b c-d", 0, 1), 4);
/// ```
#[must_use]
pub fn long_word_forward(text: &str, pos: usize, count: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let units = long_word_units(&chars);
    let mut p = pos;
    for _ in 0..count.max(1) {
        p = units
            .iter()
            .map(|&(s, _)| s)
            .find(|&s| s > p)
            .unwrap_or(chars.len());
    }
    p
}

/// `B`: the start of the `count`-th previous `WORD`, or `0`.
///
/// # Examples
/// ```
/// assert_eq!(vix_modal::helix::long_word_backward("a.b c-d", 6, 1), 4);
/// ```
#[must_use]
pub fn long_word_backward(text: &str, pos: usize, count: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let units = long_word_units(&chars);
    let mut p = pos;
    for _ in 0..count.max(1) {
        p = units
            .iter()
            .rev()
            .map(|&(s, _)| s)
            .find(|&s| s < p)
            .unwrap_or(0);
    }
    p
}

/// `E`: the last character of the `count`-th next `WORD`.
///
/// # Examples
/// ```
/// assert_eq!(vix_modal::helix::long_word_end("a.b c-d", 0, 1), 2);
/// ```
#[must_use]
pub fn long_word_end(text: &str, pos: usize, count: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let units = long_word_units(&chars);
    let mut p = pos;
    for _ in 0..count.max(1) {
        p = units
            .iter()
            .map(|&(_, e)| e.saturating_sub(1))
            .find(|&end| end > p)
            .unwrap_or(chars.len().saturating_sub(1));
    }
    p
}

/// `~`: swap the case of every character in `s`.
///
/// # Examples
/// ```
/// assert_eq!(vix_modal::helix::swap_case("Hello, World"), "hELLO, wORLD");
/// ```
#[must_use]
pub fn swap_case(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            let swapped: Vec<char> = if c.is_uppercase() {
                c.to_lowercase().collect()
            } else if c.is_lowercase() {
                c.to_uppercase().collect()
            } else {
                vec![c]
            };
            swapped
        })
        .collect()
}

/// `r<char>`: replace every character of `s` with `with`, keeping line
/// breaks, so replacing a multi-line selection keeps its line structure.
///
/// # Examples
/// ```
/// assert_eq!(vix_modal::helix::replace_chars("ab\ncd", 'x'), "xx\nxx");
/// ```
#[must_use]
pub fn replace_chars(s: &str, with: char) -> String {
    s.chars()
        .map(|c| if c == '\n' { c } else { with })
        .collect()
}

/// Where and what `p`/`P` inserts, and what ends up selected afterward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paste {
    /// The char offset the text is inserted at.
    pub at: usize,
    /// The exact text to insert.
    pub text: String,
    /// The `(start, end)` range to select once inserted — the pasted text
    /// itself, as Helix leaves it.
    pub select: (usize, usize),
}

/// Plan a paste of `clip` relative to the effective range `(start, end)`
/// (see [`effective`]): `before` is `P`, otherwise `p`. Text ending in a
/// newline is linewise and goes on its own line(s) — above the first
/// selected line for `P`, below the last for `p`; anything else goes
/// right before the range start or right after its end.
///
/// # Examples
/// ```
/// use vix_modal::helix::paste;
/// // Char-wise, after the selected "ab".
/// assert_eq!(paste("abc", 0, 2, "X", false).at, 2);
/// // Linewise, below the current line.
/// let p = paste("one\ntwo", 1, 2, "new\n", false);
/// assert_eq!((p.at, p.text.as_str()), (4, "new\n"));
/// ```
#[must_use]
pub fn paste(text: &str, start: usize, end: usize, clip: &str, before: bool) -> Paste {
    let chars: Vec<char> = text.chars().collect();
    let (start, end) = (
        start.min(end).min(chars.len()),
        start.max(end).min(chars.len()),
    );
    let linewise = clip.ends_with('\n');
    let (at, inserted) = match (linewise, before) {
        (true, true) => (line_start(&chars, start), clip.to_string()),
        (true, false) => {
            let last = if end > start { end - 1 } else { start };
            let at = line_after(&chars, last);
            if at == chars.len() && chars.last().is_some_and(|&c| c != '\n') {
                // Last line has no newline to paste after: lead with one.
                (at, format!("\n{}", &clip[..clip.len() - 1]))
            } else {
                (at, clip.to_string())
            }
        }
        (false, true) => (start, clip.to_string()),
        (false, false) => (end, clip.to_string()),
    };
    let n = inserted.chars().count();
    let lead = usize::from(inserted.starts_with('\n') && !clip.starts_with('\n'));
    Paste {
        at,
        text: inserted,
        select: (at + lead, at + n),
    }
}

/// How a selecting motion treats the selection it starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Select {
    /// Select mode (`v`): keep the anchor and move only the cursor end.
    pub extend: bool,
    /// Select one past where the motion lands (`e`, `f`, `t`).
    pub inclusive: bool,
    /// How many times to run the motion (at least once).
    pub count: usize,
}

/// One selection after a selecting motion: from `(anchor, cursor)`, select
/// from the cursor to where `f(text, cursor)` lands (one past it when
/// [`Select::inclusive`]), returning the new `(anchor, cursor)`. With
/// [`Select::extend`] the anchor stays put.
///
/// With a count, `f` runs that many times and, as in Helix, each step
/// starts afresh where the last one ended, so `3w` selects the third word
/// (in Select mode, everything up to it). A motion that stops moving (at
/// the buffer's end) ends the count early. The host applies this to every
/// selection.
///
/// # Examples
/// ```
/// use vix_modal::helix::{select_by, Select};
/// let next = |t: &str, p: usize| vix_modal::motion::word_forward(t, p, 1);
/// let once = Select { extend: false, inclusive: false, count: 1 };
/// assert_eq!(select_by("foo bar baz", (0, 0), once, next), (0, 4));
/// let twice = Select { count: 2, ..once };
/// assert_eq!(select_by("foo bar baz", (0, 0), twice, next), (4, 8));
/// ```
#[must_use]
pub fn select_by(
    text: &str,
    (anchor, pos): (usize, usize),
    how: Select,
    f: impl Fn(&str, usize) -> usize,
) -> (usize, usize) {
    let len = text.chars().count();
    let (mut prev, mut target) = (pos, f(text, pos));
    for _ in 1..how.count {
        let next = f(text, target);
        if next == target {
            break;
        }
        (prev, target) = (target, next);
    }
    let past = |p: usize| if how.inclusive { (p + 1).min(len) } else { p };
    let anchor = if how.extend {
        anchor
    } else if prev == pos {
        pos
    } else {
        past(prev)
    };
    (anchor, past(target))
}

/// `y`/`d`/`c` with several selections: their texts (in buffer order) as
/// one register value. Each piece is followed by a newline unless it
/// already ends with one or is the last, so whole-line pieces (`x`) stay
/// whole lines and word pieces become one line each. [`split_selections`]
/// splits it back for a paste into the same number of selections.
///
/// # Examples
/// ```
/// use vix_modal::helix::join_selections;
/// assert_eq!(join_selections(&["foo", "bar"]), "foo\nbar");
/// assert_eq!(join_selections(&["a\n", "b\n"]), "a\nb\n");
/// ```
#[must_use]
pub fn join_selections<S: AsRef<str>>(pieces: &[S]) -> String {
    let mut out = String::new();
    for (i, piece) in pieces.iter().enumerate() {
        let piece = piece.as_ref();
        out.push_str(piece);
        if i + 1 < pieces.len() && !piece.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// `p`/`P` into `n` selections: when `clip` holds exactly one line per
/// selection, those lines (each keeping its newline when `clip` is whole
/// lines), so a [`join_selections`] register pastes back one piece per
/// selection; otherwise `None`, and every selection gets all of `clip`.
///
/// # Examples
/// ```
/// use vix_modal::helix::split_selections;
/// assert_eq!(split_selections("foo\nbar", 2), Some(vec!["foo".into(), "bar".into()]));
/// assert_eq!(split_selections("a\nb\n", 2), Some(vec!["a\n".into(), "b\n".into()]));
/// assert_eq!(split_selections("foo\nbar", 3), None);
/// ```
#[must_use]
pub fn split_selections(clip: &str, n: usize) -> Option<Vec<String>> {
    if n < 2 {
        return None;
    }
    let pieces: Vec<String> = if clip.ends_with('\n') {
        clip.split_inclusive('\n').map(str::to_string).collect()
    } else {
        clip.split('\n').map(str::to_string).collect()
    };
    (pieces.len() == n).then_some(pieces)
}

/// Where `i` (or `a`, when `after`) starts typing for one selection: its
/// start (or end), or, with nothing selected, the cursor (or one past it,
/// like Vi's `a`).
///
/// # Examples
/// ```
/// use vix_modal::helix::insert_point;
/// assert_eq!(insert_point("abc", (0, 2), true), 2);
/// assert_eq!(insert_point("abc", (2, 0), false), 0);
/// assert_eq!(insert_point("abc", (1, 1), true), 2);
/// ```
#[must_use]
pub fn insert_point(text: &str, (anchor, cursor): (usize, usize), after: bool) -> usize {
    match (anchor == cursor, after) {
        (true, true) => (cursor + 1).min(text.chars().count()),
        (true, false) => cursor,
        (false, true) => anchor.max(cursor),
        (false, false) => anchor.min(cursor),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_by_extends_in_select_mode_and_stops_at_the_end() {
        let end = |t: &str, p: usize| crate::motion::word_end(t, p, 1);
        let how = Select {
            extend: true,
            inclusive: true,
            count: 9,
        };
        assert_eq!(select_by("ab cd", (1, 1), how, end), (1, 5));
    }

    #[test]
    fn join_and_split_selections_round_trip() {
        for pieces in [vec!["x", "y", "z"], vec!["one\n", "two\n"]] {
            let joined = join_selections(&pieces);
            let back = split_selections(&joined, pieces.len()).unwrap();
            assert_eq!(back, pieces);
        }
        assert_eq!(join_selections::<&str>(&[]), "");
        assert_eq!(
            split_selections("a\nb", 1),
            None,
            "one selection takes it all"
        );
    }

    #[test]
    fn effective_prefers_a_nonempty_selection_and_sorts_it() {
        assert_eq!(effective("abcd", Some((3, 1)), 1), (1, 3));
        assert_eq!(effective("abcd", Some((2, 2)), 2), (2, 3));
        assert_eq!(effective("", None, 0), (0, 0));
    }

    #[test]
    fn select_lines_covers_every_touched_line() {
        let text = "one\ntwo\nthree";
        // A selection from inside line 1 to inside line 2.
        assert_eq!(select_lines(text, 1, 6), (0, 8));
        // A selection ending right at a line start doesn't reach into it.
        assert_eq!(select_lines(text, 4, 8), (4, 13));
        // The last line has no trailing newline: the selection ends at EOF.
        assert_eq!(select_lines(text, 9, 9), (8, 13));
        // Already whole and at EOF: nothing further to grow into.
        assert_eq!(select_lines(text, 8, 13), (8, 13));
    }

    #[test]
    fn long_words_span_punctuation() {
        let text = "foo.bar baz";
        assert_eq!(long_word_forward(text, 0, 1), 8);
        assert_eq!(long_word_end(text, 0, 1), 6);
        assert_eq!(long_word_backward(text, 10, 1), 8);
        assert_eq!(long_word_backward(text, 8, 1), 0);
        assert_eq!(long_word_forward(text, 8, 1), 11, "no more WORDs: EOF");
    }

    #[test]
    fn swap_case_and_replace_keep_length_and_newlines() {
        assert_eq!(swap_case("aB\n1"), "Ab\n1");
        assert_eq!(replace_chars("ab\n\ncd", '-'), "--\n\n--");
    }

    #[test]
    fn paste_charwise_before_and_after() {
        let p = paste("abc", 1, 2, "XY", true);
        assert_eq!(
            p,
            Paste {
                at: 1,
                text: "XY".into(),
                select: (1, 3)
            }
        );
        let p = paste("abc", 1, 2, "XY", false);
        assert_eq!(
            p,
            Paste {
                at: 2,
                text: "XY".into(),
                select: (2, 4)
            }
        );
    }

    #[test]
    fn paste_linewise_above_and_below() {
        let text = "one\ntwo";
        let p = paste(text, 5, 6, "new\n", true);
        assert_eq!(
            p,
            Paste {
                at: 4,
                text: "new\n".into(),
                select: (4, 8)
            }
        );
        let p = paste(text, 1, 2, "new\n", false);
        assert_eq!(
            p,
            Paste {
                at: 4,
                text: "new\n".into(),
                select: (4, 8)
            }
        );
    }

    #[test]
    fn paste_linewise_below_a_final_line_without_a_newline() {
        let p = paste("one\ntwo", 5, 6, "new\n", false);
        assert_eq!(p.at, 7);
        assert_eq!(p.text, "\nnew");
        assert_eq!(p.select, (8, 11), "only the pasted line is selected");
    }
}
