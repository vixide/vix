//! Data and table state for the keybinding editor overlay (Vix →
//! Keybindings…, improvement plan T204).
//!
//! Mirrors `vix-keyboard-shortcut-panel`'s shape — a searchable, sortable
//! table over a flat row list — but the rows carry enough to actually
//! *act on* a binding, not just display one: the raw `vix-macros` token
//! (not just its display string), the action id (not just its translated
//! title), where the binding currently comes from, and a tracked
//! [`Panel::selected`] row a rebind/reset targets. Pure data — the host
//! builds rows from `vix_keybindings::TABLES`/`SHARED` plus
//! `keybindings.toml`, and applies a rebind or a reset by writing back
//! through `vix_keybindings::user_bindings` and re-resolving; this crate
//! doesn't know either of those exist.
//!
//! Scope, deliberately: only the active keymap's **top-level** bindings
//! (plus the keymap-agnostic shared ones) are listed — a chorded
//! keymap's chord-continuation bindings (Emacs's `C-x`/`C-c` families,
//! Spacemacs's leader) aren't, because `vix-keybindings`' override layer
//! itself only ever resolves the top-level context (§
//! `crates/vix-keybindings/spec/index.md`, "Overrides never see a
//! non-empty context") — listing a chord binding as "rebindable" here
//! would promise something the system underneath can't actually do.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Where a row's effective binding currently comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The active keymap's own built-in table, or the keymap-agnostic
    /// shared bindings — no override in effect.
    BuiltIn,
    /// A user override, from `keybindings.toml`.
    User,
    /// A script override, naming the script's file stem.
    Script(String),
}

/// One editable row: a key token bound to an action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The key token in `vix-macros` grammar (e.g. `"C-S-k"`).
    pub key_token: String,
    /// The token rendered for display (e.g. `"Ctrl Shift K"`).
    pub key_display: String,
    /// The `App::run_action`-dispatchable id this key currently runs.
    pub action_id: String,
    /// Translated, human-readable action title.
    pub action_title: String,
    /// Where this row's effective binding comes from.
    pub source: Source,
}

impl Row {
    /// Whether "Reset to default" applies to this row. Only a user
    /// override can be removed through this editor — a script's own
    /// binding isn't persisted in `keybindings.toml` at all, and a
    /// built-in row has no override to remove in the first place.
    #[must_use]
    pub fn resettable(&self) -> bool {
        matches!(self.source, Source::User)
    }
}

/// A sortable column of the keybinding table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    /// The action-name column (first).
    Action,
    /// The keyboard-shortcut column (second).
    Keys,
}

/// Filter + sort + scroll + selection state for the keybinding editor.
/// Sorting is tri-state per click cycle, same as
/// `vix-keyboard-shortcut-panel`'s read-only table: natural order until a
/// header is clicked, then ascending, then descending on a second click
/// of the same header.
pub struct Panel {
    /// The assembled rows, in natural (source) order.
    pub rows: Vec<Row>,
    /// The live filter; matched case-insensitively against both columns.
    pub query: String,
    /// The active sort: `None` = natural order, else the column and
    /// whether the order is ascending.
    pub sort: Option<(Column, bool)>,
    /// First visible filtered row.
    pub scroll: usize,
    /// Index, into [`Panel::matches`], of the highlighted row — what a
    /// rebind or a reset targets.
    pub selected: usize,
}

impl Panel {
    /// Open the overlay over `rows` with no filter, natural order, and
    /// the first row selected.
    #[must_use]
    pub fn open(rows: Vec<Row>) -> Self {
        Panel {
            rows,
            query: String::new(),
            sort: None,
            scroll: 0,
            selected: 0,
        }
    }

