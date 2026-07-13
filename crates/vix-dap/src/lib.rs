//! Debug Adapter Protocol (DAP) client: drive a debug adapter over stdio.
//!
//! Mirrors the LSP client ([`crate::lsp`]) — same `Content-Length` framing (reused
//! from [`vix_lsp_core::frame`]) over a child process's stdin/stdout, with a
//! background reader thread feeding a channel that the host drains via [`Dap::poll`].
//! DAP messages differ from LSP's JSON-RPC: requests are
//! `{seq, type:"request", command, arguments}`, responses
//! `{type:"response", request_seq, success, command, body}`, and events
//! `{type:"event", event, body}`.
//!
//! One debug session is active at a time. The host owns breakpoints, the UI, and
//! how `DapEvent`s are applied.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use vix_lsp_core::frame;

/// A configured debug adapter (parallels [`crate::settings::LspServer`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugAdapter {
    /// Adapter id sent in `initialize` (e.g. `"codelldb"`, `"debugpy"`).
    pub adapter_id: String,
    /// File extensions (without the dot) this adapter debugs, e.g. `["rs"]`.
    pub extensions: Vec<String>,
    /// Launch command: program then args, e.g. `["codelldb", "--port", "0"]`.
    pub command: Vec<String>,
    /// Extra fields merged into the `launch` request arguments (as TOML → JSON),
    /// e.g. `program`, `cwd`, `args`. `{program}` is replaced with the file path.
    #[serde(default)]
    pub launch: std::collections::BTreeMap<String, String>,
}

/// One stack frame from a `stackTrace` response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Adapter frame id (used to request scopes).
    pub id: i64,
    /// Display name (function).
    pub name: String,
    /// Source file path, if any.
    pub path: Option<String>,
    /// 1-based line, if any.
    pub line: usize,
}

/// One variable from a `variables` response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variable {
    /// Variable name.
    pub name: String,
    /// Rendered value.
    pub value: String,
}

/// Things the host reacts to while debugging.
#[derive(Clone, Debug)]
pub enum DapEvent {
    /// The adapter is initialized and the session is running.
    Running,
    /// Execution stopped (breakpoint/step/pause): `reason` and the thread.
    Stopped {
        /// Why execution stopped (e.g. `"breakpoint"`, `"step"`).
        reason: String,
        /// The stopped thread id.
        thread_id: i64,
    },
    /// Console / program output (category, text).
    Output(String),
    /// A `stackTrace` response with the frames.
    Stack(Vec<Frame>),
    /// A `variables` response for the requested frame's first scope.
    Variables(Vec<Variable>),
    /// A one-off `evaluate` result (for the REPL / watch), with its expression.
    Evaluated {
        /// The expression evaluated.
        expr: String,
        /// The rendered result (or error text).
        result: String,
    },
    /// The session ended.
    Terminated,
}

/// Messages from the reader thread.
enum Incoming {
    Message(Value),
    Exited,
}

/// What a pending request was, so its response can be interpreted.
enum Pending {
    /// `stackTrace` (for the stopped thread).
    StackTrace,
    /// `scopes` for a frame (carries the frame id to follow up with `variables`).
    Scopes,
    /// `variables` for a scope.
    Variables,
    /// `evaluate` (REPL/watch), carrying the source expression.
    Evaluate(String),
    /// Any other request whose response needs no special handling.
    Other,
}

/// One active debug session.
struct Session {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Incoming>,
    seq: i64,
    pending: HashMap<i64, Pending>,
    /// `launch` arguments to send once the adapter emits `initialized`.
    launch_args: Value,
    /// Breakpoints to set on `initialized`: path → 1-based lines.
    breakpoints: HashMap<String, Vec<usize>>,
    /// Whether we have sent the launch (after the `initialized` event).
    configured: bool,
    /// The most recent stopped thread id.
    thread_id: Option<i64>,
    /// Whether the program is currently stopped (vs. running).
    stopped: bool,
}

/// The DAP client: at most one active [`Session`].
pub struct Dap {
    session: Option<Session>,
}

