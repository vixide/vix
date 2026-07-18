//! Data and table state for the keyboard-shortcut help overlay.
//!
//! Each [`Row`] pairs a key combo (shown verbatim, never translated) with an
//! i18n key for its description (translated by the host). Pure data, so this
//! crate has no dependencies and the host owns rendering.
//!
//! The overlay itself is a [`Panel`]: a two-column table — **action name**,
//! **keyboard shortcut** — over [`Shortcut`] rows the host assembles from
//! every active source (the curated [`ROWS`], the menu-item accelerators, and
//! the active keymap's chord tables). The user types to filter, scrolls, and
//! sorts either column by clicking its header (first click ascending, second
//! descending).

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// One help row.
#[derive(Clone, Copy)]
pub struct Row {
    /// Key combo, shown verbatim (e.g. `"Ctrl P"`).
    pub keys: &'static str,
    /// i18n key for the description (e.g. `"help.command_palette"`).
    pub desc: &'static str,
}

/// All help rows, in display order.
pub const ROWS: &[Row] = &[
    Row {
        keys: "Ctrl P",
        desc: "help.command_palette",
    },
    Row {
        keys: "Ctrl O",
        desc: "help.open_file",
    },
    Row {
        keys: "Ctrl S / Ctrl Shift S",
        desc: "help.save",
    },
    Row {
        keys: "Ctrl N / Ctrl W",
        desc: "help.new_close",
    },
    Row {
        keys: "Ctrl Q",
        desc: "help.quit",
    },
    Row {
        keys: "Ctrl Z / Ctrl Shift Z",
        desc: "help.undo_redo",
    },
    Row {
        keys: "Ctrl X / Ctrl C / Ctrl V",
        desc: "help.cut_copy_paste",
    },
    Row {
        keys: "Ctrl A",
        desc: "help.select_all",
    },
    Row {
        keys: "Ctrl F / Ctrl R",
        desc: "help.find_replace",
    },
    Row {
        keys: "F3 / Shift F3",
        desc: "help.find_next_prev",
    },
    Row {
        keys: "Ctrl B / Ctrl E",
        desc: "help.toggle_focus_explorer",
    },
    Row {
        keys: "Ctrl Shift F",
        desc: "help.search_workspace",
    },
    Row {
        keys: "F12",
        desc: "help.goto_definition",
    },
    Row {
        keys: "Alt Left / Alt Right",
        desc: "help.position_history",
    },
    Row {
        keys: "F10 / Alt V,F,E,I,T,H",
        desc: "help.menu_bar",
    },
    Row {
        keys: "F1",
        desc: "help.this_help",
    },
    Row {
        keys: "Mouse",
        desc: "help.mouse",
    },
];

/// One assembled shortcut row: a translated action name (e.g. "Command
/// Palette") and its key combo (e.g. `"Ctrl P"`, shown verbatim).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    /// Translated, human-readable action name.
    pub action: String,
    /// Key combo, shown verbatim; never translated.
    pub keys: String,
}

/// A sortable column of the shortcut table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    /// The action-name column (first).
    Action,
    /// The keyboard-shortcut column (second).
    Keys,
}

/// Filter + sort + scroll state for the shortcut overlay. Sorting is
/// tri-state per click cycle: natural order until a header is clicked, then
/// ascending, then descending on a second click of the same header.
pub struct Panel {
    /// The assembled rows, in natural (source) order.
    pub rows: Vec<Shortcut>,
    /// The live filter; matched case-insensitively against both columns.
    pub query: String,
    /// The active sort: `None` = natural order, else the column and whether
    /// the order is ascending.
    pub sort: Option<(Column, bool)>,
    /// First visible filtered row.
    pub scroll: usize,
}

impl Panel {
    /// Open the overlay over `rows` with no filter and natural order.
    #[must_use]
    pub fn open(rows: Vec<Shortcut>) -> Self {
        Panel {
            rows,
            query: String::new(),
            sort: None,
            scroll: 0,
        }
    }

