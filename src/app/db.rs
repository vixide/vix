//! DB workbench glue: the `db.*` action dispatcher, the overlay's key handling,
//! and the pollers for its background query/connect/reconnect tasks.

#![warn(clippy::pedantic)]

#[cfg(feature = "ai")]
use super::{AiDest, AiReplace};
use super::{App, KeyEvent, Level};

impl App {
    /// Dispatch a `db.*` action (the DB menu / palette). Returns `true` if
    /// handled. Actions that need an open workbench open the overlay first.
    pub(super) fn db_action(&mut self, action: &str) -> bool {
        match action {
            "db.connections" => {
                self.open_db();
                if let Some(b) = self.db.as_mut() {
                    b.view = crate::db::View::Connections;
                }
            }
            "db.query" => {
                self.open_db();
                if let Some(b) = self.db.as_mut()
                    && b.conn.is_some()
                {
                    b.view = crate::db::View::Workbench;
                }
            }
            "db.execute" => self.with_db(crate::db::Browser::execute),
            "db.execute_all" => self.with_db(crate::db::Browser::execute_all),
            "db.explain" => self.with_db(|b| b.explain(false)),
            "db.explain_analyze" => self.with_db(|b| b.explain(true)),
            "db.format" => self.with_db(crate::db::Browser::format_at_cursor),
            "db.history" => self.with_db(crate::db::Browser::open_history),
            "db.saved" => self.with_db(crate::db::Browser::open_saved),
            "db.save_query" => self.with_db(crate::db::Browser::open_save_name),
            "db.export" => self.with_db(crate::db::Browser::open_export),
            "db.begin" => self.with_db(crate::db::Browser::begin_tx),
            "db.commit" => self.with_db(crate::db::Browser::commit_tx),
            "db.rollback" => self.with_db(crate::db::Browser::rollback_tx),
            "db.refresh" => self.with_db(crate::db::Browser::refresh_catalog),
            "db.disconnect" => self.with_db(crate::db::Browser::disconnect),
            _ => return false,
        }
        true
    }

    /// Open the DB overlay (if it is not already), keeping any live session.
    /// A fresh overlay also loads the persisted query history and saved
    /// queries.
    pub(super) fn open_db(&mut self) {
        if self.db.is_none() {
            let mut browser = crate::db::Browser::new(self.settings.db_connections.clone());
            browser.history = crate::db::store::load_history();
            browser.saved = crate::db::store::load_saved();
            self.db = Some(browser);
        }
    }

    /// Apply `f` to the open DB workbench, opening the overlay when needed.
    pub(super) fn with_db(&mut self, f: impl FnOnce(&mut crate::db::Browser)) {
        self.open_db();
        if let Some(b) = self.db.as_mut() {
            f(b);
        }
    }

    /// Route a key to the open DB workbench, persist any connection-list
    /// change, and act on the outcome.
    pub(super) fn db_key(&mut self, key: KeyEvent) {
        let pages = crate::db::Pages {
            tree: usize::from(self.layout.db_tree.height).max(1),
            editor: usize::from(self.layout.db_editor.height).max(1),
            results: usize::from(self.layout.db_results.height).max(1),
        };
        let outcome = match self.db.as_mut() {
            Some(b) => b.handle_key(key, pages),
            None => return,
        };
        if let Some(conns) = self
            .db
            .as_mut()
            .and_then(crate::db::Browser::take_dirty_connections)
        {
            self.settings.db_connections = conns;
            self.store_settings_or_warn();
        }
        if let Some(history) = self
            .db
            .as_mut()
            .and_then(crate::db::Browser::take_dirty_history)
        {
            // T545 (Run H): this used to be `let _ = ...`, discarding the
            // error entirely.
            if let Err(e) = crate::db::store::save_history(&history) {
                self.messages.push(
                    Level::Warn,
                    t!("msg.db_data_save_failed", error = e).to_string(),
                );
            }
        }
        if let Some(saved) = self
            .db
            .as_mut()
            .and_then(crate::db::Browser::take_dirty_saved)
            && let Err(e) = crate::db::store::save_saved(&saved)
        {
            self.messages.push(
                Level::Warn,
                t!("msg.db_data_save_failed", error = e).to_string(),
            );
        }
        self.db_run_ai_request();
        if outcome == crate::db::Outcome::Close {
            self.db = None;
        }
    }

    /// Run a queued natural-language → SQL request: spawn the assistant CLI and
    /// route its reply back into the workbench editor.
    #[cfg(feature = "ai")]
    fn db_run_ai_request(&mut self) {
        if let Some(req) = self
            .db
            .as_mut()
            .and_then(crate::db::Browser::take_ai_request)
        {
            if self.ai_replace.is_some() {
                if let Some(b) = self.db.as_mut() {
                    b.ai_failed();
                }
            } else if let Some(rx) = self.spawn_ai_cmd(&req.prompt, &req.context) {
                let label = t!("menu.ai").to_string();
                self.ai_replace = Some(AiReplace {
                    rx,
                    dest: AiDest::Db,
                    label,
                });
            } else if let Some(b) = self.db.as_mut() {
                b.ai_failed();
            }
        }
    }

    /// Without the `ai` capability a queued request just fails in the workbench.
    #[cfg(not(feature = "ai"))]
    fn db_run_ai_request(&mut self) {
        if let Some(b) = self.db.as_mut()
            && b.take_ai_request().is_some()
        {
            b.ai_failed();
        }
    }

    /// Drain a finished asynchronous DB query into the workbench. Called each
    /// event-loop tick; cheap when nothing is running.
    pub fn poll_db_query(&mut self) {
        if let Some(b) = self.db.as_mut() {
            b.poll_query();
        }
    }

    /// Whether a DB query is running asynchronously (keeps the loop polling).
    #[must_use]
    pub fn db_query_running(&self) -> bool {
        self.db
            .as_ref()
            .is_some_and(crate::db::Browser::query_running)
    }

    /// Drain a finished asynchronous DB connect (or password-prompted retry)
    /// into the workbench (Run H, T531). Called each event-loop tick; cheap
    /// when nothing is running.
    pub fn poll_db_connect(&mut self) {
        if let Some(b) = self.db.as_mut() {
            b.poll_connect();
        }
    }

    /// Whether a DB connect is running asynchronously (keeps the loop
    /// polling).
    #[must_use]
    pub fn db_connect_running(&self) -> bool {
        self.db
            .as_ref()
            .is_some_and(crate::db::Browser::connect_running)
    }

    /// Drain a finished asynchronous DB reconnect (a cancelled query's
    /// recovery, Run H, T531/T532) into the workbench. Called each
    /// event-loop tick; cheap when nothing is running.
    pub fn poll_db_reconnect(&mut self) {
        if let Some(b) = self.db.as_mut() {
            b.poll_reconnect();
        }
    }

    /// Whether a DB reconnect is running asynchronously (keeps the loop
    /// polling).
    #[must_use]
    pub fn db_reconnect_running(&self) -> bool {
        self.db
            .as_ref()
            .is_some_and(crate::db::Browser::reconnect_running)
    }
}
