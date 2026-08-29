//! Small pure text transforms used by Edit/Tools actions.
//!
//! Two shapes live here: whole-text transforms (`&str -> String`: line-ending
//! conversion, blank-line squeezing, ROT13, hard wrap) and cursor-relative
//! rewrites (`(&str, usize) -> Option<(String, usize)>`: increment number,
//! smart toggle, transpose characters/words/lines/sentences/paragraphs/
//! sections, wrap the paragraph at the cursor). The host applies the former via
//! `App::transform_selection_or_buffer` and the latter via
//! `App::rewrite_at_cursor`; everything here is unit-tested without a terminal.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert all line endings to LF (`\n`), dropping any `\r`.
#[must_use]
pub fn to_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Convert all line endings to CRLF (`\r\n`). Normalizes to LF first so mixed
/// input doesn't produce `\r\r\n`.
#[must_use]
pub fn to_crlf(text: &str) -> String {
    to_lf(text).replace('\n', "\r\n")
}

/// Collapse runs of two or more blank (empty or whitespace-only) lines into a
/// single empty line. A trailing newline is preserved.
#[must_use]
pub fn squeeze_blank_lines(text: &str) -> String {
    let had_trailing_newline = text.ends_with('\n');
    let mut out: Vec<&str> = Vec::new();
    let mut prev_blank = false;
    for line in text.split('\n') {
        let blank = line.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        out.push(line);
        prev_blank = blank;
    }
    let mut joined = out.join("\n");
    if had_trailing_newline && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

/// ROT13: rotate ASCII letters by 13 (its own inverse); other chars unchanged.
#[must_use]
pub fn rot13(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'a'..='z' => (b'a' + (c as u8 - b'a' + 13) % 26) as char,
            'A'..='Z' => (b'A' + (c as u8 - b'A' + 13) % 26) as char,
            _ => c,
        })
        .collect()
}

/// Increment (or decrement, `delta = -1`) the integer at or immediately after the
/// cursor char offset `cursor` in `text`. Returns the rewritten text and the new
/// cursor offset (kept at the number's start), or `None` if no digit is found on
/// the cursor's line at/after the cursor. Handles an optional leading `-`.
#[must_use]
pub fn bump_number_at(text: &str, cursor: usize, delta: i64) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    // Search from the cursor to the end of the current line for a digit.
    let mut i = cursor.min(n);
    while i < n && chars[i] != '\n' && !chars[i].is_ascii_digit() {
        i += 1;
    }
    if i >= n || chars[i] == '\n' {
        return None;
    }
    // Expand left over digits, then include a leading '-' if present.
    let mut start = i;
    while start > 0 && chars[start - 1].is_ascii_digit() {
        start -= 1;
    }
    if start > 0 && chars[start - 1] == '-' {
        start -= 1;
    }
    let mut end = i;
    while end < n && chars[end].is_ascii_digit() {
        end += 1;
    }
    let token: String = chars[start..end].iter().collect();
    let value: i64 = token.parse().ok()?;
    let bumped = value.saturating_add(delta).to_string();
    let mut out: String = chars[..start].iter().collect();
    out.push_str(&bumped);
    out.extend(chars[end..].iter());
    Some((out, start))
}

/// Transpose the two characters around char offset `cursor` (Emacs `C-t`): swap
/// the char before the cursor with the one at it, advancing the cursor. At the
/// end of a line/buffer, swaps the last two characters. Never crosses newlines.
/// Returns the rewritten text and new cursor, or `None` if there is no pair.
#[must_use]
pub fn transpose_chars_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let mut chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    // The left index of the pair to swap.
    let i = if cursor >= 1 && cursor < n && chars[cursor] != '\n' {
        cursor - 1
    } else if cursor >= 2 {
        cursor - 2
    } else {
        return None;
    };
    if i + 1 >= n || chars[i] == '\n' || chars[i + 1] == '\n' {
        return None;
    }
    chars.swap(i, i + 1);
    Some((chars.iter().collect(), (i + 2).min(n)))
}

/// Transpose the word before the cursor with the word at/after it (Emacs `M-t`),
/// preserving the separator between them and leaving the cursor after the moved
/// pair. Returns `None` if two words can't be found.
#[must_use]
pub fn transpose_words_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    // Start of the second word: the word containing the cursor, else the next one.
    let mut b = cursor.min(n);
    if b < n && is_word(chars[b]) {
        while b > 0 && is_word(chars[b - 1]) {
            b -= 1;
        }
    } else {
        while b < n && !is_word(chars[b]) {
            b += 1;
        }
    }
    if b >= n {
        return None;
    }
    let mut b_end = b;
    while b_end < n && is_word(chars[b_end]) {
        b_end += 1;
    }
    // The first word: the word ending before `b`.
    let mut a_end = b;
    while a_end > 0 && !is_word(chars[a_end - 1]) {
        a_end -= 1;
    }
    let mut a = a_end;
    while a > 0 && is_word(chars[a - 1]) {
        a -= 1;
    }
    if a == a_end {
        return None; // no preceding word
    }
    let word1: String = chars[a..a_end].iter().collect();
    let sep: String = chars[a_end..b].iter().collect();
    let word2: String = chars[b..b_end].iter().collect();
    let mut out: String = chars[..a].iter().collect();
    out.push_str(&word2);
    out.push_str(&sep);
    out.push_str(&word1);
    out.extend(chars[b_end..].iter());
    let new_cursor = a + word2.chars().count() + sep.chars().count() + word1.chars().count();
    Some((out, new_cursor))
}