    /// Indices into [`Panel::rows`] of the rows matching the filter, ordered
    /// per the active sort (case-insensitive; the other column breaks ties).
    #[must_use]
    pub fn matches(&self) -> Vec<usize> {
        let needle = self.query.to_lowercase();
        let mut out: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                needle.is_empty()
                    || s.action.to_lowercase().contains(&needle)
                    || s.keys.to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect();
        if let Some((col, ascending)) = self.sort {
            out.sort_by(|&a, &b| {
                let (ra, rb) = (&self.rows[a], &self.rows[b]);
                let key = |r: &Shortcut| match col {
                    Column::Action => (r.action.to_lowercase(), r.keys.to_lowercase()),
                    Column::Keys => (r.keys.to_lowercase(), r.action.to_lowercase()),
                };
                let ord = key(ra).cmp(&key(rb));
                if ascending { ord } else { ord.reverse() }
            });
        }
        out
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

    /// A header click: sort `col` ascending, or flip to descending when `col`
    /// is already the ascending sort column (and back again).
    pub fn toggle_sort(&mut self, col: Column) {
        self.sort = match self.sort {
            Some((c, ascending)) if c == col => Some((col, !ascending)),
            _ => Some((col, true)),
        };
        self.scroll = 0;
    }

    /// Append a character to the filter and rewind the scroll.
    pub fn push(&mut self, c: char) {
        self.query.push(c);
        self.scroll = 0;
    }

    /// Remove the last character of the filter and rewind the scroll.
    pub fn backspace(&mut self) {
        self.query.pop();
        self.scroll = 0;
    }

    /// Scroll up by `n` rows, stopping at the top.
    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    /// Scroll down by `n` rows; the host clamps to the view in
    /// [`Panel::clamp_scroll`].
    pub fn scroll_down(&mut self, n: usize) {
        self.scroll += n;
    }

    /// Keep the scroll within the filtered list for a `view_h`-row viewport.
    pub fn clamp_scroll(&mut self, view_h: usize) {
        self.scroll = self.scroll.min(self.len().saturating_sub(view_h.max(1)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Panel {
        Panel::open(vec![
            Shortcut {
                action: "Command Palette".into(),
                keys: "Ctrl P".into(),
            },
            Shortcut {
                action: "Open File".into(),
                keys: "Ctrl O".into(),
            },
            Shortcut {
                action: "Save".into(),
                keys: "Ctrl S".into(),
            },
        ])
    }

    fn actions(p: &Panel) -> Vec<&str> {
        p.matches()
            .into_iter()
            .map(|i| p.rows[i].action.as_str())
            .collect()
    }

    #[test]
    fn filter_matches_both_columns_case_insensitively() {
        let mut p = sample();
        p.query = "palette".into();
        assert_eq!(actions(&p), ["Command Palette"]);
        p.query = "ctrl o".into();
        assert_eq!(actions(&p), ["Open File"]);
        p.query = "zzz".into();
        assert!(p.is_empty());
    }

    #[test]
    fn header_clicks_cycle_ascending_then_descending() {
        let mut p = sample();
        // Natural order until a header is clicked.
        assert_eq!(actions(&p), ["Command Palette", "Open File", "Save"]);
        p.toggle_sort(Column::Keys);
        assert_eq!(p.sort, Some((Column::Keys, true)));
        assert_eq!(actions(&p), ["Open File", "Command Palette", "Save"]);
        p.toggle_sort(Column::Keys);
        assert_eq!(p.sort, Some((Column::Keys, false)));
        assert_eq!(actions(&p), ["Save", "Command Palette", "Open File"]);
        // Switching columns starts ascending again.
        p.toggle_sort(Column::Action);
        assert_eq!(p.sort, Some((Column::Action, true)));
        assert_eq!(actions(&p), ["Command Palette", "Open File", "Save"]);
    }

    #[test]
    fn scroll_clamps_to_the_filtered_list() {
        let mut p = sample();
        p.scroll_down(10);
        p.clamp_scroll(2);
        assert_eq!(p.scroll, 1);
        p.scroll_up(5);
        assert_eq!(p.scroll, 0);
    }
}
