//! Query history and saved queries (pgsavvy-style recall) for the workbench.
//!
//! Two small stores, both persisted outside `config.toml` so frequent query
//! runs do not rewrite the settings file: `db_history.toml` keeps the most
//! recent statements (deduplicated, newest first, capped) and
//! `db_queries.toml` keeps named saved queries. The models here are pure and
//! unit-tested; [`load_history`] / [`save_history`] / [`load_saved`] /
//! [`save_saved`] are the thin [`confy`] wrappers the host calls — the
//! workbench itself only mutates in-memory copies and raises dirty flags.

use serde::{Deserialize, Serialize};

/// Application name used by [`confy`] (same config directory as settings).
const APP_NAME: &str = "vix";

/// File stem of the history store (`db_history.toml`).
const HISTORY_NAME: &str = "db_history";

/// File stem of the saved-queries store (`db_queries.toml`).
const SAVED_NAME: &str = "db_queries";

/// Most history entries kept; the oldest fall off.
pub const HISTORY_CAP: usize = 200;

/// Most query-log entries kept in a session; the oldest fall off.
pub const LOG_CAP: usize = 200;

/// Where an executed statement originated, for the query log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The user ran it (F5 / F9 / EXPLAIN).
    User,
    /// The workbench ran it in the background (catalog, preview, ERD).
    App,
}

impl Origin {
    /// Short display tag.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Origin::User => "user",
            Origin::App => "app",
        }
    }
}

/// One executed statement with its timing and outcome (surus-style log line).
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// The statement text.
    pub sql: String,
    /// Wall-clock duration in milliseconds.
    pub ms: u128,
    /// Rows returned (0 for writes / errors).
    pub rows: usize,
    /// Whether the statement succeeded.
    pub ok: bool,
    /// Where the statement came from.
    pub origin: Origin,
}

/// The session query log, newest first (in-memory only; not persisted).
#[derive(Debug, Clone, Default)]
pub struct Log {
    /// Executed statements, most recent first, capped at [`LOG_CAP`].
    pub entries: Vec<LogEntry>,
}

impl Log {
    /// Record one execution at the front, dropping the oldest past the cap.
    pub fn push(&mut self, entry: LogEntry) {
        self.entries.insert(0, entry);
        self.entries.truncate(LOG_CAP);
    }
}

/// Executed-statement history, newest first.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    /// Statements, most recent first, no duplicates.
    pub entries: Vec<String>,
}

impl History {
    /// Record `sql` as the most recent entry: an existing duplicate moves to
    /// the front instead of repeating, and the list stays under
    /// [`HISTORY_CAP`]. Whitespace-only statements are ignored.
    pub fn push(&mut self, sql: &str) {
        let sql = sql.trim();
        if sql.is_empty() {
            return;
        }
        self.entries.retain(|e| e != sql);
        self.entries.insert(0, sql.to_string());
        self.entries.truncate(HISTORY_CAP);
    }
}

/// One saved query.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedQuery {
    /// Display name.
    pub name: String,
    /// The SQL text.
    pub sql: String,
}

/// The named saved queries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    /// Saved queries, in insertion order.
    pub queries: Vec<SavedQuery>,
}

impl Saved {
    /// Save `sql` under `name`, replacing an existing query of the same name.
    pub fn upsert(&mut self, name: &str, sql: &str) {
        if let Some(q) = self.queries.iter_mut().find(|q| q.name == name) {
            q.sql = sql.to_string();
        } else {
            self.queries.push(SavedQuery {
                name: name.to_string(),
                sql: sql.to_string(),
            });
        }
    }
}

/// Load the query history from the config directory (default when missing).
#[must_use]
pub fn load_history() -> History {
    confy::load(APP_NAME, Some(HISTORY_NAME)).unwrap_or_default()
}

/// Persist the query history.
///
/// # Errors
///
/// Returns a [`confy::ConfyError`] if the config directory cannot be
/// determined or the file cannot be written.
pub fn save_history(history: &History) -> Result<(), confy::ConfyError> {
    confy::store(APP_NAME, Some(HISTORY_NAME), history)
}

/// Load the saved queries from the config directory (default when missing).
#[must_use]
pub fn load_saved() -> Saved {
    confy::load(APP_NAME, Some(SAVED_NAME)).unwrap_or_default()
}

/// Persist the saved queries.
///
/// # Errors
///
/// Returns a [`confy::ConfyError`] if the config directory cannot be
/// determined or the file cannot be written.
pub fn save_saved(saved: &Saved) -> Result<(), confy::ConfyError> {
    confy::store(APP_NAME, Some(SAVED_NAME), saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_dedups_to_front_and_caps() {
        let mut h = History::default();
        h.push("select 1");
        h.push("select 2");
        h.push("select 1"); // duplicate moves to the front
        assert_eq!(h.entries, vec!["select 1", "select 2"]);
        h.push("   "); // whitespace ignored
        assert_eq!(h.entries.len(), 2);
        for i in 0..(HISTORY_CAP + 50) {
            h.push(&format!("select {i}"));
        }
        assert_eq!(h.entries.len(), HISTORY_CAP, "capped");
        assert_eq!(
            h.entries[0],
            format!("select {}", HISTORY_CAP + 49),
            "newest first"
        );
    }

    #[test]
    fn log_keeps_newest_first_and_caps() {
        let mut log = Log::default();
        for i in 0..(LOG_CAP + 10) {
            log.push(LogEntry {
                sql: format!("select {i}"),
                ms: u128::from(i as u64),
                rows: i,
                ok: true,
                origin: Origin::User,
            });
        }
        assert_eq!(log.entries.len(), LOG_CAP, "capped");
        assert_eq!(
            log.entries[0].sql,
            format!("select {}", LOG_CAP + 9),
            "newest first"
        );
        assert_eq!(Origin::App.label(), "app");
    }

    #[test]
    fn saved_upsert_replaces_by_name() {
        let mut s = Saved::default();
        s.upsert("top users", "select * from users");
        s.upsert("top users", "select id from users");
        assert_eq!(s.queries.len(), 1);
        assert_eq!(s.queries[0].sql, "select id from users");
        s.upsert("orders", "select * from orders");
        assert_eq!(s.queries.len(), 2);
    }
}
