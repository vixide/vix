//! Several terminals at once (T631): an ordered set of tabs, one active, each
//! with a stable id, plus whether the panel is shown. Generic over the tab's
//! contents so the bookkeeping is tested without a PTY.
//!
//! The host instantiates it as `Tabs<Terminal>`; nothing here knows about PTYs.
//! Invariant: `active` is a valid index whenever there is at least one tab
//! (and 0 otherwise), maintained by every mutator.
//!
//! Spec: `crates/vix-terminal/spec/index.md`.

/// An ordered set of terminal tabs. Ids are never reused, so a reference by id
/// (Cowork's shared terminal, T697) cannot silently move to another tab.
///
/// # Examples
///
/// ```
/// use vix_terminal::Tabs;
///
/// let mut tabs: Tabs<&str> = Tabs::default();
/// let a = tabs.push("a");
/// let b = tabs.push("b");
/// assert_eq!(tabs.active(), Some(&"b"));
/// tabs.remove(a);
/// let c = tabs.push("c");
/// assert!(c > b, "ids are never reused");
/// ```
#[derive(Debug)]
pub struct Tabs<T> {
    /// Tabs in display order, each with its stable id.
    items: Vec<(u64, T)>,
    /// Index into `items` of the active tab.
    active: usize,
    /// The id the next `push` hands out; only ever increases.
    next_id: u64,
    /// Whether the panel is open; kept separate so hiding does not close tabs.
    shown: bool,
}

/// An empty, hidden tab set. Hand-written because a derived `Default` would
/// require `T: Default` and would start `next_id` at 0.
impl<T> Default for Tabs<T> {
    fn default() -> Self {
        Tabs {
            items: Vec::new(),
            active: 0,
            next_id: 1,
            shown: false,
        }
    }
}

