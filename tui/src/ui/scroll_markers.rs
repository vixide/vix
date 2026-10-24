//! Scroll bar markers (T578): colored cells on the editor's vertical track where
//! git changes, diagnostics and search matches are, like an editor's overview
//! ruler. The sources are the ones the gutter and the text already use; this
//! module only buckets them into track rows. See
//! `crates/vix-editor/spec/scrollbars/index.md`.

use ratatui::style::Color;

use crate::editor::Tab;

/// Which source a marker came from; higher wins when several share a track row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Source {
    /// A live search match.
    Search,
    /// A git change or coverage gutter mark.
    Gutter,
    /// An LSP diagnostic.
    Diagnostic,
}

/// One marker: the 0-based buffer line, its color and its source.
#[derive(Debug, Clone, Copy)]
pub(super) struct Marker {
    pub line: usize,
    pub color: Color,
    pub source: Source,
}

/// Bucket `markers` into `height` track rows for a buffer of `total_lines`
/// lines: a marker on line `l` lands on row `round(l * (height - 1) / (total - 1))`,
/// the same proportional mapping the thumb uses. A row holding several markers
/// shows the highest-[`Source`] one (the later wins a tie, so the nearer-to-the-
/// bottom change of equal rank is not hidden by an earlier one).
#[must_use]
pub(super) fn bucket(markers: &[Marker], total_lines: usize, height: usize) -> Vec<Option<Color>> {
    let mut rows: Vec<Option<(Source, Color)>> = vec![None; height];
    if height == 0 {
        return Vec::new();
    }
    let max_line = total_lines.saturating_sub(1);
    let span = height - 1;
    for m in markers {
        let line = m.line.min(max_line);
        let row = (line * span + max_line / 2)
            .checked_div(max_line)
            .unwrap_or(0)
            .min(span);
        if rows[row].is_none_or(|(source, _)| m.source >= source) {
            rows[row] = Some((m.source, m.color));
        }
    }
    rows.into_iter().map(|r| r.map(|(_, c)| c)).collect()
}

/// The markers for `tab`'s editor from every source.
#[must_use]
pub(super) fn collect(tab: &Tab) -> Vec<Marker> {
    let editor = &tab.editor;
    let code = editor.code_ref();
    let len = code.len_chars();
    let line_of = |char_idx: usize| code.char_to_line(char_idx.min(len));
    let mut out = Vec::new();
    if let Some(marks) = editor.marks() {
        out.extend(marks.iter().map(|&(start, _, color)| Marker {
            line: line_of(start),
            color,
            source: Source::Search,
        }));
    }
    if let Some(marks) = editor.gutter_marks() {
        out.extend(marks.iter().map(|&(line, color)| Marker {
            line,
            color,
            source: Source::Gutter,
        }));
    }
    if let Some(marks) = editor.diagnostic_marks() {
        out.extend(marks.iter().map(|&(start, _, color)| Marker {
            line: line_of(start),
            color,
            source: Source::Diagnostic,
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Color = Color::Red;
    const GREEN: Color = Color::Green;
    const YELLOW: Color = Color::Yellow;

    fn m(line: usize, color: Color, source: Source) -> Marker {
        Marker {
            line,
            color,
            source,
        }
    }

    #[test]
    fn markers_land_on_the_proportional_row() {
        // 101 lines over 11 rows: line 0 -> row 0, line 50 -> row 5, line 100 -> row 10.
        let rows = bucket(
            &[
                m(0, RED, Source::Gutter),
                m(50, GREEN, Source::Gutter),
                m(100, YELLOW, Source::Gutter),
            ],
            101,
            11,
        );
        assert_eq!(rows[0], Some(RED));
        assert_eq!(rows[5], Some(GREEN));
        assert_eq!(rows[10], Some(YELLOW));
        assert_eq!(rows.iter().flatten().count(), 3);
    }

    #[test]
    fn the_highest_source_wins_a_shared_row() {
        let rows = bucket(
            &[
                m(10, GREEN, Source::Search),
                m(11, YELLOW, Source::Gutter),
                m(12, RED, Source::Diagnostic),
                m(13, GREEN, Source::Search),
            ],
            1000,
            10,
        );
        assert_eq!(rows[0], Some(RED), "a diagnostic beats gutter and search");
    }

    #[test]
    fn short_files_and_degenerate_tracks_are_safe() {
        assert_eq!(bucket(&[m(5, RED, Source::Gutter)], 1, 0), Vec::new());
        // One line, one row.
        assert_eq!(bucket(&[m(0, RED, Source::Gutter)], 1, 1), vec![Some(RED)]);
        // A marker past the end clamps to the last row.
        let rows = bucket(&[m(9_999, RED, Source::Gutter)], 10, 5);
        assert_eq!(rows[4], Some(RED));
        // No markers: an empty track.
        assert!(bucket(&[], 100, 6).iter().all(Option::is_none));
    }
}
