//! Matching HTML/XML tag navigation.
//!
//! Given a cursor inside a `<tag>` or `</tag>`, [`matching_tag`] finds the char
//! offset of its partner's `<` — the closing tag for an opening one, or the
//! opening tag for a closing one — accounting for nested same-name tags. Pure and
//! unit-tested; the host uses it to move the cursor.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// One parsed tag occurrence in the document.
struct Tag {
    /// Char offset of the `<`.
    open: usize,
    /// Lowercased tag name.
    name: String,
    /// A closing tag (`</name>`).
    closing: bool,
    /// A self-closing tag (`<name/>`) — matched by nothing.
    self_closing: bool,
}

/// Scan `text` for tags (`<name …>`, `</name>`, `<name/>`), skipping comments and
/// declarations (`<!-- … -->`, `<!doctype …>`, `<?…?>`).
fn scan(text: &str) -> Vec<Tag> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut tags = Vec::new();
    let mut i = 0;
    while i < n {
        if chars[i] != '<' {
            i += 1;
            continue;
        }
        // Skip comments / declarations / processing instructions.
        if chars.get(i + 1) == Some(&'!') || chars.get(i + 1) == Some(&'?') {
            i += 1;
            continue;
        }
        let open = i;
        let closing = chars.get(i + 1) == Some(&'/');
        let mut j = i + if closing { 2 } else { 1 };
        let name_start = j;
        while j < n && (chars[j].is_alphanumeric() || matches!(chars[j], '-' | '_' | ':' | '.')) {
            j += 1;
        }
        let name: String = chars[name_start..j].iter().collect::<String>().to_ascii_lowercase();
        if name.is_empty() {
            i += 1;
            continue;
        }
        // Find the tag's `>` and whether it self-closes.
        let mut k = j;
        while k < n && chars[k] != '>' {
            k += 1;
        }
        let self_closing = k > 0 && chars.get(k - 1) == Some(&'/');
        tags.push(Tag { open, name, closing, self_closing });
        i = k + 1;
    }
    tags
}

/// The char offset of the tag matching the one under `cursor`, or `None` when the
/// cursor isn't in a tag, the tag self-closes, or no partner is found.
#[must_use]
pub fn matching_tag(text: &str, cursor: usize) -> Option<usize> {
    let tags = scan(text);
    // The tag whose `<…>` span contains the cursor. Recompute each tag's `>` by
    // finding the next '>' from its open (cheap; documents are small enough).
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let end_of = |open: usize| {
        let mut k = open;
        while k < n && chars[k] != '>' {
            k += 1;
        }
        k
    };
    let idx = tags.iter().position(|t| cursor >= t.open && cursor <= end_of(t.open))?;
    let tag = &tags[idx];
    if tag.self_closing {
        return None;
    }
    if tag.closing {
        // Walk backward for the matching opener, tracking nesting depth.
        let mut depth = 0i32;
        for t in tags[..idx].iter().rev() {
            if t.name != tag.name || t.self_closing {
                continue;
            }
            if t.closing {
                depth += 1;
            } else if depth == 0 {
                return Some(t.open);
            } else {
                depth -= 1;
            }
        }
    } else {
        // Walk forward for the matching closer.
        let mut depth = 0i32;
        for t in &tags[idx + 1..] {
            if t.name != tag.name || t.self_closing {
                continue;
            }
            if t.closing {
                if depth == 0 {
                    return Some(t.open);
                }
                depth -= 1;
            } else {
                depth += 1;
            }
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
}
