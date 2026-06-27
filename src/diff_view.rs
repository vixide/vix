//! Read-only side-by-comparison: a unified diff between two texts, for the
//! "Compare With File…" overlay.
//!
//! Built on `similar`'s line diff with a small context radius, grouped into
//! hunks. Pure data (no IO, no terminal) so the layout is unit-testable; the host
//! owns scrolling and `ui` renders the colored lines.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// The kind of a rendered diff line, selecting its color/prefix.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Unchanged context line.
    Context,
    /// Line present only in the new text (added).
    Add,
    /// Line present only in the old text (removed).
    Del,
    /// A separator between non-adjacent hunks.
    Sep,
}

/// One rendered diff line: its kind and text (without a trailing newline).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// What kind of line this is.
    pub kind: Kind,
    /// The line text, prefix not included (the renderer adds `+`/`-`/space).
    pub text: String,
}

/// Build a unified diff (3 lines of context) from `old` to `new`. Returns a
/// single context-free note line when the texts are identical.
#[must_use]
pub fn build(old: &str, new: &str) -> Vec<Line> {
    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(old, new);
    let groups = diff.grouped_ops(3);
    if groups.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (gi, group) in groups.iter().enumerate() {
        if gi > 0 {
            out.push(Line { kind: Kind::Sep, text: "\u{22ef}".to_string() });
        }
        for op in group {
            for change in diff.iter_changes(op) {
                let kind = match change.tag() {
                    ChangeTag::Delete => Kind::Del,
                    ChangeTag::Insert => Kind::Add,
                    ChangeTag::Equal => Kind::Context,
                };
                let text = change.value().trim_end_matches('\n').to_string();
                out.push(Line { kind, text });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_texts_have_no_diff() {
        assert!(build("a\nb\n", "a\nb\n").is_empty());
    }

    #[test]
    fn marks_added_and_removed_lines() {
        let lines = build("one\ntwo\nthree\n", "one\nTWO\nthree\n");
        assert!(lines.iter().any(|l| l.kind == Kind::Del && l.text == "two"));
        assert!(lines.iter().any(|l| l.kind == Kind::Add && l.text == "TWO"));
        assert!(lines.iter().any(|l| l.kind == Kind::Context && l.text == "one"));
    }

    #[test]
    fn separates_distant_hunks() {
        let mut newv: Vec<String> = (0..40).map(|i| format!("line {i}\n")).collect();
        let old: String = newv.concat();
        newv[2] = "CHANGED A\n".to_string();
        newv[37] = "CHANGED B\n".to_string();
        let lines = build(&old, &newv.concat());
        assert!(lines.iter().any(|l| l.kind == Kind::Sep), "distant hunks are separated");
    }
}
