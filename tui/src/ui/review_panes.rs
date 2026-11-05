//! The two-pane (side-by-side) branch review (T665): old text on the left,
//! new text on the right, aligned per hunk. The layout (pairing, blank
//! cells, wrapping, which inline row each row stands for) is
//! `vix_review::SideBySide`; this file only paints it. The inline buffer stays
//! the source of the cursor and of scrolling: the view starts at the row for
//! the buffer's top line and moves just far enough to keep the cursor's row
//! in sight, so the two panes scroll together. With the editor's soft wrap on
//! a long line wraps within its pane and the shorter side is padded, so the
//! panes stay row-aligned.

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthChar;
use vix_review::{Cell, GUTTER, LineKind, PaneKind, PaneRow, SideBySide, wrap_text};

use crate::app::App;
use crate::theme;

/// Draw the review's two-pane view into `area` when it is on and `area` is
/// wide enough; `false` (nothing drawn) means the caller draws the inline
/// buffer instead.
pub(super) fn draw_review_panes(app: &App, frame: &mut Frame, area: Rect) -> bool {
    let Some(panes) = app.review_panes_for(area.width) else {
        return false;
    };
    if area.height == 0 {
        return true;
    }
    let wrap = app.review_wrap();
    let (width, height) = (usize::from(area.width), usize::from(area.height));
    let cursor = app.editor.cursor_line();
    let first = panes.view_start(app.editor.top_visible_line(), cursor, height, |i| {
        panes.row_height(i, width, wrap)
    });
    let mut lines: Vec<Line> = Vec::new();
    for row in panes.rows.iter().skip(first) {
        if lines.len() >= height {
            break;
        }
        let here = row.first <= cursor && cursor <= row.last;
        lines.extend(row_lines(row, width, here, wrap));
    }
    lines.truncate(height);
    frame.render_widget(Paragraph::new(lines).style(theme::base()), area);
    true
}

/// The painted screen lines of one row, `width` columns wide: one, or with
/// `wrap` as many as the longer side of a pair needs.
fn row_lines(row: &PaneRow, width: usize, here: bool, wrap: bool) -> Vec<Line<'static>> {
    let mark = if here {
        Modifier::BOLD | Modifier::UNDERLINED
    } else {
        Modifier::empty()
    };
    match &row.kind {
        PaneKind::Pair { left, right } => {
            let (l, r) = SideBySide::halves(width);
            let count = if wrap { row.height(l, r) } else { 1 };
            (0..count)
                .map(|k| {
                    Line::from(
                        [
                            cell_spans(left.as_ref(), l, k, wrap, mark),
                            vec![Span::styled("│", theme::dim())],
                            cell_spans(right.as_ref(), r, k, wrap, mark),
                        ]
                        .concat(),
                    )
                })
                .collect()
        }
        PaneKind::Comment => vec![Line::from(Span::styled(
            fit(&row.text, width),
            theme::dim().add_modifier(Modifier::ITALIC | mark),
        ))],
        _ => vec![Line::from(Span::styled(
            fit(&row.text, width),
            Style::default().add_modifier(Modifier::BOLD | mark),
        ))],
    }
}

