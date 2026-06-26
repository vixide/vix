//! The outline editor: hierarchical hide/show and restructuring for prose text.
//!
//! Vix's Tools menu offers an *Edit Outline* command that reads the active buffer as an
//! indented outline — each line is an item whose **level** is its indentation
//! depth (tabs, or two spaces per level). It behaves like code folding, but for
//! prose, and like a file explorer's tree: collapse an item to hide its
//! descendants, navigate item to item, re-indent items to change the hierarchy,
//! and move items (with their subtrees) up and down.
//!
//! This module is self-contained and host-agnostic: it owns the items, the
//! cursor, the collapse state, and an undo/redo history, and it interprets key
//! events itself (returning an [`Outcome`] telling the host when to close or
//! save). The host ([`crate::app`]) renders the visible items, syncs the scroll
//! window, and persists saves.
//!
//! Keys (the host routes them here):
//! - **↑ / ↓** (or `k` / `j`): move to the previous / next visible item.
//! - **← / →** (or `h` / `l`): close / open (collapse / expand; `←` on a leaf
//!   jumps to the parent, `→` on an expanded item jumps to the first child).
//! - **Tab / Shift+Tab** (or **Alt+→ / Alt+←**): indent / outdent the item.
//! - **Alt+↑ / Alt+↓**: move the item (with its subtree) up / down. Terminals
//!   cannot send Tab+arrow, so these "tab-up/tab-down" moves use Alt+arrows.
//! - **Space**: toggle collapse. **Ctrl+S**: save. **u** / **Ctrl+R**:
//!   undo / redo. **Esc** / **q**: close.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What the host should do after the outline handled a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The key was handled internally; nothing further for the host to do.
    Consumed,
    /// The user asked to close the outline editor (Esc/`q`).
    Close,
    /// The user asked to save (Ctrl+S); the host should persist the outline.
    Save,
}

/// One outline item: a line of text at an indentation `level`, optionally
/// collapsed (its descendants hidden).
#[derive(Clone)]
struct Item {
    text: String,
    level: usize,
    collapsed: bool,
}

/// A point-in-time snapshot for undo/redo.
#[derive(Clone)]
struct Snapshot {
    items: Vec<Item>,
    sel: usize,
}

/// Maximum number of undo steps retained.
const HISTORY_CAP: usize = 200;

/// An outline of indented prose items with a cursor, collapse state, and history.
pub struct Tree {
    /// The items in document order.
    items: Vec<Item>,
    /// Selected item index (always a visible item).
    sel: usize,
    /// First visible row shown (an index into the visible list).
    scroll: usize,
    /// Whether indentation is a tab per level (else two spaces per level).
    tab_indent: bool,
    /// Whether there are unsaved changes.
    dirty: bool,
    /// Undo history (most recent last).
    undo: Vec<Snapshot>,
    /// Redo history (most recent last).
    redo: Vec<Snapshot>,
}

