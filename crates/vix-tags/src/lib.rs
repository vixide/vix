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
        tags.push(Tag {
            open,
            end,
            name: chars[name_start..name_end]
                .iter()
                .collect::<String>()
                .to_ascii_lowercase(),
            closing,
            // `/` immediately before the `>`.
            self_closing: chars[..end].last() == Some(&'/'),
        });
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
    let tag = &tags[idx];
    if tag.self_closing {
        return None;
    }
    // Same-name, non-self-closing tags on the side of the partner, nearest
    // first: before the cursor's tag for a closer, after it for an opener.
    let partners = |t: &&Tag| t.name == tag.name && !t.self_closing;
    let (mut before, mut after) = (
        tags[..idx].iter().rev().filter(partners),
        tags[idx + 1..].iter().filter(partners),
    );
    let side: &mut dyn Iterator<Item = &Tag> = if tag.closing { &mut before } else { &mut after };
    // The partner is the first tag of the opposite kind not paired with a
    // same-kind tag nested in between.
    let mut depth = 0u32;
    for t in side {
        if t.closing == tag.closing {
            depth += 1;
        } else if depth == 0 {
            return Some(t.open);
        } else {
            depth -= 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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
