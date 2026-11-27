//! Matching HTML/XML tag navigation.
//!
//! Given a cursor inside a `<tag>` or `</tag>`, [`matching_tag`] finds the char
//! offset of its partner's `<` — the closing tag for an opening one, or the
//! opening tag for a closing one — accounting for nested same-name tags. Pure and
//! unit-tested; the host uses it to move the cursor.
//!
//! The implementation is a lightweight scanner, not an HTML parser: it does not
//! know void elements (`<br>` without `/`), raw-text elements, or attribute
//! quoting, and it passes over only the opening `<!`/`<?` of comments and
//! declarations. Spec: `crates/vix-tags/spec/index.md`.
//!
//! All offsets are **char** offsets (not bytes), matching the editor buffer.

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

/// One parsed tag occurrence in the document (private; produced by [`scan`]).
struct Tag {
    /// Char offset of the `<`.
    open: usize,
    /// Char offset of the closing `>`, or the text's length when unterminated.
    end: usize,
    /// Lowercased tag name.
    name: String,
    /// A closing tag (`</name>`).
    closing: bool,
    /// A self-closing tag (`<name/>`) — matched by nothing.
    self_closing: bool,
}

/// Whether `c` can be part of a tag name.
fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')
}

/// Scan `text` for tags (`<name …>`, `</name>`, `<name/>`).
///
/// A `<` not followed by a name (optionally after `/`) starts no tag, which is
/// how comments, declarations and processing instructions (`<!-- … -->`,
/// `<!doctype …>`, `<?…?>`) are passed over. Only that `<` is passed over, not
/// the body, so a tag-looking string inside a comment is still reported. A `<`
/// inside another tag's `<…>` span starts no tag. Tag names are lowercased so
/// matching is case-insensitive.
fn scan(text: &str) -> Vec<Tag> {
    let chars: Vec<char> = text.chars().collect();
    let mut tags = Vec::new();
    // First offset past the last tag found; a `<` before it is inside that tag.
    let mut resume = 0;
    for (open, &c) in chars.iter().enumerate() {
        if c != '<' || open < resume {
            continue;
        }
        // Every offset below is at most `chars.len()`: each is an index of
        // `chars` plus a count of chars taken from the rest of it.
        #[expect(
            clippy::arithmetic_side_effects,
            clippy::indexing_slicing,
            reason = "RCA-EXC-2: open is an index of chars, name_start is at most chars.len(), and each count is of chars after its start, so every sum and slice stays within chars.len()"
        )]
        let tag = {
            let closing = chars.get(open + 1) == Some(&'/');
            let name_start = open + 1 + usize::from(closing);
            let name_len = chars[name_start..]
                .iter()
                .take_while(|&&c| is_name_char(c))
                .count();
            if name_len == 0 {
                continue;
            }
            let name_end = name_start + name_len;
            // The tag's `>`; an unterminated tag runs to the end of the text.
            let end = name_end + chars[name_end..].iter().take_while(|&&c| c != '>').count();
            Tag {
                open,
                end,
                name: chars[name_start..name_end]
                    .iter()
                    .collect::<String>()
                    .to_ascii_lowercase(),
                closing,
                // `/` immediately before the `>`.
                self_closing: chars[..end].last() == Some(&'/'),
            }
        };
        let end = tag.end;
        tags.push(tag);
        resume = end;
    }
    tags
}

