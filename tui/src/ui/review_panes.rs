//! The two-pane (side-by-side) branch review (T665): old text on the left,
//! new text on the right, aligned per hunk. The layout (pairing, blank
//! cells, which inline row each row stands for) is `vix_review::SideBySide`;
//! this file only paints it. The inline buffer stays the source of the cursor
//! and of scrolling: the view starts at the row for the buffer's top line and
//! moves just far enough to keep the cursor's row in sight, so the two panes
//! scroll together.

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthChar;
use vix_review::{Cell, LineKind, PaneKind, PaneRow, SideBySide};

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
    let cursor = app.editor.cursor_line();
    let top_inline = app.editor.top_visible_line();
    let first = panes.view_start(top_inline, cursor, usize::from(area.height));
    let lines: Vec<Line> = panes
        .rows
        .iter()
        .skip(first)
        .take(usize::from(area.height))
        .map(|row| {
            let here = row.first <= cursor && cursor <= row.last;
            row_line(row, usize::from(area.width), here)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).style(theme::base()), area);
    true
}

/// One painted row, `width` columns wide.
fn row_line(row: &PaneRow, width: usize, here: bool) -> Line<'static> {
    let mark = if here {
        Modifier::BOLD | Modifier::UNDERLINED
    } else {
        Modifier::empty()
    };
    match &row.kind {
        PaneKind::Pair { left, right } => {
            let (half, rest) = SideBySide::halves(width);
            Line::from(
                [
                    cell_spans(left.as_ref(), half, mark),
                    vec![Span::styled("│", theme::dim())],
                    cell_spans(right.as_ref(), rest, mark),
                ]
                .concat(),
            )
        }
        PaneKind::Comment => Line::from(Span::styled(
            fit(&row.text, width),
            theme::dim().add_modifier(Modifier::ITALIC | mark),
        )),
        _ => Line::from(Span::styled(
            fit(&row.text, width),
            Style::default().add_modifier(Modifier::BOLD | mark),
        )),
    }
}

/// One half of a pair row: line number and text, padded or cut to `width`.
/// Words that differ from the paired line (`Cell::marks`) are drawn reversed.
fn cell_spans(cell: Option<&Cell>, width: usize, mark: Modifier) -> Vec<Span<'static>> {
    let Some(c) = cell else {
        return vec![Span::styled(" ".repeat(width), theme::dim())];
    };
    let base = match c.kind {
        LineKind::Removed => Style::default().fg(Color::Red),
        LineKind::Added => Style::default().fg(Color::Green),
        _ => Style::default(),
    }
    .add_modifier(mark);
    let strong = base.add_modifier(Modifier::REVERSED | Modifier::BOLD);
    let prefix = cut(&format!("{:>4} ", c.number), width);
    let mut used = prefix
        .chars()
        .map(|ch| ch.width().unwrap_or(0))
        .sum::<usize>();
    let mut spans = vec![Span::styled(prefix, base)];
    let mut run = String::new();
    let mut run_marked = false;
    for (i, ch) in c.text.chars().enumerate() {
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
        let first = panes.view_start(0, last, 3);
        assert_eq!(first, panes.rows.len() - 3);
        // Cursor already in view: the view stays where the buffer's top is.
        assert_eq!(panes.view_start(0, 0, 3), 0);
    }
}