/// Char offsets where each sentence begins: the first non-space char, then the
/// first non-space char after any `.`/`!`/`?` (plus trailing quotes/brackets)
/// followed by whitespace. Shared by the Go → Sentence navigation and
/// [`transpose_sentences_at`], so both agree on where a sentence starts.
#[must_use]
pub fn sentence_starts(text: &str) -> Vec<usize> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut starts = Vec::new();
    let mut i = 0;
    while i < n && chars[i].is_whitespace() {
        i += 1;
    }
    if i < n {
        starts.push(i);
    }
    while i < n {
        if matches!(chars[i], '.' | '!' | '?') {
            let mut j = i + 1;
            while j < n && matches!(chars[j], '.' | '!' | '?' | '"' | '\'' | ')' | ']' | '}') {
                j += 1;
            }
            if j < n && chars[j].is_whitespace() {
                while j < n && chars[j].is_whitespace() {
                    j += 1;
                }
                if j < n {
                    starts.push(j);
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    starts.dedup();
    starts
}

/// Swap the two units around `cursor`, keeping the text between them (the
/// separator) in place. `units` are sorted, non-overlapping `(start, end)` char
/// ranges; the pair is the unit holding the cursor and its predecessor, or the
/// last two when the cursor sits past every unit. Returns the rewritten text
/// and the cursor just after the swapped pair, or `None` when there is no pair.
fn transpose_units_at(
    text: &str,
    cursor: usize,
    units: &[(usize, usize)],
) -> Option<(String, usize)> {
    let i = units
        .iter()
        .position(|&(s, e)| cursor >= s && cursor <= e)
        .or_else(|| units.iter().position(|&(s, _)| s > cursor))
        .unwrap_or(units.len().saturating_sub(1));
    if i == 0 {
        return None;
    }
    let (a, a_end) = units[i - 1];
    let (b, b_end) = units[i];
    let chars: Vec<char> = text.chars().collect();
    let first: String = chars[a..a_end].iter().collect();
    let sep: String = chars[a_end..b].iter().collect();
    let second: String = chars[b..b_end].iter().collect();
    let mut out: String = chars[..a].iter().collect();
    out.push_str(&second);
    out.push_str(&sep);
    out.push_str(&first);
    out.extend(chars[b_end..].iter());
    Some((out, b_end))
}

/// The `(start, end)` char range of every line's content, the newline excluded.
/// A trailing newline does not add an empty final line.
fn line_ranges(chars: &[char]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0;
    for (i, c) in chars.iter().enumerate() {
        if *c == '\n' {
            ranges.push((start, i));
            start = i + 1;
        }
    }
    if start < chars.len() || ranges.is_empty() {
        ranges.push((start, chars.len()));
    }
    ranges
}

/// Whether the char range `r` is blank (empty or whitespace-only).
fn range_is_blank(chars: &[char], r: (usize, usize)) -> bool {
    chars[r.0..r.1].iter().all(|c| c.is_whitespace())
}

/// Group consecutive lines into units, starting a new unit after every line for
/// which `is_break` holds; break lines and blank edges are left out of the units
/// (so they stay put as separators when two units are swapped).
fn line_group_units(
    chars: &[char],
    rows: &[(usize, usize)],
    is_break: impl Fn(usize) -> bool,
) -> Vec<(usize, usize)> {
    let mut units: Vec<(usize, usize)> = Vec::new();
    let mut group: Vec<(usize, usize)> = Vec::new();
    let flush = |group: &mut Vec<(usize, usize)>, units: &mut Vec<(usize, usize)>| {
        while group.last().is_some_and(|&r| range_is_blank(chars, r)) {
            group.pop();
        }
        while group.first().is_some_and(|&r| range_is_blank(chars, r)) {
            group.remove(0);
        }
        if let (Some(first), Some(last)) = (group.first(), group.last()) {
            units.push((first.0, last.1));
        }
        group.clear();
    };
    for (row, &range) in rows.iter().enumerate() {
        if is_break(row) {
            flush(&mut group, &mut units);
        } else {
            group.push(range);
        }
    }
    flush(&mut group, &mut units);
    units
}

/// Transpose the line before the cursor's line with the cursor's line (Emacs
/// `C-x C-t`), preserving the newline between them and leaving the cursor at the
/// end of the pair. `None` on the first line of the buffer.
#[must_use]
pub fn transpose_lines_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let units = line_ranges(&chars);
    transpose_units_at(text, cursor, &units)
}

/// The `(start, end)` char range of every sentence, trailing whitespace
/// excluded. Sentences begin where [`sentence_starts`] says they do, so the
/// transpose and delete commands agree with the Go → Sentence navigation.
fn sentence_units(text: &str, chars: &[char]) -> Vec<(usize, usize)> {
    let starts = sentence_starts(text);
    starts
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let mut end = starts.get(i + 1).copied().unwrap_or(chars.len());
            while end > start && chars[end - 1].is_whitespace() {
                end -= 1;
            }
            (start, end)
        })
        .collect()
}

