//! The Org `#+RESULTS:` block that follows an evaluated `src` block.
//!
//! [`results_block`] formats a result the way Org does: `: ` lines for
//! short text, a `#+begin_example` block (with Org's comma escaping) for
//! [`MIN_LINES_FOR_BLOCK`] lines or more, and a `[[file:…]]` link for an
//! image. [`results_edit`] says where that block goes in a buffer: replacing
//! the existing results of the block, or inserted after `#+end_src` with
//! a blank line between, as Org writes it. [`apply`] performs an edit on a
//! string.

/// Text with this many lines or more goes in an example block
/// (`org-babel-min-lines-for-block-output`).
pub const MIN_LINES_FOR_BLOCK: usize = 10;

/// A result to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultValue {
    /// Text output.
    Text(String),
    /// A link to a file, such as a saved image.
    File(String),
}

/// A line-range replacement: lines `start..end` (0-based, `end`
/// exclusive) become `text` (whole lines, each ending in `\n`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultsEdit {
    /// First replaced line.
    pub start: usize,
    /// One past the last replaced line (`start` for a pure insertion).
    pub end: usize,
    /// The new lines.
    pub text: String,
}

fn needs_escape(line: &str) -> bool {
    let rest = line.trim_start_matches([' ', '\t']).trim_start_matches(',');
    rest.starts_with('*') || rest.starts_with("#+")
}

