//! One native provider turn over newline-delimited JSON on standard I/O.
//! The host owns the subprocess, display sanitization and explicit approval.
//!
//! Vix never changes a provider's permission settings: Codex threads start
//! with the user's own sandbox and approval policy, and Claude Code with the
//! user's own permission mode. Vix only answers the requests the provider
//! decides to ask.
use crate::Provider;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// The request id of the Codex `initialize` request.
const CODEX_INITIALIZE: u64 = 1;
/// The request id of the Codex `thread/start` or `thread/resume` request.
const CODEX_THREAD: u64 = 2;
/// The request id of the Codex `turn/start` request.
const CODEX_TURN: u64 = 3;
/// The request id of the Codex `turn/interrupt` request.
const CODEX_INTERRUPT: u64 = 4;

/// What one line of an approval summary describes. The host chooses the
/// translated label; [`Self::Other`] carries the provider's own key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Field {
    /// The tool the provider wants to use.
    Tool,
    /// A command line to run.
    Command,
    /// The directory a command runs in.
    Directory,
    /// The provider's reason for asking.
    Reason,
    /// A file the operation reads or writes.
    File,
    /// The text an edit replaces.
    Old,
    /// The text an edit puts in its place.
    New,
    /// A whole file's new content.
    Content,
    /// A changed file and its diff.
    Change,
    /// Any other input value, under the provider's key.
    Other(String),
}

/// A single provider request awaiting a user decision.
#[derive(Debug)]
pub struct Approval {
    /// The full request to display before allowing an operation. For a Codex
    /// file change it also holds the item's `changes`, remembered from
    /// `item/started` and `item/fileChange/patchUpdated`.
    pub details: Value,
    reply: ApprovalReply,
}

#[derive(Debug)]
enum ApprovalReply {
    Codex(Value),
    Claude { id: Value, input: Value },
}

impl Approval {
    /// Answer only this request. Claude requires the unchanged input on allow.
    #[must_use]
    pub fn answer(self, allow: bool) -> Value {
        match self.reply {
            ApprovalReply::Codex(id) => {
                json!({"id":id,"result":{"decision": if allow {"accept"} else {"decline"}}})
            }
            ApprovalReply::Claude { id, input } => {
                let decision = if allow {
                    json!({"behavior":"allow","updatedInput":input})
                } else {
                    json!({"behavior":"deny","message":"User declined"})
                };
                json!({"type":"control_response","response":{"subtype":"success","request_id":id,"response":decision}})
            }
        }
    }

    /// The provider's id for this request, to match a cancellation.
    #[must_use]
    pub fn id(&self) -> &Value {
        match &self.reply {
            ApprovalReply::Codex(id) | ApprovalReply::Claude { id, .. } => id,
        }
    }

    /// The request as labeled lines, in display order. Strings are the
    /// provider's own text, unescaped and unsanitized: sanitize before display.
    #[must_use]
    pub fn summary(&self) -> Vec<(Field, String)> {
        match &self.reply {
            ApprovalReply::Codex(_) => codex_summary(&self.details),
            ApprovalReply::Claude { .. } => claude_summary(&self.details),
        }
    }
}

/// Codex: the command, where it runs and why; for a file change, each file
/// and its diff.
fn codex_summary(details: &Value) -> Vec<(Field, String)> {
    let mut out = Vec::new();
    let text = |key: &str| details.get(key).and_then(Value::as_str).map(str::to_owned);
    if let Some(command) = text("command") {
        out.push((Field::Command, command));
    }
    if let Some(cwd) = text("cwd") {
        out.push((Field::Directory, cwd));
    }
    if let Some(reason) = text("reason") {
        out.push((Field::Reason, reason));
    }
    if let Some(root) = text("grantRoot") {
        out.push((Field::Other("grantRoot".into()), root));
    }
    out.extend(
        changes(details.get("changes"))
            .into_iter()
            .map(|c| (Field::Change, c)),
    );
    if out.is_empty() {
        out.push((Field::Other("request".into()), plain(details)));
    }
    out
}

