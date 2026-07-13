//! The IANA-style media-type (MIME) table and the picker's filter + selection
//! state.
//!
//! Vix's Tools menu offers a *Media Types* panel: a searchable list of common
//! media types, each shown with its description and file extension(s). The user
//! types to filter (by media type, description, or extension), browses with the
//! arrow keys (or the mouse), and inserts the highlighted media type into the
//! active editor. This module is pure data — the table is bundled as a TSV and
//! parsed once on first use — plus a [`Panel`] holding the query, highlighted
//! row, and scroll offset over the *filtered* rows.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::sync::OnceLock;

/// One media type: its name, a human description, the associated file
/// extension(s) (a comma-separated list such as `.yaml, .yml`), and whether its
/// content is fundamentally `text` or `binary`.
pub struct MediaType {
    /// The media type, e.g. `"image/png"`.
    pub media_type: &'static str,
    /// Human description, e.g. `"Portable Network Graphics (PNG)"`.
    pub description: &'static str,
    /// Associated extension(s), e.g. `".png"` or `".yaml, .yml"`.
    pub extension: &'static str,
    /// Base content kind: `"text"` or `"binary"`.
    pub base: &'static str,
}

impl MediaType {
    /// Whether the content is text (as opposed to binary).
    #[must_use]
    pub fn is_text(&self) -> bool {
        self.base.eq_ignore_ascii_case("text")
    }
}

/// The media-type table, parsed once from the bundled TSV (the header row is
/// skipped). Never empty in practice.
#[must_use]
pub fn all() -> &'static [MediaType] {
    static TABLE: OnceLock<Vec<MediaType>> = OnceLock::new();
    TABLE.get_or_init(|| {
        // The spec TSV is the single source of truth (see this crate's spec/).
        include_str!("../spec/media-types.tsv")
            .lines()
            .skip(1)
            .filter_map(parse_line)
            .collect()
    })
}

/// Parse one `MediaType<TAB>Description<TAB>Extension<TAB>Base` row; `None` for a
/// malformed or blank line. A missing `Base` column defaults to `"binary"`.
fn parse_line(line: &'static str) -> Option<MediaType> {
    let mut f = line.split('\t');
    let media_type = f.next()?.trim();
    let description = f.next()?.trim();
    let extension = f.next()?.trim();
    let base = f.next().map_or("binary", str::trim);
    if media_type.is_empty() {
        return None;
    }
    Some(MediaType {
        media_type,
        description,
        extension,
        base,
    })
}

/// Normalize a file extension for comparison: lowercase, no leading dot.
fn norm_ext(ext: &str) -> String {
    ext.trim().trim_start_matches('.').to_ascii_lowercase()
}

/// The first media type whose extension list contains `ext` (with or without a
/// leading dot, case-insensitive). `None` if unknown.
#[must_use]
pub fn for_extension(ext: &str) -> Option<&'static MediaType> {
    let want = norm_ext(ext);
    if want.is_empty() {
        return None;
    }
    all()
        .iter()
        .find(|m| m.extension.split(',').any(|e| norm_ext(e) == want))
}

/// Whether a row matches the (already lowercased) `query` across its media type,
/// description, and extension.
fn row_matches(m: &MediaType, query: &str) -> bool {
    query.is_empty()
        || m.media_type.to_ascii_lowercase().contains(query)
        || m.description.to_ascii_lowercase().contains(query)
        || m.extension.to_ascii_lowercase().contains(query)
}

/// Query + selection + scroll state for the Media Types overlay. `selected` and
/// `scroll` index into the *filtered* row list (see [`Panel::matches`]).
pub struct Panel {
    /// The current filter text (matched case-insensitively).
    pub query: String,
    /// Index of the highlighted row within the filtered list.
    pub selected: usize,
    /// First visible filtered row, kept in sync by [`Panel::ensure_visible`].
    pub scroll: usize,
}

impl Default for Panel {
    fn default() -> Self {
        Panel::open()
    }
}

impl Panel {
    /// Open the panel with an empty filter and the first row highlighted.
    #[must_use]
    pub fn open() -> Self {
        Panel {
            query: String::new(),
            selected: 0,
            scroll: 0,
        }
    }