impl Default for Dap {
    fn default() -> Self {
        Self::new()
    }
}

impl Dap {
    /// A client with no active session.
    #[must_use]
    pub fn new() -> Self {
        Dap { session: None }
    }

    /// Whether a debug session is active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.session.is_some()
    }

    /// Whether the program is currently stopped at a location.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.stopped)
    }

    /// Tick faster while a session is running.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.session.is_some()
    }

    /// Launch `adapter` to debug `program_path` with `breakpoints` (path → lines).
    /// Returns whether the adapter spawned.
    pub fn start(
        &mut self,
        adapter: &DebugAdapter,
        program_path: &str,
        breakpoints: HashMap<String, Vec<usize>>,
    ) -> bool {
        self.stop();
        let Some((prog, args)) = adapter.command.split_first() else {
            return false;
        };
        let Ok(mut child) = Command::new(prog)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait(); // reap the just-killed child
            return false;
        };
        let (tx, rx) = channel();
        spawn_reader(stdout, tx);
        let launch_args = build_launch_args(adapter, program_path);
        let mut session = Session {
            child,
            stdin,
            rx,
            seq: 1,
            pending: HashMap::new(),
            launch_args,
            breakpoints,
            configured: false,
            thread_id: None,
            stopped: false,
        };
        // The handshake begins with `initialize`; `launch` follows the
        // `initialized` event (see `poll`).
        let init = json!({
            "clientID": "vix",
            "adapterID": adapter.adapter_id,
            "linesStartAt1": true,
            "columnsStartAt1": true,
            "pathFormat": "path",
            "supportsRunInTerminalRequest": false,
        });
        session.request("initialize", &init, Pending::Other);
        self.session = Some(session);
        true
    }

    /// Terminate the active session (if any).
    pub fn stop(&mut self) {
        if let Some(mut s) = self.session.take() {
            s.request(
                "disconnect",
                &json!({ "terminateDebuggee": true }),
                Pending::Other,
            );
            let _ = s.child.kill();
            let _ = s.child.wait(); // reap so no zombie adapter is left behind
        }
    }

    /// Continue execution.
    pub fn continue_(&mut self) {
        self.thread_command("continue");
    }

    /// Step over (next).
    pub fn step_over(&mut self) {
        self.thread_command("next");
    }

    /// Step into.
    pub fn step_into(&mut self) {
        self.thread_command("stepIn");
    }

    /// Step out.
    pub fn step_out(&mut self) {
        self.thread_command("stepOut");
    }

    /// Pause the running program.
    pub fn pause(&mut self) {
        self.thread_command("pause");
    }

    fn thread_command(&mut self, command: &str) {
        if let Some(s) = self.session.as_mut()
            && let Some(tid) = s.thread_id
        {
            s.stopped = false;
            s.request(command, &json!({ "threadId": tid }), Pending::Other);
        }
    }

    /// Update the breakpoints for `path` (1-based `lines`) on the running adapter.
    pub fn set_breakpoints(&mut self, path: &str, lines: &[usize]) {
        if let Some(s) = self.session.as_mut() {
            s.breakpoints.insert(path.to_string(), lines.to_vec());
            if s.configured {
                s.send_breakpoints(path, lines);
            }
        }
    }

    /// Evaluate `expr` in the top frame (REPL / watch); the result arrives as a
    /// [`DapEvent::Evaluated`].
    pub fn evaluate(&mut self, expr: &str) {
        if let Some(s) = self.session.as_mut() {
            let args = json!({ "expression": expr, "context": "repl" });
            s.request("evaluate", &args, Pending::Evaluate(expr.to_string()));
        }
    }

    /// Drain adapter messages, returning events for the host to apply.
    pub fn poll(&mut self) -> Vec<DapEvent> {
        let mut events = Vec::new();
        let drained: Vec<Incoming> = {
            let Some(s) = self.session.as_ref() else {
                return events;
            };
            let mut v = Vec::new();
            while let Ok(m) = s.rx.try_recv() {
                v.push(m);
            }
            v
        };
        for inc in drained {
            match inc {
                Incoming::Exited => {
                    self.session = None;
                    events.push(DapEvent::Terminated);
                    return events;
                }
                Incoming::Message(msg) => self.handle(&msg, &mut events),
            }
        }
        events
    }

    fn handle(&mut self, msg: &Value, events: &mut Vec<DapEvent>) {
        match msg.get("type").and_then(Value::as_str) {
            Some("event") => self.handle_event(msg, events),
            Some("response") => self.handle_response(msg, events),
            _ => {}
        }
    }

    fn handle_event(&mut self, msg: &Value, events: &mut Vec<DapEvent>) {
        let Some(event) = msg.get("event").and_then(Value::as_str) else {
            return;
        };
        let body = msg.get("body").cloned().unwrap_or(Value::Null);
        match event {
            "initialized" => {
                if let Some(s) = self.session.as_mut() {
                    s.configure();
                }
                events.push(DapEvent::Running);
            }
            "stopped" => {
                let reason = body
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let tid = body.get("threadId").and_then(Value::as_i64).unwrap_or(0);
                if let Some(s) = self.session.as_mut() {
                    s.thread_id = Some(tid);
                    s.stopped = true;
                    s.request(
                        "stackTrace",
                        &json!({ "threadId": tid, "levels": 50 }),
                        Pending::StackTrace,
                    );
                }
                events.push(DapEvent::Stopped {
                    reason,
                    thread_id: tid,
                });
            }
            "output" => {
                if let Some(text) = body.get("output").and_then(Value::as_str) {
                    events.push(DapEvent::Output(text.trim_end_matches('\n').to_string()));
                }
            }
            "terminated" | "exited" => {
                self.stop();
                events.push(DapEvent::Terminated);
            }
            _ => {}
        }
    }

    fn handle_response(&mut self, msg: &Value, events: &mut Vec<DapEvent>) {
        let id = msg.get("request_seq").and_then(Value::as_i64).unwrap_or(-1);
        let body = msg.get("body").cloned().unwrap_or(Value::Null);
        let success = msg.get("success").and_then(Value::as_bool).unwrap_or(false);
        let Some(s) = self.session.as_mut() else {
            return;
        };
        let Some(kind) = s.pending.remove(&id) else {
            return;
        };
        match kind {
            Pending::StackTrace => {
                let frames = parse_frames(&body);
                // Follow up: request scopes for the top frame.
                if let Some(top) = frames.first() {
                    s.request("scopes", &json!({ "frameId": top.id }), Pending::Scopes);
                }
                events.push(DapEvent::Stack(frames));
            }
            Pending::Scopes => {
                if let Some(reference) = first_scope_reference(&body) {
                    s.request(
                        "variables",
                        &json!({ "variablesReference": reference }),
                        Pending::Variables,
                    );
                }
            }
            Pending::Variables => events.push(DapEvent::Variables(parse_variables(&body))),
            Pending::Evaluate(expr) => {
                let result = if success {
                    body.get("result")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string()
                } else {
                    msg.get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("error")
                        .to_string()
                };
                events.push(DapEvent::Evaluated { expr, result });
            }
            Pending::Other => {}
        }
    }
}

