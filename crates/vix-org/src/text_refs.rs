//! Text-reference and inline-markup features that each stand alone but are
//! individually too small for their own file: hyperlinks (find/cycle the
//! `[[target][description]]` under the cursor), sparse trees (fold
//! everything but TODO/occur matches), footnotes (jump to/create a `[fn:x]`
//! reference or definition), source blocks (find/replace a `#+begin_src`
//! body), and the read-only column-view table. Extracted from `lib.rs`
//! (T516). `LINK`/`BARE_LINK` live in `export.rs` (T516 slice 6), which
//! defines them; referenced back via `crate::export::`.

use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;

use super::{
    TAGS, governing, headline_level, line_of_char, split_keyword, strip_priority, subtree_range,
};
use crate::export::{BARE_LINK, LINK};

// ----- Hyperlinks ------------------------------------------------------------

/// The `[[target][description]]` or `[[target]]` link under the cursor, as
/// `(target, Some(description))` / `(target, None)`. `None` off any link.
#[must_use]
pub fn link_at(text: &str, cursor: usize) -> Option<(String, Option<String>)> {
    let (line_idx, line_start) = line_of_char(text, cursor);
    let l = *text.split('\n').collect::<Vec<_>>().get(line_idx)?;
    let col = cursor - line_start.min(cursor);
    let contains = |l: &str, start: usize, end: usize| {
        let sc = l[..start].chars().count();
        let ec = sc + l[start..end].chars().count();
        (sc..=ec).contains(&col)
    };
    for c in LINK.captures_iter(l) {
        let m = c.get(0)?;
        if contains(l, m.start(), m.end()) {
            return Some((c[1].to_string(), Some(c[2].to_string())));
        }
    }
    for c in BARE_LINK.captures_iter(l) {
        let m = c.get(0)?;
        if contains(l, m.start(), m.end()) {
            return Some((c[1].to_string(), None));
        }
    }
    None
}

/// The char offset of the next (`forward`) or previous link start after/before
/// `cursor`, cycling not included. `None` when there is no link that way.
#[must_use]
pub fn link_pos(text: &str, cursor: usize, forward: bool) -> Option<usize> {
    let mut starts: Vec<usize> = LINK
        .find_iter(text)
        .chain(BARE_LINK.find_iter(text))
        .map(|m| text[..m.start()].chars().count())
        .collect();
    starts.sort_unstable();
    starts.dedup();
    if forward {
        starts.into_iter().find(|&s| s > cursor)
    } else {
        starts.into_iter().rev().find(|&s| s < cursor)
    }
}

// ----- Sparse trees ----------------------------------------------------------

/// The fold ranges of a sparse tree: for every subtree containing no line
/// matching `pred`, the topmost such headline and the last line of its subtree
/// (inclusive — the headline stays visible as a fold marker, the body hides).
/// Subtrees with a match are descended into so non-matching children fold.
fn sparse_folds(text: &str, pred: impl Fn(&str) -> bool) -> Vec<(usize, usize)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut folds = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if headline_level(lines[i]).is_some() {
            let Some((s, e)) = subtree_range(&lines, i) else {
                break;
            };
            if lines[s..e].iter().any(|l| pred(l)) {
                i += 1; // a match inside: descend, folding only its misses
            } else {
                if e - 1 > s {
                    folds.push((s, e - 1));
                }
                i = e;
            }
        } else {
            i += 1;
        }
    }
    folds
}

/// Whether a line is a headline whose keyword is `TODO`.
fn is_todo_headline(line: &str) -> bool {
    headline_level(line).is_some_and(|lv| line[lv..].split_whitespace().next() == Some("TODO"))
}

/// Sparse-tree folds showing only `TODO` headlines (Org `C-c / t`): every
/// subtree without a TODO folds down to its headline.
#[must_use]
pub fn todo_tree_folds(text: &str) -> Vec<(usize, usize)> {
    sparse_folds(text, is_todo_headline)
}

/// Sparse-tree folds showing only subtrees containing `query`
/// (case-insensitive; Org `C-c /`'s occur view).
#[must_use]
pub fn occur_folds(text: &str, query: &str) -> Vec<(usize, usize)> {
    let q = query.to_lowercase();
    sparse_folds(text, |l| l.to_lowercase().contains(&q))
}

// ----- Footnotes -------------------------------------------------------------

/// A `[fn:LABEL]` footnote token.
static FOOTNOTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[fn:([A-Za-z0-9_-]+)\]").expect("footnote regex"));

