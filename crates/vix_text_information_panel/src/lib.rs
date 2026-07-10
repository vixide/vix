//! Statistics about a span of text, plus the panel's row-selection state.
//!
//! Vix's Tools → About → Text… panel reports counts for the selection (or the
//! whole buffer when nothing is selected): characters, words, lines, sentences,
//! and paragraphs. The host gathers the text and opens a [`Panel`]; this crate
//! computes the [`Stats`], formats them into [`Row`]s, and tracks the selection
//! so a value can be inserted into the editor.
//!
//! The heuristics are deliberately simple and language-agnostic:
//! - **characters**: Unicode scalar values.
//! - **words**: whitespace-separated runs.
//! - **lines**: newline-separated lines ([`str::lines`] semantics).
//! - **sentences**: runs of sentence-ending punctuation (`.`, `!`, `?`); text
//!   with content but no terminator counts as one sentence.
//! - **paragraphs**: maximal runs of non-blank lines.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Computed statistics for a span of text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Number of Unicode characters.
    pub characters: usize,
    /// Number of whitespace-separated words.
    pub words: usize,
    /// Number of lines.
    pub lines: usize,
    /// Number of sentences.
    pub sentences: usize,
    /// Number of paragraphs.
    pub paragraphs: usize,
}

/// Compute [`Stats`] for `text`.
#[must_use]
pub fn analyze(text: &str) -> Stats {
    Stats {
        characters: text.chars().count(),
        words: text.split_whitespace().count(),
        lines: text.lines().count(),
        sentences: count_sentences(text),
        paragraphs: count_paragraphs(text),
    }
}

/// Count sentences as runs of `.`/`!`/`?`. Text that has content but no
/// terminator counts as a single sentence.
fn count_sentences(text: &str) -> usize {
    let is_term = |c: char| matches!(c, '.' | '!' | '?');
    let mut runs = 0usize;
    let mut prev_term = false;
    for c in text.chars() {
        let term = is_term(c);
        if term && !prev_term {
            runs += 1;
        }
        prev_term = term;
    }
    if runs == 0 && text.chars().any(|c| !c.is_whitespace()) {
        1
    } else {
        runs
    }
}

/// Count paragraphs as maximal runs of non-blank lines (blank = empty or only
/// whitespace).
fn count_paragraphs(text: &str) -> usize {
    let mut paragraphs = 0usize;
    let mut in_paragraph = false;
    for line in text.lines() {
        if line.trim().is_empty() {
            in_paragraph = false;
        } else if !in_paragraph {
            paragraphs += 1;
            in_paragraph = true;
        }
    }
    paragraphs
}

/// One row of the table: a `label` and its `value`.
#[derive(Clone, Debug)]
pub struct Row {
    /// Left-hand label.
    pub label: String,
    /// Right-hand value.
    pub value: String,
}

/// Build the display rows for `stats`.
#[must_use]
pub fn rows(stats: &Stats) -> Vec<Row> {
    let row = |label: &str, n: usize| Row {
        label: label.to_string(),
        value: n.to_string(),
    };
    vec![
        row("Characters", stats.characters),
        row("Words", stats.words),
        row("Lines", stats.lines),
        row("Sentences", stats.sentences),
        row("Paragraphs", stats.paragraphs),
    ]
}

/// Row-selection state for the Text Information overlay.
pub struct Panel {
    /// The rows, in display order.
    pub rows: Vec<Row>,
    /// Index of the highlighted row.
    pub selected: usize,
}

impl Panel {
    /// Open the panel over the rows built from `stats`.
    #[must_use]
    pub fn open(stats: &Stats) -> Self {
        Panel {
            rows: rows(stats),
            selected: 0,
        }
    }

    /// Number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the table has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Move the highlight up one row.
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move the highlight down one row.
    pub fn down(&mut self) {
        if self.selected + 1 < self.rows.len() {
            self.selected += 1;
        }
    }

    /// Select a row directly (e.g. from a click); returns whether `idx` was real.
    pub fn select_index(&mut self, idx: usize) -> bool {
        if idx < self.rows.len() {
            self.selected = idx;
            true
        } else {
            false
        }
    }

    /// The highlighted row's value (what insertion uses).
    #[must_use]
    pub fn selected_value(&self) -> String {
        self.rows
            .get(self.selected)
            .map(|r| r.value.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_basic_text() {
        let s = analyze("Hello world.\nHow are you?");
        assert_eq!(s.characters, 25);
        assert_eq!(s.words, 5);
        assert_eq!(s.lines, 2);
        assert_eq!(s.sentences, 2);
        assert_eq!(s.paragraphs, 1);
    }

    #[test]
    fn empty_text_is_all_zero() {
        assert_eq!(analyze(""), Stats::default());
    }

    #[test]
    fn content_without_terminator_is_one_sentence() {
        assert_eq!(analyze("just some words").sentences, 1);
    }

    #[test]
    fn consecutive_terminators_count_once() {
        assert_eq!(count_sentences("Wait... really?!"), 2);
    }

    #[test]
    fn paragraphs_split_on_blank_lines() {
        let text = "Para one\nstill one\n\nPara two\n\n\nPara three";
        assert_eq!(count_paragraphs(text), 3);
    }

    #[test]
    fn rows_and_panel_selection() {
        let s = analyze("a b c");
        let mut p = Panel::open(&s);
        assert_eq!(p.len(), 5);
        assert_eq!(p.rows[1].label, "Words");
        assert_eq!(p.selected_value(), "5"); // Characters row: "a b c" = 5 chars
        p.down();
        assert_eq!(p.selected_value(), "3"); // Words = 3
    }
}