/// The `(start, end)` char range of every paragraph: a run of non-blank lines,
/// as in the Go → Paragraph navigation.
fn paragraph_units(chars: &[char]) -> Vec<(usize, usize)> {
    let rows = line_ranges(chars);
    line_group_units(chars, &rows, |row| range_is_blank(chars, rows[row]))
}

/// The `(start, end)` char range of every section: a run of lines delimited by
/// two or more blank lines, as in the Go → Section navigation.
fn section_units(chars: &[char]) -> Vec<(usize, usize)> {
    let rows = line_ranges(chars);
    let blank = |row: usize| range_is_blank(chars, rows[row]);
    line_group_units(chars, &rows, |row| {
        blank(row) && ((row > 0 && blank(row - 1)) || (row + 1 < rows.len() && blank(row + 1)))
    })
}

/// Transpose the sentence before the cursor with the sentence at/after it
/// (Emacs `M-x transpose-sentences`), preserving the whitespace between them.
/// Sentences are split as in the Go → Sentence navigation
/// ([`sentence_starts`]). `None` when there is no pair.
#[must_use]
pub fn transpose_sentences_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let units = sentence_units(text, &chars);
    transpose_units_at(text, cursor, &units)
}

/// Transpose the paragraph before the cursor with the paragraph at/after it
/// (Emacs `M-x transpose-paragraphs`), preserving the blank lines between them.
/// Paragraphs are runs of non-blank lines, as in the Go → Paragraph navigation.
/// `None` when there is no pair.
#[must_use]
pub fn transpose_paragraphs_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let units = paragraph_units(&chars);
    transpose_units_at(text, cursor, &units)
}

/// Transpose the section before the cursor with the section at/after it,
/// preserving the break between them. Sections are delimited by a run of two or
/// more blank lines, as in the Go → Section navigation. `None` when there is no
/// pair.
#[must_use]
pub fn transpose_sections_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let units = section_units(&chars);
    transpose_units_at(text, cursor, &units)
}

/// The `(start, end)` char range of every word: a run of alphanumeric or `_`
/// characters, as used by the word motions.
fn word_units(chars: &[char]) -> Vec<(usize, usize)> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut units = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if is_word(chars[i]) {
            let start = i;
            while i < chars.len() && is_word(chars[i]) {
                i += 1;
            }
            units.push((start, i));
        } else {
            i += 1;
        }
    }
    units
}

/// Delete the unit holding `cursor` — or the next one, when the cursor sits
/// between units — together with the separator that follows it, so the
/// surrounding text closes up. The separator before the unit is taken instead
/// when nothing follows (the last unit, or a line break `keep_lines` protects).
/// `keep_lines` bars the separator from crossing a newline, so deleting a word
/// or a sentence never joins two lines; line-based units (paragraphs, sections)
/// pass `false` and swallow the blank lines between them. `units` are sorted,
/// non-overlapping `(start, end)` char ranges. Returns the rewritten text and
/// the cursor at the hole left behind, or `None` when there is no unit.
fn delete_unit_at(
    text: &str,
    cursor: usize,
    units: &[(usize, usize)],
    keep_lines: bool,
) -> Option<(String, usize)> {
    let i = units
        .iter()
        .position(|&(s, e)| cursor >= s && cursor <= e)
        .or_else(|| units.iter().position(|&(s, _)| s > cursor))
        .unwrap_or(units.len().saturating_sub(1));
    let &(start, end) = units.get(i)?;
    let chars: Vec<char> = text.chars().collect();
    let separator = |c: char| c.is_whitespace() && !(keep_lines && c == '\n');
    let mut from = start;
    let mut to = end;
    if let Some(&(next, _)) = units.get(i + 1) {
        while to < next && separator(chars[to]) {
            to += 1;
        }
    }
    if to == end {
        let prev_end = if i == 0 { 0 } else { units[i - 1].1 };
        while from > prev_end && separator(chars[from - 1]) {
            from -= 1;
        }
    }
    if from == start && to == end {
        // A lone unit: take whatever whitespace trails it.
        while to < chars.len() && separator(chars[to]) {
            to += 1;
        }
    }
    let mut out: String = chars[..from].iter().collect();
    out.extend(chars[to..].iter());
    Some((out, from))
}