/// Claude: the tool, then its input, with the well-known keys first.
fn claude_summary(details: &Value) -> Vec<(Field, String)> {
    const KNOWN: [(&str, Field); 7] = [
        ("command", Field::Command),
        ("file_path", Field::File),
        ("notebook_path", Field::File),
        ("path", Field::File),
        ("old_string", Field::Old),
        ("new_string", Field::New),
        ("content", Field::Content),
    ];
    let mut out = Vec::new();
    if let Some(tool) = details.get("tool_name").and_then(Value::as_str) {
        out.push((Field::Tool, tool.to_owned()));
    }
    let Some(input) = details.get("input").and_then(Value::as_object) else {
        if let Some(input) = details.get("input") {
            out.push((Field::Other("input".into()), plain(input)));
        }
        return out;
    };
    for (key, field) in &KNOWN {
        if let Some(value) = input.get(*key) {
            out.push((field.clone(), plain(value)));
        }
    }
    for (key, value) in input {
        if !KNOWN.iter().any(|(k, _)| k == key) {
            out.push((Field::Other(key.clone()), plain(value)));
        }
    }
    out
}

/// A string as itself; anything else as indented `key: value` lines whose
/// string values are shown unescaped (real newlines, no quotes or escapes).
fn plain(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| match v {
                Value::Object(_) | Value::Array(_) => format!("{k}:\n{}", indent(&plain(v))),
                Value::String(s) if s.contains('\n') => format!("{k}:\n{}", indent(s)),
                _ => format!("{k}: {}", plain(v)),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Array(items) => items
            .iter()
            .map(|v| format!("- {}", plain(v)))
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    }
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Each Codex `FileUpdateChange` as its path then its diff.
fn changes(list: Option<&Value>) -> Vec<String> {
    list.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|change| {
            let path = change["path"].as_str().unwrap_or_default();
            let moved = change.pointer("/kind/move_path").and_then(Value::as_str);
            let head = moved.map_or_else(|| path.to_owned(), |to| format!("{path} → {to}"));
            match change["diff"].as_str() {
                Some(diff) if !diff.is_empty() => format!("{head}\n{}", diff.trim_end()),
                _ => head,
            }
        })
        .collect()
}

/// The host's next step after a provider message.
#[derive(Debug)]
#[non_exhaustive]
pub enum Event {
    /// Write a protocol message to the provider.
    Send(Value),
    /// Persist the provider-specific resume ID.
    Session(String),
    /// A completed block of assistant text.
    Text(String),
    /// A progress description for a tool operation.
    Tool(String),
    /// Show a decision that belongs to this turn.
    Ask(Approval),
    /// The provider withdrew the request with this id: forget its decision.
    Cancelled(Value),
    /// The provider asked a question Vix cannot answer (Codex user input or
    /// an MCP elicitation); it was declined. The text is the question.
    Declined(String),
    /// The provider finished the turn.
    Done,
    /// The provider stopped the turn early (it was interrupted).
    Interrupted,
    /// The provider refused or failed the turn.
    Failed(String),
}

/// State for one prompt. Each turn gets a fresh subprocess and resumes by ID.
pub struct Turn {
    provider: Provider,
    cwd: String,
    session: Option<String>,
    prompt: String,
    /// The Codex turn id, once `turn/start` answered.
    running_id: Option<String>,
    /// Codex file-change items by item id: their latest `changes`.
    files: HashMap<String, Value>,
}

impl Turn {
    /// Start a new turn, optionally resuming this provider's saved session.
    #[must_use]
    pub fn new(provider: Provider, cwd: String, session: Option<String>, prompt: String) -> Self {
        Self {
            provider,
            cwd,
            session,
            prompt,
            running_id: None,
            files: HashMap::new(),
        }
    }

    /// The installed program and arguments, passed directly to the OS.
    /// Claude Code keeps the user's own permission mode; only its permission
    /// prompts are routed to Vix.
    #[must_use]
    pub fn argv(&self) -> Vec<String> {
        let mut args = vec![self.provider.executable().to_owned()];
        match self.provider {
            Provider::Codex => args.push("app-server".into()),
            Provider::Claude => {
                args.extend(
                    [
                        "-p",
                        "--input-format",
                        "stream-json",
                        "--output-format",
                        "stream-json",
                        "--verbose",
                        "--permission-prompt-tool",
                        "stdio",
                    ]
                    .map(str::to_owned),
                );
                if let Some(id) = &self.session {
                    args.extend(["--resume".into(), id.clone()]);
                }
            }
        }
        args
    }

    /// The first message sent to the subprocess.
    #[must_use]
    pub fn initialize(&self) -> Value {
        match self.provider {
            Provider::Codex => {
                json!({"method":"initialize","id":CODEX_INITIALIZE,"params":{"clientInfo":{"name":"vix","title":"Vix","version":env!("CARGO_PKG_VERSION")}}})
            }
            Provider::Claude => {
                json!({"type":"control_request","request_id":"init","request":{"subtype":"initialize","hooks":{}}})
            }
        }
    }

