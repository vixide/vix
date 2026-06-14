//! Selection state for the Time Zone chooser overlay (Tools → Time Zone…).
//!
//! There are hundreds of IANA zones, so the chooser is a **filterable** list: the
//! user types to narrow `ZONES` by a case-insensitive substring of the name (or
//! abbreviation), arrows move the highlight, and accepting sets the active zone in
//! [`vix_time_zone_model`]. This crate is pure data — it owns the query, the
//! filtered match indices, the highlight, and the scroll offset; the host renders
//! it and applies the result.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use vix_time_zone_model::{Zone, ZONES};

/// Filterable selection state for the time-zone chooser.
pub struct Chooser {
    /// The current search query (lowercased matching is applied to a copy).
    pub query: String,
    /// Index into [`Self::matches`] of the highlighted row.
    pub selected: usize,
    /// First visible match index (viewport top), maintained by the host via
    /// [`Self::ensure_visible`].
    pub scroll: usize,
    /// Indices into [`ZONES`] that currently match the query, in table order.
    matches: Vec<usize>,
}

impl Chooser {
    /// Open the chooser with an empty query (all zones, ordered by UTC offset),
    /// highlighting `active_name` if present (else the first row).
    #[must_use]
    pub fn open(active_name: &str) -> Self {
        let matches = sorted_matches("");
        let selected = matches.iter().position(|&i| ZONES[i].name == active_name).unwrap_or(0);
        Chooser { query: String::new(), selected, scroll: 0, matches }
    }

    /// The matching `ZONES` indices, in order.
    #[must_use]
    pub fn matches(&self) -> &[usize] {
        &self.matches
    }

    /// Number of matching zones.
    #[must_use]
    pub fn len(&self) -> usize {
        self.matches.len()
    }

    /// Whether no zone matches the current query.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    /// The highlighted [`Zone`], or `None` when nothing matches.
    #[must_use]
    pub fn selected_zone(&self) -> Option<&'static Zone> {
        self.matches.get(self.selected).map(|&i| &ZONES[i])
    }

    /// Highlight the previous match (clamped, not wrapping).
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Highlight the next match (clamped, not wrapping).
    pub fn down(&mut self) {
        if !self.matches.is_empty() {
            self.selected = (self.selected + 1).min(self.matches.len() - 1);
        }
    }

    /// Jump up by `n` rows (clamped).
    pub fn page_up(&mut self, n: usize) {
        self.selected = self.selected.saturating_sub(n);
    }

    /// Jump down by `n` rows (clamped).
    pub fn page_down(&mut self, n: usize) {
        if !self.matches.is_empty() {
            self.selected = (self.selected + n).min(self.matches.len() - 1);
        }
    }

    /// Append a character to the query and re-filter, keeping the highlighted
    /// zone if it still matches (else snapping to the first match).
    pub fn push(&mut self, c: char) {
        self.query.push(c);
        self.recompute();
    }

    /// Delete the last query character and re-filter.
    pub fn backspace(&mut self) {
        self.query.pop();
        self.recompute();
    }

    /// Adjust [`Self::scroll`] so the highlighted row is visible in a `viewport`
    /// of the given height (rows).
    pub fn ensure_visible(&mut self, viewport: usize) {
        if viewport == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + viewport {
            self.scroll = self.selected + 1 - viewport;
        }
    }

    /// Set the highlight to an absolute match row (e.g. from a mouse click).
    pub fn select(&mut self, row: usize) {
        if row < self.matches.len() {
            self.selected = row;
        }
    }

    // Re-filter `matches` from `query`, preserving the highlighted zone where
    // possible and clamping the selection/scroll.
    fn recompute(&mut self) {
        let prev = self.selected_zone().map(|z| z.name);
        self.matches = sorted_matches(&self.query);
        self.selected = prev
            .and_then(|name| self.matches.iter().position(|&i| ZONES[i].name == name))
            .unwrap_or(0);
        self.scroll = 0;
    }
}

/// Indices into [`ZONES`] matching `query` (case-insensitive substring of name
/// or abbreviation), ordered by UTC offset and then by name.
fn sorted_matches(query: &str) -> Vec<usize> {
    let q = query.to_ascii_lowercase();
    let mut idx: Vec<usize> = (0..ZONES.len())
        .filter(|&i| {
            let z = &ZONES[i];
            q.is_empty()
                || z.name.to_ascii_lowercase().contains(&q)
                || z.abbrev.to_ascii_lowercase().contains(&q)
        })
        .collect();
    idx.sort_by(|&a, &b| {
        ZONES[a]
            .std_offset_minutes
            .cmp(&ZONES[b].std_offset_minutes)
            .then_with(|| ZONES[a].name.cmp(ZONES[b].name))
    });
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_on_active_and_lists_all() {
        let c = Chooser::open("America/New_York");
        assert_eq!(c.len(), ZONES.len());
        assert_eq!(c.selected_zone().unwrap().name, "America/New_York");
    }

    #[test]
    fn filters_by_substring_case_insensitive() {
        let mut c = Chooser::open("UTC");
        for ch in "new_y".chars() {
            c.push(ch);
        }
        assert!(!c.is_empty());
        assert!(c.matches().iter().all(|&i| ZONES[i].name.to_lowercase().contains("new_y")));
        assert_eq!(c.selected_zone().unwrap().name, "America/New_York");
    }

    #[test]
    fn backspace_widens_results() {
        let mut c = Chooser::open("UTC");
        for ch in "zzz".chars() {
            c.push(ch);
        }
        assert!(c.is_empty());
        c.backspace();
        c.backspace();
        c.backspace();
        assert_eq!(c.len(), ZONES.len());
    }

    #[test]
    fn navigation_is_clamped() {
        // Open on an unknown name so the highlight starts at row 0.
        let mut c = Chooser::open("");
        assert_eq!(c.selected, 0);
        c.up();
        assert_eq!(c.selected, 0);
        c.page_down(10_000);
        assert_eq!(c.selected, c.len() - 1);
        c.down();
        assert_eq!(c.selected, c.len() - 1);
    }
}