/// Delete the character at char offset `cursor` (Emacs `C-d`), leaving the
/// cursor where it was. `None` at the end of the buffer, where there is nothing
/// to delete.
#[must_use]
pub fn delete_char_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    if cursor >= chars.len() {
        return None;
    }
    let mut out: String = chars[..cursor].iter().collect();
    out.extend(chars[cursor + 1..].iter());
    Some((out, cursor))
}

/// Delete the word at the cursor (or the next one, when the cursor is between
/// words) along with the spacing after it, staying on the line. `None` when the
/// text holds no word.
#[must_use]
pub fn delete_word_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let units = word_units(&chars);
    delete_unit_at(text, cursor, &units, true)
}

/// Delete the sentence at the cursor along with the spacing after it, staying on
/// the line. Sentences are split as in the Go → Sentence navigation
/// ([`sentence_starts`]). `None` when the text holds no sentence.
#[must_use]
pub fn delete_sentence_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let units = sentence_units(text, &chars);
    delete_unit_at(text, cursor, &units, true)
}

/// Delete the paragraph at the cursor (a run of non-blank lines) along with the
/// blank lines that separate it from the next one. `None` when the text holds no
/// paragraph.
#[must_use]
pub fn delete_paragraph_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let units = paragraph_units(&chars);
    delete_unit_at(text, cursor, &units, false)
}

/// Delete the section at the cursor (lines delimited by two or more blank lines)
/// along with the break that separates it from the next one. `None` when the
/// text holds no section.
#[must_use]
pub fn delete_section_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let units = section_units(&chars);
    delete_unit_at(text, cursor, &units, false)
}

/// Line leaders a wrapped chunk may repeat on every line (comment and quote
/// markers). Longest first, so `///` wins over `//`. Bullet markers are handled
/// separately (see [`bullet_len`]) because each bullet is its own chunk.
const WRAP_MARKERS: &[&str] = &["///", "//", "#", "--", ";;", ";", ">"];

/// Length in chars of the list bullet starting `line` (after its indentation) —
/// `-`, `*`, `+`, `1.`, `1)` — including the whitespace after it, or `None` when
/// the line does not start a list item.
fn bullet_len(line: &str) -> Option<usize> {
    let body = line.trim_start();
    let marker = if body.starts_with(['-', '*', '+']) {
        1
    } else {
        let digits = body.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 || !matches!(body.chars().nth(digits), Some('.' | ')')) {
            return None;
        }
        digits + 1
    };
    let spaces = body.chars().skip(marker).take_while(|c| *c == ' ').count();
    if spaces == 0 {
        return None;
    }
    Some(marker + spaces)
}

/// The prefix repeated on every wrapped line of a chunk, plus the marker it was
/// built from (stripped from each line's words). Falls back to the first line's
/// indentation when the lines share no marker.
fn fill_prefix(lines: &[&str]) -> (String, Option<&'static str>) {
    let first = lines[0];
    let indent: String = first.chars().take_while(|c| c.is_whitespace()).collect();
    let rest = &first[indent.len()..];
    for marker in WRAP_MARKERS {
        if let Some(after) = rest.strip_prefix(marker)
            && lines.iter().all(|l| l.trim_start().starts_with(marker))
        {
            let spaces: String = after.chars().take_while(|c| *c == ' ').collect();
            return (format!("{indent}{marker}{spaces}"), Some(marker));
        }
    }
    (indent, None)
}

/// Greedily fill `lines` (one chunk: no blank lines, one list item at most) into
/// lines of at most `width` chars, repeating the chunk's prefix. A word longer
/// than the width still gets its own line rather than being split.
fn wrap_chunk(lines: &[&str], width: usize) -> Vec<String> {
    let bullet = bullet_len(lines[0]);
    let (prefix, marker) = match bullet {
        Some(_) => (String::new(), None),
        None => fill_prefix(lines),
    };
    let mut words: Vec<&str> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let mut body = line.trim_start();
        if let Some(m) = marker {
            body = body.strip_prefix(m).unwrap_or(body);
        }
        if i == 0
            && let Some(len) = bullet
        {
            body = &body[body
                .char_indices()
                .nth(len)
                .map_or(body.len(), |(byte, _)| byte)..];
        }
        words.extend(body.split_whitespace());
    }
    if words.is_empty() {
        return lines.iter().map(|l| (*l).to_string()).collect();
    }
    // A list item keeps its bullet on the first line and hangs the rest under it.
    let indent: String = lines[0].chars().take_while(|c| c.is_whitespace()).collect();
    let (first_prefix, cont_prefix) = match bullet {
        Some(len) => {
            let head: String = lines[0].trim_start().chars().take(len).collect();
            let hang = " ".repeat(indent.chars().count() + len);
            (format!("{indent}{head}"), hang)
        }
        None => (prefix.clone(), prefix),
    };
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in words {
        let lead = if out.is_empty() {
            &first_prefix
        } else {
            &cont_prefix
        };
        let room = lead.chars().count() + cur.chars().count() + 1 + word.chars().count() <= width;
        if cur.is_empty() {
            cur.push_str(word);
        } else if room {
            cur.push(' ');
            cur.push_str(word);
        } else {
            out.push(format!("{lead}{cur}"));
            cur = word.to_string();
        }
    }
    let lead = if out.is_empty() {
        &first_prefix
    } else {
        &cont_prefix
    };
    out.push(format!("{lead}{cur}"));
    out
}