/// The `#+RESULTS:` block for `value`, ending in a newline.
#[must_use]
pub fn results_block(value: &ResultValue) -> String {
    let mut out = String::from("#+RESULTS:\n");
    match value {
        ResultValue::File(path) => {
            out.push_str("[[file:");
            out.push_str(&path.replace(']', "%5D"));
            out.push_str("]]\n");
        }
        ResultValue::Text(text) => {
            let text = text.strip_suffix('\n').unwrap_or(text);
            if text.is_empty() {
                return out;
            }
            let lines: Vec<&str> = text.split('\n').collect();
            if lines.len() >= MIN_LINES_FOR_BLOCK {
                out.push_str("#+begin_example\n");
                for line in lines {
                    if needs_escape(line) {
                        out.push(',');
                    }
                    out.push_str(line);
                    out.push('\n');
                }
                out.push_str("#+end_example\n");
            } else {
                for line in lines {
                    if line.is_empty() {
                        out.push_str(":\n");
                    } else {
                        out.push_str(": ");
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            }
        }
    }
    out
}

fn is_results_line(line: &str) -> bool {
    let t = line.trim_start().to_ascii_lowercase();
    t.starts_with("#+results:") || t.starts_with("#+results[")
}

/// The line range (`start..end`) of the results that belong to the `src`
/// block ending at line `end_src`, if there are any. Only blank lines may
/// separate `#+end_src` from `#+RESULTS:`.
#[must_use]
pub fn find_results(buffer: &str, end_src: usize) -> Option<(usize, usize)> {
    let lines: Vec<&str> = buffer.split('\n').collect();
    let mut i = end_src + 1;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    if i >= lines.len() || !is_results_line(lines[i]) {
        return None;
    }
    let start = i;
    i += 1;
    let Some(first) = lines.get(i).map(|l| l.trim_start().to_ascii_lowercase()) else {
        return Some((start, i.min(lines.len())));
    };
    if let Some(kind) = first.strip_prefix("#+begin_") {
        let kind = kind
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
        let close = format!("#+end_{kind}");
        let mut j = i + 1;
        while j < lines.len() {
            if lines[j]
                .trim_start()
                .to_ascii_lowercase()
                .starts_with(&close)
            {
                return Some((start, j + 1));
            }
            j += 1;
        }
        // Unclosed: take only the opening line rather than the rest of the file.
        return Some((start, i + 1));
    }
    if first.starts_with("[[") {
        return Some((start, i + 1));
    }
    while i < lines.len() {
        let t = lines[i].trim_start();
        if t.starts_with(": ") || t == ":" || t.starts_with('|') {
            i += 1;
        } else {
            break;
        }
    }
    Some((start, i))
}

/// The edit that writes `block` (from [`results_block`]) as the results of
/// the `src` block ending at line `end_src`, or removes them when `block` is
/// `None`.
#[must_use]
pub fn results_edit(buffer: &str, end_src: usize, block: Option<&str>) -> ResultsEdit {
    match (find_results(buffer, end_src), block) {
        (Some((start, end)), Some(block)) => ResultsEdit {
            start,
            end,
            text: block.to_owned(),
        },
        // Clearing also removes the blank lines Org put before the results.
        (Some((_, end)), None) => ResultsEdit {
            start: end_src + 1,
            end,
            text: String::new(),
        },
        (None, Some(block)) => ResultsEdit {
            start: end_src + 1,
            end: end_src + 1,
            text: format!("\n{block}"),
        },
        (None, None) => ResultsEdit {
            start: end_src + 1,
            end: end_src + 1,
            text: String::new(),
        },
    }
}

fn line_offset(buffer: &str, line: usize) -> usize {
    if line == 0 {
        return 0;
    }
    buffer
        .match_indices('\n')
        .nth(line - 1)
        .map_or(buffer.len(), |(i, _)| i + 1)
}

/// `buffer` with `edit` applied.
#[must_use]
pub fn apply(buffer: &str, edit: &ResultsEdit) -> String {
    let start = line_offset(buffer, edit.start);
    let end = line_offset(buffer, edit.end).max(start);
    let mut out = String::with_capacity(buffer.len() + edit.text.len() + 1);
    out.push_str(&buffer[..start]);
    if start == buffer.len()
        && !buffer.is_empty()
        && !buffer.ends_with('\n')
        && !edit.text.is_empty()
    {
        out.push('\n');
    }
    out.push_str(&edit.text);
    out.push_str(&buffer[end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "* Heading\n#+begin_src python\n1 + 1\n#+end_src\n\nAfter.\n";

    #[test]
    fn short_text_uses_colon_lines() {
        assert_eq!(
            results_block(&ResultValue::Text("2\n\nthree\n".into())),
            "#+RESULTS:\n: 2\n:\n: three\n"
        );
        assert_eq!(
            results_block(&ResultValue::Text(String::new())),
            "#+RESULTS:\n"
        );
    }

    #[test]
    fn long_text_uses_an_escaped_example_block() {
        let text: String = (0..10)
            .map(|i| {
                if i == 3 {
                    "* not a heading\n".to_owned()
                } else {
                    format!("{i}\n")
                }
            })
            .collect();
        let block = results_block(&ResultValue::Text(text));
        assert!(block.starts_with("#+RESULTS:\n#+begin_example\n0\n"));
        assert!(block.contains("\n,* not a heading\n"));
        assert!(block.ends_with("9\n#+end_example\n"));
    }

    #[test]
    fn file_link() {
        assert_eq!(
            results_block(&ResultValue::File("notes-images/ab.png".into())),
            "#+RESULTS:\n[[file:notes-images/ab.png]]\n"
        );
    }

    #[test]
    fn insert_then_replace_then_clear() {
        let first = results_block(&ResultValue::Text("2".into()));
        let edit = results_edit(SRC, 3, Some(&first));
        let once = apply(SRC, &edit);
        assert_eq!(
            once,
            "* Heading\n#+begin_src python\n1 + 1\n#+end_src\n\n#+RESULTS:\n: 2\n\nAfter.\n"
        );

        let second = results_block(&ResultValue::Text("3".into()));
        let twice = apply(&once, &results_edit(&once, 3, Some(&second)));
        assert_eq!(twice, once.replace(": 2", ": 3"));

        let cleared = apply(&twice, &results_edit(&twice, 3, None));
        assert_eq!(cleared, SRC);
    }

    #[test]
    fn replaces_an_example_block_whole() {
        let buf = "#+end_src\n\n#+RESULTS:\n#+begin_example\na\n\nb\n#+end_example\nkeep\n";
        let edit = results_edit(buf, 0, Some("#+RESULTS:\n: x\n"));
        assert_eq!(apply(buf, &edit), "#+end_src\n\n#+RESULTS:\n: x\nkeep\n");
    }

    #[test]
    fn does_not_steal_other_content() {
        let buf = "#+end_src\nText\n#+RESULTS:\n: old\n";
        assert_eq!(find_results(buf, 0), None);
        let buf = "#+end_src\n#+results[abc]:\n| 1 | 2 |\n| 3 | 4 |\nx\n";
        assert_eq!(find_results(buf, 0), Some((1, 4)));
    }

    #[test]
    fn insert_at_end_without_newline() {
        let buf = "#+begin_src sh\necho\n#+end_src";
        let out = apply(buf, &results_edit(buf, 2, Some("#+RESULTS:\n: hi\n")));
        assert_eq!(out, "#+begin_src sh\necho\n#+end_src\n\n#+RESULTS:\n: hi\n");
    }
}
