//! `%%` code cells: the "percent" format that `VSCode`, Spyder, Zed and
//! Jupytext use to split a plain source file into notebook-style cells.
//!
//! A cell marker is a line whose text, after leading whitespace, is the
//! language's line-comment prefix followed by `%%` (`# %%`, `// %%`,
//! `-- %%`, …; anything may follow, such as a title). A cell runs from the
//! line after its marker up to the line before the next marker, or the end
//! of the text. Lines above the first marker form a cell of their own.

use std::ops::Range;

/// The cell around a line: its marker line (if any) and its body lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// The marker line that opens the cell, or `None` for the lines above
    /// the first marker.
    pub marker: Option<usize>,
    /// The cell's body as a 0-based line range (end exclusive): from the
    /// line after the marker up to the next marker or the end of the text.
    pub body: Range<usize>,
}

/// Whether `line` is a cell marker for a language whose line comment starts
/// with `comment` (such as `#` or `//`; surrounding spaces are ignored). An
/// empty `comment` falls back to `#`.
#[must_use]
pub fn is_marker(line: &str, comment: &str) -> bool {
    let comment = match comment.trim() {
        "" => "#",
        c => c,
    };
    line.trim_start()
        .strip_prefix(comment)
        .is_some_and(|rest| rest.trim_start().starts_with("%%"))
}

/// The cell containing 0-based `line` of `text`, or `None` when `text` has
/// no cell marker at all (so the caller falls back to the current line). A
/// cursor on a marker line is in the cell that marker opens.
#[must_use]
pub fn cell_at(text: &str, line: usize, comment: &str) -> Option<Cell> {
    let lines: Vec<&str> = text.split('\n').collect();
    let markers: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| is_marker(l, comment))
        .map(|(i, _)| i)
        .collect();
    if markers.is_empty() {
        return None;
    }
    let line = line.min(lines.len().saturating_sub(1));
    let marker = markers.iter().copied().rev().find(|&m| m <= line);
    let start = marker.map_or(0, |m| m + 1);
    let end = markers
        .iter()
        .copied()
        .find(|&m| m > line)
        .unwrap_or(lines.len());
    Some(Cell {
        marker,
        body: start..end.max(start),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "import os\n# %% first\nx = 1\ny = 2\n  #%%\nprint(x)\n";

    #[test]
    fn markers_follow_the_comment_prefix() {
        assert!(is_marker("# %%", "#"));
        assert!(is_marker("  #%% title", "# "));
        assert!(is_marker("// %%", "//"));
        assert!(!is_marker("# %%", "//"));
        assert!(!is_marker("x = '# %%'", "#"));
        assert!(!is_marker("# % not a cell", "#"));
        assert!(is_marker("# %%", ""));
    }

    #[test]
    fn a_cell_runs_from_its_marker_to_the_next() {
        let cell = cell_at(TEXT, 2, "#").unwrap();
        assert_eq!(cell.marker, Some(1));
        assert_eq!(cell.body, 2..4);
        // On the marker line itself: the cell it opens.
        assert_eq!(cell_at(TEXT, 1, "#").unwrap().body, 2..4);
        // The last cell runs to the end of the text.
        let last = cell_at(TEXT, 5, "#").unwrap();
        assert_eq!(last.marker, Some(4));
        assert_eq!(last.body, 5..7);
    }

    #[test]
    fn lines_above_the_first_marker_are_a_cell() {
        let cell = cell_at(TEXT, 0, "#").unwrap();
        assert_eq!(cell.marker, None);
        assert_eq!(cell.body, 0..1);
    }

    #[test]
    fn no_marker_means_no_cell() {
        assert_eq!(cell_at("x = 1\ny = 2\n", 1, "#"), None);
        assert_eq!(cell_at("", 0, "#"), None);
    }

    #[test]
    fn a_line_past_the_end_is_clamped() {
        let cell = cell_at(TEXT, 99, "#").unwrap();
        assert_eq!(cell.marker, Some(4));
    }
}