/// Hard-wrap (fill) `text` to at most `width` chars per line. Blank lines and
/// list items separate chunks, and each chunk is refilled on its own: its words
/// are re-flowed greedily, keeping the chunk's indentation, any comment/quote
/// marker shared by every line, and a hanging indent under a list bullet.
/// Widths count chars, not terminal columns.
#[must_use]
pub fn wrap(text: &str, width: usize) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut chunk: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        let blank = line.trim().is_empty();
        if (blank || bullet_len(line).is_some()) && !chunk.is_empty() {
            out.extend(wrap_chunk(&chunk, width));
            chunk.clear();
        }
        if blank {
            out.push(line.to_string());
        } else {
            chunk.push(line);
        }
    }
    if !chunk.is_empty() {
        out.extend(wrap_chunk(&chunk, width));
    }
    out.join("\n")
}

/// Hard-wrap the paragraph around the cursor (the run of non-blank lines holding
/// it) to `width` chars, leaving the cursor at the end of the rewritten
/// paragraph. `None` when the cursor is not in a paragraph or the paragraph is
/// already wrapped.
#[must_use]
pub fn wrap_paragraph_at(text: &str, cursor: usize, width: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let rows = line_ranges(&chars);
    let units = line_group_units(&chars, &rows, |row| range_is_blank(&chars, rows[row]));
    let &(start, end) = units
        .iter()
        .find(|&&(s, e)| cursor >= s && cursor <= e)
        .or_else(|| units.iter().find(|&&(s, _)| s > cursor))?;
    let filled = wrap(&chars[start..end].iter().collect::<String>(), width);
    if filled == chars[start..end].iter().collect::<String>() {
        return None;
    }
    let mut out: String = chars[..start].iter().collect();
    out.push_str(&filled);
    out.extend(chars[end..].iter());
    Some((out, start + filled.chars().count()))
}

/// Opposite-value pairs for [`smart_toggle_at`]. Word pairs are matched
/// whole-word and case-preserved; symbol pairs are matched literally.
const TOGGLE_WORDS: &[(&str, &str)] = &[
    ("true", "false"),
    ("yes", "no"),
    ("on", "off"),
    ("enable", "disable"),
    ("enabled", "disabled"),
    ("left", "right"),
    ("up", "down"),
    ("min", "max"),
    ("and", "or"),
];
const TOGGLE_SYMBOLS: &[(&str, &str)] = &[
    ("&&", "||"),
    ("==", "!="),
    ("<=", ">="),
    ("<", ">"),
    ("++", "--"),
];

/// Toggle the boolean-ish token at char offset `cursor` to its opposite: word
/// pairs (`true`/`false`, `yes`/`no`, …) matched as whole words with case
/// preserved, or symbol pairs (`&&`/`||`, `==`/`!=`, …) at/around the cursor.
/// Returns the rewritten text and the cursor's new offset, or `None` if nothing
/// togglable is under the cursor.
#[must_use]
pub fn smart_toggle_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';

    // Word pairs: find the identifier span covering the cursor (or just before it).
    let mut s = cursor.min(n);
    while s > 0 && is_word(chars[s - 1]) {
        s -= 1;
    }
    let mut e = s;
    while e < n && is_word(chars[e]) {
        e += 1;
    }
    if s < e {
        let word: String = chars[s..e].iter().collect();
        let lower = word.to_ascii_lowercase();
        for (a, b) in TOGGLE_WORDS {
            let to = if lower == *a {
                Some(*b)
            } else if lower == *b {
                Some(*a)
            } else {
                None
            };
            if let Some(to) = to {
                let replacement = match_case(&word, to);
                let mut out: String = chars[..s].iter().collect();
                out.push_str(&replacement);
                out.extend(chars[e..].iter());
                return Some((out, s));
            }
        }
    }

    // Symbol pairs: try each starting at, or one char before, the cursor.
    for (a, b) in TOGGLE_SYMBOLS {
        for start in [cursor, cursor.saturating_sub(1)] {
            for (from, to) in [(*a, *b), (*b, *a)] {
                let flen = from.chars().count();
                if start + flen <= n
                    && chars[start..start + flen].iter().collect::<String>() == from
                {
                    let mut out: String = chars[..start].iter().collect();
                    out.push_str(to);
                    out.extend(chars[start + flen..].iter());
                    return Some((out, start));
                }
            }
        }
    }
    None
}