/// The char offset of the tag matching the one under `cursor`, or `None` when the
/// cursor isn't in a tag, the tag self-closes, or no partner is found.
///
/// The cursor counts as "in" a tag anywhere from its `<` through its `>`
/// inclusive. Only same-name tags participate in nesting depth.
///
/// # Examples
/// ```
/// let html = "<div><span>x</span></div>";
/// assert_eq!(vix_tags::matching_tag(html, 1), Some(19));
/// assert_eq!(vix_tags::matching_tag(html, 20), Some(0));
/// assert_eq!(vix_tags::matching_tag("<br/>", 1), None);
/// ```
#[must_use]
pub fn matching_tag(text: &str, cursor: usize) -> Option<usize> {
    let tags = scan(text);
    let idx = tags
        .iter()
        .position(|t| cursor >= t.open && cursor <= t.end)?;
    let tag = tags.get(idx)?;
    if tag.self_closing {
        return None;
    }
    // Same-name, non-self-closing tags on the side of the partner, nearest
    // first: before the cursor's tag for a closer, after it for an opener.
    let partners = |t: &&Tag| t.name == tag.name && !t.self_closing;
    let (mut before, mut after) = (
        tags.iter().take(idx).rev().filter(partners),
        tags.iter().skip(idx.checked_add(1)?).filter(partners),
    );
    let side: &mut dyn Iterator<Item = &Tag> = if tag.closing { &mut before } else { &mut after };
    // The partner is the first tag of the opposite kind not paired with a
    // same-kind tag nested in between.
    let mut depth = 0u32;
    for t in side {
        if t.closing == tag.closing {
            // Nested same-kind tags are bounded by the number of tags.
            depth = depth.checked_add(1)?;
        } else if let Some(shallower) = depth.checked_sub(1) {
            depth = shallower;
        } else {
            return Some(t.open);
        }
    }
    None
}

/// One tag read from the text by [`read_tag`]: its `>` (or the text's end)
/// and what the matcher compares.
struct ReadTag {
    /// Char offset of the `<`.
    open: usize,
    /// Char offset of the closing `>`, or the text's length when unterminated.
    end: usize,
    /// Lowercased tag name.
    name: String,
    /// A closing tag (`</name>`).
    closing: bool,
    /// A self-closing tag (`<name/>`).
    self_closing: bool,
}

/// The tag whose `<` is at char offset `open`, reading `chars` (the text
/// from `open` on) only to its `>`: [`scan`]'s rules for one tag. `None`
/// when no tag starts there (no `<`, or no name after it).
fn read_tag(open: usize, mut chars: impl Iterator<Item = char>) -> Option<ReadTag> {
    if chars.next()? != '<' {
        return None;
    }
    let mut c = chars.next();
    let closing = c == Some('/');
    if closing {
        c = chars.next();
    }
    // `pos` is the char offset of `c`; it counts chars actually read, so
    // it never passes the text's length.
    let mut pos = open.checked_add(1)?.checked_add(usize::from(closing))?;
    let mut name = String::new();
    let mut last = None;
    while let Some(ch) = c.filter(|&ch| is_name_char(ch)) {
        name.push(ch);
        last = Some(ch);
        pos = pos.checked_add(1)?;
        c = chars.next();
    }
    if name.is_empty() {
        return None;
    }
    while let Some(ch) = c.filter(|&ch| ch != '>') {
        last = Some(ch);
        pos = pos.checked_add(1)?;
        c = chars.next();
    }
    Some(ReadTag {
        open,
        end: pos,
        name: name.to_ascii_lowercase(),
        closing,
        self_closing: last == Some('/'),
    })
}

/// Whether a `<` followed by `c1` then `c2` starts a tag (a name, after an
/// optional `/`).
fn starts_tag(c1: Option<char>, c2: Option<char>) -> bool {
    c1.is_some_and(is_name_char) || (c1 == Some('/') && c2.is_some_and(is_name_char))
}