/// Screen line `k` of one half of a pair row: the line number (blank on
/// continuation lines) and its slice of the text, padded or cut to `width`.
/// With `wrap` the text is split with `wrap_text`; without it line 0 holds
/// the whole text, cut. Words that differ from the paired line
/// (`Cell::marks`) are drawn reversed.
fn cell_spans(
    cell: Option<&Cell>,
    width: usize,
    k: usize,
    wrap: bool,
    mark: Modifier,
) -> Vec<Span<'static>> {
    let blank = || vec![Span::styled(" ".repeat(width), theme::dim())];
    let Some(c) = cell else {
        return blank();
    };
    let (from, to) = if wrap {
        match wrap_text(&c.text, width.saturating_sub(GUTTER)).get(k) {
            Some(&range) => range,
            None => return blank(),
        }
    } else {
        (0, usize::MAX)
    };
    let base = match c.kind {
        LineKind::Removed => Style::default().fg(Color::Red),
        LineKind::Added => Style::default().fg(Color::Green),
        _ => Style::default(),
    }
    .add_modifier(mark);
    let strong = base.add_modifier(Modifier::REVERSED | Modifier::BOLD);
    let gutter = if k == 0 {
        format!("{:>4} ", c.number)
    } else {
        " ".repeat(GUTTER)
    };
    let prefix = cut(&gutter, width);
    let mut used = prefix
        .chars()
        .map(|ch| ch.width().unwrap_or(0))
        .sum::<usize>();
    let mut spans = vec![Span::styled(prefix, base)];
    let mut run = String::new();
    let mut run_marked = false;
    for (i, ch) in c
        .text
        .chars()
        .enumerate()
        .skip(from)
        .take(to - from.min(to))
    {
        let w = ch.width().unwrap_or(0);
        if used + w > width {
            break;
        }
        let marked = c.marks.iter().any(|&(a, b)| a <= i && i < b);
        if marked != run_marked && !run.is_empty() {
            spans.push(Span::styled(
                std::mem::take(&mut run),
                if run_marked { strong } else { base },
            ));
        }
        run_marked = marked;
        run.push(ch);
        used += w;
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, if run_marked { strong } else { base }));
    }
    spans.push(Span::styled(" ".repeat(width - used), base));
    spans
}

/// `text` cut to `width` display columns and padded with spaces to exactly
/// that width.
fn fit(text: &str, width: usize) -> String {
    let mut out = cut(text, width);
    let used: usize = out.chars().map(|ch| ch.width().unwrap_or(0)).sum();
    out.push_str(&" ".repeat(width - used));
    out
}

/// `text` cut to at most `width` display columns.
fn cut(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_review::Review;

    const DIFF: &str = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1,2 +1,2 @@\n-old\n+new\n ctx\n";

    #[test]
    fn fit_pads_and_cuts_to_the_width() {
        assert_eq!(fit("ab", 4), "ab  ");
        assert_eq!(fit("abcdef", 3), "abc");
        assert_eq!(fit("日本", 3), "日 ");
    }

    #[test]
    fn view_keeps_the_cursor_row_visible() {
        let panes = Review::new("r", DIFF).side_by_side();
        let last = panes.rows.last().unwrap().last;
        // Top of the buffer, cursor at the end, three rows of room.
        let first = panes.view_start(0, last, 3, |_| 1);
        assert_eq!(first, panes.rows.len() - 3);
        // Cursor already in view: the view stays where the buffer's top is.
        assert_eq!(panes.view_start(0, 0, 3, |_| 1), 0);
    }

    #[test]
    fn a_wrapped_cell_continues_on_blank_gutter_lines() {
        let text = |spans: Vec<Span<'static>>| {
            spans.iter().map(|s| s.content.as_ref()).collect::<String>()
        };
        let cell = Cell {
            number: 7,
            text: "abcdefghij".to_string(),
            kind: LineKind::Added,
            inline: 0,
            marks: vec![(4, 6)],
        };
        // Width 9: five columns of gutter, four of text.
        assert_eq!(
            text(cell_spans(Some(&cell), 9, 0, true, Modifier::empty())),
            "   7 abcd"
        );
        assert_eq!(
            text(cell_spans(Some(&cell), 9, 1, true, Modifier::empty())),
            "     efgh"
        );
        assert_eq!(
            text(cell_spans(Some(&cell), 9, 2, true, Modifier::empty())),
            "     ij  "
        );
        // Past the last piece the cell is blank, so the other side can go on.
        assert_eq!(
            text(cell_spans(Some(&cell), 9, 3, true, Modifier::empty())),
            "         "
        );
        // Not wrapped: one cut line.
        assert_eq!(
            text(cell_spans(Some(&cell), 9, 0, false, Modifier::empty())),
            "   7 abcd"
        );
        // The marked chars (4..6) are the reversed span on the second piece.
        let second = cell_spans(Some(&cell), 9, 1, true, Modifier::empty());
        let reversed: String = second
            .iter()
            .filter(|s| s.style.add_modifier.contains(Modifier::REVERSED))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(reversed, "ef");
    }
}