/// Recase `replacement` to match `sample`: all-upper, Titlecase, else lowercase.
fn match_case(sample: &str, replacement: &str) -> String {
    if sample
        .chars()
        .all(|c| c.is_uppercase() || !c.is_alphabetic())
        && sample.chars().any(char::is_uppercase)
    {
        replacement.to_ascii_uppercase()
    } else if sample.chars().next().is_some_and(char::is_uppercase) {
        let mut c = replacement.chars();
        c.next()
            .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
            .unwrap_or_default()
    } else {
        replacement.to_string()
    }
}

/// The 0-based char column of `tag` in `line` when it appears as a whole word
/// (bounded by non-word characters), or `None`. Used by the TODO/FIXME finder.
#[must_use]
pub fn tag_column(line: &str, tag: &str) -> Option<usize> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    for (col, (byte_idx, _)) in line.char_indices().enumerate() {
        if line[byte_idx..].starts_with(tag) {
            let before_ok = line[..byte_idx]
                .chars()
                .next_back()
                .is_none_or(|c| !is_word(c));
            let after_ok = line[byte_idx + tag.len()..]
                .chars()
                .next()
                .is_none_or(|c| !is_word(c));
            if before_ok && after_ok {
                return Some(col);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_number_increments_and_decrements() {
        // Cursor before the digits: increments in place, cursor at the number start.
        let (out, pos) = bump_number_at("x = 41;", 0, 1).unwrap();
        assert_eq!(out, "x = 42;");
        assert_eq!(pos, 4);
        // Decrement, cursor sitting on a digit.
        assert_eq!(bump_number_at("v9", 1, -1).unwrap().0, "v8");
        // Negative numbers: the leading '-' is part of the token.
        assert_eq!(bump_number_at("-1", 0, -1).unwrap().0, "-2");
        // No digit on the cursor's line → None.
        assert!(bump_number_at("no digits here", 0, 1).is_none());
        // A digit only on a later line is not reached from this line.
        assert!(bump_number_at("abc\n5", 0, 1).is_none());
    }

    #[test]
    fn transpose_chars_swaps_around_the_cursor() {
        // Cursor between 'a' and 'b' (offset 1): swap → "ba", cursor advances to 2.
        assert_eq!(transpose_chars_at("ab", 1), Some(("ba".to_string(), 2)));
        // At end of buffer: swap the last two.
        assert_eq!(transpose_chars_at("abc", 3), Some(("acb".to_string(), 3)));
        // No pair at the very start.
        assert!(transpose_chars_at("ab", 0).is_none());
        // Never across a newline.
        assert!(transpose_chars_at("a\nb", 1).is_none());
    }

    #[test]
    fn transpose_words_swaps_neighboring_words() {
        assert_eq!(transpose_words_at("foo bar", 5).unwrap().0, "bar foo");
        // Punctuation separator is preserved.
        assert_eq!(
            transpose_words_at("alpha, beta", 8).unwrap().0,
            "beta, alpha"
        );
        // Only one word → nothing to do.
        assert!(transpose_words_at("solo", 0).is_none());
    }

    #[test]
    fn sentence_starts_splits_on_terminators() {
        // Two sentences, then one after a newline; abbreviations without a
        // following space do not split.
        let text = "One. Two! Three?\nFour";
        assert_eq!(sentence_starts(text), vec![0, 5, 10, 17]);
        assert_eq!(sentence_starts("pi is 3.14 today"), vec![0]);
    }

    #[test]
    fn transpose_lines_swaps_with_the_line_above() {
        assert_eq!(transpose_lines_at("a\nb\nc", 2).unwrap().0, "b\na\nc");
        // Cursor past the last line: the last two swap.
        assert_eq!(transpose_lines_at("a\nb\n", 4).unwrap().0, "b\na\n");
        // Nothing above the first line.
        assert!(transpose_lines_at("a\nb", 0).is_none());
        assert!(transpose_lines_at("solo", 2).is_none());
    }

    #[test]
    fn transpose_sentences_swaps_around_the_cursor() {
        let (out, pos) = transpose_sentences_at("One. Two. Three.", 5).unwrap();
        assert_eq!(out, "Two. One. Three.");
        assert_eq!(pos, 9, "cursor lands after the swapped pair");
        // The separator (here a newline) stays where it was.
        assert_eq!(
            transpose_sentences_at("One.\nTwo.", 5).unwrap().0,
            "Two.\nOne."
        );
        assert!(transpose_sentences_at("Only one.", 0).is_none());
    }

    #[test]
    fn transpose_paragraphs_swaps_blank_line_delimited_blocks() {
        let text = "a1\na2\n\nb1\nb2\n";
        assert_eq!(
            transpose_paragraphs_at(text, 6).unwrap().0,
            "b1\nb2\n\na1\na2\n"
        );
        // Inside the first paragraph there is nothing to swap with.
        assert!(transpose_paragraphs_at(text, 0).is_none());
    }

    #[test]
    fn transpose_sections_swaps_across_double_blank_lines() {
        // A single blank line stays inside a section; two or more break it.
        let text = "a\n\nb\n\n\nc\n";
        assert_eq!(transpose_sections_at(text, 9).unwrap().0, "c\n\n\na\n\nb\n");
        assert!(transpose_sections_at("a\n\nb\n", 0).is_none());
    }

    #[test]
    fn delete_char_removes_the_character_at_the_cursor() {
        let (out, pos) = delete_char_at("abc", 1).unwrap();
        assert_eq!(out, "ac");
        assert_eq!(pos, 1, "cursor stays put");
        // A newline is just another character.
        assert_eq!(delete_char_at("a\nb", 1).unwrap().0, "ab");
        // Nothing to delete at the end of the buffer.
        assert!(delete_char_at("abc", 3).is_none());
        assert!(delete_char_at("", 0).is_none());
    }

    #[test]
    fn delete_word_removes_the_word_and_its_spacing() {
        let (out, pos) = delete_word_at("one two three", 4).unwrap();
        assert_eq!(out, "one three");
        assert_eq!(pos, 4, "cursor lands where the word was");
        // Between words: the next one goes.
        assert_eq!(delete_word_at("one two", 3).unwrap().0, "two");
        // The last word takes the space before it, never the newline after it.
        assert_eq!(delete_word_at("one two\nthree", 5).unwrap().0, "one\nthree");
        assert!(delete_word_at("   ", 0).is_none());
    }

    #[test]
    fn delete_sentence_removes_the_sentence_at_the_cursor() {
        assert_eq!(
            delete_sentence_at("One. Two. Three.", 5).unwrap().0,
            "One. Three."
        );
        // A sentence on its own line leaves the line break alone.
        assert_eq!(
            delete_sentence_at("One. Two.\nThree.", 5).unwrap().0,
            "One.\nThree."
        );
        assert_eq!(delete_sentence_at("Only one.", 0).unwrap().0, "");
        assert!(delete_sentence_at("", 0).is_none());
    }

    #[test]
    fn delete_paragraph_removes_the_block_and_its_blank_lines() {
        let text = "a1\na2\n\nb1\nb2\n";
        assert_eq!(delete_paragraph_at(text, 0).unwrap().0, "b1\nb2\n");
        // The last paragraph takes the blank lines before it.
        assert_eq!(delete_paragraph_at(text, 8).unwrap().0, "a1\na2\n");
        assert!(delete_paragraph_at("\n\n", 0).is_none());
    }

    #[test]
    fn delete_section_removes_the_double_blank_delimited_block() {
        // A single blank line stays inside a section; two or more break it.
        let text = "a\n\nb\n\n\nc\n";
        assert_eq!(delete_section_at(text, 0).unwrap().0, "c\n");
        assert_eq!(delete_section_at(text, 9).unwrap().0, "a\n\nb\n");
    }

    #[test]
    fn wrap_fills_paragraphs_and_keeps_blank_lines() {
        assert_eq!(wrap("one two three four", 9), "one two\nthree\nfour");
        // Blank lines separate chunks and are preserved verbatim.
        assert_eq!(wrap("a b c\n\nd e f\n", 3), "a b\nc\n\nd e\nf\n");
        // A word longer than the width still gets a line of its own.
        assert_eq!(wrap("aaaaaa b", 3), "aaaaaa\nb");
    }

    #[test]
    fn wrap_keeps_indentation_markers_and_bullets() {
        // The first line's indentation is repeated on every wrapped line.
        assert_eq!(wrap("    one two three", 10), "    one\n    two\n    three");
        // A comment marker shared by every line is kept as the fill prefix.
        assert_eq!(wrap("// one two\n// three", 9), "// one\n// two\n// three");
        // Each list item is its own chunk, with a hanging indent.
        assert_eq!(wrap("- one two\n- three", 7), "- one\n  two\n- three");
    }

    #[test]
    fn wrap_paragraph_at_rewrites_only_the_cursor_paragraph() {
        let text = "one two three\n\nkeep me\n";
        let (out, pos) = wrap_paragraph_at(text, 0, 7).unwrap();
        assert_eq!(out, "one two\nthree\n\nkeep me\n");
        assert_eq!(pos, 13);
        // Already wrapped → nothing to do.
        assert!(wrap_paragraph_at(&out, 0, 7).is_none());
        // No paragraph after the cursor.
        assert!(wrap_paragraph_at("", 0, 40).is_none());
    }

    #[test]
    fn smart_toggle_flips_words_and_symbols() {
        // Word pair, case preserved.
        assert_eq!(
            smart_toggle_at("let ok = true;", 9).unwrap().0,
            "let ok = false;"
        );
        assert_eq!(smart_toggle_at("v = FALSE", 4).unwrap().0, "v = TRUE");
        assert_eq!(smart_toggle_at("Yes", 0).unwrap().0, "No");
        // Symbol pair at the cursor.
        assert_eq!(smart_toggle_at("a && b", 2).unwrap().0, "a || b");
        assert_eq!(smart_toggle_at("x == y", 2).unwrap().0, "x != y");
        // Whole-word only: "online" is not "on".
        assert!(smart_toggle_at("online", 0).is_none());
        // Nothing togglable.
        assert!(smart_toggle_at("hello", 0).is_none());
    }

    #[test]
    fn tag_column_matches_whole_words_only() {
        assert_eq!(tag_column("// TODO: fix", "TODO"), Some(3));
        assert_eq!(
            tag_column("let todos = 1;", "TODO"),
            None,
            "identifier is not a tag"
        );
        assert_eq!(tag_column("no tags here", "TODO"), None);
    }

    #[test]
    fn line_ending_conversions_round_trip() {
        assert_eq!(to_lf("a\r\nb\rc\n"), "a\nb\nc\n");
        assert_eq!(to_crlf("a\nb\n"), "a\r\nb\r\n");
        // Mixed input normalizes cleanly (no doubled \r).
        assert_eq!(to_crlf("a\r\nb\n"), "a\r\nb\r\n");
    }

    #[test]
    fn squeeze_collapses_runs_of_blanks() {
        assert_eq!(squeeze_blank_lines("a\n\n\n\nb\n"), "a\n\nb\n");
        // A single blank line is kept; whitespace-only counts as blank.
        assert_eq!(squeeze_blank_lines("a\n\nb"), "a\n\nb");
        assert_eq!(squeeze_blank_lines("a\n \n\t\nb"), "a\n \nb");
    }

    #[test]
    fn rot13_is_its_own_inverse() {
        assert_eq!(rot13("Hello, World!"), "Uryyb, Jbeyq!");
        assert_eq!(rot13(&rot13("Hello, World!")), "Hello, World!");
    }

    // ---- property-based ("fuzz") tests ------------------------------------

    use proptest::prelude::*;

    proptest! {
        // Arbitrary text (including multibyte) and an ARBITRARY cursor — the
        // cursor is a caller-supplied integer, so out-of-range values must be
        // clamped, never panic on indexing or a non-char-boundary slice.
        #[test]
        fn cursor_ops_never_panic(text in ".*", cursor in 0usize..5000, delta in -4i64..4) {
            let _ = bump_number_at(&text, cursor, delta);
            let _ = transpose_chars_at(&text, cursor);
            let _ = transpose_words_at(&text, cursor);
            let _ = transpose_lines_at(&text, cursor);
            let _ = transpose_sentences_at(&text, cursor);
            let _ = transpose_paragraphs_at(&text, cursor);
            let _ = transpose_sections_at(&text, cursor);
            let _ = delete_char_at(&text, cursor);
            let _ = delete_word_at(&text, cursor);
            let _ = delete_sentence_at(&text, cursor);
            let _ = delete_paragraph_at(&text, cursor);
            let _ = delete_section_at(&text, cursor);
            let _ = smart_toggle_at(&text, cursor);
            let _ = wrap(&text, 0);
            let _ = wrap(&text, 40);
            let _ = wrap_paragraph_at(&text, cursor, 40);
            let _ = to_lf(&text);
            let _ = to_crlf(&text);
            let _ = squeeze_blank_lines(&text);
            let _ = rot13(&text);
            let _ = tag_column(&text, "TODO");
            let _ = tag_column(&text, &text); // tag == whole line edge case
        }

        // Any cursor a cursor-op returns must land within the returned text.
        #[test]
        fn returned_cursor_stays_in_bounds(text in ".*", cursor in 0usize..2000) {
            let ops = [
                bump_number_at(&text, cursor, 1),
                transpose_chars_at(&text, cursor),
                transpose_words_at(&text, cursor),
                transpose_lines_at(&text, cursor),
                transpose_sentences_at(&text, cursor),
                transpose_paragraphs_at(&text, cursor),
                transpose_sections_at(&text, cursor),
                delete_char_at(&text, cursor),
                delete_word_at(&text, cursor),
                delete_sentence_at(&text, cursor),
                delete_paragraph_at(&text, cursor),
                delete_section_at(&text, cursor),
                smart_toggle_at(&text, cursor),
                wrap_paragraph_at(&text, cursor, 40),
            ];
            for (out, pos) in ops.into_iter().flatten() {
                prop_assert!(
                    pos <= out.chars().count(),
                    "returned cursor {pos} past end {}",
                    out.chars().count()
                );
            }
        }
    }
}