/// [`matching_tag`] without a copy of the text: the same answer, read from
/// the cursor outward (T851).
///
/// `len` is the text's length in chars. `forward(at)` yields the text's
/// chars from offset `at` on, in order, and `backward(at)` the chars before
/// `at`, nearest first (`at - 1`, `at - 2`, …, `0`); both are only called
/// with `at <= len`. A tag ends at the first `>` after its `<`, so the tag
/// under the cursor is found from the last `>` before it, and the partner
/// by reading tags only as far as it (or to a text end when there is none).
///
/// # Examples
/// ```
/// let html: Vec<char> = "<div><span>x</span></div>".chars().collect();
/// let found = vix_tags::matching_tag_in(
///     html.len(),
///     1,
///     |at| html[at..].iter().copied(),
///     |at| html[..at].iter().rev().copied(),
/// );
/// assert_eq!(found, Some(19));
/// ```
pub fn matching_tag_in<F, IF, B, IB>(
    len: usize,
    cursor: usize,
    forward: F,
    backward: B,
) -> Option<usize>
where
    F: Fn(usize) -> IF,
    IF: Iterator<Item = char>,
    B: Fn(usize) -> IB,
    IB: Iterator<Item = char>,
{
    if cursor > len {
        return None;
    }
    // The tag under the cursor starts at the first tag-starting `<` after
    // the last `>` before the cursor, if that `<` is at or before it.
    let gap = backward(cursor).take_while(|&c| c != '>').count();
    let segment = cursor.checked_sub(gap)?;
    let mut tag = None;
    for (k, c) in forward(segment).take(gap.checked_add(1)?).enumerate() {
        if c == '<' {
            let open = segment.checked_add(k)?;
            tag = read_tag(open, forward(open));
            if tag.is_some() {
                break;
            }
        }
    }
    let tag = tag.filter(|t| t.end >= cursor)?;
    if tag.self_closing {
        return None;
    }
    let mut depth = 0u32;
    // One partner candidate, nearest first: `Some(found)` ends the search.
    let mut visit = |t: &ReadTag| -> Option<Option<usize>> {
        if t.name != tag.name || t.self_closing {
            return Some(None);
        }
        if t.closing == tag.closing {
            depth = depth.checked_add(1)?;
        } else if let Some(shallower) = depth.checked_sub(1) {
            depth = shallower;
        } else {
            return Some(Some(t.open));
        }
        Some(None)
    };
    if tag.closing {
        // Tags before this one, nearest first. Each ends at a `>`; the tag
        // ending at one starts at the first tag-starting `<` after the `>`
        // before it. Reading back, `ahead` holds the two chars after the
        // current one.
        let mut ahead = (Some('<'), None);
        let mut in_segment = false;
        let mut first_open = None;
        for (k, c) in backward(tag.open).enumerate() {
            let i = tag.open.checked_sub(k)?.checked_sub(1)?;
            if c == '>' {
                if let Some(open) = first_open.take()
                    && let Some(t) = read_tag(open, forward(open))
                    && let Some(found) = visit(&t)?
                {
                    return Some(found);
                }
                in_segment = true;
            } else if c == '<' && in_segment && starts_tag(ahead.0, ahead.1) {
                first_open = Some(i);
            }
            ahead = (Some(c), ahead.0);
        }
        if let Some(open) = first_open
            && let Some(t) = read_tag(open, forward(open))
        {
            return visit(&t)?;
        }
        return None;
    }
    // Tags after this one, in order: each search resumes past the last tag.
    let mut from = tag.end.checked_add(1)?;
    'tags: while from <= len {
        for (k, c) in forward(from).enumerate() {
            if c != '<' {
                continue;
            }
            let open = from.checked_add(k)?;
            if let Some(t) = read_tag(open, forward(open)) {
                if let Some(found) = visit(&t)? {
                    return Some(found);
                }
                from = t.end.checked_add(1)?;
                continue 'tags;
            }
        }
        break;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`matching_tag_in`] over a char list, as a host would over a rope.
    fn matching_in(text: &str, cursor: usize) -> Option<usize> {
        let chars: Vec<char> = text.chars().collect();
        matching_tag_in(
            chars.len(),
            cursor,
            |at| chars[at..].iter().copied(),
            |at| chars[..at].iter().rev().copied(),
        )
    }

    /// The streaming matcher gives [`matching_tag`]'s answer at every
    /// cursor position of `text` and two past its end.
    fn assert_streaming_matches(text: &str) {
        for cursor in 0..=text.chars().count() + 2 {
            assert_eq!(
                matching_in(text, cursor),
                matching_tag(text, cursor),
                "at {cursor} in {text:?}"
            );
        }
    }

    #[test]
    fn streaming_matcher_agrees_on_a_table() {
        for text in [
            "",
            "<",
            "<a",
            "<div><span>x</span></div>",
            "<a><a></a></a>",
            "<A>x</a>",
            "<br/><p>hi</p>",
            "<!-- <b> --> <b>x</b>",
            "<a <b> c> </b></a>",
            "</a><a>",
            "<p>日本<em>é</em></p>\n<p>",
            "<x></y></x>",
            "a > b < c <d>e</d>",
        ] {
            assert_streaming_matches(text);
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig { cases: 512, ..Default::default() })]
        #[test]
        fn streaming_matcher_agrees_on_generated_text(
            chars in proptest::collection::vec(
                proptest::sample::select(vec!['<', '<', '>', '>', '/', '/', 'a', 'a', 'B', 'b', ' ', '!', 'é', '-']),
                0..40,
            )
        ) {
            let text: String = chars.into_iter().collect();
            assert_streaming_matches(&text);
        }
    }

    #[test]
    fn jumps_open_to_close() {
        let html = "<div><span>x</span></div>";
        // Cursor in the outer <div> (offset 1) → its </div> at offset 19.
        assert_eq!(matching_tag(html, 1), Some(19));
    }

    #[test]
    fn jumps_close_to_open() {
        let html = "<div><span>x</span></div>";
        // Cursor in </div> (offset 20) → the opening <div> at 0.
        assert_eq!(matching_tag(html, 20), Some(0));
    }

    #[test]
    fn handles_nested_same_name() {
        let html = "<a><a></a></a>";
        // Outer <a> at 0 → outer </a> at 10 (not the inner </a> at 6).
        assert_eq!(matching_tag(html, 1), Some(10));
        // Inner <a> at 3 → inner </a> at 6.
        assert_eq!(matching_tag(html, 4), Some(6));
    }

    #[test]
    fn self_closing_and_non_tag_return_none() {
        assert!(matching_tag("<br/> text", 1).is_none());
        assert!(matching_tag("plain text", 3).is_none());
        assert!(matching_tag("<div>", 1).is_none()); // no closer
    }

    #[test]
    fn closer_skips_nested_pairs_walking_back() {
        let html = "<a><a></a></a>";
        // Outer </a> at 10 → outer <a> at 0; inner </a> at 6 → inner <a> at 3.
        assert_eq!(matching_tag(html, 11), Some(0));
        assert_eq!(matching_tag(html, 7), Some(3));
    }

    #[test]
    fn only_same_name_tags_nest() {
        // An unclosed <b> is not a partner for </a>, either way.
        assert_eq!(matching_tag("<a><b></a>", 7), Some(0));
        assert_eq!(matching_tag("<a></b></a>", 1), Some(7));
    }

    #[test]
    fn self_closing_tags_are_not_partners() {
        assert_eq!(matching_tag("<a><a/></a>", 1), Some(7));
        assert_eq!(matching_tag("<a><a/></a>", 8), Some(0));
        assert_eq!(matching_tag("<a><a /></a>", 1), Some(8));
        assert_eq!(matching_tag("<a/", 1), None);
    }

    #[test]
    fn cursor_spans_from_lt_through_gt() {
        let html = "<ab></ab> x";
        assert_eq!(matching_tag(html, 0), Some(4));
        assert_eq!(matching_tag(html, 3), Some(4));
        assert_eq!(matching_tag(html, 8), Some(0));
        assert_eq!(matching_tag(html, 9), None);
    }

    #[test]
    fn names_are_case_insensitive_and_may_hold_punctuation() {
        assert_eq!(matching_tag("<DIV></div>", 1), Some(5));
        assert_eq!(matching_tag("<my-el.x_y:z></my-el.x_y:z>", 1), Some(13));
        assert_eq!(matching_tag("<a href=\"x\">t</a>", 1), Some(13));
    }

    #[test]
    fn non_tag_lt_is_passed_over() {
        assert_eq!(matching_tag("<!doctype html><a></a>", 16), Some(18));
        assert_eq!(matching_tag("<?xml?><a></a>", 8), Some(10));
        assert_eq!(matching_tag("1 < 2 <a></a>", 7), Some(9));
        assert_eq!(matching_tag("1 < 2", 2), None);
        // Only the `<` of a comment is passed over: its body is still scanned.
        assert_eq!(matching_tag("<a><!-- </a> --></a>", 1), Some(8));
    }

    #[test]
    fn lt_inside_a_tag_starts_no_tag() {
        // The `<a` at 3 is inside the first tag's `<…>` span.
        assert_eq!(matching_tag("<a <a></a>", 1), Some(6));
    }

    #[test]
    fn offsets_are_chars_not_bytes() {
        assert_eq!(matching_tag("é<a>ü</a>", 2), Some(5));
        assert_eq!(matching_tag("é<a>ü</a>", 6), Some(1));
    }
}
