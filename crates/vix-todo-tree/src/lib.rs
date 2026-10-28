//! The Todo Tree: comment tags found in the project, grouped under a header per
//! tag (or per file) that can be folded, with a selection a panel can walk and
//! jump from.
//!
//! [`scan`] finds the tags in one file's text; [`Panel`] holds every [`Item`]
//! found, the grouping, which groups are folded, and the selection. Everything
//! is pure: the app reads the files and draws the rows.

use std::collections::BTreeSet;

/// The tags looked for by default, in the order their groups appear.
pub const DEFAULT_TAGS: &[&str] = &["TODO", "FIXME", "HACK", "XXX", "BUG", "NOTE"];

/// One tag found in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The tag, e.g. `TODO`.
    pub tag: String,
    /// The file, relative to the project root.
    pub file: String,
    /// 1-based line number.
    pub line: usize,
    /// 1-based column of the tag.
    pub col: usize,
    /// The trimmed line, cut to 120 characters.
    pub text: String,
}

/// Every line of `text` that carries one of `tags` as a whole word. A line
/// reports its first matching tag (in `tags` order) only.
#[must_use]
pub fn scan(file: &str, text: &str, tags: &[&str]) -> Vec<Item> {
    text.lines()
        .enumerate()
        .filter_map(|(i, raw)| {
            let (tag, col) = tags
                .iter()
                .find_map(|tag| vix_textops::tag_column(raw, tag).map(|c| (*tag, c)))?;
            Some(Item {
                tag: tag.to_string(),
                file: file.to_string(),
                line: i + 1,
                col: col + 1,
                text: raw.trim().chars().take(120).collect(),
            })
        })
        .collect()
}

/// What the groups are keyed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    /// One group per tag (`TODO`, `FIXME`, ...).
    Tag,
    /// One group per file.
    File,
}

/// A visible row of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A group header.
    Group {
        /// The tag or file path.
        label: String,
        /// How many items the group holds.
        count: usize,
        /// Whether the group is folded.
        collapsed: bool,
    },
    /// An item; the index is into [`Panel::items`].
    Item(usize),
}

/// The tree's state: items, grouping, folds and selection.
pub struct Panel {
    /// Every item found, sorted by file then line.
    pub items: Vec<Item>,
    /// What the groups are keyed on.
    pub group_by: GroupBy,
    /// Index of the highlighted row (see [`Panel::rows`]).
    pub selected: usize,
    /// First visible row, kept in sync by [`Panel::ensure_visible`].
    pub scroll: usize,
    /// Labels of folded groups. Kept per grouping, since a tag and a file
    /// could share a label.
    collapsed: BTreeSet<(bool, String)>,
}

impl Panel {
    /// A tree over `items`, grouped by tag, nothing folded.
    #[must_use]
    pub fn new(mut items: Vec<Item>) -> Self {
        items.sort_by(|a, b| (&a.file, a.line, a.col).cmp(&(&b.file, b.line, b.col)));
        Panel {
            items,
            group_by: GroupBy::Tag,
            selected: 0,
            scroll: 0,
            collapsed: BTreeSet::new(),
        }
    }

    fn key(&self, label: &str) -> (bool, String) {
        (self.group_by == GroupBy::File, label.to_string())
    }