    /// Open the panel pre-selected to the media type for `ext`, if known.
    #[must_use]
    pub fn open_for_extension(ext: &str) -> Self {
        let mut p = Panel::open();
        if let Some(target) = for_extension(ext)
            && let Some(pos) = all().iter().position(|m| std::ptr::eq(m, target))
        {
            // The filter is empty, so filtered indices equal table indices.
            p.selected = pos;
        }
        p
    }

    /// Indices into [`all`] of the rows matching the current query.
    #[must_use]
    pub fn matches(&self) -> Vec<usize> {
        let q = self.query.to_ascii_lowercase();
        all()
            .iter()
            .enumerate()
            .filter(|(_, m)| row_matches(m, &q))
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
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move the highlight down one row, stopping at the bottom.
    pub fn down(&mut self) {
        if self.selected + 1 < self.len() {
            self.selected += 1;
        }
    }

    /// Move the highlight up one page, stopping at the top.
    pub fn page_up(&mut self, page: usize) {
        self.selected = self.selected.saturating_sub(page.max(1));
    }

    /// Move the highlight down one page, stopping at the bottom.
    pub fn page_down(&mut self, page: usize) {
        self.selected = (self.selected + page.max(1)).min(self.len().saturating_sub(1));
    }

    /// Select a filtered row directly (e.g. from a click); returns whether `idx`
    /// landed on a real row.
    pub fn select_index(&mut self, idx: usize) -> bool {
        if idx < self.len() {
            self.selected = idx;
            true
        } else {
            false
        }
    }

    /// Adjust [`scroll`](Self::scroll) so the highlight stays within `height`
    /// visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        let height = height.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
        let max_scroll = self.len().saturating_sub(height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// The highlighted media type, or `None` when the filter matches nothing.
    #[must_use]
    pub fn selected_entry(&self) -> Option<&'static MediaType> {
        let idx = *self.matches().get(self.selected)?;
        all().get(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_parses_with_common_types() {
        let t = all();
        assert!(t.len() > 100, "the media-type table has well over 100 rows");
        assert!(
            t.iter()
                .any(|m| m.media_type == "image/png" && m.extension == ".png")
        );
        assert!(t.iter().any(|m| m.media_type == "application/json"));
        // The Base column classifies content as text or binary.
        let png = t.iter().find(|m| m.media_type == "image/png").unwrap();
        assert_eq!(png.base, "binary");
        assert!(!png.is_text());
        let json = t
            .iter()
            .find(|m| m.media_type == "application/json")
            .unwrap();
        assert_eq!(json.base, "text");
        assert!(json.is_text());
    }

    #[test]
    fn looks_up_by_extension() {
        assert_eq!(for_extension("png").unwrap().media_type, "image/png");
        assert_eq!(for_extension(".PNG").unwrap().media_type, "image/png");
        // Multi-extension rows match every listed extension.
        assert_eq!(for_extension("yml").unwrap().media_type, "application/yaml");
        assert_eq!(
            for_extension(".yaml").unwrap().media_type,
            "application/yaml"
        );
        assert!(for_extension("nope-xyz").is_none());
        assert!(for_extension("").is_none());
    }

    #[test]
    fn filtering_narrows_the_rows() {
        let mut p = Panel::open();
        let total = p.len();
        p.push('s');
        p.push('v');
        p.push('g');
        assert!(p.len() < total);
        assert_eq!(p.selected_entry().unwrap().media_type, "image/svg+xml");
        p.backspace();
        p.backspace();
        p.backspace();
        assert_eq!(p.len(), total, "clearing the filter restores all rows");
    }

    #[test]
    fn open_for_extension_preselects() {
        let p = Panel::open_for_extension("svg");
        assert_eq!(p.selected_entry().unwrap().media_type, "image/svg+xml");
    }

    #[test]
    fn navigation_clamps() {
        let mut p = Panel::open();
        let last = p.len() - 1;
        p.up();
        assert_eq!(p.selected, 0);
        p.page_down(1_000_000);
        assert_eq!(p.selected, last);
        p.down();
        assert_eq!(p.selected, last);
    }

    proptest::proptest! {
        // Filtering with an arbitrary query never panics, and every returned
        // index is in range for `all()`.
        #[test]
        fn filtering_with_arbitrary_query_is_safe(q in ".*") {
            let mut p = Panel::open();
            for c in q.chars().take(100) {
                p.push(c);
            }
            let matches = p.matches();
            let n = all().len();
            for i in matches {
                proptest::prop_assert!(i < n, "index {i} out of range {n}");
            }
        }
    }
}
