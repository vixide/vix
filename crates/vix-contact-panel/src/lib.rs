//! Browse a directory of vCard files as a table of contacts.
//!
//! The host scans a directory for `.vcf` files, parses each one's display name
//! (with `vix-vcard-parser`), and builds a list of [`Contact`]s. This crate holds
//! that list and tracks the highlighted row + scroll offset. Choosing a row opens
//! that contact's vCard (the host then displays it with `vix-vcard-panel`). Pure
//! data — no filesystem IO here.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::path::PathBuf;

/// One contact entry: its display name and the file it came from.
#[derive(Clone, Debug)]
pub struct Contact {
    /// Display name (e.g. the vCard `FN`).
    pub name: String,
    /// Path to the `.vcf` file.
    pub path: PathBuf,
}

/// Selection + scroll state for the contact browser.
pub struct Panel {
    /// Contacts, in display order (the host sorts before passing them in).
    pub contacts: Vec<Contact>,
    /// Index of the highlighted contact.
    pub selected: usize,
    /// First visible row, kept in sync by [`Panel::ensure_visible`].
    pub scroll: usize,
}

impl Panel {
    /// Open the browser over `contacts`.
    #[must_use]
    pub fn open(contacts: Vec<Contact>) -> Self {
        Panel {
            contacts,
            selected: 0,
            scroll: 0,
        }
    }

    /// Number of contacts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.contacts.len()
    }

    /// Whether the directory had no vCards.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.contacts.is_empty()
    }

    /// Move the highlight up one row.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Move the highlight down one row.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.contacts.len());
    }

    /// Move up one page.
    pub fn page_up(&mut self, page: usize) {
        self.selected = vix_list_state::page_up(self.selected, page);
    }

    /// Move down one page.
    pub fn page_down(&mut self, page: usize) {
        self.selected = vix_list_state::page_down(self.selected, page, self.contacts.len());
    }

    /// Select a row directly (e.g. a click); returns whether `idx` was real.
    pub fn select_index(&mut self, idx: usize) -> bool {
        match vix_list_state::select_index(idx, self.contacts.len()) {
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
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.contacts.len());
    }

    /// The highlighted contact, if any.
    #[must_use]
    pub fn selected_contact(&self) -> Option<&Contact> {
        self.contacts.get(self.selected)
    }

    /// The highlighted contact's file path, if any.
    #[must_use]
    pub fn selected_path(&self) -> Option<PathBuf> {
        self.selected_contact().map(|c| c.path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Contact> {
        vec![
            Contact {
                name: "Ada".into(),
                path: "/c/ada.vcf".into(),
            },
            Contact {
                name: "Grace".into(),
                path: "/c/grace.vcf".into(),
            },
        ]
    }

    #[test]
    fn navigation_and_selection() {
        let mut p = Panel::open(sample());
        assert_eq!(p.len(), 2);
        assert_eq!(p.selected_contact().unwrap().name, "Ada");
        p.down();
        assert_eq!(p.selected_path().unwrap(), PathBuf::from("/c/grace.vcf"));
        p.down();
        assert_eq!(p.selected, 1, "down at the bottom stays put");
        assert!(p.select_index(0));
        assert!(!p.select_index(2));
    }

    #[test]
    fn empty_directory() {
        let p = Panel::open(vec![]);
        assert!(p.is_empty());
        assert!(p.selected_path().is_none());
    }
}
