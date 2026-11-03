//! The ACP session state machine. No IO: feed it the agent's messages, send
//! whatever it returns.
//!
//! The flow is `initialize` -> `session/new` (or `session/load` when resuming)
//! -> any number of `session/prompt` turns. Each call to [`Client::handle`]
//! returns a [`Step`]: [`Event`]s for the host UI plus JSON-RPC messages for
//! the host to write to the agent (see [`crate::process`]). Because the machine
//! does no IO it can be driven entirely by canned messages in tests.
//!
//! Security note: the client never touches the file system. Agent requests to
//! read or write files, and permission prompts, are surfaced as events that the
//! host answers after consulting [`crate::policy`]; unknown methods and
//! requests for a session other than ours are rejected here, before the host
//! sees them.
//!
//! Owning spec: `crates/vix-acp/spec/index.md`.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::wire::{self, Incoming, RpcError};

/// The ACP protocol version this client speaks.
pub const PROTOCOL_VERSION: u64 = 1;

/// What kind of grant a permission option is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionKind {
    /// Allow this one call.
    AllowOnce,
    /// Allow this and later calls.
    AllowAlways,
    /// Reject this one call.
    RejectOnce,
    /// Reject this and later calls.
    RejectAlways,
}

/// One choice offered with a permission request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermOption {
    /// The id to answer with.
    pub id: String,
    /// The label the agent suggests.
    pub name: String,
    /// What choosing it means.
    pub kind: OptionKind,
}

/// One line of the agent's plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanEntry {
    /// What the step is.
    pub content: String,
    /// `pending`, `in_progress` or `completed`.
    pub status: String,
}

/// Something the host should show or answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// The session exists; prompts are accepted.
    Ready {
        /// The agent's session id.
        session_id: String,
    },
    /// A chunk of the agent's reply.
    AgentText(String),
    /// A chunk of the agent's reasoning.
    Thought(String),
    /// A tool call started or changed.
    ToolCall {
        /// The agent's id for the call.
        id: String,
        /// Its title (may be empty on an update that did not change it).
        title: String,
        /// `pending`, `in_progress`, `completed` or `failed`; may be empty.
        status: String,
    },
    /// The agent's plan, replacing any earlier one.
    Plan(Vec<PlanEntry>),
    /// The agent asks permission; answer with [`Client::answer_permission`].
    Permission {
        /// The request id to answer.
        req_id: Value,
        /// What it wants to do.
        title: String,
        /// The choices.
        options: Vec<PermOption>,
    },
    /// The agent wants a file's text; answer with [`Client::answer_read`].
    ReadFile {
        /// The request id to answer.
        req_id: Value,
        /// The path it named (unchecked).
        path: String,
        /// First line to return (1-based), if limited.
        line: Option<u64>,
        /// Maximum number of lines, if limited.
        limit: Option<u64>,
    },
    /// The agent wants to write a file; answer with [`Client::answer_write`].
    WriteFile {
        /// The request id to answer.
        req_id: Value,
        /// The path it named (unchecked).
        path: String,
        /// The full new text.
        content: String,
    },
    /// A prompt turn finished.
    TurnEnded {
        /// Why: `end_turn`, `cancelled`, `refusal`, ...
        stop_reason: String,
    },
    /// The agent reported an error or sent an unusable reply.
    Failed(String),
}

/// Where the conversation stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// `start` has not been called.
    Idle,
    /// `initialize` is in flight.
    Initializing,
    /// `session/new` is in flight.
    CreatingSession,
    /// A session exists and no turn is running.
    Ready,
    /// A prompt turn is running.
    Prompting,
}

/// Which of our outstanding requests a response id belongs to.
#[derive(Clone, Copy)]
enum Pending {
    /// The `initialize` handshake.
    Initialize,
    /// `session/new`.
    NewSession,
    /// `session/load` (resume); on error we fall back to `session/new`.
    LoadSession,
    /// A `session/prompt` turn.
    Prompt,
}

/// The ACP client state machine.
pub struct Client {
    /// Next JSON-RPC request id; ids are never reused within a client.
    next_id: u64,
    /// Where the conversation stands.
    phase: Phase,
    /// Requests awaiting a response, keyed by id.
    pending: HashMap<u64, Pending>,
    /// The active session id; set early on resume (see `on_response`).
    session_id: Option<String>,
    /// Workspace root sent as the session's `cwd`.
    cwd: String,
    /// A session to load instead of creating one (T635 resume).
    resume: Option<String>,
    /// Whether the session we are in was loaded rather than created.
    resumed: bool,
}

