//! The palette's query history (T630): accepted inputs, recalled with
//! `Up`/`Down` like a shell's.

/// Most queries the history keeps.
pub const HISTORY_CAP: usize = 50;

/// Accepted palette inputs, oldest first, plus where a recall is browsing.
#[derive(Clone, Debug, Default)]
pub struct QueryHistory {
    /// Queries, oldest first, without duplicates.
    entries: Vec<String>,
    /// Index into `entries` of the entry being shown, while browsing.
    pos: Option<usize>,
}

impl QueryHistory {
    /// An empty history.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The stored queries, oldest first.
    #[must_use]
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Record an accepted `query` as the newest (moving an earlier copy up);
    /// blank queries are ignored. Ends any browsing.
    pub fn push(&mut self, query: &str) {
        self.pos = None;
        if query.trim().is_empty() {
            return;
        }
        self.entries.retain(|q| q != query);
        self.entries.push(query.to_string());
        if self.entries.len() > HISTORY_CAP {
            let extra = self.entries.len() - HISTORY_CAP;
            self.entries.drain(..extra);
        }
    }

    /// Stop browsing (the input was edited or the picker closed).
    pub fn reset(&mut self) {
        self.pos = None;
    }

    /// Whether `input` is what a recall is currently showing, so `Up`/`Down`
    /// should keep browsing rather than move the selection.
    #[must_use]
    pub fn is_browsing(&self, input: &str) -> bool {
        self.pos
            .and_then(|i| self.entries.get(i))
            .is_some_and(|q| q == input)
    }

    /// Step to the previous (older) query, or `None` when there is none
    /// older. Starts from the newest when not yet browsing.
    pub fn older(&mut self) -> Option<&str> {
        let next = match self.pos {
            None => self.entries.len().checked_sub(1)?,
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.pos = Some(next);
        self.entries.get(next).map(String::as_str)
    }

    /// Step to the next (newer) query. Past the newest, browsing ends and
    /// the result is the empty input `""`. `None` when not browsing.
    pub fn newer(&mut self) -> Option<&str> {
        let i = self.pos?;
        if i + 1 < self.entries.len() {
            self.pos = Some(i + 1);
            self.entries.get(i + 1).map(String::as_str)
        } else {
            self.pos = None;
            Some("")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_and_newer_walk_the_history() {
        let mut h = QueryHistory::new();
        h.push("a");
        h.push("b");
        h.push("c");
        assert_eq!(h.older(), Some("c"));
        assert!(h.is_browsing("c"));
        assert_eq!(h.older(), Some("b"));
        assert_eq!(h.older(), Some("a"));
        assert_eq!(h.older(), Some("a"), "stays at the oldest");
        assert_eq!(h.newer(), Some("b"));
        assert_eq!(h.newer(), Some("c"));
        assert_eq!(h.newer(), Some(""));
        assert!(!h.is_browsing(""));
        assert_eq!(h.newer(), None);
    }

    #[test]
    fn push_dedupes_ignores_blank_and_caps() {
        let mut h = QueryHistory::new();
        h.push("a");
        h.push("b");
        h.push("a");
        h.push("  ");
        assert_eq!(h.entries(), ["b", "a"]);
        for i in 0..HISTORY_CAP + 5 {
            h.push(&i.to_string());
        }
        assert_eq!(h.entries().len(), HISTORY_CAP);
        assert_eq!(h.entries().last().map(String::as_str), Some("54"));
    }

    #[test]
    fn an_empty_history_recalls_nothing() {
        let mut h = QueryHistory::new();
        assert_eq!(h.older(), None);
        assert_eq!(h.newer(), None);
    }
}
