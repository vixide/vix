//! AI features: Ask a schema-grounded question, fix the last error, explain
//! a query in plain English, optimize the statement at the cursor, and
//! apply the reply (or report failure) once the host's assistant CLI returns it.
//! Extracted from `lib.rs` (T516): the one cohesive slice of [`Browser`]'s
//! methods that talk to [`ai`] rather than the database itself.
//!
//! # Request lifecycle
//!
//! This crate never spawns the assistant process; it only *queues* work and
//! consumes the result, so it stays free of process and threading concerns:
//!
//! 1. A feature method ([`Browser::submit_ask`], [`Browser::fix_error`],
//!    [`Browser::explain_query`], [`Browser::optimize_current`]) builds a
//!    schema-only brief with [`ai`] and stores it as `AiState::Pending`.
//! 2. The host drains it with [`Browser::take_ai_request`] (state becomes
//!    `Running`), spawns the configured assistant CLI, and feeds
//!    [`AiRequest::prompt`] as the argument and [`AiRequest::context`] on stdin.
//! 3. The host reports back with [`Browser::apply_ai_reply`] or
//!    [`Browser::ai_failed`], returning the state to `Idle`.
//!
//! Only one request may be outstanding ([`Browser::ai_busy`]); a second
//! trigger just shows a "busy" message. Row data never leaves the machine via
//! this path -- only the catalog's tables, columns, types and foreign keys.
//! Owning spec: `crates/vix-db/spec/index.md`.

use vix_input::{KeyCode, KeyEvent};

use super::{Browser, Outcome, Pane, View, ai, catalog, editor, object_triples};

/// Where an assistant reply is routed once it lands. Recorded at queue time
/// (not derived from the reply) so the same CLI output is interpreted by the
/// feature that asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiReply {
    /// Recover SQL and place it in the editor (Ask, optimize, fix-error).
    Sql,
    /// Show the reply verbatim in the text viewer (explain, schema Q&A).
    Prose,
}

/// A pending request for the host to run through the configured assistant CLI:
/// a fixed command-line `prompt` and the schema-plus-question `context` fed on
/// stdin (see [`ai`]).
#[derive(Debug, Clone)]
pub struct AiRequest {
    /// The instruction placed on the assistant's command line.
    pub prompt: String,
    /// The schema-only brief and question, fed to the CLI on stdin.
    pub context: String,
    /// Where the reply should go (not exposed to the host).
    reply: AiReply,
}

/// The AI request lifecycle for the workbench.
#[derive(Debug, Clone, Default)]
pub(crate) enum AiState {
    /// No request outstanding.
    #[default]
    Idle,
    /// A request is queued for the host to spawn (drained by
    /// [`Browser::take_ai_request`]).
    Pending(AiRequest),
    /// A request has been spawned and its reply is awaited.
    Running(AiReply),
}

/// Typed columns `(table, column, type)` from the catalog.
type SchemaColumns = Vec<(String, String, String)>;

/// Foreign-key edges `(child, child_col, parent, parent_col)` from the catalog.
type SchemaRels = Vec<(String, String, String, String)>;

impl Browser {
    /// Whether an AI request is queued or in flight. The host uses this to
    /// show a spinner; the feature methods use it to refuse overlapping asks.
    #[must_use]
    pub fn ai_busy(&self) -> bool {
        !matches!(self.ai, AiState::Idle)
    }

    /// Drain a queued AI request for the host to spawn, marking it in flight.
    ///
    /// Returns `None` when nothing is queued (idle or already running), so the
    /// host can call it every tick. Each request is handed out exactly once.
    pub fn take_ai_request(&mut self) -> Option<AiRequest> {
        if let AiState::Pending(req) = &self.ai {
            let reply = req.reply;
            if let AiState::Pending(req) = std::mem::replace(&mut self.ai, AiState::Running(reply))
            {
                return Some(req);
            }
        }
        None
    }