/// What [`Client::handle`] produced: events for the host, messages to send.
#[derive(Default, Debug)]
pub struct Step {
    /// For the host to show or answer.
    pub events: Vec<Event>,
    /// To write to the agent.
    pub out: Vec<Value>,
}

impl Client {
    /// A client whose session will be rooted at `cwd`.
    #[must_use]
    pub fn new(cwd: &str) -> Self {
        Self {
            next_id: 1,
            phase: Phase::Idle,
            pending: HashMap::new(),
            session_id: None,
            cwd: cwd.to_string(),
            resume: None,
            resumed: false,
        }
    }

    /// A client that tries to resume `session_id` (`session/load`) when the
    /// agent advertises `loadSession`, and otherwise (or if the agent
    /// refuses the id) starts a fresh session.
    #[must_use]
    pub fn resuming(cwd: &str, session_id: &str) -> Self {
        let mut c = Self::new(cwd);
        c.resume = Some(session_id.to_string());
        c
    }

    /// Whether the current session was loaded from an earlier one rather than
    /// created new. Meaningful once [`Event::Ready`] has been seen.
    #[must_use]
    pub fn resumed(&self) -> bool {
        self.resumed
    }

    /// The conversation phase.
    #[must_use]
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The session id, once there is one.
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Allocate an id, remember what it is for, and build the request.
    fn send(&mut self, method: &str, params: &Value, kind: Pending) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(id, kind);
        wire::request(id, method, params)
    }

    /// Begin: the `initialize` request to write first.
    ///
    /// Advertises only the `fs` read/write capabilities (no terminal), so the
    /// agent cannot ask the client to run commands. Returns nothing if already
    /// started.
    pub fn start(&mut self) -> Vec<Value> {
        if self.phase != Phase::Idle {
            return Vec::new();
        }
        self.phase = Phase::Initializing;
        let params = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "clientCapabilities": {"fs": {"readTextFile": true, "writeTextFile": true}},
        });
        vec![self.send("initialize", &params, Pending::Initialize)]
    }

    /// Send a prompt, or `None` unless a session is ready and idle.
    ///
    /// Moves the phase to [`Phase::Prompting`] until the matching response
    /// arrives as [`Event::TurnEnded`] (or [`Event::Failed`]).
    pub fn prompt(&mut self, text: &str) -> Option<Value> {
        if self.phase != Phase::Ready {
            return None;
        }
        let sid = self.session_id.clone()?;
        self.phase = Phase::Prompting;
        let params = json!({"sessionId": sid, "prompt": [{"type": "text", "text": text}]});
        Some(self.send("session/prompt", &params, Pending::Prompt))
    }

    /// Ask the agent to stop the running turn, or `None` if none is running.
    #[must_use]
    pub fn cancel(&self) -> Option<Value> {
        if self.phase != Phase::Prompting {
            return None;
        }
        let sid = self.session_id.as_deref()?;
        Some(wire::notification(
            "session/cancel",
            &json!({"sessionId": sid}),
        ))
    }

    /// Answer a [`Event::Permission`]: `Some(option_id)` selects it, `None`
    /// cancels. An id that was not offered is sent as `cancelled`.
    #[must_use]
    pub fn answer_permission(
        req_id: &Value,
        offered: &[PermOption],
        choice: Option<&str>,
    ) -> Value {
        let outcome = match choice.filter(|c| offered.iter().any(|o| o.id == *c)) {
            Some(id) => json!({"outcome": "selected", "optionId": id}),
            None => json!({"outcome": "cancelled"}),
        };
        wire::response(req_id, &json!({"outcome": outcome}))
    }

    /// Answer a [`Event::ReadFile`] with the text, or `Err(reason)`.
    #[must_use]
    pub fn answer_read(req_id: &Value, text: Result<&str, &str>) -> Value {
        match text {
            Ok(t) => wire::response(req_id, &json!({"content": t})),
            Err(e) => wire::error_response(req_id, -32602, e),
        }
    }

    /// Answer a [`Event::WriteFile`]: success, or `Err(reason)`.
    #[must_use]
    pub fn answer_write(req_id: &Value, result: Result<(), &str>) -> Value {
        match result {
            Ok(()) => wire::response(req_id, &Value::Null),
            Err(e) => wire::error_response(req_id, -32602, e),
        }
    }

    /// Feed one decoded message from the agent.
    ///
    /// Messages that are not JSON-RPC, responses to ids we never issued, and
    /// unknown notifications are ignored (an empty [`Step`]).
    pub fn handle(&mut self, msg: &Value) -> Step {
        let mut step = Step::default();
        match wire::parse_incoming(msg) {
            Some(Incoming::Response { id, result }) => self.on_response(id, result, &mut step),
            Some(Incoming::Notification { method, params }) => {
                if method == "session/update" {
                    self.on_update(&params, &mut step);
                }
            }
            Some(Incoming::Request { id, method, params }) => {
                self.on_request(&id, &method, &params, &mut step);
            }
            None => {}
        }
        step
    }

    /// Advance the state machine on a response to request `id`.
    fn on_response(&mut self, id: u64, result: Result<Value, RpcError>, step: &mut Step) {
        let Some(kind) = self.pending.remove(&id) else {
            return;
        };
        let value = match result {
            Ok(v) => v,
            // Any error to `session/load` (not just "not found") falls back to
            // a fresh session so a stale saved id never strands the user.
            Err(_) if matches!(kind, Pending::LoadSession) => {
                // The agent no longer knows that session: start a fresh one.
                self.session_id = None;
                self.resume = None;
                let params = json!({"cwd": self.cwd, "mcpServers": []});
                step.out
                    .push(self.send("session/new", &params, Pending::NewSession));
                return;
            }
            Err(e) => {
                // A failed prompt must free the turn, else the UI would wait
                // forever for a `TurnEnded` that will not come.
                if matches!(kind, Pending::Prompt) {
                    self.phase = Phase::Ready;
                }
                step.events.push(Event::Failed(e.message));
                return;
            }
        };
        match kind {
            Pending::Initialize => {
                self.phase = Phase::CreatingSession;
                let can_load = value
                    .pointer("/agentCapabilities/loadSession")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let params = json!({"cwd": self.cwd, "mcpServers": []});
                match self.resume.clone().filter(|_| can_load) {
                    Some(sid) => {
                        // Set before the reply: the agent replays history as
                        // `session/update`s first, and those carry this id.
                        self.session_id = Some(sid.clone());
                        let params = json!({
                            "sessionId": sid, "cwd": self.cwd, "mcpServers": []
                        });
                        step.out
                            .push(self.send("session/load", &params, Pending::LoadSession));
                    }
                    None => step
                        .out
                        .push(self.send("session/new", &params, Pending::NewSession)),
                }
            }
            Pending::LoadSession => {
                // The session id was already set when `session/load` was sent.
                self.resumed = true;
                self.phase = Phase::Ready;
                let sid = self.session_id.clone().unwrap_or_default();
                step.events.push(Event::Ready { session_id: sid });
            }
            Pending::NewSession => {
                if let Some(sid) = value.get("sessionId").and_then(Value::as_str) {
                    self.session_id = Some(sid.to_string());
                    self.phase = Phase::Ready;
                    step.events.push(Event::Ready {
                        session_id: sid.to_string(),
                    });
                } else {
                    step.events
                        .push(Event::Failed("session/new returned no sessionId".into()));
                }
            }
            Pending::Prompt => {
                self.phase = Phase::Ready;
                let stop_reason = value
                    .get("stopReason")
                    .and_then(Value::as_str)
                    // ACP requires `stopReason`; tolerate an agent that omits it.
                    .unwrap_or("end_turn")
                    .to_string();
                step.events.push(Event::TurnEnded { stop_reason });
            }
        }
    }

    /// Whether `params.sessionId` names our session. Anything else (including
    /// no session yet) is untrusted and must not produce events.
    fn session_matches(&self, params: &Value) -> bool {
        match (
            &self.session_id,
            params.get("sessionId").and_then(Value::as_str),
        ) {
            (Some(ours), Some(theirs)) => ours == theirs,
            _ => false,
        }
    }

    /// Translate a `session/update` notification into an [`Event`].
    /// Unrecognised `sessionUpdate` kinds are skipped, so newer agents that add
    /// kinds do not break older clients.
    fn on_update(&self, params: &Value, step: &mut Step) {
        if !self.session_matches(params) {
            return;
        }
        let Some(update) = params.get("update") else {
            return;
        };
        let text = || {
            update
                .pointer("/content/text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let field = |k: &str| {
            update
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        match update.get("sessionUpdate").and_then(Value::as_str) {
            Some("agent_message_chunk") => step.events.push(Event::AgentText(text())),
            Some("agent_thought_chunk") => step.events.push(Event::Thought(text())),
            Some("tool_call" | "tool_call_update") => step.events.push(Event::ToolCall {
                id: field("toolCallId"),
                title: field("title"),
                status: field("status"),
            }),
            Some("plan") => {
                let entries = update
                    .get("entries")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .map(|e| PlanEntry {
                                content: e
                                    .get("content")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                status: e
                                    .get("status")
                                    .and_then(Value::as_str)
                                    .unwrap_or("pending")
                                    .to_string(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                step.events.push(Event::Plan(entries));
            }
            _ => {}
        }
    }

    /// Handle a request from the agent. Unsupported methods get `-32601`,
    /// foreign sessions and malformed params `-32602`; valid ones become events
    /// the host must answer via the `answer_*` functions.
    fn on_request(&self, id: &Value, method: &str, params: &Value, step: &mut Step) {
        let known = matches!(
            method,
            "session/request_permission" | "fs/read_text_file" | "fs/write_text_file"
        );
        if !known {
            step.out
                .push(wire::error_response(id, -32601, "method not found"));
            return;
        }
        if !self.session_matches(params) {
            step.out
                .push(wire::error_response(id, -32602, "unknown session"));
            return;
        }
        let s = |k: &str| params.get(k).and_then(Value::as_str).map(str::to_string);
        match method {
            "session/request_permission" => {
                step.events.push(Event::Permission {
                    req_id: id.clone(),
                    title: params
                        .pointer("/toolCall/title")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    options: parse_options(params),
                });
            }
            "fs/read_text_file" => match s("path") {
                Some(path) => step.events.push(Event::ReadFile {
                    req_id: id.clone(),
                    path,
                    line: params.get("line").and_then(Value::as_u64),
                    limit: params.get("limit").and_then(Value::as_u64),
                }),
                None => step
                    .out
                    .push(wire::error_response(id, -32602, "missing path")),
            },
            _ => match (s("path"), s("content")) {
                (Some(path), Some(content)) => step.events.push(Event::WriteFile {
                    req_id: id.clone(),
                    path,
                    content,
                }),
                _ => step
                    .out
                    .push(wire::error_response(id, -32602, "missing path or content")),
            },
        }
    }
}

/// Parse the `options` array of a permission request, dropping entries with an
/// unknown `kind` or missing `optionId` so the host never offers a choice it
/// cannot classify.
fn parse_options(params: &Value) -> Vec<PermOption> {
    params
        .get("options")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|o| {
                    let kind = match o.get("kind").and_then(Value::as_str)? {
                        "allow_once" => OptionKind::AllowOnce,
                        "allow_always" => OptionKind::AllowAlways,
                        "reject_once" => OptionKind::RejectOnce,
                        "reject_always" => OptionKind::RejectAlways,
                        _ => return None,
                    };
                    Some(PermOption {
                        id: o.get("optionId").and_then(Value::as_str)?.to_string(),
                        name: o
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        kind,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Apply ACP's `line`/`limit` (1-based first line, max line count) to `text`.
///
/// Lines keep their terminators (`split_inclusive`). Absent both parameters the
/// text is returned whole; a `line` of `0` is treated like `1`.
///
/// # Examples
///
/// ```
/// use vix_acp::client::slice_lines;
/// assert_eq!(slice_lines("a\nb\nc\n", Some(2), Some(1)), "b\n");
/// assert_eq!(slice_lines("a\nb\n", None, None), "a\nb\n");
/// ```
#[must_use]
pub fn slice_lines(text: &str, line: Option<u64>, limit: Option<u64>) -> String {
    if line.is_none() && limit.is_none() {
        return text.to_string();
    }
    let skip = usize::try_from(line.unwrap_or(1).saturating_sub(1)).unwrap_or(usize::MAX);
    let take = limit.map_or(usize::MAX, |l| usize::try_from(l).unwrap_or(usize::MAX));
    text.split_inclusive('\n').skip(skip).take(take).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client driven through initialize and session/new.
    fn ready() -> Client {
        let mut c = Client::new("/w");
        let out = c.start();
        assert_eq!(out[0]["method"], "initialize");
        assert_eq!(out[0]["params"]["protocolVersion"], 1);
        let s = c.handle(&json!({"id":1,"result":{"protocolVersion":1}}));
        assert_eq!(s.out[0]["method"], "session/new");
        assert_eq!(s.out[0]["params"]["cwd"], "/w");
        let s = c.handle(&json!({"id":2,"result":{"sessionId":"S"}}));
        assert_eq!(
            s.events,
            vec![Event::Ready {
                session_id: "S".into()
            }]
        );
        assert_eq!(c.phase(), Phase::Ready);
        c
    }

    #[test]
    fn resume_loads_the_session_when_the_agent_can() {
        let mut c = Client::resuming("/w", "OLD");
        c.start();
        let s = c.handle(&json!({"id":1,"result":{"agentCapabilities":{"loadSession":true}}}));
        assert_eq!(s.out[0]["method"], "session/load");
        assert_eq!(s.out[0]["params"]["sessionId"], "OLD");
        let s = c.handle(&json!({"id":2,"result":null}));
        assert_eq!(
            s.events,
            vec![Event::Ready {
                session_id: "OLD".into()
            }]
        );
        assert!(c.resumed());
        assert_eq!(c.phase(), Phase::Ready);
    }

    #[test]
    fn resume_without_the_capability_starts_a_new_session() {
        let mut c = Client::resuming("/w", "OLD");
        c.start();
        let s = c.handle(&json!({"id":1,"result":{"protocolVersion":1}}));
        assert_eq!(s.out[0]["method"], "session/new");
        c.handle(&json!({"id":2,"result":{"sessionId":"NEW"}}));
        assert!(!c.resumed());
        assert_eq!(c.session_id(), Some("NEW"));
    }

    #[test]
    fn a_refused_load_falls_back_to_a_new_session() {
        let mut c = Client::resuming("/w", "OLD");
        c.start();
        c.handle(&json!({"id":1,"result":{"agentCapabilities":{"loadSession":true}}}));
        let s = c.handle(&json!({"id":2,"error":{"code":-32602,"message":"unknown"}}));
        assert_eq!(s.events.len(), 0);
        assert_eq!(s.out[0]["method"], "session/new");
        c.handle(&json!({"id":3,"result":{"sessionId":"NEW"}}));
        assert!(!c.resumed());
        assert_eq!(c.session_id(), Some("NEW"));
    }

    fn update(u: &Value) -> Value {
        json!({"method":"session/update","params":{"sessionId":"S","update":u}})
    }

    #[test]
    fn prompt_turn_streams_updates_then_ends() {
        let mut c = ready();
        let p = c.prompt("hi").unwrap();
        assert_eq!(p["params"]["prompt"][0]["text"], "hi");
        assert_eq!(c.phase(), Phase::Prompting);
        assert!(c.prompt("again").is_none());
        let s = c.handle(&update(&json!({
            "sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"yo"}})));
        assert_eq!(s.events, vec![Event::AgentText("yo".into())]);
        let s = c.handle(&update(&json!({
            "sessionUpdate":"tool_call","toolCallId":"t1","title":"Edit","status":"pending"})));
        assert_eq!(
            s.events,
            vec![Event::ToolCall {
                id: "t1".into(),
                title: "Edit".into(),
                status: "pending".into()
            }]
        );
        let s = c.handle(&update(&json!({
            "sessionUpdate":"plan","entries":[{"content":"a","status":"pending","priority":"high"}]})));
        assert_eq!(
            s.events,
            vec![Event::Plan(vec![PlanEntry {
                content: "a".into(),
                status: "pending".into()
            }])]
        );
        let s = c.handle(&json!({"id":3,"result":{"stopReason":"end_turn"}}));
        assert_eq!(
            s.events,
            vec![Event::TurnEnded {
                stop_reason: "end_turn".into()
            }]
        );
        assert_eq!(c.phase(), Phase::Ready);
    }

    #[test]
    fn updates_for_another_session_are_ignored() {
        let mut c = ready();
        let s = c.handle(&json!({"method":"session/update","params":{"sessionId":"X",
            "update":{"sessionUpdate":"agent_message_chunk","content":{"text":"no"}}}}));
        assert_eq!(s.events.len(), 0);
    }

    #[test]
    fn unknown_agent_request_gets_method_not_found() {
        let mut c = ready();
        let s = c.handle(&json!({"id":"r","method":"terminal/create","params":{}}));
        assert_eq!(s.out[0]["error"]["code"], -32601);
        assert_eq!(s.out[0]["id"], "r");
    }

    #[test]
    fn request_for_another_session_is_refused() {
        let mut c = ready();
        let s = c.handle(&json!({"id":9,"method":"fs/write_text_file",
            "params":{"sessionId":"X","path":"/w/a","content":"c"}}));
        assert_eq!(s.events.len(), 0);
        assert_eq!(s.out[0]["error"]["code"], -32602);
    }

    #[test]
    fn permission_round_trip_and_unoffered_id_cancels() {
        let mut c = ready();
        let s = c.handle(
            &json!({"id":7,"method":"session/request_permission","params":{
            "sessionId":"S","toolCall":{"toolCallId":"t","title":"rm"},
            "options":[{"optionId":"y","name":"Yes","kind":"allow_once"},
                       {"optionId":"n","name":"No","kind":"reject_once"}]}}),
        );
        let Event::Permission {
            req_id,
            title,
            options,
        } = &s.events[0]
        else {
            panic!("{:?}", s.events)
        };
        assert_eq!(title, "rm");
        assert_eq!(options.len(), 2);
        let a = Client::answer_permission(req_id, options, Some("y"));
        assert_eq!(
            a["result"]["outcome"],
            json!({"outcome":"selected","optionId":"y"})
        );
        let a = Client::answer_permission(req_id, options, Some("zzz"));
        assert_eq!(a["result"]["outcome"], json!({"outcome":"cancelled"}));
        let a = Client::answer_permission(req_id, options, None);
        assert_eq!(a["result"]["outcome"]["outcome"], "cancelled");
    }

    #[test]
    fn file_requests_surface_as_events_and_answers_have_the_right_shape() {
        let mut c = ready();
        let s = c.handle(&json!({"id":4,"method":"fs/read_text_file",
            "params":{"sessionId":"S","path":"/w/a","line":2,"limit":1}}));
        assert_eq!(
            s.events,
            vec![Event::ReadFile {
                req_id: json!(4),
                path: "/w/a".into(),
                line: Some(2),
                limit: Some(1)
            }]
        );
        assert_eq!(
            Client::answer_read(&json!(4), Ok("x"))["result"]["content"],
            "x"
        );
        assert!(
            Client::answer_read(&json!(4), Err("no"))
                .get("error")
                .is_some()
        );
        let s = c.handle(&json!({"id":5,"method":"fs/write_text_file",
            "params":{"sessionId":"S","path":"/w/a","content":"new"}}));
        assert!(matches!(&s.events[0], Event::WriteFile { content, .. } if content == "new"));
        assert!(Client::answer_write(&json!(5), Ok(()))["result"].is_null());
    }

    #[test]
    fn errors_surface_and_a_failed_prompt_frees_the_session() {
        let mut c = ready();
        c.prompt("x").unwrap();
        let s = c.handle(&json!({"id":3,"error":{"code":-1,"message":"boom"}}));
        assert_eq!(s.events, vec![Event::Failed("boom".into())]);
        assert_eq!(c.phase(), Phase::Ready);
        assert!(c.cancel().is_none());
    }

    #[test]
    fn cancel_only_while_prompting() {
        let mut c = ready();
        assert!(c.cancel().is_none());
        c.prompt("x").unwrap();
        assert_eq!(c.cancel().unwrap()["method"], "session/cancel");
    }

    #[test]
    fn slice_lines_is_one_based() {
        let t = "a\nb\nc\n";
        assert_eq!(slice_lines(t, None, None), t);
        assert_eq!(slice_lines(t, Some(2), None), "b\nc\n");
        assert_eq!(slice_lines(t, Some(2), Some(1)), "b\n");
        assert_eq!(slice_lines(t, None, Some(1)), "a\n");
        assert_eq!(slice_lines(t, Some(0), Some(1)), "a\n");
    }
}
