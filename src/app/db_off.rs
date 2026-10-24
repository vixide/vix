//! The `sql` capability's absent half (T588): in a build without the feature the
//! DB workbench and SQL editor don't exist, but the shared dispatch, key-routing
//! and event-loop code still calls these entry points. Each is a no-op, so the
//! call sites need no `cfg` of their own. `App::db` / `App::edit_sql` are
//! uninhabited-typed `Option`s that are always `None`.

use vix_input::KeyEvent;

use super::App;

impl App {
    /// No `db.*` actions in this build (they are also refused up front by
    /// `run_action`'s capability check).
    pub(super) fn db_action(&mut self, _action: &str) -> bool {
        debug_assert!(self.db.is_none());
        false
    }

    /// The DB workbench can't be open, so it never takes a key.
    pub(super) fn db_key(&mut self, _key: KeyEvent) {
        debug_assert!(self.db.is_none());
    }

    /// The SQL editor can't be open, so it never takes a key.
    pub(super) fn edit_sql_key(&mut self, _key: KeyEvent) {
        debug_assert!(self.edit_sql.is_none());
    }

    /// No background DB query to drain.
    pub fn poll_db_query(&mut self) {}

    /// No background DB connect to drain.
    pub fn poll_db_connect(&mut self) {}

    /// No background DB reconnect to drain.
    pub fn poll_db_reconnect(&mut self) {}

    /// Never running.
    #[must_use]
    pub fn db_query_running(&self) -> bool {
        false
    }

    /// Never running.
    #[must_use]
    pub fn db_connect_running(&self) -> bool {
        false
    }

    /// Never running.
    #[must_use]
    pub fn db_reconnect_running(&self) -> bool {
        false
    }
}