impl Session {
    /// Send a DAP request, recording its `Pending` kind by sequence number.
    fn request(&mut self, command: &str, arguments: &Value, pending: Pending) {
        let seq = self.seq;
        self.seq += 1;
        self.pending.insert(seq, pending);
        let msg =
            json!({ "seq": seq, "type": "request", "command": command, "arguments": arguments });
        let _ = self.stdin.write_all(&frame::encode(&msg));
        let _ = self.stdin.flush();
    }

    /// After the `initialized` event: set breakpoints, mark configured, send the
    /// launch request and `configurationDone`.
    fn configure(&mut self) {
        let paths: Vec<(String, Vec<usize>)> = self
            .breakpoints
            .iter()
            .map(|(p, l)| (p.clone(), l.clone()))
            .collect();
        for (path, lines) in paths {
            self.send_breakpoints(&path, &lines);
        }
        self.configured = true;
        let launch = self.launch_args.clone();
        self.request("launch", &launch, Pending::Other);
        self.request("configurationDone", &json!({}), Pending::Other);
    }

    /// Send a `setBreakpoints` request for `path` with 1-based `lines`.
    fn send_breakpoints(&mut self, path: &str, lines: &[usize]) {
        let bps: Vec<Value> = lines.iter().map(|l| json!({ "line": l })).collect();
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let args = json!({
            "source": { "path": path, "name": name },
            "breakpoints": bps,
        });
        self.request("setBreakpoints", &args, Pending::Other);
    }
}