    /// Keys on the "Ask AI" prompt: a single-line text field, Enter submits,
    /// Esc returns to the workbench.
    pub(super) fn key_ask(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Char(c) => self.ask_input.push(c),
            KeyCode::Backspace => {
                self.ask_input.pop();
            }
            KeyCode::Enter => self.submit_ask(),
            KeyCode::Esc => self.view = View::Workbench,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Open the natural-language "Ask AI" prompt (needs an active connection).
    pub fn open_ask(&mut self) {
        if self.conn.is_none() {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        }
        self.ask_input.clear();
        self.view = View::Ask;
    }

    /// The live schema as `(columns, relationships)` for an AI brief — types and
    /// foreign keys, never row data.
    ///
    /// Runs two catalog queries synchronously on the session; a failed query
    /// degrades to an empty list (the brief is then just thinner) rather than
    /// aborting the request. Returns empty lists when not connected.
    pub(super) fn schema_facts(&mut self) -> (SchemaColumns, SchemaRels) {
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            return (Vec::new(), Vec::new());
        };
        let columns = self
            .run_catalog(catalog::columns_typed_sql(kind))
            .map_or_default(|(_, rows)| object_triples(rows));
        let relationships = self
            .run_catalog(catalog::relationships_sql(kind))
            .map_or_default(|(_, rows)| {
                rows.into_iter()
                    .filter_map(|r| match r.as_slice() {
                        [a, b, c, d, ..] => Some((a.clone(), b.clone(), c.clone(), d.clone())),
                        _ => None,
                    })
                    .collect()
            });
        (columns, relationships)
    }

    /// Send the typed question to the assistant: build a schema-only brief and
    /// queue an [`AiRequest`] for the host. Read-only unless writes are enabled.
    ///
    /// A leading `?` switches to a data-model question answered in prose; an
    /// empty question just closes the prompt.
    pub fn submit_ask(&mut self) {
        if self.ai_busy() {
            self.message = Some(t!("msg.db_ai_busy").to_string());
            return;
        }
        // A leading "?" asks a data-model question (prose answer); otherwise the
        // input is a request to generate SQL.
        let raw = self.ask_input.trim();
        let (prose, question) = match raw.strip_prefix('?') {
            Some(rest) => (true, rest.trim().to_string()),
            None => (false, raw.to_string()),
        };
        if question.is_empty() {
            self.view = View::Workbench;
            return;
        }
        let Some(engine) = self.conn.as_ref().map(|c| c.kind.label()) else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        let (columns, rels) = self.schema_facts();
        let context = ai::context(engine, &columns, &rels, &question);
        if prose {
            self.queue_ai(ai::answer_instruction(), context, AiReply::Prose);
        } else {
            self.queue_ai(ai::instruction(!self.write_enabled), context, AiReply::Sql);
        }
        self.view = View::Workbench;
    }

    /// Queue an [`AiRequest`] for the host and mark the workbench busy.
    // The status line shows "thinking" until the reply (or failure) replaces it.
    fn queue_ai(&mut self, prompt: String, context: String, reply: AiReply) {
        self.ai = AiState::Pending(AiRequest {
            prompt,
            context,
            reply,
        });
        self.message = Some(t!("msg.db_ai_thinking").to_string());
    }

    /// Ask the assistant to fix the last failed query, feeding it the query and
    /// the database's error message alongside the schema.
    pub fn fix_error(&mut self) {
        if self.ai_busy() {
            self.message = Some(t!("msg.db_ai_busy").to_string());
            return;
        }
        let (Some(engine), Some((sql, error))) = (
            self.conn.as_ref().map(|c| c.kind.label()),
            self.last_error.clone(),
        ) else {
            self.message = Some(t!("msg.db_ai_no_error").to_string());
            return;
        };
        let (columns, rels) = self.schema_facts();
        let context = ai::error_context(engine, &columns, &rels, &sql, &error);
        self.queue_ai(ai::instruction(!self.write_enabled), context, AiReply::Sql);
    }

