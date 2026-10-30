//! Which diagnostics to show inline, and which one sits at the cursor (T614).
//!
//! Pure selection rules over a file's [`Diagnostic`]s, kept out of the host so
//! they are unit-tested without an editor: the host maps the result onto the
//! editor widget's end-of-line virtual text and the status line.

use vix_lsp_core::{Diagnostic, Severity};

/// The `inline_diagnostics` setting: which lines show their diagnostic as
/// end-of-line virtual text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineMode {
    /// No inline diagnostics.
    Off,
    /// Only the cursor's line (the default).
    CursorLine,
    /// Every line that has a diagnostic.
    All,
}

impl InlineMode {
    /// Parse the setting's value: `"off"`, `"cursor-line"` or `"all"`. Any
    /// other value is [`InlineMode::CursorLine`], the default.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "off" => InlineMode::Off,
            "all" => InlineMode::All,
            _ => InlineMode::CursorLine,
        }
    }
}

/// One line's inline diagnostic: its zero-based line, severity and the first
/// line of its message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineDiagnostic {
    /// Zero-based line the diagnostic starts on.
    pub line: u32,
    /// The diagnostic's severity.
    pub severity: Severity,
    /// The first line of the message.
    pub message: String,
}

/// Rank a severity for "most severe wins": lower is more severe.
fn rank(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Information => 2,
        Severity::Hint => 3,
    }
}

/// The diagnostic to show for zero-based `line`: the most severe one whose
/// range starts there, the first published on a tie.
#[must_use]
pub fn diagnostic_on_line(diags: &[Diagnostic], line: u32) -> Option<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.range.start.line == line)
        // `min_by_key` keeps the first of equal keys, so the earliest wins a tie.
        .min_by_key(|d| rank(d.severity))
}

/// The inline diagnostics to draw under `mode`, one per line, sorted by line.
/// `cursor_line` is the zero-based line the cursor is on.
#[must_use]
pub fn inline_entries(
    diags: &[Diagnostic],
    mode: InlineMode,
    cursor_line: u32,
) -> Vec<InlineDiagnostic> {
    let mut lines: Vec<u32> = match mode {
        InlineMode::Off => return Vec::new(),
        InlineMode::CursorLine => vec![cursor_line],
        InlineMode::All => diags.iter().map(|d| d.range.start.line).collect(),
    };
    lines.sort_unstable();
    lines.dedup();
    lines
        .into_iter()
        .filter_map(|line| {
            diagnostic_on_line(diags, line).map(|d| InlineDiagnostic {
                line,
                severity: d.severity,
                message: first_line(&d.message),
            })
        })
        .collect()
}

/// The first line of `message`, trimmed (servers often end with a newline).
#[must_use]
pub fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or("").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_lsp_core::{Position, Range};

    fn diag(line: u32, severity: Severity, message: &str) -> Diagnostic {
        let at = Position { line, character: 0 };
        Diagnostic {
            range: Range { start: at, end: at },
            severity,
            message: message.to_string(),
            source: None,
            related: Vec::new(),
        }
    }

    fn sample() -> Vec<Diagnostic> {
        vec![
            diag(4, Severity::Warning, "unused variable"),
            diag(1, Severity::Hint, "consider this"),
            diag(1, Severity::Error, "mismatched types\nexpected u8"),
            diag(1, Severity::Error, "second error"),
        ]
    }

    #[test]
    fn parse_reads_the_three_values_and_defaults_to_cursor_line() {
        assert_eq!(InlineMode::parse("off"), InlineMode::Off);
        assert_eq!(InlineMode::parse("cursor-line"), InlineMode::CursorLine);
        assert_eq!(InlineMode::parse("all"), InlineMode::All);
        assert_eq!(InlineMode::parse("bogus"), InlineMode::CursorLine);
    }

    #[test]
    fn most_severe_first_published_wins_a_line() {
        let d = sample();
        let got = diagnostic_on_line(&d, 1).expect("line 1 has diagnostics");
        assert_eq!(got.message, "mismatched types\nexpected u8");
        assert!(diagnostic_on_line(&d, 2).is_none());
    }

    #[test]
    fn off_shows_nothing() {
        assert_eq!(inline_entries(&sample(), InlineMode::Off, 1), Vec::new());
    }

    #[test]
    fn cursor_line_shows_only_the_cursor_line() {
        let got = inline_entries(&sample(), InlineMode::CursorLine, 4);
        assert_eq!(
            got,
            vec![InlineDiagnostic {
                line: 4,
                severity: Severity::Warning,
                message: "unused variable".into(),
            }]
        );
        assert_eq!(
            inline_entries(&sample(), InlineMode::CursorLine, 0),
            Vec::new()
        );
    }

    #[test]
    fn all_shows_every_line_once_with_its_first_message_line() {
        let got = inline_entries(&sample(), InlineMode::All, 0);
        let lines: Vec<(u32, &str)> = got.iter().map(|e| (e.line, e.message.as_str())).collect();
        assert_eq!(lines, vec![(1, "mismatched types"), (4, "unused variable")]);
        assert_eq!(got[0].severity, Severity::Error);
    }
}
