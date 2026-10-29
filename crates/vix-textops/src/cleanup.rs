//! The text half of "Code Cleanup" (T723): the whitespace tidy-ups that need
//! no language server.
//!
//! [`cleanup`] trims trailing whitespace from every line, drops blank lines at
//! the end of the file and makes sure a non-empty file ends in exactly one
//! newline (the file's own line ending style, `\n` or `\r\n`, is kept). The
//! host adds the language-server steps (organize imports, format) around it.

/// What [`cleanup`] changed, for the status line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Lines that had trailing whitespace removed.
    pub trimmed_lines: usize,
    /// Blank lines removed from the end of the file.
    pub trailing_blank_lines: usize,
    /// Whether a final newline was added.
    pub added_final_newline: bool,
}

impl Report {
    /// Whether the text was already clean.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        *self == Report::default()
    }
}

/// Tidy `text`; returns the new text and what changed.
#[must_use]
pub fn cleanup(text: &str) -> (String, Report) {
    let crlf = text.contains("\r\n");
    let eol = if crlf { "\r\n" } else { "\n" };
    let mut report = Report::default();
    let mut lines: Vec<&str> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim_end_matches([' ', '\t']);
        if trimmed.len() != line.len() {
            report.trimmed_lines += 1;
        }
        lines.push(trimmed);
    }
    // `split` yields a final empty piece when the text ends in a newline.
    let had_final_newline = lines.last().is_some_and(|l| l.is_empty()) && lines.len() > 1;
    if had_final_newline {
        lines.pop();
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
        report.trailing_blank_lines += 1;
    }
    if lines.is_empty() {
        // Whitespace-only or empty: nothing left, no final newline to add.
        report.trailing_blank_lines = report.trailing_blank_lines.saturating_sub(1);
        return (String::new(), report);
    }
    report.added_final_newline = !had_final_newline;
    let mut out = lines.join(eol);
    out.push_str(eol);
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_is_untouched() {
        let (out, r) = cleanup("a\nb\n");
        assert_eq!(out, "a\nb\n");
        assert!(r.is_clean());
    }

    #[test]
    fn trims_trailing_whitespace_and_blank_tail() {
        let (out, r) = cleanup("a  \n\tb\t\n\n\n");
        assert_eq!(out, "a\n\tb\n");
        assert_eq!(r.trimmed_lines, 2);
        assert_eq!(r.trailing_blank_lines, 2);
        assert!(!r.added_final_newline);
    }

    #[test]
    fn adds_a_missing_final_newline() {
        let (out, r) = cleanup("a\nb");
        assert_eq!(out, "a\nb\n");
        assert!(r.added_final_newline);
    }

    #[test]
    fn keeps_crlf_line_endings() {
        let (out, _) = cleanup("a \r\nb\r\n\r\n");
        assert_eq!(out, "a\r\nb\r\n");
    }

    #[test]
    fn empty_and_blank_files_become_empty() {
        assert_eq!(cleanup("").0, "");
        assert_eq!(cleanup("\n\n").0, "");
        assert_eq!(cleanup("  \n").0, "");
    }

    #[test]
    fn interior_blank_lines_are_kept() {
        assert_eq!(cleanup("a\n\n\nb\n").0, "a\n\n\nb\n");
    }
}