    /// Indices into [`Panel::rows`] of the rows matching the filter,
    /// ordered per the active sort (case-insensitive; the other column
    /// breaks ties).
    #[must_use]
    pub fn matches(&self) -> Vec<usize> {
        let needle = self.query.to_lowercase();
        let mut out: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                needle.is_empty()
                    || r.action_title.to_lowercase().contains(&needle)
                    || r.key_display.to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect();
        if let Some((col, ascending)) = self.sort {
            out.sort_by(|&a, &b| {
                let (ra, rb) = (&self.rows[a], &self.rows[b]);
                let key = |r: &Row| match col {
                    Column::Action => (r.action_title.to_lowercase(), r.key_display.to_lowercase()),
                    Column::Keys => (r.key_display.to_lowercase(), r.action_title.to_lowercase()),
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

    /// The currently-highlighted row, if any (empty results select
    /// nothing).
    #[must_use]
    pub fn selected_row(&self) -> Option<&Row> {
        self.matches().get(self.selected).map(|&i| &self.rows[i])
    }

    /// A header click: sort `col` ascending, or flip to descending when
    /// `col` is already the ascending sort column (and back again).
    pub fn toggle_sort(&mut self, col: Column) {
        self.sort = match self.sort {
            Some((c, ascending)) if c == col => Some((col, !ascending)),
            _ => Some((col, true)),
        };
        self.selected = 0;
        self.scroll = 0;
    }

    /// Append a character to the filter, rewind the scroll, and reselect
    /// the first (now possibly different) row.
    pub fn push(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
        self.scroll = 0;
    }

    /// Remove the last character of the filter, rewind the scroll, and
    /// reselect the first row.
    pub fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
        self.scroll = 0;
    }

    /// Move the selection up by `n` rows, stopping at the top.
    pub fn select_up(&mut self, n: usize) {
        self.selected = self.selected.saturating_sub(n);
    }

    /// Move the selection down by `n` rows, stopping at the last match.
    pub fn select_down(&mut self, n: usize) {
        let last = self.len().saturating_sub(1);
        self.selected = (self.selected + n).min(last);
    }

    /// Keep the scroll within the filtered list for a `view_h`-row
    /// viewport, following the selection: scrolls up or down just enough
    /// to keep [`Panel::selected`] visible, never further.
    pub fn clamp_scroll(&mut self, view_h: usize) {
        let view_h = view_h.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + view_h {
            self.scroll = self.selected + 1 - view_h;
        }
        self.scroll = self.scroll.min(self.len().saturating_sub(view_h));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key_token: &str, action_id: &str, source: Source) -> Row {
        Row {
            key_token: key_token.to_string(),
            key_display: key_token.to_string(),
            action_id: action_id.to_string(),
            action_title: action_id.to_string(),
            source,
        }
    }

    fn sample() -> Panel {
        Panel::open(vec![
            row("C-p", "tools.palette", Source::BuiltIn),
            row("C-o", "file.open", Source::BuiltIn),
            row("C-S-k", "edit.duplicate_line", Source::User),
            row("F9", "script:demo:go", Source::Script("demo".to_string())),
        ])
    }

    #[test]
    fn only_a_user_override_is_resettable() {
        let p = sample();
        assert!(!p.rows[0].resettable()); // BuiltIn
        assert!(p.rows[2].resettable()); // User
        assert!(!p.rows[3].resettable()); // Script
    }

    #[test]
    fn filter_matches_both_columns_case_insensitively() {
        let mut p = sample();
        p.query = "palette".into();
        assert_eq!(p.len(), 1);
        assert_eq!(p.selected_row().unwrap().action_id, "tools.palette");
        p.query = "c-o".into();
        assert_eq!(p.len(), 1);
        assert_eq!(p.selected_row().unwrap().action_id, "file.open");
    }

    #[test]
    fn selection_moves_within_filtered_bounds_and_clamps() {
        let mut p = sample();
        assert_eq!(p.selected, 0);
        p.select_up(5); // already at 0, stays there
        assert_eq!(p.selected, 0);
        p.select_down(2);
        assert_eq!(p.selected, 2);
        p.select_down(10); // clamps to the last row, not past it
        assert_eq!(p.selected, 3);
        p.select_up(1);
        assert_eq!(p.selected, 2);
    }

    #[test]
    fn filtering_resets_selection_and_scroll() {
        let mut p = sample();
        p.select_down(3);
        p.scroll = 2;
        p.push('x'); // no row matches "x"
        assert_eq!(p.selected, 0);
        assert_eq!(p.scroll, 0);
        assert!(p.is_empty());
        assert!(p.selected_row().is_none());
    }

    #[test]
    fn toggle_sort_cycles_ascending_then_descending() {
        let mut p = sample();
        p.toggle_sort(Column::Keys);
        assert_eq!(p.sort, Some((Column::Keys, true)));
        p.toggle_sort(Column::Keys);
        assert_eq!(p.sort, Some((Column::Keys, false)));
        // Switching to the other column starts ascending again.
        p.toggle_sort(Column::Action);
        assert_eq!(p.sort, Some((Column::Action, true)));
    }

    #[test]
    fn clamp_scroll_follows_the_selection_in_both_directions() {
        let mut p = sample();
        p.select_down(3); // selected = 3, the last row
        p.clamp_scroll(2); // a 2-row viewport
        assert_eq!(p.scroll, 2, "scrolled down to keep row 3 visible");
        p.select_up(3); // selected = 0
        p.clamp_scroll(2);
        assert_eq!(p.scroll, 0, "scrolled back up to keep row 0 visible");
    }
}