impl Tree {
    /// Parse `text` into an outline. Each line becomes an item; its level is the
    /// indentation depth (leading tabs, or leading spaces / 2). Always has at
    /// least one item.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let body = text.strip_suffix('\n').unwrap_or(text);
        let tab_indent = body.lines().any(|l| l.starts_with('\t'));
        let mut items: Vec<Item> = body
            .split('\n')
            .map(|line| {
                let (level, rest) = if tab_indent {
                    let n = line.chars().take_while(|&c| c == '\t').count();
                    (n, &line[n..])
                } else {
                    let n = line.chars().take_while(|&c| c == ' ').count();
                    (n / 2, &line[n..])
                };
                Item { text: rest.to_string(), level, collapsed: false }
            })
            .collect();
        if items.is_empty() {
            items.push(Item { text: String::new(), level: 0, collapsed: false });
        }
        Tree { items, sel: 0, scroll: 0, tab_indent, dirty: false, undo: Vec::new(), redo: Vec::new() }
    }

    /// Total number of items (including hidden ones).
    #[must_use]
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    /// The selected item index.
    #[must_use]
    pub fn sel(&self) -> usize {
        self.sel
    }

    /// The first visible row shown (index into [`Tree::visible`]).
    #[must_use]
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Whether there are unsaved edits.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The indentation level of item `i` (0 when out of range).
    #[must_use]
    pub fn level(&self, i: usize) -> usize {
        self.items.get(i).map_or(0, |it| it.level)
    }

    /// The text of item `i` (`""` when out of range).
    #[must_use]
    pub fn text(&self, i: usize) -> &str {
        self.items.get(i).map_or("", |it| it.text.as_str())
    }

    /// Whether item `i` is collapsed.
    #[must_use]
    pub fn is_collapsed(&self, i: usize) -> bool {
        self.items.get(i).is_some_and(|it| it.collapsed)
    }

    /// Whether item `i` has at least one child (a deeper following item).
    #[must_use]
    pub fn has_children(&self, i: usize) -> bool {
        match self.items.get(i) {
            Some(it) => self.items.get(i + 1).is_some_and(|n| n.level > it.level),
            None => false,
        }
    }

    /// The visible item indices in display order: items hidden under a collapsed
    /// ancestor are omitted.
    #[must_use]
    pub fn visible(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.items.len());
        let mut hide: Option<usize> = None;
        for (i, it) in self.items.iter().enumerate() {
            if let Some(l) = hide {
                if it.level > l {
                    continue;
                }
                hide = None;
            }
            out.push(i);
            if it.collapsed && self.has_children(i) {
                hide = Some(it.level);
            }
        }
        out
    }

    /// Serialize the outline back to text, regenerating indentation from each
    /// item's level (tabs or two spaces per level). Collapse state is a view
    /// concern and is not written.
    #[must_use]
    pub fn to_text(&self) -> String {
        let unit = if self.tab_indent { "\t" } else { "  " };
        let mut out = String::new();
        for it in &self.items {
            out.push_str(&unit.repeat(it.level));
            out.push_str(&it.text);
            out.push('\n');
        }
        out
    }

    /// Mark the outline as saved (called by the host after a successful write).
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Adjust the scroll so the selected item stays within a window of `height`
    /// visible rows. Called by the renderer before drawing.
    pub fn ensure_visible(&mut self, height: usize) {
        let height = height.max(1);
        let vis = self.visible();
        let pos = vis.iter().position(|&i| i == self.sel).unwrap_or(0);
        if pos < self.scroll {
            self.scroll = pos;
        } else if pos >= self.scroll + height {
            self.scroll = pos + 1 - height;
        }
        let max = vis.len().saturating_sub(height);
        self.scroll = self.scroll.min(max);
    }

    /// Interpret a key event and report what the host should do next. `page` is
    /// the number of items to move for `PageUp`/`PageDown`.
    pub fn handle_key(&mut self, key: KeyEvent, page: usize) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char('s') if ctrl => return Outcome::Save,
            KeyCode::Up if alt => self.move_item_up(),
            KeyCode::Down if alt => self.move_item_down(),
            KeyCode::Left if alt => self.outdent(),
            KeyCode::Right if alt => self.indent(),
            KeyCode::Up | KeyCode::Char('k') => self.step_sel(true, 1),
            KeyCode::Down | KeyCode::Char('j') => self.step_sel(false, 1),
            KeyCode::Left | KeyCode::Char('h') => self.close(),
            KeyCode::Right | KeyCode::Char('l') => self.open(),
            KeyCode::Tab => self.indent(),
            KeyCode::BackTab => self.outdent(),
            KeyCode::Char(' ') => self.toggle(),
            KeyCode::Char('u') => self.undo(),
            KeyCode::Char('r') if ctrl => self.redo(),
            KeyCode::Home => self.sel_edge(false),
            KeyCode::End => self.sel_edge(true),
            KeyCode::PageUp => self.step_sel(true, page.max(1)),
            KeyCode::PageDown => self.step_sel(false, page.max(1)),
            KeyCode::Esc | KeyCode::Char('q') => return Outcome::Close,
            _ => {}
        }
        Outcome::Consumed
    }

    // ----- navigation -------------------------------------------------------

    /// Move the selection `n` positions up or down within the visible list,
    /// clamped to its ends.
    fn step_sel(&mut self, up: bool, n: usize) {
        let vis = self.visible();
        if vis.is_empty() {
            return;
        }
        let pos = vis.iter().position(|&i| i == self.sel).unwrap_or(0);
        let np = if up { pos.saturating_sub(n) } else { (pos + n).min(vis.len() - 1) };
        self.sel = vis[np];
    }

    /// Move the selection to the first or last visible item.
    fn sel_edge(&mut self, last: bool) {
        let vis = self.visible();
        let target = if last { vis.last() } else { vis.first() };
        if let Some(&i) = target {
            self.sel = i;
        }
    }

    /// Close (collapse) the current item, or jump to its parent if it has no
    /// collapsible children.
    fn close(&mut self) {
        if self.has_children(self.sel) && !self.items[self.sel].collapsed {
            self.items[self.sel].collapsed = true;
        } else if let Some(p) = self.parent(self.sel) {
            self.sel = p;
        }
    }

    /// Open (expand) the current item, or jump to its first child if already
    /// expanded.
    fn open(&mut self) {
        if !self.has_children(self.sel) {
            return;
        }
        if self.items[self.sel].collapsed {
            self.items[self.sel].collapsed = false;
        } else {
            self.sel += 1;
        }
    }

    /// Toggle the current item's collapse state.
    fn toggle(&mut self) {
        if self.has_children(self.sel) {
            self.items[self.sel].collapsed = !self.items[self.sel].collapsed;
        }
    }

    // ----- structure --------------------------------------------------------

    /// Indent the item (and its subtree) one level, if it has a preceding
    /// sibling to become a child of. Expands that new parent so it stays visible.
    fn indent(&mut self) {
        let Some(p) = self.prev_sibling(self.sel) else { return };
        self.push_undo();
        let (i, j) = self.block(self.sel);
        for it in &mut self.items[i..j] {
            it.level += 1;
        }
        self.items[p].collapsed = false;
        self.dirty = true;
    }

    /// Outdent the item (and its subtree) one level, if not already at the root.
    fn outdent(&mut self) {
        if self.items[self.sel].level == 0 {
            return;
        }
        self.push_undo();
        let (i, j) = self.block(self.sel);
        for it in &mut self.items[i..j] {
            it.level -= 1;
        }
        self.dirty = true;
    }

    /// Move the current item (with its subtree) above its previous sibling.
    fn move_item_up(&mut self) {
        let (i, j) = self.block(self.sel);
        let Some(p) = self.prev_sibling(i) else { return };
        self.push_undo();
        let block: Vec<Item> = self.items.drain(i..j).collect();
        self.items.splice(p..p, block);
        self.sel = p;
        self.dirty = true;
    }

    /// Move the current item (with its subtree) below its next sibling.
    fn move_item_down(&mut self) {
        let (i, j) = self.block(self.sel);
        let l = self.items[i].level;
        if j >= self.items.len() || self.items[j].level != l {
            return; // no next sibling at the same level
        }
        let (_, j2) = self.block(j);
        self.push_undo();
        let block: Vec<Item> = self.items.drain(i..j).collect();
        let insert_at = i + (j2 - j);
        self.items.splice(insert_at..insert_at, block);
        self.sel = insert_at;
        self.dirty = true;
    }

    // ----- tree helpers -----------------------------------------------------

    /// The half-open index range `[i, end)` of the item at `i` plus its subtree
    /// (all following deeper items).
    fn block(&self, i: usize) -> (usize, usize) {
        let l = self.items[i].level;
        let mut k = i + 1;
        while k < self.items.len() && self.items[k].level > l {
            k += 1;
        }
        (i, k)
    }

    /// The nearest preceding item at the same level without crossing a shallower
    /// item (i.e. the previous sibling), if any.
    fn prev_sibling(&self, i: usize) -> Option<usize> {
        let l = self.items[i].level;
        let mut k = i;
        while k > 0 {
            k -= 1;
            if self.items[k].level < l {
                return None;
            }
            if self.items[k].level == l {
                return Some(k);
            }
        }
        None
    }

    /// The nearest preceding item at a shallower level (the parent), if any.
    fn parent(&self, i: usize) -> Option<usize> {
        let l = self.items[i].level;
        if l == 0 {
            return None;
        }
        let mut k = i;
        while k > 0 {
            k -= 1;
            if self.items[k].level < l {
                return Some(k);
            }
        }
        None
    }

    // ----- undo/redo --------------------------------------------------------

    /// Capture the current state onto the undo stack and clear redo.
    fn push_undo(&mut self) {
        self.undo.push(Snapshot { items: self.items.clone(), sel: self.sel });
        if self.undo.len() > HISTORY_CAP {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Restore `snap` and return the prior state for the other stack.
    fn restore(&mut self, snap: Snapshot) -> Snapshot {
        let prior = Snapshot { items: self.items.clone(), sel: self.sel };
        self.items = snap.items;
        self.sel = snap.sel.min(self.items.len().saturating_sub(1));
        self.dirty = true;
        prior
    }

    /// Undo the most recent change.
    fn undo(&mut self) {
        if let Some(snap) = self.undo.pop() {
            let prior = self.restore(snap);
            self.redo.push(prior);
        }
    }

    /// Redo the most recently undone change.
    fn redo(&mut self) {
        if let Some(snap) = self.redo.pop() {
            let prior = self.restore(snap);
            self.undo.push(prior);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn code(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn alt(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::ALT)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    // A → B(→ B1) → C  (two-space indentation)
    fn tree() -> Tree {
        Tree::from_text("A\nB\n  B1\nC\n")
    }

    #[test]
    fn parses_levels_from_spaces() {
        let t = tree();
        assert_eq!(t.item_count(), 4);
        assert_eq!((t.level(0), t.text(0)), (0, "A"));
        assert_eq!((t.level(2), t.text(2)), (1, "B1"));
        assert!(t.has_children(1), "B has a child");
        assert!(!t.has_children(2), "B1 is a leaf");
    }

    #[test]
    fn parses_levels_from_tabs() {
        let t = Tree::from_text("A\n\tA1\n");
        assert_eq!(t.level(1), 1);
        assert_eq!(t.text(1), "A1");
        assert_eq!(t.to_text(), "A\n\tA1\n", "tabs round-trip");
    }

    #[test]
    fn empty_input_has_one_item() {
        let t = Tree::from_text("");
        assert_eq!(t.item_count(), 1);
    }

    #[test]
    fn collapsing_hides_descendants() {
        let mut t = tree();
        t.handle_key(code(KeyCode::Down), 4); // sel = B (index 1)
        assert_eq!(t.sel(), 1);
        t.handle_key(code(KeyCode::Left), 4); // collapse B
        assert!(t.is_collapsed(1));
        let vis = t.visible();
        assert_eq!(vis, vec![0, 1, 3], "B1 is hidden under collapsed B");
    }

    #[test]
    fn navigation_skips_hidden_items() {
        let mut t = tree();
        t.handle_key(code(KeyCode::Down), 4); // B
        t.handle_key(code(KeyCode::Left), 4); // collapse B
        t.handle_key(code(KeyCode::Down), 4); // should land on C, not B1
        assert_eq!(t.text(t.sel()), "C");
    }

    #[test]
    fn right_expands_then_enters_first_child() {
        let mut t = tree();
        t.handle_key(code(KeyCode::Down), 4); // B
        t.handle_key(code(KeyCode::Left), 4); // collapse
        t.handle_key(code(KeyCode::Right), 4); // expand
        assert!(!t.is_collapsed(1));
        t.handle_key(code(KeyCode::Right), 4); // into first child
        assert_eq!(t.text(t.sel()), "B1");
    }

    #[test]
    fn indent_and_outdent_change_level() {
        let mut t = tree();
        t.handle_key(code(KeyCode::Down), 4); // B (index 1), prev sibling A exists
        t.handle_key(code(KeyCode::Tab), 4); // indent B under A
        assert_eq!(t.level(1), 1);
        assert_eq!(t.level(2), 2, "B1 subtree indented too");
        t.handle_key(code(KeyCode::BackTab), 4); // outdent
        assert_eq!(t.level(1), 0);
        assert_eq!(t.level(2), 1);
    }

    #[test]
    fn indent_needs_a_previous_sibling() {
        let mut t = tree();
        // A is the first item: no previous sibling, cannot indent.
        t.handle_key(code(KeyCode::Tab), 4);
        assert_eq!(t.level(0), 0);
    }

    #[test]
    fn move_item_down_reorders_subtree() {
        let mut t = tree(); // A, B(+B1), C
        t.handle_key(code(KeyCode::Down), 4); // B
        t.handle_key(alt(KeyCode::Down), 4); // move B (with B1) below C
        assert_eq!(t.text(1), "C");
        assert_eq!(t.text(2), "B");
        assert_eq!(t.text(3), "B1");
        assert_eq!(t.text(t.sel()), "B", "selection follows the moved item");
    }

    #[test]
    fn move_item_up_reorders_subtree() {
        let mut t = tree();
        t.handle_key(code(KeyCode::End), 4); // C (last)
        t.handle_key(alt(KeyCode::Up), 4); // move C above B
        assert_eq!(t.text(1), "C");
        assert_eq!(t.text(2), "B");
    }

    #[test]
    fn undo_and_redo() {
        let mut t = tree();
        t.handle_key(code(KeyCode::Down), 4); // B
        t.handle_key(code(KeyCode::Tab), 4); // indent
        assert_eq!(t.level(1), 1);
        t.handle_key(key('u'), 4);
        assert_eq!(t.level(1), 0, "undo restores level");
        t.handle_key(ctrl('r'), 4);
        assert_eq!(t.level(1), 1, "redo reapplies");
    }

    #[test]
    fn round_trips_to_text() {
        let t = tree();
        assert_eq!(t.to_text(), "A\nB\n  B1\nC\n");
    }

    #[test]
    fn save_and_close_outcomes() {
        let mut t = tree();
        assert_eq!(t.handle_key(ctrl('s'), 4), Outcome::Save);
        assert_eq!(t.handle_key(key('q'), 4), Outcome::Close);
        assert_eq!(t.handle_key(code(KeyCode::Esc), 4), Outcome::Close);
    }
}
