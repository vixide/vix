//! Display one parsed vCard as a table of labelled fields, plus the panel's
//! row-selection + scroll state.
//!
//! [`Panel::open`] takes a parsed [`Vcard`] and turns
//! its properties into friendly `(label, value)` [`Row`]s — mapping names to
//! readable labels, appending `TYPE` parameters (e.g. `Phone (work)`), and
//! flattening structured `N`/`ADR` values. Pure data; the host renders the rows
//! and inserts the selected value into the editor.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use vix_vcard_parser::Vcard;

/// One display row: a label and a value.
#[derive(Clone, Debug)]
pub struct Row {
    /// Friendly label (e.g. `Email (work)`).
    pub label: String,
    /// The field's value.
    pub value: String,
}

/// A readable label for a vCard property name, or empty to use the raw name.
fn label_for(name: &str) -> &'static str {
    match name {
        "FN" => "Name",
        "N" => "Name (full)",
        "NICKNAME" => "Nickname",
        "ORG" => "Organization",
        "TITLE" => "Title",
        "ROLE" => "Role",
        "EMAIL" => "Email",
        "TEL" => "Phone",
        "ADR" => "Address",
        "URL" => "URL",
        "IMPP" => "IM",
        "BDAY" => "Birthday",
        "ANNIVERSARY" => "Anniversary",
        "NOTE" => "Note",
        "CATEGORIES" => "Categories",
        "GEO" => "Location",
        "TZ" => "Time zone",
        "LANG" => "Language",
        _ => "",
    }
}

/// Properties not worth showing as a row (binary blobs and bookkeeping).
fn is_hidden(name: &str) -> bool {
    matches!(
        name,
        "PHOTO" | "LOGO" | "SOUND" | "KEY" | "PRODID" | "REV" | "UID"
    )
}

/// Build display rows for `vcard`.
#[must_use]
pub fn rows(vcard: &Vcard) -> Vec<Row> {
    let mut rows = Vec::new();
    for p in &vcard.properties {
        if is_hidden(&p.name) {
            continue;
        }
        let base = label_for(&p.name);
        let mut label = if base.is_empty() {
            p.name.clone()
        } else {
            base.to_string()
        };
        if let Some(types) = p.types() {
            label = format!("{label} ({types})");
        }
        // Structured values (N, ADR) are `;`-separated components.
        let value = if matches!(p.name.as_str(), "N" | "ADR") {
            p.value
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            p.value.replace('\n', " ")
        };
        rows.push(Row { label, value });
    }
    rows
}

/// Selection + scroll state for a single-vCard view.
pub struct Panel {
    /// The parsed vCard.
    pub vcard: Vcard,
    /// The display rows.
    pub rows: Vec<Row>,
    /// Index of the highlighted row.
    pub selected: usize,
    /// First visible row, kept in sync by [`Panel::ensure_visible`].
    pub scroll: usize,
}

impl Panel {
    /// Open the panel over `vcard`'s display rows.
    #[must_use]
    pub fn open(vcard: Vcard) -> Self {
        let rows = rows(&vcard);
        Panel {
            vcard,
            rows,
            selected: 0,
            scroll: 0,
        }
    }

    /// The contact's display name (panel title).
    #[must_use]
    pub fn title(&self) -> String {
        self.vcard.display_name()
    }

    /// Number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether there are no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Move the highlight up one row.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Move the highlight down one row.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.rows.len());
    }

    /// Move up one page.
    pub fn page_up(&mut self, page: usize) {
        self.selected = vix_list_state::page_up(self.selected, page);
    }

    /// Move down one page.
    pub fn page_down(&mut self, page: usize) {
        self.selected = vix_list_state::page_down(self.selected, page, self.rows.len());
    }

    /// Select a row directly (e.g. a click); returns whether `idx` was real.
    pub fn select_index(&mut self, idx: usize) -> bool {
        match vix_list_state::select_index(idx, self.rows.len()) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    /// Keep the highlighted row within a window of `height` rows.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.rows.len());
    }

    /// The highlighted row's value (what insertion uses).
    #[must_use]
    pub fn selected_value(&self) -> String {
        self.rows
            .get(self.selected)
            .map(|r| r.value.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_labelled_rows() {
        let v = vix_vcard_parser::parse(
            "FN:Ada Lovelace\nEMAIL;TYPE=work:ada@x.org\nADR:;;1 Main St;Town;;12345;UK\n",
        );
        let p = Panel::open(v);
        assert_eq!(p.title(), "Ada Lovelace");
        assert!(
            p.rows
                .iter()
                .any(|r| r.label == "Name" && r.value == "Ada Lovelace")
        );
        assert!(p.rows.iter().any(|r| r.label == "Email (work)"));
        let adr = p.rows.iter().find(|r| r.label == "Address").unwrap();
        assert_eq!(adr.value, "1 Main St, Town, 12345, UK");
    }

    #[test]
    fn navigation_clamps() {
        let v = vix_vcard_parser::parse("FN:A\nEMAIL:a@b.c\nTEL:123\n");
        let mut p = Panel::open(v);
        let last = p.len() - 1;
        p.up();
        assert_eq!(p.selected, 0);
        p.page_down(100);
        assert_eq!(p.selected, last);
        assert!(!p.selected_value().is_empty());
    }
}
