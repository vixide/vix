//! A small library of reusable text snippets, plus the picker's selection state.
//!
//! Vix's Tools → Snippets… picker lists each snippet by name; choosing one
//! inserts its body at the cursor. The set is curated and language-agnostic; the
//! host owns insertion.

/// One named, insertable snippet.
pub struct Snippet {
    /// Display name shown in the picker.
    pub name: &'static str,
    /// Text inserted at the cursor when chosen.
    pub body: &'static str,
}

/// The bundled snippets, in display order.
pub static SNIPPETS: &[Snippet] = &[
    Snippet { name: "Bash shebang", body: "#!/usr/bin/env bash\nset -euo pipefail\n" },
    Snippet {
        name: "HTML5 boilerplate",
        body: "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <title>Title</title>\n</head>\n<body>\n</body>\n</html>\n",
    },
    Snippet {
        name: "MIT license header",
        body: "SPDX-License-Identifier: MIT\nCopyright (c) \n",
    },
    Snippet { name: "TODO comment", body: "TODO: " },
    Snippet { name: "FIXME comment", body: "FIXME: " },
    Snippet {
        name: "Markdown table",
        body: "| Column A | Column B |\n| -------- | -------- |\n|          |          |\n",
    },
    Snippet {
        name: "Lorem ipsum",
        body: "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.\n",
    },
];

/// Selection state for the Snippets picker.
#[derive(Default)]
pub struct Picker {
    /// Highlighted row.
    pub selected: usize,
}

impl Picker {
    /// A picker with the first snippet highlighted.
    #[must_use]
    pub fn new() -> Self {
        Picker { selected: 0 }
    }

    /// Number of snippets.
    #[must_use]
    pub fn len(&self) -> usize {
        SNIPPETS.len()
    }

    /// Whether there are no snippets (there always are, but clippy asks).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        SNIPPETS.is_empty()
    }

    /// Move the highlight up one row.
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move the highlight down one row.
    pub fn down(&mut self) {
        if self.selected + 1 < SNIPPETS.len() {
            self.selected += 1;
        }
    }

    /// Select a row directly (e.g. from a click); returns whether it was real.
    pub fn select_index(&mut self, idx: usize) -> bool {
        if idx < SNIPPETS.len() {
            self.selected = idx;
            true
        } else {
            false
        }
    }

    /// The highlighted snippet's body, ready to insert.
    #[must_use]
    pub fn selected_body(&self) -> &'static str {
        SNIPPETS.get(self.selected).map_or("", |s| s.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_navigates_and_yields_body() {
        let mut p = Picker::new();
        assert_eq!(p.selected, 0);
        assert_eq!(p.selected_body(), SNIPPETS[0].body);
        p.down();
        assert_eq!(p.selected, 1);
        p.up();
        p.up();
        assert_eq!(p.selected, 0, "saturates at the top");
    }

    #[test]
    fn down_saturates_at_the_end() {
        let mut p = Picker::new();
        for _ in 0..100 {
            p.down();
        }
        assert_eq!(p.selected, SNIPPETS.len() - 1);
        assert_eq!(p.selected_body(), SNIPPETS[SNIPPETS.len() - 1].body);
    }

    #[test]
    fn select_index_bounds() {
        let mut p = Picker::new();
        assert!(p.select_index(2));
        assert_eq!(p.selected, 2);
        assert!(!p.select_index(9999));
    }
}