impl<T> Tabs<T> {
    /// Add `item` as the last tab, make it active and show the panel; returns
    /// its id.
    pub fn push(&mut self, item: T) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push((id, item));
        self.active = self.items.len() - 1;
        self.shown = true;
        id
    }

    /// How many tabs there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Whether the panel is shown (and so takes keys). Always `false` with no
    /// tabs.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.shown && !self.items.is_empty()
    }

    /// Show or hide the panel; hidden tabs keep running.
    pub fn set_shown(&mut self, shown: bool) {
        self.shown = shown;
    }

    /// The active tab's index (0 when empty).
    #[must_use]
    pub fn active_index(&self) -> usize {
        self.active
    }

    /// The active tab's id.
    #[must_use]
    pub fn active_id(&self) -> Option<u64> {
        self.items.get(self.active).map(|(id, _)| *id)
    }

    /// The active tab.
    #[must_use]
    pub fn active(&self) -> Option<&T> {
        self.items.get(self.active).map(|(_, t)| t)
    }

    /// The active tab, mutably.
    pub fn active_mut(&mut self) -> Option<&mut T> {
        self.items.get_mut(self.active).map(|(_, t)| t)
    }

    /// The tab with `id`.
    #[must_use]
    pub fn get(&self, id: u64) -> Option<&T> {
        self.items.iter().find(|(i, _)| *i == id).map(|(_, t)| t)
    }

    /// The tab with `id`, mutably.
    pub fn get_mut(&mut self, id: u64) -> Option<&mut T> {
        self.items
            .iter_mut()
            .find(|(i, _)| *i == id)
            .map(|(_, t)| t)
    }

    /// Every tab in order, with its id.
    pub fn iter(&self) -> impl Iterator<Item = (u64, &T)> {
        self.items.iter().map(|(id, t)| (*id, t))
    }

    /// Every tab in order, mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (u64, &mut T)> {
        self.items.iter_mut().map(|(id, t)| (*id, t))
    }

    /// Activate the next tab, wrapping around.
    pub fn next(&mut self) {
        if !self.items.is_empty() {
            self.active = (self.active + 1) % self.items.len();
        }
    }

    /// Activate the previous tab, wrapping around.
    pub fn prev(&mut self) {
        if !self.items.is_empty() {
            self.active = (self.active + self.items.len() - 1) % self.items.len();
        }
    }

    /// Activate the tab at `index`, if there is one.
    pub fn select(&mut self, index: usize) {
        if index < self.items.len() {
            self.active = index;
        }
    }

    /// Remove the tab with `id`, keeping the same tab active when another one
    /// goes (the next one when the active one goes, else the new last).
    /// Hides the panel when the last tab goes.
    pub fn remove(&mut self, id: u64) -> Option<T> {
        let index = self.items.iter().position(|(i, _)| *i == id)?;
        let (_, item) = self.items.remove(index);
        // A tab before the active one left: shift down so the same tab stays
        // active. (Removing the active tab itself keeps the index, which now
        // names its successor.)
        if index < self.active {
            self.active -= 1;
        }
        // Removed the last tab while it was active: fall back to the new last.
        if self.active >= self.items.len() {
            self.active = self.items.len().saturating_sub(1);
        }
        if self.items.is_empty() {
            self.shown = false;
        }
        Some(item)
    }

    /// Remove the active tab.
    pub fn close_active(&mut self) -> Option<T> {
        let id = self.active_id()?;
        self.remove(id)
    }

    /// Remove every tab for which `keep` is `false`; returns their ids.
    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) -> Vec<u64> {
        let gone: Vec<u64> = self
            .items
            .iter()
            .filter(|(_, t)| !keep(t))
            .map(|(id, _)| *id)
            .collect();
        // Collect ids first, then remove: removing while iterating would shift
        // indices under the `keep` pass. Ids are stable, so this is order-safe.
        for id in &gone {
            self.remove(*id);
        }
        gone
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushing_activates_and_shows() {
        let mut tabs: Tabs<&str> = Tabs::default();
        assert!(tabs.is_empty());
        assert!(!tabs.is_visible());
        let id_a = tabs.push("a");
        let id_b = tabs.push("b");
        assert_ne!(id_a, id_b);
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs.active(), Some(&"b"));
        assert!(tabs.is_visible());
        tabs.set_shown(false);
        assert!(!tabs.is_visible());
        assert_eq!(tabs.get(id_a), Some(&"a"));
    }

    #[test]
    fn next_and_prev_wrap() {
        let mut tabs = Tabs::default();
        for name in ["a", "b", "c"] {
            tabs.push(name);
        }
        tabs.next();
        assert_eq!(tabs.active(), Some(&"a"));
        tabs.prev();
        assert_eq!(tabs.active(), Some(&"c"));
        tabs.select(1);
        assert_eq!(tabs.active(), Some(&"b"));
        tabs.select(9);
        assert_eq!(tabs.active(), Some(&"b"));
        if let Some(x) = tabs.active_mut() {
            *x = "B";
        }
        let names: Vec<&str> = tabs.iter().map(|(_, x)| *x).collect();
        assert_eq!(names, ["a", "B", "c"]);
    }

    #[test]
    fn removing_keeps_the_right_tab_active() {
        let mut tabs = Tabs::default();
        let id_a = tabs.push("a");
        let id_b = tabs.push("b");
        let id_c = tabs.push("c");
        tabs.select(1); // b
        assert_eq!(tabs.remove(id_a), Some("a"));
        assert_eq!(tabs.active(), Some(&"b"), "an earlier tab going keeps b");
        assert_eq!(tabs.close_active(), Some("b"));
        assert_eq!(tabs.active(), Some(&"c"));
        assert_eq!(tabs.remove(id_b), None, "ids are not reused");
        let id_d = tabs.push("d");
        assert!(id_d > id_c);
        tabs.select(1);
        assert_eq!(tabs.close_active(), Some("d"));
        assert_eq!(
            tabs.active(),
            Some(&"c"),
            "the last going activates the new last"
        );
        assert_eq!(tabs.close_active(), Some("c"));
        assert!(tabs.is_empty());
        assert!(!tabs.is_visible());
        assert_eq!(tabs.active_id(), None);
        assert_eq!(tabs.close_active(), None);
    }

    #[test]
    fn retain_reports_what_went() {
        let mut tabs = Tabs::default();
        let id_a = tabs.push(1);
        let id_b = tabs.push(2);
        let id_c = tabs.push(3);
        assert_eq!(tabs.retain(|n| n % 2 == 1), vec![id_b]);
        assert_eq!(tabs.get(id_a), Some(&1));
        assert_eq!(tabs.get_mut(id_c), Some(&mut 3));
        assert_eq!(tabs.len(), 2);
    }
}
