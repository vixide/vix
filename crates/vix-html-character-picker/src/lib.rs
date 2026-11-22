//! The HTML named-character table and the picker's row-selection + scroll state.
//!
//! Vix's Tools menu offers an *HTML Characters* panel: a scrollable table of the
//! HTML named character references, each shown as its rendered glyph, its entity
//! name, and its Unicode code point. The user browses with the arrow keys (or the
//! mouse) and inserts the highlighted entity reference (`&name;`) into the active
//! editor. This crate is pure data — the table is bundled as a TSV and parsed
//! once on first use, and a [`Panel`] tracks the highlighted row and scroll
//! offset. The host renders the rows, maps clicks to rows, and inserts the chosen
//! reference.
//!
//! The TSV (`html-character-list.tsv`) is `name<TAB>U+code<TAB>glyph`, one
//! entity per line; it is embedded with `include_str!` so the `&'static str`
//! fields borrow straight from the binary with no per-row allocation. Spec:
//! `crates/vix-html-character-picker/spec/index.md`.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use std::sync::OnceLock;

/// One HTML named character reference.
pub struct Entity {
    /// Entity name as it appears between `&` and the optional `;`, e.g.
    /// `"Aacute;"` (modern) or `"Aacute"` (legacy, no semicolon).
    pub name: &'static str,
    /// Unicode code point label, e.g. `"U+000C1"`.
    pub code: &'static str,
    /// The rendered character(s) the entity expands to.
    pub glyph: &'static str,
}

impl Entity {
    /// The full reference to insert, e.g. `&Aacute;` — an `&` followed by
    /// [`name`](Self::name) (which already carries the trailing `;` when present).
    #[must_use]
    pub fn reference(&self) -> String {
        format!("&{}", self.name)
    }
}

/// The HTML named-character table, parsed once from the bundled TSV. Never empty
/// in practice.
#[must_use]
pub fn entities() -> &'static [Entity] {
    // `OnceLock`: parse on first use only, then share one immutable slice
    // across all panels and threads.
    /// The lazily-parsed table.
    static ENTITIES: OnceLock<Vec<Entity>> = OnceLock::new();
    ENTITIES.get_or_init(|| {
        include_str!("html-character-list.tsv")
            .lines()
            .filter_map(parse_line)
            .collect()
    })
}

/// Parse one `name<TAB>U+code<TAB>glyph` row; `None` for malformed lines.
fn parse_line(line: &'static str) -> Option<Entity> {
    let mut f = line.split('\t');
    let name = f.next()?;
    let code = f.next()?;
    let glyph = f.next()?;
    // A blank name means a stray/blank line; the glyph column may be anything
    // (even whitespace), so only the name is validated.
    if name.is_empty() {
        return None;
    }
    Some(Entity { name, code, glyph })
}

/// Selection + scroll state for the HTML character palette overlay: a row index
/// into [`entities`] plus the first visible row, so the host can render a
/// scrolling window.
pub struct Panel {
    /// Index of the highlighted row.
    pub selected: usize,
    /// First visible row, kept in sync with `selected` by [`Panel::ensure_visible`].
    pub scroll: usize,
}

/// Same as [`Panel::open`].
impl Default for Panel {
    fn default() -> Self {
        Panel::open()
    }
}

impl Panel {
    /// Open the panel with the first entity highlighted.
    #[must_use]
    pub fn open() -> Self {
        Panel {
            selected: 0,
            scroll: 0,
        }
    }

    /// Total number of entities in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        entities().len()
    }

    /// Whether the table is empty (it never is; clippy asks for this).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        entities().is_empty()
    }

    /// Move the highlight up one row, stopping at the top.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Move the highlight down one row, stopping at the bottom.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.len());
    }

    /// Move the highlight up one page (`page` rows), stopping at the top.
    pub fn page_up(&mut self, page: usize) {
        self.selected = vix_list_state::page_up(self.selected, page);
    }

    /// Move the highlight down one page (`page` rows), stopping at the bottom.
    pub fn page_down(&mut self, page: usize) {
        self.selected = vix_list_state::page_down(self.selected, page, self.len());
    }

    /// Select a row directly (e.g. from a mouse click); returns whether `idx`
    /// landed on a real row.
    pub fn select_index(&mut self, idx: usize) -> bool {
        match vix_list_state::select_index(idx, self.len()) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    /// Adjust [`scroll`](Self::scroll) so the highlighted row stays within a
    /// window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.len());
    }

    /// The highlighted entity, or `None` when the public [`selected`](Self::selected)
    /// field was set past the last entity (the navigation methods never do): an
    /// out-of-range selection is not moved to the nearest entity.
    #[must_use]
    pub fn selected_entity(&self) -> Option<&'static Entity> {
        entities().get(self.selected)
    }

    /// The highlighted entity as an HTML reference, e.g. `&Aacute;`, or `None`
    /// when nothing valid is highlighted.
    #[must_use]
    pub fn selected_reference(&self) -> Option<String> {
        self.selected_entity().map(Entity::reference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_parses_and_starts_with_aacute() {
        let e = entities();
        assert!(e.len() > 1000, "the HTML table has well over 1000 entries");
        assert_eq!(e[0].name, "Aacute;");
        assert_eq!(e[0].code, "U+000C1");
        assert_eq!(e[0].glyph, "Á");
    }

    #[test]
    fn reference_prepends_ampersand() {
        let p = Panel::open();
        assert_eq!(p.selected_reference().as_deref(), Some("&Aacute;"));
    }

    #[test]
    fn navigation_moves_and_clamps() {
        let mut p = Panel::open();
        let last = p.len() - 1;
        p.up();
        assert_eq!(p.selected, 0, "up at the top stays put");
        p.down();
        assert_eq!(p.selected, 1);
        p.page_down(10_000_000);
        assert_eq!(p.selected, last, "page down clamps to the bottom");
        p.page_up(10_000_000);
        assert_eq!(p.selected, 0, "page up clamps to the top");
    }

    #[test]
    fn select_index_hits_real_rows_only() {
        let mut p = Panel::open();
        assert!(p.select_index(9));
        assert_eq!(p.selected, 9);
        assert!(!p.select_index(p.len()));
        assert_eq!(p.selected, 9);
    }

    #[test]
    fn table_is_not_empty() {
        let p = Panel::open();
        assert!(!p.is_empty());
    }

    #[test]
    fn up_moves_back_one_row() {
        let mut p = Panel::open();
        p.select_index(5);
        p.up();
        assert_eq!(p.selected, 4);
    }

    #[test]
    fn ensure_visible_scrolls_the_selection_into_the_window() {
        let mut p = Panel::open();
        p.selected = 50;
        p.ensure_visible(10);
        assert_eq!(p.scroll, 41, "selection sits on the last visible row");
        p.selected = 3;
        p.ensure_visible(10);
        assert_eq!(p.scroll, 3);
    }

    #[test]
    fn a_selection_past_the_end_is_none_not_the_last_entity() {
        let mut p = Panel::open();
        p.selected = entities().len();
        assert!(p.selected_entity().is_none());
        assert_eq!(p.selected_reference(), None);
        p.selected = entities().len() - 1;
        assert!(p.selected_entity().is_some(), "the last entity is valid");
    }
}