/// The char offset of the start of 0-based `line` in `text`.
fn line_start_char(text: &str, line: usize) -> usize {
    text.split('\n')
        .take(line)
        .map(|l| l.chars().count() + 1)
        .sum()
}

/// Org's footnote action (`C-c C-x f`), all three behaviors in one:
/// on a `[fn:x]` **reference**, jump to its definition (created under a
/// `* Footnotes` headline when missing); on a **definition** line (starting
/// with `[fn:x]`), jump back to the first reference; anywhere else, insert a
/// new numbered reference at the cursor and append its empty definition.
/// Returns the new text (unchanged for pure jumps) and cursor.
#[must_use]
pub fn footnote(text: &str, cursor: usize) -> (String, usize) {
    let (line_idx, line_start) = line_of_char(text, cursor);
    let lines: Vec<&str> = text.split('\n').collect();
    let line = lines.get(line_idx).copied().unwrap_or_default();
    let col = cursor - line_start.min(cursor);
    let on_definition = FOOTNOTE.find(line).is_some_and(|m| m.start() == 0);
    let label_at_cursor = FOOTNOTE.captures_iter(line).find_map(|c| {
        let m = c.get(0)?;
        let sc = line[..m.start()].chars().count();
        let ec = sc + line[m.start()..m.end()].chars().count();
        (sc..=ec).contains(&col).then(|| c[1].to_string())
    });
    match label_at_cursor {
        // On a definition: jump to the first reference elsewhere.
        Some(label) if on_definition && col <= label.chars().count() + 5 => {
            let needle = format!("[fn:{label}]");
            for (i, l) in lines.iter().enumerate() {
                if let Some(pos) = l.find(&needle)
                    && (i != line_idx || pos != 0)
                {
                    let sc = l[..pos].chars().count();
                    return (text.to_string(), line_start_char(text, i) + sc);
                }
            }
            (text.to_string(), cursor)
        }
        // On a reference: jump to (or create) the definition.
        Some(label) => {
            let needle = format!("[fn:{label}]");
            if let Some(i) = lines
                .iter()
                .enumerate()
                .find_map(|(i, l)| (i != line_idx && l.starts_with(&needle)).then_some(i))
            {
                return (text.to_string(), line_start_char(text, i));
            }
            let new = append_footnote_definition(text, &label);
            let pos = new.chars().count();
            (new, pos)
        }
        // Elsewhere: create the next numbered footnote.
        None => {
            let next = FOOTNOTE
                .captures_iter(text)
                .filter_map(|c| c[1].parse::<u64>().ok())
                .max()
                .unwrap_or(0)
                + 1;
            let mut new: String = text.chars().take(cursor).collect();
            let tail: String = text.chars().skip(cursor).collect();
            let _ = write!(new, "[fn:{next}]");
            new.push_str(&tail);
            let new = append_footnote_definition(&new, &next.to_string());
            let pos = new.chars().count();
            (new, pos)
        }
    }
}

/// Append an empty `[fn:label] ` definition at the end of `text`, under a
/// `* Footnotes` headline (created when missing, matching Org's default
/// `org-footnote-section`). The cursor belongs at the end of the result.
fn append_footnote_definition(text: &str, label: &str) -> String {
    let mut out = text.trim_end_matches('\n').to_string();
    let has_section = out
        .split('\n')
        .any(|l| headline_level(l).is_some_and(|lv| l[lv..].trim() == "Footnotes"));
    if !has_section {
        out.push_str("\n\n* Footnotes");
    }
    let _ = write!(out, "\n\n[fn:{label}] ");
    out
}

/// The governing headline line of the `:ID: value` property line matching
/// `id` (the property line itself before any headline). `None` when `text`
/// carries no such property.
#[must_use]
pub fn id_location(text: &str, id: &str) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let hit = lines.iter().position(|l| {
        let t = l.trim();
        // `t.get(..4)` (not `t.len() >= 4 && t[..4]`, T559): a byte-length
        // check alone doesn't prove offset 4 is a *char* boundary -- a
        // multi-byte character straddling it panics the raw slice before
        // the literal comparison even runs, on a line that doesn't even
        // match. `get` returns `None` instead, and once it returns `Some`
        // offset 4 is proven a boundary, so the later `t[4..]` is safe too.
        t.get(..4).is_some_and(|p| p.eq_ignore_ascii_case(":id:")) && t[4..].trim() == id
    })?;
    Some(governing(&lines, hit).unwrap_or(hit))
}