    fn group_label<'a>(&self, item: &'a Item) -> &'a str {
        match self.group_by {
            GroupBy::Tag => &item.tag,
            GroupBy::File => &item.file,
        }
    }

    /// The group labels in display order: known tags first, in
    /// [`DEFAULT_TAGS`] order, then everything else alphabetically.
    fn group_order(&self) -> Vec<String> {
        let mut labels: Vec<String> = self
            .items
            .iter()
            .map(|i| self.group_label(i).to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if self.group_by == GroupBy::Tag {
            let rank = |l: &str| {
                DEFAULT_TAGS
                    .iter()
                    .position(|t| *t == l)
                    .unwrap_or(DEFAULT_TAGS.len())
            };
            labels.sort_by_key(|l| rank(l));
        }
        labels
    }

    /// The visible rows: each group header, then its items unless folded.
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for label in self.group_order() {
            let members: Vec<usize> = (0..self.items.len())
                .filter(|&i| self.group_label(&self.items[i]) == label)
                .collect();
            let collapsed = self.collapsed.contains(&self.key(&label));
            rows.push(Row::Group {
                label,
                count: members.len(),
                collapsed,
            });
            if !collapsed {
                rows.extend(members.into_iter().map(Row::Item));
            }
        }
        rows
    }

    /// Number of visible rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows().len()
    }

    /// Whether there is nothing to show.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
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

    /// Select a row directly (e.g. from a click); returns whether `idx` was real.
    pub fn select_index(&mut self, idx: usize) -> bool {
        match vix_list_state::select_index(idx, self.len()) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    /// Keep the highlighted row within a window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.len());
    }

    /// Switch between grouping by tag and by file; the selection returns to
    /// the top.
    pub fn toggle_group_by(&mut self) {
        self.group_by = match self.group_by {
            GroupBy::Tag => GroupBy::File,
            GroupBy::File => GroupBy::Tag,
        };
        self.selected = 0;
        self.scroll = 0;
    }

    /// The group header at or above the highlighted row.
    fn current_group(&self) -> Option<(usize, String)> {
        let rows = self.rows();
        let upto = self.selected.min(rows.len().checked_sub(1)?);
        rows[..=upto]
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, r)| match r {
                Row::Group { label, .. } => Some((i, label.clone())),
                Row::Item(_) => None,
            })
    }

    /// Fold the highlighted row's group (or unfold it if folded), leaving the
    /// highlight on the group's header.
    pub fn toggle_fold(&mut self) {
        let Some((header, label)) = self.current_group() else {
            return;
        };
        let key = self.key(&label);
        if !self.collapsed.remove(&key) {
            self.collapsed.insert(key);
        }
        self.selected = header;
    }

    /// Fold every group.
    pub fn collapse_all(&mut self) {
        for label in self.group_order() {
            let key = self.key(&label);
            self.collapsed.insert(key);
        }
        self.selected = 0;
        self.scroll = 0;
    }

    /// Unfold every group.
    pub fn expand_all(&mut self) {
        self.collapsed.clear();
    }

    /// The highlighted row's item, if it is an item row.
    #[must_use]
    pub fn selected_item(&self) -> Option<&Item> {
        match self.rows().get(self.selected)? {
            Row::Item(i) => self.items.get(*i),
            Row::Group { .. } => None,
        }
    }

    /// Enter: an item row yields the item to jump to; a group header folds or
    /// unfolds and yields `None`.
    pub fn activate(&mut self) -> Option<Item> {
        if let Some(item) = self.selected_item() {
            return Some(item.clone());
        }
        self.toggle_fold();
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(tag: &str, file: &str, line: usize) -> Item {
        Item {
            tag: tag.into(),
            file: file.into(),
            line,
            col: 1,
            text: format!("// {tag} {line}"),
        }
    }

    fn panel() -> Panel {
        Panel::new(vec![
            item("FIXME", "b.rs", 3),
            item("TODO", "b.rs", 9),
            item("TODO", "a.rs", 20),
            item("TODO", "a.rs", 2),
            item("ZZZ", "a.rs", 30),
        ])
    }

    fn labels(p: &Panel) -> Vec<String> {
        p.rows()
            .iter()
            .map(|r| match r {
                Row::Group { label, count, .. } => format!("[{label} {count}]"),
                Row::Item(i) => format!("{}:{}", p.items[*i].file, p.items[*i].line),
            })
            .collect()
    }

    #[test]
    fn scan_finds_whole_word_tags_once_per_line() {
        let text = "fn f() {}\n// TODO: wire it\nlet todos = 1;\n// FIXME and TODO\n";
        let found = scan("x.rs", text, DEFAULT_TAGS);
        assert_eq!(found.len(), 2);
        assert_eq!(
            (found[0].tag.as_str(), found[0].line, found[0].col),
            ("TODO", 2, 4)
        );
        // Both tags on one line: the earlier tag in the list wins.
        assert_eq!(found[1].tag, "TODO");
        assert_eq!(found[1].file, "x.rs");
        assert_eq!(scan("x.rs", "// TODO", &["FIXME"]), vec![]);
    }

    #[test]
    fn long_lines_are_cut() {
        let text = format!("// TODO {}", "x".repeat(300));
        assert_eq!(scan("a", &text, DEFAULT_TAGS)[0].text.chars().count(), 120);
    }

    #[test]
    fn groups_by_tag_in_known_order_then_alphabetical() {
        assert_eq!(
            labels(&panel()),
            [
                "[TODO 3]",
                "a.rs:2",
                "a.rs:20",
                "b.rs:9",
                "[FIXME 1]",
                "b.rs:3",
                "[ZZZ 1]",
                "a.rs:30"
            ]
        );
    }

    #[test]
    fn groups_by_file_alphabetically() {
        let mut p = panel();
        p.selected = 3;
        p.toggle_group_by();
        assert_eq!(p.selected, 0);
        assert_eq!(
            labels(&p),
            [
                "[a.rs 3]", "a.rs:2", "a.rs:20", "a.rs:30", "[b.rs 2]", "b.rs:3", "b.rs:9"
            ]
        );
    }

    #[test]
    fn folding_hides_items_and_lands_on_the_header() {
        let mut p = panel();
        p.selected = 2; // an item inside TODO
        p.toggle_fold();
        assert_eq!(p.selected, 0);
        assert_eq!(
            labels(&p),
            ["[TODO 3]", "[FIXME 1]", "b.rs:3", "[ZZZ 1]", "a.rs:30"]
        );
        p.toggle_fold();
        assert_eq!(p.len(), 8);
        p.collapse_all();
        assert_eq!(p.len(), 3);
        p.expand_all();
        assert_eq!(p.len(), 8);
    }

    #[test]
    fn folds_are_kept_per_grouping() {
        let mut p = Panel::new(vec![item("TODO", "TODO", 1)]);
        p.toggle_fold(); // folds the tag group "TODO"
        p.toggle_group_by(); // the file group "TODO" is a different group
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn activate_jumps_on_items_and_folds_on_headers() {
        let mut p = panel();
        assert!(p.activate().is_none(), "header folds");
        assert_eq!(p.len(), 5);
        p.selected = 1; // FIXME header now
        p.expand_all();
        p.selected = 1;
        let it = p.activate().unwrap();
        assert_eq!((it.file.as_str(), it.line), ("a.rs", 2));
    }

    #[test]
    fn navigation_clamps_and_selects() {
        let mut p = panel();
        p.up();
        assert_eq!(p.selected, 0);
        for _ in 0..50 {
            p.down();
        }
        assert_eq!(p.selected, p.len() - 1);
        p.page_up(3);
        assert_eq!(p.selected, p.len() - 4);
        assert!(p.select_index(1));
        assert!(!p.select_index(99));
        p.ensure_visible(2);
        assert_eq!(p.scroll, 0);
        assert!(Panel::new(vec![]).is_empty());
        assert!(Panel::new(vec![]).activate().is_none());
    }
}
