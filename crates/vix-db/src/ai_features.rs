//! AI features: Ask a schema-grounded question, fix the last error, explain
//! a query in plain English, optimize the statement at the cursor, and
//! apply/discard the reply once the host's assistant CLI returns it.
//! Extracted from `lib.rs` (T516): the one cohesive slice of [`Browser`]'s
//! methods that talk to [`ai`] rather than the database itself.

use crossterm::event::{KeyCode, KeyEvent};

use super::{Browser, Outcome, Pane, View, ai, catalog, editor, object_triples};

/// Where an assistant reply is routed once it lands.
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
    /// Whether an AI request is queued or in flight.
    #[must_use]
    pub fn ai_busy(&self) -> bool {
        !matches!(self.ai, AiState::Idle)
    }

    /// Drain a queued AI request for the host to spawn, marking it in flight.
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

    /// Keys on the "Ask AI" prompt.
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
    pub(super) fn schema_facts(&mut self) -> (SchemaColumns, SchemaRels) {
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            return (Vec::new(), Vec::new());
        };
        let columns = self
            .run_catalog(catalog::columns_typed_sql(kind))
            .map(|(_, rows)| object_triples(rows))
            .unwrap_or_default();
        let relationships = self
            .run_catalog(catalog::relationships_sql(kind))
            .map(|(_, rows)| {
                rows.into_iter()
                    .filter(|r| r.len() >= 4)
                    .map(|r| (r[0].clone(), r[1].clone(), r[2].clone(), r[3].clone()))
                    .collect()
            })
            .unwrap_or_default();
        (columns, relationships)
    }

    /// Send the typed question to the assistant: build a schema-only brief and
    /// queue an [`AiRequest`] for the host. Read-only unless writes are enabled.
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
        let plan = self
            .run_catalog(&catalog::explain_sql(kind, &stmt, false))
            .map(|(_, rows)| {
                rows.iter()
                    .map(|r| r.join(" | "))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        let (columns, rels) = self.schema_facts();
        let context = ai::optimize_context(kind.label(), &columns, &rels, &stmt, &plan);
        self.queue_ai(ai::instruction(!self.write_enabled), context, AiReply::Sql);
    }

    /// Apply the assistant's reply, routed by the request kind: SQL replies land
    /// in the editor (validated with `EXPLAIN`); prose replies open in the
    /// scrollable text viewer.
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
    pub fn ai_failed(&mut self) {
        self.ai = AiState::Idle;
        self.message = Some(t!("msg.db_ai_failed").to_string());
    }
}