    /// The message that asks the provider to stop this turn, when there is a
    /// turn to stop: Codex `turn/interrupt` once its turn id is known, Claude
    /// Code's `interrupt` control request.
    #[must_use]
    pub fn interrupt(&self) -> Option<Value> {
        match self.provider {
            Provider::Codex => {
                let (thread, turn) = (self.session.as_ref()?, self.running_id.as_ref()?);
                Some(
                    json!({"method":"turn/interrupt","id":CODEX_INTERRUPT,"params":{"threadId":thread,"turnId":turn}}),
                )
            }
            Provider::Claude => Some(
                json!({"type":"control_request","request_id":"interrupt","request":{"subtype":"interrupt"}}),
            ),
        }
    }

    /// Interpret an incoming message from this turn's subprocess.
    #[must_use]
    pub fn receive(&mut self, msg: &Value) -> Vec<Event> {
        match self.provider {
            Provider::Codex => self.codex(msg),
            Provider::Claude => self.claude(msg),
        }
    }

    fn codex(&mut self, msg: &Value) -> Vec<Event> {
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            return self.codex_response(msg);
        };
        let params = &msg["params"];
        if let Some(id) = msg.get("id") {
            return self.codex_request(id, method, params);
        }
        if !self.owns_codex(params) {
            return vec![];
        }
        let item = &params["item"];
        let kind = item["type"].as_str();
        match method {
            "item/completed" if kind == Some("agentMessage") => item["text"]
                .as_str()
                .map(|s| Event::Text(s.to_owned()))
                .into_iter()
                .collect(),
            "item/started" if kind == Some("commandExecution") => item["command"]
                .as_str()
                .map(|s| Event::Tool(s.to_owned()))
                .into_iter()
                .collect(),
            "item/started" if kind == Some("fileChange") => {
                let Some(id) = item["id"].as_str() else {
                    return vec![];
                };
                self.files.insert(id.to_owned(), item["changes"].clone());
                let shown = changes(item.get("changes"));
                if shown.is_empty() {
                    vec![]
                } else {
                    vec![Event::Tool(shown.join("\n"))]
                }
            }
            "item/fileChange/patchUpdated" => {
                if let Some(id) = params["itemId"].as_str() {
                    self.files.insert(id.to_owned(), params["changes"].clone());
                }
                vec![]
            }
            "turn/started" => {
                if let Some(id) = params.pointer("/turn/id").and_then(Value::as_str) {
                    self.running_id = Some(id.to_owned());
                }
                vec![]
            }
            "turn/completed" => match params.pointer("/turn/status").and_then(Value::as_str) {
                Some("completed") => vec![Event::Done],
                Some("interrupted") => vec![Event::Interrupted],
                status => vec![Event::Failed(
                    params
                        .pointer("/turn/error/message")
                        .and_then(Value::as_str)
                        .or(status)
                        .unwrap_or("failed")
                        .to_owned(),
                )],
            },
            _ => vec![],
        }
    }

    /// A reply to one of our requests.
    fn codex_response(&mut self, msg: &Value) -> Vec<Event> {
        let id = msg.get("id").and_then(Value::as_u64);
        if let Some(error) = msg.get("error") {
            // A turn that ended before the interrupt arrived is not a failure.
            if id == Some(CODEX_INTERRUPT) {
                return vec![];
            }
            return vec![Event::Failed(message(error))];
        }
        let result = &msg["result"];
        match id {
            Some(CODEX_INITIALIZE) => {
                // No sandbox or approval policy: the user's Codex config decides.
                let mut params = Map::from_iter([("cwd".into(), json!(self.cwd))]);
                let method = if let Some(id) = &self.session {
                    params.insert("threadId".into(), json!(id));
                    "thread/resume"
                } else {
                    "thread/start"
                };
                vec![
                    Event::Send(json!({"method":"initialized"})),
                    Event::Send(json!({"method":method,"id":CODEX_THREAD,"params":params})),
                ]
            }
            Some(CODEX_THREAD) => {
                let Some(id) = result.pointer("/thread/id").and_then(Value::as_str) else {
                    return vec![Event::Failed(plain(result))];
                };
                self.session = Some(id.to_owned());
                vec![
                    Event::Session(id.to_owned()),
                    Event::Send(
                        json!({"method":"turn/start","id":CODEX_TURN,"params":{"threadId":id,"input":[{"type":"text","text":self.prompt}]}}),
                    ),
                ]
            }
            Some(CODEX_TURN) => {
                if let Some(id) = result.pointer("/turn/id").and_then(Value::as_str) {
                    self.running_id = Some(id.to_owned());
                }
                vec![]
            }
            _ => vec![],
        }
    }

    /// A request from Codex. Only this thread's requests are considered.
    fn codex_request(&self, id: &Value, method: &str, params: &Value) -> Vec<Event> {
        let unsupported = || {
            vec![Event::Send(
                json!({"id":id,"error":{"code":-32601,"message":"Unsupported request"}}),
            )]
        };
        if !self.owns_codex(params) {
            return unsupported();
        }
        match method {
            "item/commandExecution/requestApproval" => vec![Event::Ask(Approval {
                details: params.clone(),
                reply: ApprovalReply::Codex(id.clone()),
            })],
            "item/fileChange/requestApproval" => {
                let mut details = params.clone();
                if let (Some(map), Some(changes)) = (
                    details.as_object_mut(),
                    params["itemId"].as_str().and_then(|i| self.files.get(i)),
                ) {
                    map.insert("changes".into(), changes.clone());
                }
                vec![Event::Ask(Approval {
                    details,
                    reply: ApprovalReply::Codex(id.clone()),
                })]
            }
            // No answers: the schema's way to leave every question unanswered.
            "item/tool/requestUserInput" => {
                let questions = params["questions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|q| q["question"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                vec![
                    Event::Send(json!({"id":id,"result":{"answers":{}}})),
                    Event::Declined(questions),
                ]
            }
            "mcpServer/elicitation/request" => {
                let text = params["message"]
                    .as_str()
                    .map_or_else(|| plain(params), str::to_owned);
                vec![
                    Event::Send(json!({"id":id,"result":{"action":"decline"}})),
                    Event::Declined(text),
                ]
            }
            _ => unsupported(),
        }
    }

    fn owns_codex(&self, params: &Value) -> bool {
        self.session
            .as_deref()
            .is_some_and(|id| params.get("threadId").and_then(Value::as_str) == Some(id))
    }

    fn claude(&self, msg: &Value) -> Vec<Event> {
        match msg["type"].as_str() {
            Some("control_response")
                if msg.pointer("/response/request_id").and_then(Value::as_str) == Some("init") =>
            {
                if msg.pointer("/response/subtype").and_then(Value::as_str) != Some("success") {
                    return vec![Event::Failed(
                        msg.pointer("/response/error")
                            .map_or_else(|| plain(&msg["response"]), message),
                    )];
                }
                vec![Event::Send(
                    json!({"type":"user","message":{"role":"user","content":self.prompt},"parent_tool_use_id":null,"session_id":self.session.as_deref().unwrap_or_default()}),
                )]
            }
            Some("control_request") => {
                let req = &msg["request"];
                if req["subtype"] == "can_use_tool" {
                    vec![Event::Ask(Approval {
                        details: req.clone(),
                        reply: ApprovalReply::Claude {
                            id: msg["request_id"].clone(),
                            input: req["input"].clone(),
                        },
                    })]
                } else {
                    vec![Event::Send(
                        json!({"type":"control_response","response":{"subtype":"error","request_id":msg["request_id"],"error":"Unsupported request"}}),
                    )]
                }
            }
            Some("control_cancel_request") => vec![Event::Cancelled(msg["request_id"].clone())],
            Some("system") if msg["subtype"] == "init" => msg["session_id"]
                .as_str()
                .map(|s| Event::Session(s.to_owned()))
                .into_iter()
                .collect(),
            Some("assistant") => msg
                .pointer("/message/content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|block| match block["type"].as_str() {
                    Some("text") => block["text"].as_str().map(|s| Event::Text(s.to_owned())),
                    Some("tool_use") => block["name"].as_str().map(|s| Event::Tool(s.to_owned())),
                    _ => None,
                })
                .collect(),
            Some("result") if msg["subtype"] == "success" && msg["is_error"] != true => {
                vec![Event::Done]
            }
            Some("result") => vec![Event::Failed(
                msg.get("result")
                    .or_else(|| msg.get("errors"))
                    .map_or_else(|| plain(&msg["subtype"]), plain),
            )],
            _ => vec![],
        }
    }
}

/// A JSON-RPC error's message, or the whole error when it has none.
fn message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .map_or_else(|| plain(error), str::to_owned)
}

#[cfg(test)]
mod tests;