// ----- Source blocks ---------------------------------------------------------

/// Whether `line` opens a source block, returning its language (may be empty).
fn src_begin(line: &str) -> Option<String> {
    let t = line.trim_start();
    // `t.get(..11)` (T559, same char-boundary reasoning as `id_location`
    // above): proves offset 11 is a char boundary before the later
    // `t[11..]` slice, instead of only checking byte length.
    let rest = if t
        .get(..11)
        .is_some_and(|p| p.eq_ignore_ascii_case("#+begin_src"))
    {
        &t[11..]
    } else {
        return None;
    };
    Some(
        rest.split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string(),
    )
}

/// The `#+begin_src` block containing `line` (begin/end line inclusive, or the
/// cursor on either fence), as `(begin, end, language)`. `None` outside any
/// block or in an unterminated one.
#[must_use]
pub fn src_block_at(text: &str, line: usize) -> Option<(usize, usize, String)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let begin = (0..=line.min(lines.len().saturating_sub(1)))
        .rev()
        .find(|&i| src_begin(lines[i]).is_some())?;
    let lang = src_begin(lines[begin])?;
    let end = (begin + 1..lines.len()).find(|&i| {
        lines[i]
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("#+end_src")
    })?;
    (line <= end).then_some((begin, end, lang))
}

/// Replace the body of the source block opening at `begin_line` with `body`
/// (trailing newline optional). `None` when `begin_line` no longer opens a
/// terminated source block — e.g. the buffer changed while a dedicated-buffer
/// edit was open.
#[must_use]
pub fn replace_src_body(text: &str, begin_line: usize, body: &str) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    src_begin(lines.get(begin_line)?)?;
    let (begin, end, _) = src_block_at(text, begin_line)?;
    if begin != begin_line {
        return None;
    }
    let mut out: Vec<&str> = lines[..=begin].to_vec();
    let body = body.trim_end_matches('\n');
    if !body.is_empty() {
        out.extend(body.split('\n'));
    }
    out.extend_from_slice(&lines[end..]);
    Some(out.join("\n"))
}

// ----- Column view -----------------------------------------------------------

/// Render the buffer's headlines as an Org column-view table (Org
/// `C-c C-x C-c`, read-only flavor): `ITEM` (indented by level), `TODO`,
/// `PRIORITY`, and `TAGS` columns.
#[must_use]
pub fn column_view(text: &str) -> String {
    let mut out = String::from("| ITEM | TODO | PRIORITY | TAGS |\n|---|---|---|---|\n");
    for line in text.split('\n') {
        let Some(level) = headline_level(line) else {
            continue;
        };
        let bare = TAGS.replace(line, "");
        let tags = TAGS
            .captures(line)
            .map_or_else(String::new, |c| c[1].to_string());
        let (keyword, body) = split_keyword(bare[level..].trim());
        let (prio, title) = match strip_priority(body) {
            Some((p, after)) => (format!("[#{p}]"), after),
            None => (String::new(), body),
        };
        let _ = writeln!(
            out,
            "| {}{} | {} | {} | {} |",
            "  ".repeat(level - 1),
            title.replace('|', "\\vert{}"),
            keyword.trim(),
            prio,
            tags,
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // T559: `id_location`/`src_begin` used to check byte *length* before
    // slicing at a fixed byte offset, not that the offset was actually a
    // char *boundary* -- a multi-byte character straddling it panicked the
    // slice before the keyword comparison even ran, on a line that
    // doesn't even match. Byte offsets verified precisely, not eyeballed.

    #[test]
    fn id_location_does_not_panic_on_a_multibyte_char_at_the_checked_offset() {
        // 3 ASCII bytes then a 2-byte 'é' starting at byte 3 -- byte
        // offset 4 (the check's slice point) lands inside that character.
        let doc = "abcé rest of line, not a real property\n";
        assert_eq!(id_location(doc, "whatever"), None);
    }

    #[test]
    fn src_begin_does_not_panic_on_a_multibyte_char_at_the_checked_offset() {
        // Same reasoning as `category_value`'s own regression test in
        // columns.rs: 10 ASCII bytes then 'é' starting at byte 10 --
        // offset 11 lands inside it.
        let line = "1234567890é rest of line, not a real keyword";
        assert_eq!(src_begin(line), None);
    }
}
