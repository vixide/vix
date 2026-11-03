//! The Bundle viewer's list state.
//!
//! A thin wrapper binding a parsed [`Bundle`] to a selection and scroll offset;
//! all index arithmetic is delegated to `vix_list_state` so clamping behaves
//! identically to every other list panel. Rendering lives in the host UI, not
//! here. Spec: `crates/vix-fhir/spec/index.md`.

use crate::{Bundle, Entry};

/// A browsable Bundle: one selectable row per entry. `selected` and `scroll`
/// index the entry list, driven by `vix_list_state` like every other panel.
#[derive(Debug, Clone, PartialEq)]
pub struct BundlePanel {
    /// The Bundle being browsed.
    pub bundle: Bundle,
    /// Index of the highlighted entry.
    pub selected: usize,
    /// First visible row, kept in sync by [`BundlePanel::ensure_visible`].
    pub scroll: usize,
}

impl BundlePanel {
    /// Open a viewer on `bundle` with the first entry highlighted.
    #[must_use]
    pub fn open(bundle: Bundle) -> Self {
        Self {
            bundle,
            selected: 0,
            scroll: 0,
        }
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bundle.entries.len()
    }

    /// Whether the Bundle has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bundle.entries.is_empty()
    }

    /// Move the highlight up one row.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Move the highlight down one row.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.len());
    }

    /// Move the highlight up one page.
    pub fn page_up(&mut self, page: usize) {
        self.selected = vix_list_state::page_up(self.selected, page);
    }

    /// Move the highlight down one page.
    pub fn page_down(&mut self, page: usize) {
        self.selected = vix_list_state::page_down(self.selected, page, self.len());
    }

    /// Select a row directly (e.g. from a click); whether it was a real row.
    pub fn select_index(&mut self, idx: usize) -> bool {
        match vix_list_state::select_index(idx, self.len()) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    /// Keep the highlight inside `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.len());
    }

    /// The highlighted entry.
    #[must_use]
    pub fn selected_entry(&self) -> Option<&Entry> {
        self.bundle.entries.get(self.selected)
    }

    /// The `next` page link, if the Bundle has one.
    #[must_use]
    pub fn next_link(&self) -> Option<&str> {
        self.bundle.next.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_bundle;

    fn panel(n: usize) -> BundlePanel {
        let entries: Vec<String> = (0..n)
            .map(|i| format!(r#"{{"resource":{{"resourceType":"Patient","id":"{i}"}}}}"#))
            .collect();
        let text = format!(
            r#"{{"resourceType":"Bundle","link":[{{"relation":"next","url":"https://s/n"}}],"entry":[{}]}}"#,
            entries.join(",")
        );
        BundlePanel::open(parse_bundle(&text).unwrap())
    }

    #[test]
    fn navigation_clamps() {
        let mut p = panel(3);
        p.up();
        assert_eq!(p.selected, 0);
        p.down();
        p.down();
        p.down();
        assert_eq!(p.selected, 2);
        p.page_up(10);
        assert_eq!(p.selected, 0);
        p.page_down(10);
        assert_eq!(p.selected, 2);
        assert!(p.select_index(1));
        assert!(!p.select_index(3));
        assert_eq!(p.selected_entry().unwrap().id.as_deref(), Some("1"));
        assert_eq!(p.next_link(), Some("https://s/n"));
    }

    #[test]
    fn empty_panel_is_inert() {
        let mut p = panel(0);
        assert!(p.is_empty());
        p.down();
        p.page_down(5);
        assert_eq!(p.selected, 0);
        assert!(p.selected_entry().is_none());
        p.ensure_visible(5);
        assert_eq!(p.scroll, 0);
    }

    #[test]
    fn scroll_follows_selection() {
        let mut p = panel(20);
        p.page_down(15);
        p.ensure_visible(5);
        assert!(p.scroll > 0 && p.scroll <= p.selected);
    }
}
