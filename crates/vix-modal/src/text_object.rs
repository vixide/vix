//! Text objects (`spec/index.md`, § Design: text objects): `i`/`a` + `w` /
//! a delimiter pair / a quote. Pure `fn(text, pos, count) ->
//! Option<(usize, usize)>` — unlike a motion, a text object can fail to
//! find its target (`di"` with no quote on the line has nothing to act on).
//!
//! v1 set, exactly `tasks.md`'s T115 list: `iw aw i( a( i" a"` plus the
//! natural bracket/quote siblings it implies — `(`/`)`/`b`, `{`/`}`/`B`,
//! `[`/`]`, `<`/`>` (all delimiter pairs, parameterized rather than four
//! near-identical functions) and `"`/`'`/`` ` `` (all quotes, same
//! parameterization). Delimiter-pair objects are a character/bracket-
//! matching scan, deliberately not the Tree-sitter structural-expand
//! feature — see the spec's own § Design: text objects for why.

fn chars_of(text: &str) -> Vec<char> {
    text.chars().collect()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The maximal run of same-class (word vs. non-word) characters containing
/// `pos`, extended `count` times by alternating to the next class each
/// time (`2iw` on `"foo bar"` from inside `foo` is `"foo "` — the word,
/// then the space that follows it — matching real Vim's own `iw` count
/// behavior).
fn word_or_space_run(chars: &[char], pos: usize, count: usize) -> Option<(usize, usize)> {
    if chars.is_empty() {
        return None;
    }
    let pos = pos.min(chars.len() - 1);
    let is_word = is_word_char(chars[pos]);
    let mut start = pos;
    while start > 0 && is_word_char(chars[start - 1]) == is_word {
        start -= 1;
    }
    let mut end = pos + 1;
    while end < chars.len() && is_word_char(chars[end]) == is_word {
        end += 1;
    }
    for _ in 1..count.max(1) {
        if end >= chars.len() {
            break;
        }
        let next_is_word = is_word_char(chars[end]);
        while end < chars.len() && is_word_char(chars[end]) == next_is_word {
            end += 1;
        }
    }
    Some((start, end))
}

/// `iw`: the word (or, if `pos` sits on one, the whitespace run) at `pos`,
/// no surrounding whitespace included.
#[must_use]
pub fn inner_word(text: &str, pos: usize, count: usize) -> Option<(usize, usize)> {
    word_or_space_run(&chars_of(text), pos, count)
}

/// `aw`: `iw` plus the whitespace that follows it — or, if there is none
/// (end of line/buffer), the whitespace that precedes it instead. If `iw`
/// itself landed on a whitespace run, `aw` is that whitespace plus the word
/// that follows — real Vim's own rule either way.
#[must_use]
pub fn around_word(text: &str, pos: usize, count: usize) -> Option<(usize, usize)> {
    let chars = chars_of(text);
    let (start, end) = word_or_space_run(&chars, pos, count)?;
    if start >= chars.len() {
        return Some((start, end));
    }
    let is_space = |i: usize| chars[i].is_whitespace() && chars[i] != '\n';
    if is_word_char(chars[start]) {
        let mut e2 = end;
        while e2 < chars.len() && is_space(e2) {
            e2 += 1;
        }
        if e2 > end {
            return Some((start, e2));
        }
        let mut s2 = start;
        while s2 > 0 && is_space(s2 - 1) {
            s2 -= 1;
        }
        return Some((s2, end));
    }
    // `iw` landed on whitespace: absorb the following word instead.
    let mut e2 = end;
    while e2 < chars.len() && is_word_char(chars[e2]) {
        e2 += 1;
    }
    Some((start, e2))
}

/// The `(open_pos, close_pos)` of the pair of `open`/`close` delimiters
/// (which may be equal — see quote handling below, though this function
/// itself assumes they differ and does real bracket-depth matching) that
/// most tightly encloses `pos`, counting a nested, already-balanced pair in
/// between as one unit rather than stopping at its first delimiter. `pos`
/// sitting exactly on `open` or `close` counts as inside that same pair.
fn find_enclosing_pair(
    chars: &[char],
    pos: usize,
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    let n = chars.len();
    if n == 0 {
        return None;
    }
    let pos = pos.min(n - 1);
    let open_pos = if chars[pos] == open {
        pos
    } else {
        let mut depth = 0i32;
        let mut i = pos;
        loop {
            if i == 0 {
                return None;
            }
            i -= 1;
            let c = chars[i];
            if c == close {
                depth += 1;
            } else if c == open {
                if depth == 0 {
                    break i;
                }
                depth -= 1;
            }
        }
    };
    let mut depth = 0i32;
    for (k, &c) in chars.iter().enumerate().take(n).skip(open_pos + 1) {
        if c == open {
            depth += 1;
        } else if c == close {
            if depth == 0 {
                return Some((open_pos, k));
            }
            depth -= 1;
        }
    }
    None
}

/// `i(`/`i)`/`ib` and siblings: the innermost `open...close` pair enclosing
/// `pos`, delimiters excluded. `count` climbs that many levels further out
/// (`2di(` acts on the pair one level out from the innermost). `None` if
/// `pos` isn't inside `count` levels of a balanced pair.
#[must_use]
pub fn inner_pair(
    text: &str,
    pos: usize,
    count: usize,
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    let (o, c) = outer_pair(text, pos, count, open, close)?;
    Some((o + 1, c))
}

/// `a(`/`a)`/`ab` and siblings: like [`inner_pair`], delimiters included.
#[must_use]
pub fn around_pair(
    text: &str,
    pos: usize,
    count: usize,
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    let (o, c) = outer_pair(text, pos, count, open, close)?;
    Some((o, c + 1))
}

fn outer_pair(
    text: &str,
    pos: usize,
    count: usize,
    open: char,
    close: char,
) -> Option<(usize, usize)> {
    let chars = chars_of(text);
    let count = count.max(1);
    let mut p = pos;
    let mut found = None;
    for i in 0..count {
        let (o, c) = find_enclosing_pair(&chars, p, open, close)?;
        found = Some((o, c));
        // Only climb to the next level out when there's another iteration
        // to use it -- on the last one, `found` is the answer even if `o`
        // is 0 and there's nowhere further out to climb from.
        if i + 1 < count {
            p = o.checked_sub(1)?;
        }
    }
    found
}

/// The `(start, end)` positions of the pair of `quote` characters on `pos`'s own
/// line that `pos` falls at-or-before the close of — real Vim's own
/// left-to-right pairing rule for a character that (unlike a bracket) is
/// the same on both ends: quotes on a line pair up `(1st, 2nd), (3rd,
/// 4th), …`, and the first such pair `pos` doesn't come *after* is the one
/// used, whether `pos` is inside it or still approaching it. Never crosses
/// a line, matching real Vim.
fn find_quote_pair(chars: &[char], pos: usize, quote: char) -> Option<(usize, usize)> {
    let n = chars.len();
    let pos = pos.min(n.saturating_sub(1));
    let mut line_start = pos;
    while line_start > 0 && chars[line_start - 1] != '\n' {
        line_start -= 1;
    }
    let mut line_end = pos;
    while line_end < n && chars[line_end] != '\n' {
        line_end += 1;
    }
    let positions: Vec<usize> = (line_start..line_end)
        .filter(|&i| chars[i] == quote)
        .collect();
    let mut i = 0;
    while i + 1 < positions.len() {
        let (a, b) = (positions[i], positions[i + 1]);
        if pos <= b {
            return Some((a, b));
        }
        i += 2;
    }
    None
}

/// `i"`/`i'`/`` i` ``: the text between the relevant quote pair (this
/// module's own left-to-right pairing rule), quotes excluded. `count` is
/// accepted for
/// uniformity with the other text objects but unused — quotes don't nest,
/// so "one level further out" has no meaning here.
#[must_use]
pub fn inner_quote(text: &str, pos: usize, quote: char) -> Option<(usize, usize)> {
    let chars = chars_of(text);
    let (a, b) = find_quote_pair(&chars, pos, quote)?;
    Some((a + 1, b))
}

/// `a"`/`a'`/`` a` ``: like [`inner_quote`], quotes included.
#[must_use]
pub fn around_quote(text: &str, pos: usize, quote: char) -> Option<(usize, usize)> {
    let chars = chars_of(text);
    let (a, b) = find_quote_pair(&chars, pos, quote)?;
    Some((a, b + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_word_selects_just_the_word() {
        assert_eq!(inner_word("foo bar", 1, 1), Some((0, 3)));
    }

    #[test]
    fn inner_word_on_whitespace_selects_the_whitespace_run() {
        assert_eq!(inner_word("foo   bar", 4, 1), Some((3, 6)));
    }

    #[test]
    fn inner_word_with_a_count_extends_alternating_classes() {
        // "foo bar baz": from inside "foo", 2iw is "foo" + the space after
        // it ("foo ").
        assert_eq!(inner_word("foo bar baz", 1, 2), Some((0, 4)));
    }

    #[test]
    fn around_word_absorbs_trailing_whitespace() {
        assert_eq!(around_word("foo bar", 1, 1), Some((0, 4)));
    }

    #[test]
    fn around_word_at_the_end_of_line_absorbs_leading_whitespace_instead() {
        assert_eq!(around_word("foo bar", 5, 1), Some((3, 7)));
    }

    #[test]
    fn around_word_on_whitespace_absorbs_the_following_word() {
        assert_eq!(around_word("foo   bar", 4, 1), Some((3, 9)));
    }

    #[test]
    fn inner_pair_excludes_the_delimiters() {
        assert_eq!(inner_pair("(a(b)c)", 3, 1, '(', ')'), Some((3, 4)));
    }

    #[test]
    fn around_pair_includes_the_delimiters() {
        assert_eq!(around_pair("(a(b)c)", 3, 1, '(', ')'), Some((2, 5)));
    }

    #[test]
    fn a_pair_on_the_open_delimiter_itself_still_counts_as_inside() {
        assert_eq!(inner_pair("(a(b)c)", 0, 1, '(', ')'), Some((1, 6)));
    }

    #[test]
    fn a_pair_on_the_close_delimiter_itself_still_counts_as_inside() {
        assert_eq!(inner_pair("(a(b)c)", 6, 1, '(', ')'), Some((1, 6)));
    }

    #[test]
    fn a_count_climbs_further_out_levels_of_nesting() {
        assert_eq!(inner_pair("(a(b)c)", 3, 2, '(', ')'), Some((1, 6)));
    }

    #[test]
    fn no_enclosing_pair_is_none() {
        assert_eq!(inner_pair("abc", 1, 1, '(', ')'), None);
    }

    #[test]
    fn unbalanced_pairs_are_not_matched() {
        assert_eq!(inner_pair("(abc", 1, 1, '(', ')'), None);
    }

    #[test]
    fn inner_quote_excludes_the_quotes() {
        assert_eq!(inner_quote("a \"hi\" b", 3, '"'), Some((3, 5)));
    }

    #[test]
    fn around_quote_includes_the_quotes() {
        assert_eq!(around_quote("a \"hi\" b", 3, '"'), Some((2, 6)));
    }

    #[test]
    fn a_quote_object_before_the_pair_still_finds_it() {
        assert_eq!(inner_quote("a \"hi\" b", 0, '"'), Some((3, 5)));
    }

    #[test]
    fn a_quote_object_never_crosses_a_line() {
        let text = "a \"hi\nthere\" b";
        assert_eq!(inner_quote(text, 0, '"'), None);
    }

    #[test]
    fn a_second_quote_pair_on_the_same_line_is_found_by_position() {
        let text = "\"one\" \"two\"";
        assert_eq!(inner_quote(text, 8, '"'), Some((7, 10)));
    }
}