/// Build the `launch` arguments by merging the adapter's `launch` map (values
/// with `{program}` expanded) with the program path.
fn build_launch_args(adapter: &DebugAdapter, program_path: &str) -> Value {
    let mut map = serde_json::Map::new();
    map.insert(
        "program".to_string(),
        Value::String(program_path.to_string()),
    );
    map.insert("stopOnEntry".to_string(), Value::Bool(false));
    for (k, v) in &adapter.launch {
        map.insert(
            k.clone(),
            Value::String(v.replace("{program}", program_path)),
        );
    }
    Value::Object(map)
}

/// Parse a `stackTrace` body into [`Frame`]s.
fn parse_frames(body: &Value) -> Vec<Frame> {
    body.get("stackFrames")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|f| Frame {
                    id: f.get("id").and_then(Value::as_i64).unwrap_or(0),
                    name: f
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    path: f
                        .get("source")
                        .and_then(|s| s.get("path"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    line: usize::try_from(f.get("line").and_then(Value::as_i64).unwrap_or(0))
                        .unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The `variablesReference` of the first scope in a `scopes` body.
fn first_scope_reference(body: &Value) -> Option<i64> {
    body.get("scopes")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|s| s.get("variablesReference"))
        .and_then(Value::as_i64)
        .filter(|r| *r != 0)
}

/// Parse a `variables` body into [`Variable`]s.
fn parse_variables(body: &Value) -> Vec<Variable> {
    body.get("variables")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|v| Variable {
                    name: v
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    value: v
                        .get("value")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Background thread: decode framed DAP messages from `stdout` into `tx`.
fn spawn_reader(mut stdout: std::process::ChildStdout, tx: Sender<Incoming>) {
    std::thread::spawn(move || {
        let mut decoder = frame::Decoder::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(Incoming::Exited);
                    return;
                }
                Ok(n) => {
                    decoder.push(&chunk[..n]);
                    while let Some(msg) = decoder.pop() {
                        if tx.send(Incoming::Message(msg)).is_err() {
                            return;
                        }
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_args_expand_program() {
        let adapter = DebugAdapter {
            adapter_id: "x".into(),
            extensions: vec!["rs".into()],
            command: vec!["dbg".into()],
            launch: [("cwd".to_string(), "{program}.dir".to_string())]
                .into_iter()
                .collect(),
        };
        let args = build_launch_args(&adapter, "/tmp/a.out");
        assert_eq!(args["program"], json!("/tmp/a.out"));
        assert_eq!(args["cwd"], json!("/tmp/a.out.dir"));
    }

    #[test]
    fn parse_stack_and_variables() {
        let body = json!({
            "stackFrames": [
                { "id": 7, "name": "main", "line": 12, "source": { "path": "/x.rs" } },
            ]
        });
        let frames = parse_frames(&body);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].id, 7);
        assert_eq!(frames[0].path.as_deref(), Some("/x.rs"));
        assert_eq!(frames[0].line, 12);

        let vars = parse_variables(&json!({ "variables": [{ "name": "n", "value": "42" }] }));
        assert_eq!(
            vars,
            vec![Variable {
                name: "n".into(),
                value: "42".into()
            }]
        );

        assert_eq!(
            first_scope_reference(&json!({ "scopes": [{ "variablesReference": 3 }] })),
            Some(3)
        );
        assert_eq!(
            first_scope_reference(&json!({ "scopes": [{ "variablesReference": 0 }] })),
            None
        );
    }
}
