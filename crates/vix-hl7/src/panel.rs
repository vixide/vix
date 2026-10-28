//! State of the HL7 view overlay: a filterable list of [`Row`]s with a highlight.

use crate::message::Message;
use crate::view::{Row, rows};

/// Query + selection + scroll state for the HL7 View overlay. `selected` and
/// `scroll` index into the *filtered* row list (see [`Panel::matches`]).
#[derive(Debug, Clone)]
pub struct Panel {
    /// All rows of the message.
    pub rows: Vec<Row>,
    /// The current filter text (matched case-insensitively against label and value).
    pub query: String,
    /// Index of the highlighted row within the filtered list.
    pub selected: usize,
    /// First visible filtered row, kept in sync by [`Panel::ensure_visible`].
    pub scroll: usize,
}

impl Panel {
    /// Open the view for `msg`, with the highlight on the segment row at or
    /// before source line `cursor_line`.
    #[must_use]
    pub fn open(msg: &Message, cursor_line: usize) -> Panel {
        let rows = rows(msg);
        let selected = rows
            .iter()
            .rposition(|r| r.line <= cursor_line && r.path.field == 0)
            .unwrap_or(0);
        Panel {
            rows,
            query: String::new(),
            selected,
            scroll: 0,
        }
    }

    /// Indices into [`Panel::rows`] of the rows matching the current query.
    #[must_use]
    pub fn matches(&self) -> Vec<usize> {
        let q = self.query.to_lowercase();
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                q.is_empty()
                    || r.label.to_lowercase().contains(&q)
                    || r.value.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Number of rows matching the current filter.
    #[must_use]
    pub fn len(&self) -> usize {
        self.matches().len()
    }

    /// Whether the filter matches no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Append a character to the filter and reset the highlight to the top.
    pub fn push(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
        self.scroll = 0;
    }

    /// Remove the last character of the filter and reset the highlight.
    pub fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
        self.scroll = 0;
    }

    /// Move the highlight up one row, stopping at the top.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Move the highlight down one row, stopping at the bottom.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.len());
    }

    /// Move the highlight up one page, stopping at the top.
    pub fn page_up(&mut self, page: usize) {
        self.selected = vix_list_state::page_up(self.selected, page);
    }

    /// Move the highlight down one page, stopping at the bottom.
    pub fn page_down(&mut self, page: usize) {
        self.selected = vix_list_state::page_down(self.selected, page, self.len());
    }

    /// Select a filtered row directly (e.g. from a click); returns whether
    /// `idx` landed on a real row.
    pub fn select_index(&mut self, idx: usize) -> bool {
        match vix_list_state::select_index(idx, self.len()) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    /// Adjust [`scroll`](Self::scroll) so the highlight stays within `height`
    /// visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.len());
    }

    /// The highlighted row, or `None` when the filter matches nothing.
    #[must_use]
    pub fn selected_row(&self) -> Option<&Row> {
        let idx = *self.matches().get(self.selected)?;
        self.rows.get(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> Panel {
        Panel::open(
            &Message::parse("MSH|^~\\&|APP\rPID|1||DOE^JANE\rPV1|1|I"),
            1,
        )
    }

    #[test]
    fn opens_on_the_segment_at_the_cursor() {
        let p = panel();
        assert_eq!(p.selected_row().unwrap().label, "PID");
    }

    #[test]
    fn filters_by_label_and_value() {
        let mut p = panel();
        for c in "jane".chars() {
            p.push(c);
        }
        assert!(!p.is_empty());
        assert!(p.selected_row().unwrap().value.contains("JANE"));
        p.backspace();
        p.backspace();
        p.backspace();
        p.backspace();
        assert!(p.len() > 3);
    }

    #[test]
    fn navigation_clamps() {
        let mut p = panel();
        p.page_down(100);
        assert_eq!(p.selected, p.len() - 1);
        p.page_up(100);
        assert_eq!(p.selected, 0);
        assert!(!p.select_index(999));
    }
}