    /// Ask the assistant to explain the statement at the cursor in plain
    /// English (answer shown in the viewer, no SQL run).
    pub fn explain_query(&mut self) {
        if self.ai_busy() {
            self.message = Some(t!("msg.db_ai_busy").to_string());
            return;
        }
        let Some(engine) = self.conn.as_ref().map(|c| c.kind.label()) else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        let Some(stmt) = self.query.statement_at_cursor() else {
            self.message = Some(t!("msg.db_no_statement").to_string());
            return;
        };
        let (columns, rels) = self.schema_facts();
        let context = ai::explain_context(engine, &columns, &rels, &stmt);
        self.queue_ai(ai::explain_instruction(), context, AiReply::Prose);
    }

    /// Ask the assistant to optimize the statement at the cursor, feeding it the
    /// query's own `EXPLAIN` plan (the surus draft → EXPLAIN → iterate loop).
    pub fn optimize_current(&mut self) {
        if self.ai_busy() {
            self.message = Some(t!("msg.db_ai_busy").to_string());
            return;
        }
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        let Some(stmt) = self.query.statement_at_cursor() else {
            self.message = Some(t!("msg.db_no_statement").to_string());
            return;
        };
        // A failed EXPLAIN (e.g. syntax error) yields an empty plan; the model
        // still gets the query and schema, just without plan hints.
        let plan = self
            .run_catalog(&catalog::explain_sql(kind, &stmt, false))
            .map_or_default(|(_, rows)| {
                rows.iter()
                    .map(|r| r.join(" | "))
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        let (columns, rels) = self.schema_facts();
        let context = ai::optimize_context(kind.label(), &columns, &rels, &stmt, &plan);
        self.queue_ai(ai::instruction(!self.write_enabled), context, AiReply::Sql);
    }

    /// Apply the assistant's reply, routed by the request kind: SQL replies land
    /// in the editor (validated with `EXPLAIN`); prose replies open in the
    /// scrollable text viewer.
    ///
    /// A SQL reply *replaces* the editor buffer. If the request state was not
    /// `Running` (a stray reply) it is treated as SQL. An empty reply only sets
    /// a message and changes nothing else.
    pub fn apply_ai_reply(&mut self, reply: &str) {
        let kind = match self.ai {
            AiState::Running(k) => k,
            _ => AiReply::Sql,
        };
        self.ai = AiState::Idle;
        match kind {
            AiReply::Sql => {
                let sql = ai::extract_sql(reply);
                if sql.trim().is_empty() {
                    self.message = Some(t!("msg.db_ai_empty").to_string());
                    return;
                }
                self.query = editor::Query::default();
                self.insert_statement(&sql);
                self.focus = Pane::Editor;
                // Validate with EXPLAIN synchronously (never ANALYZE, so it is
                // read-only even if the model drafted a write). Not the async
                // user path — the reply is applied outside the event loop.
                if let Some(kind) = self.conn.as_ref().map(|c| c.kind) {
                    let wrapped = catalog::explain_sql(kind, &sql, false);
                    if let Ok((headers, rows)) = self.run_catalog(&wrapped) {
                        self.grid.set(headers, rows);
                        self.grid_source = Some((wrapped, None));
                        self.focus = Pane::Results;
                        self.set_uneditable();
                    }
                }
                self.message = Some(t!("msg.db_ai_ready").to_string());
            }
            AiReply::Prose => {
                let text = reply.trim();
                if text.is_empty() {
                    self.message = Some(t!("msg.db_ai_empty").to_string());
                    return;
                }
                self.cell_text = text.to_string();
                self.cell_pretty = false;
                self.view_scroll = 0;
                self.view = View::Cell;
                self.message = Some(t!("msg.db_ai_answer").to_string());
            }
        }
    }

    /// Report that the AI request failed (spawn error or empty output).
    /// Clears the in-flight state so a new request can be made.
    pub fn ai_failed(&mut self) {
        self.ai = AiState::Idle;
        self.message = Some(t!("msg.db_ai_failed").to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ctrl, family, press, typed, workbench};
    use vix_input::KeyCode;

    fn queued(b: &mut Browser) -> AiRequest {
        assert!(b.ai_busy());
        assert_eq!(b.message, Some(t!("msg.db_ai_thinking").to_string()));
        b.take_ai_request().expect("a queued request")
    }

    #[test]
    fn ask_needs_a_connection() {
        let mut b = Browser::new(Vec::new());
        b.open_ask();
        assert_eq!(b.message, Some(t!("msg.db_not_connected").to_string()));
        assert_ne!(b.view, View::Ask);
        for f in [Browser::explain_query, Browser::optimize_current] {
            b.message = None;
            f(&mut b);
            assert_eq!(b.message, Some(t!("msg.db_not_connected").to_string()));
        }
        b.ask_input = "list users".into();
        b.submit_ask();
        assert_eq!(b.message, Some(t!("msg.db_not_connected").to_string()));
        assert!(!b.ai_busy());
        assert_eq!(b.schema_facts(), (Vec::new(), Vec::new()));
    }

    #[test]
    fn typing_a_question_queues_a_schema_only_sql_request() {
        let mut b = family(false);
        ctrl(&mut b, 'a');
        assert_eq!(b.view, View::Ask);
        typed(&mut b, "kids per parent");
        press(&mut b, KeyCode::Backspace);
        typed(&mut b, "t");
        press(&mut b, KeyCode::Tab); // ignored
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.view, View::Workbench);
        let req = queued(&mut b);
        assert!(req.context.contains("kids per parent"), "{}", req.context);
        assert!(req.context.contains("child"), "{}", req.context);
        assert!(!req.context.contains("alpha"), "row data never leaves");
        assert_eq!(req.prompt, ai::instruction(true), "read-only connection");
        assert_eq!(req.reply, AiReply::Sql);
        assert!(b.ai_busy(), "now in flight");
        assert!(b.take_ai_request().is_none(), "handed out exactly once");
    }

    #[test]
    fn a_writable_connection_gets_the_writable_instruction() {
        let mut b = family(true);
        b.ask_input = "add a parent".into();
        b.submit_ask();
        assert_eq!(queued(&mut b).prompt, ai::instruction(false));
    }

    #[test]
    fn a_leading_question_mark_asks_for_a_prose_answer() {
        let mut b = family(false);
        b.ask_input = "  ? how are they related  ".into();
        b.submit_ask();
        let req = queued(&mut b);
        assert_eq!(req.reply, AiReply::Prose);
        assert_eq!(req.prompt, ai::answer_instruction());
        assert!(req.context.contains("how are they related"));
    }

    #[test]
    fn an_empty_question_closes_the_prompt_without_asking() {
        let mut b = family(false);
        b.open_ask();
        b.ask_input = " ? ".into();
        press(&mut b, KeyCode::Enter);
        assert_eq!(b.view, View::Workbench);
        assert!(!b.ai_busy());
        b.open_ask();
        press(&mut b, KeyCode::Esc);
        assert_eq!(b.view, View::Workbench);
    }

    #[test]
    fn only_one_request_may_be_outstanding() {
        let mut b = family(false);
        b.ask_input = "first".into();
        b.submit_ask();
        let busy = t!("msg.db_ai_busy").to_string();
        b.message = None;
        b.ask_input = "second".into();
        b.submit_ask();
        assert_eq!(b.message, Some(busy.clone()));
        for f in [
            Browser::fix_error,
            Browser::explain_query,
            Browser::optimize_current,
        ] {
            b.message = None;
            f(&mut b);
            assert_eq!(b.message, Some(busy.clone()));
        }
    }

    #[test]
    fn fixing_needs_a_failed_query_and_feeds_it_to_the_assistant() {
        let mut b = family(false);
        b.fix_error();
        assert_eq!(b.message, Some(t!("msg.db_ai_no_error").to_string()));
        assert!(!b.ai_busy());
        b.last_error = Some(("SELEKT 1".into(), "syntax error".into()));
        b.fix_error();
        let req = queued(&mut b);
        assert!(req.context.contains("SELEKT 1") && req.context.contains("syntax error"));
        assert_eq!(req.reply, AiReply::Sql);
    }

    #[test]
    fn explain_and_optimize_use_the_statement_at_the_cursor() {
        let mut b = family(false);
        b.explain_query();
        assert_eq!(b.message, Some(t!("msg.db_no_statement").to_string()));
        b.optimize_current();
        assert_eq!(b.message, Some(t!("msg.db_no_statement").to_string()));
        b.focus = Pane::Editor;
        typed(&mut b, "SELECT * FROM parent");
        b.explain_query();
        let req = queued(&mut b);
        assert_eq!(req.reply, AiReply::Prose);
        assert!(req.context.contains("SELECT * FROM parent"));
        b.ai_failed();
        b.optimize_current();
        let req = queued(&mut b);
        assert_eq!(req.reply, AiReply::Sql);
        assert!(req.context.contains("SELECT * FROM parent"));
        assert!(
            req.context.contains("SCAN"),
            "the plan is included: {}",
            req.context
        );
    }

    #[test]
    fn a_sql_reply_replaces_the_editor_and_is_validated_with_explain() {
        let mut b = family(false);
        b.focus = Pane::Editor;
        typed(&mut b, "old text");
        b.ask_input = "all parents".into();
        b.submit_ask();
        let _ = b.take_ai_request();
        b.apply_ai_reply("Here you go:\n```sql\nSELECT * FROM parent\n```");
        assert!(!b.ai_busy());
        assert_eq!(b.query.text(), "SELECT * FROM parent");
        assert_eq!(b.focus, Pane::Results, "the EXPLAIN plan is shown");
        assert_ne!(b.grid.rows.len(), 0);
        assert!(!b.editable());
        assert_eq!(b.message, Some(t!("msg.db_ai_ready").to_string()));
    }

    #[test]
    fn a_sql_reply_that_does_not_explain_still_lands_in_the_editor() {
        let mut b = family(false);
        b.apply_ai_reply("SELECT * FROM nowhere");
        assert_eq!(b.query.text(), "SELECT * FROM nowhere");
        assert_eq!(b.focus, Pane::Editor);
        assert_eq!(b.message, Some(t!("msg.db_ai_ready").to_string()));
    }

    #[test]
    fn an_empty_reply_changes_nothing() {
        let mut b = family(false);
        b.focus = Pane::Editor;
        typed(&mut b, "keep");
        b.ai = AiState::Running(AiReply::Sql);
        b.apply_ai_reply("   ");
        assert_eq!(b.query.text(), "keep");
        assert_eq!(b.message, Some(t!("msg.db_ai_empty").to_string()));
        assert!(!b.ai_busy());
        b.ai = AiState::Running(AiReply::Prose);
        b.apply_ai_reply("  \n ");
        assert_eq!(b.message, Some(t!("msg.db_ai_empty").to_string()));
        assert_ne!(b.view, View::Cell);
    }

    #[test]
    fn a_prose_reply_opens_in_the_viewer() {
        let mut b = family(false);
        b.ask_input = "?what is parent".into();
        b.submit_ask();
        let _ = b.take_ai_request();
        b.apply_ai_reply("  It holds the owners.\n");
        assert_eq!(b.view, View::Cell);
        assert_eq!(b.cell_text, "It holds the owners.");
        assert_eq!(b.view_scroll, 0);
        assert_eq!(b.message, Some(t!("msg.db_ai_answer").to_string()));
    }

    #[test]
    fn a_failed_request_frees_the_queue() {
        let mut b = family(false);
        b.ask_input = "x".into();
        b.submit_ask();
        b.ai_failed();
        assert!(!b.ai_busy());
        assert_eq!(b.message, Some(t!("msg.db_ai_failed").to_string()));
    }

    #[test]
    fn schema_facts_list_typed_columns_and_foreign_keys() {
        let mut b = workbench(
            false,
            &[
                "CREATE TABLE p (id INTEGER PRIMARY KEY)",
                "CREATE TABLE c (id INTEGER PRIMARY KEY, p_id INTEGER REFERENCES p(id))",
            ],
        );
        let (columns, rels) = b.schema_facts();
        assert!(columns.contains(&("c".into(), "p_id".into(), "INTEGER".into())));
        assert_eq!(rels, [("c".into(), "p_id".into(), "p".into(), "id".into())]);
    }
}
