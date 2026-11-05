//! Debug Adapter Protocol (DAP) client: drive a debug adapter over stdio.
//!
//! Mirrors the LSP client (`vix-lsp`) — same `Content-Length` framing (reused
//! from [`vix_lsp_core::frame`]) over a child process's stdin/stdout, with a
//! background reader thread feeding a channel that the host drains via [`Dap::poll`].
//! DAP messages differ from LSP's JSON-RPC: requests are
//! `{seq, type:"request", command, arguments}`, responses
//! `{type:"response", request_seq, success, command, body}`, and events
//! `{type:"event", event, body}`.
//!
//! One debug session is active at a time. The host owns breakpoints, the UI, and
//! how `DapEvent`s are applied.
//!
//! # Session flow
//!
//! [`Dap::start`] spawns the adapter and sends `initialize`. When the adapter
//! emits `initialized`, `Dap::configure` (private) sends the breakpoints,
//! exception filters and function breakpoints, then `launch`/`attach`, then
//! `configurationDone` -- the order DAP requires. A `stopped` event triggers
//! `threads` + `stackTrace`; the stack response chains into `scopes`, whose
//! response chains into `variables` for the first cheap scope, so the host gets
//! a full snapshot from one event. Every request is tracked in a private
//! `Pending` map keyed by `seq` so its response can be interpreted.
//!
//! Threading: a reader thread decodes stdout into a channel and a writer
//! thread drains an outgoing channel into stdin, so the UI thread never blocks
//! on a slow adapter in either direction. Stderr is discarded.
//!
//! The `process` feature gates actually spawning an adapter (without it
//! [`Dap::start`] always returns `false`), keeping the crate testable without
//! a child process. [`view`] holds the pure panel model built from the events.
//!
//! Owning spec: `crates/vix-dap/spec/index.md`.

use std::collections::HashMap;
use std::io::{Read, Write};
#[cfg(feature = "process")]
use std::process::Stdio;
use std::process::{Child, ChildStdin};
use std::sync::mpsc::{Receiver, Sender, channel};

use serde_json::{Value, json};

use vix_lsp_core::frame;

pub use vix_settings::DebugAdapter;

pub mod view;

/// One stack frame from a `stackTrace` response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
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

impl Frame {
    /// A frame `id` named `name` at 1-based `line` of `path`.
    #[must_use]
    pub fn new(id: i64, name: impl Into<String>, path: Option<String>, line: usize) -> Self {
        Frame {
            id,
            name: name.into(),
            path,
            line,
        }
    }
}

/// One variable from a `variables` response.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Variable {
    /// Variable name.
    pub name: String,
    /// Rendered value.
    pub value: String,
    /// The reference its children are fetched with (`variables`); 0 when it
    /// has none (a leaf).
    pub variables_reference: i64,
}

impl Variable {
    /// A leaf variable `name` = `value`.
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Variable {
            name: name.into(),
            value: value.into(),
            variables_reference: 0,
        }
    }

    /// The same variable, with children under `reference`.
    #[must_use]
    pub fn with_reference(mut self, reference: i64) -> Self {
        self.variables_reference = reference;
        self
    }
}

/// One scope of a frame from a `scopes` response (T617).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Scope {
    /// Display name (`Locals`, `Globals`, …).
    pub name: String,
    /// The reference its variables are fetched with.
    pub variables_reference: i64,
    /// Whether the adapter says fetching it is expensive (never auto-expanded).
    pub expensive: bool,
}

impl Scope {
    /// A scope `name` whose variables are under `reference`.
    #[must_use]
    pub fn new(name: impl Into<String>, reference: i64) -> Self {
        Scope {
            name: name.into(),
            variables_reference: reference,
            expensive: false,
        }
    }

    /// The same scope, marked expensive.
    #[must_use]
    pub fn expensive(mut self) -> Self {
        self.expensive = true;
        self
    }
}

/// The scope that is expanded automatically when a frame's scopes arrive:
/// the first one that is not `expensive` and has variables (T617). The client
/// fetches its variables unasked, and [`view::DebugView`] shows it expanded,
/// so both use this one rule.
#[must_use]
pub fn auto_expand_scope(scopes: &[Scope]) -> Option<usize> {
    scopes
        .iter()
        .position(|s| !s.expensive && s.variables_reference != 0)
}

/// One thread from a `threads` response (T617).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Thread {
    /// Adapter thread id.
    pub id: i64,
    /// Display name.
    pub name: String,
}

impl Thread {
    /// A thread `id` named `name`.
    #[must_use]
    pub fn new(id: i64, name: impl Into<String>) -> Self {
        Thread {
            id,
            name: name.into(),
        }
    }
}

/// A line breakpoint with its optional condition, hit condition and log
/// message (T617), as sent in `setBreakpoints`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceBreakpoint {
    /// 1-based line.
    pub line: usize,
    /// Expression; the adapter stops only when it is true.
    pub condition: Option<String>,
    /// Hit-count condition (`3`, `>= 5`, …; the syntax is the adapter's).
    pub hit_condition: Option<String>,
    /// Log message: a logpoint prints this instead of stopping.
    pub log_message: Option<String>,
}

impl SourceBreakpoint {
    /// A plain breakpoint on 1-based `line`.
    #[must_use]
    pub fn new(line: usize) -> Self {
        SourceBreakpoint {
            line,
            ..Self::default()
        }
    }

    /// Whether it has no condition, hit condition or log message.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.condition.is_none() && self.hit_condition.is_none() && self.log_message.is_none()
    }

    /// The `SourceBreakpoint` JSON object for `setBreakpoints`.
    fn to_json(&self) -> Value {
        let mut map = serde_json::Map::new();
        map.insert("line".to_string(), json!(self.line));
        for (key, value) in [
            ("condition", &self.condition),
            ("hitCondition", &self.hit_condition),
            ("logMessage", &self.log_message),
        ] {
            if let Some(v) = value {
                map.insert(key.to_string(), Value::String(v.clone()));
            }
        }
        Value::Object(map)
    }
}

/// A function breakpoint (T617): stop when the named function is entered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FunctionBreakpoint {
    /// The function's name, as the adapter resolves it.
    pub name: String,
    /// Expression; the adapter stops only when it is true.
    pub condition: Option<String>,
}

impl FunctionBreakpoint {
    /// A breakpoint on function `name`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        FunctionBreakpoint {
            name: name.into(),
            condition: None,
        }
    }

    /// The `FunctionBreakpoint` JSON object for `setFunctionBreakpoints`.
    fn to_json(&self) -> Value {
        let mut map = serde_json::Map::new();
        map.insert("name".to_string(), Value::String(self.name.clone()));
        if let Some(c) = &self.condition {
            map.insert("condition".to_string(), Value::String(c.clone()));
        }
        Value::Object(map)
    }
}

/// A data breakpoint (T617): stop when the memory behind a variable is read
/// or written. Its [`data_id`](Self::data_id) comes from the adapter's reply
/// to `dataBreakpointInfo`, is meaningful only for the program run it was
/// asked in, and so these are never saved across sessions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct DataBreakpoint {
    /// The adapter's opaque id for the watched data.
    pub data_id: String,
    /// What the user sees: the variable's name, or the adapter's description.
    pub description: String,
    /// `read`, `write` or `readWrite`; `None` leaves the adapter's default.
    pub access_type: Option<String>,
    /// Expression; the adapter stops only when it is true.
    pub condition: Option<String>,
}

impl DataBreakpoint {
    /// A breakpoint on `data_id`, shown as `description`.
    #[must_use]
    pub fn new(data_id: impl Into<String>, description: impl Into<String>) -> Self {
        DataBreakpoint {
            data_id: data_id.into(),
            description: description.into(),
            access_type: None,
            condition: None,
        }
    }

    /// The `DataBreakpoint` JSON object for `setDataBreakpoints`.
    fn to_json(&self) -> Value {
        let mut map = serde_json::Map::new();
        map.insert("dataId".to_string(), Value::String(self.data_id.clone()));
        for (key, value) in [
            ("accessType", &self.access_type),
            ("condition", &self.condition),
        ] {
            if let Some(v) = value {
                map.insert(key.to_string(), Value::String(v.clone()));
            }
        }
        Value::Object(map)
    }
}

/// An instruction breakpoint (T617): stop when execution reaches the
/// instruction at a memory address.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct InstructionBreakpoint {
    /// The memory reference of the instruction (typically an address such as
    /// `0x4005d0`, as the adapter spells them).
    pub reference: String,
    /// Expression; the adapter stops only when it is true.
    pub condition: Option<String>,
    /// Hit-count condition (the syntax is the adapter's).
    pub hit_condition: Option<String>,
}

impl InstructionBreakpoint {
    /// A breakpoint on the instruction at memory `reference`.
    #[must_use]
    pub fn new(reference: impl Into<String>) -> Self {
        InstructionBreakpoint {
            reference: reference.into(),
            condition: None,
            hit_condition: None,
        }
    }

    /// The `InstructionBreakpoint` JSON object for `setInstructionBreakpoints`.
    fn to_json(&self) -> Value {
        let mut map = serde_json::Map::new();
        map.insert(
            "instructionReference".to_string(),
            Value::String(self.reference.clone()),
        );
        for (key, value) in [
            ("condition", &self.condition),
            ("hitCondition", &self.hit_condition),
        ] {
            if let Some(v) = value {
                map.insert(key.to_string(), Value::String(v.clone()));
            }
        }
        Value::Object(map)
    }
}

/// One exception filter an adapter advertises in `initialize`
/// (`exceptionBreakpointFilters`, T617).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExceptionFilter {
    /// Filter id sent in `setExceptionBreakpoints`.
    pub filter: String,
    /// The adapter's display label.
    pub label: String,
    /// Whether it is on until the user chooses.
    pub default: bool,
}

impl ExceptionFilter {
    /// A filter `filter` labelled `label`, on by default when `default`.
    #[must_use]
    pub fn new(filter: impl Into<String>, label: impl Into<String>, default: bool) -> Self {
        ExceptionFilter {
            filter: filter.into(),
            label: label.into(),
            default,
        }
    }
}

/// What an adapter said it supports in its `initialize` response (T617).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Capabilities {
    /// Exception filters, in the adapter's order.
    pub exception_filters: Vec<ExceptionFilter>,
    /// `supportsSetVariable`.
    pub supports_set_variable: bool,
    /// `supportsFunctionBreakpoints`.
    pub supports_function_breakpoints: bool,
    /// `supportsDataBreakpoints` (T617).
    pub supports_data_breakpoints: bool,
    /// `supportsInstructionBreakpoints` (T617).
    pub supports_instruction_breakpoints: bool,
}

impl Capabilities {
    /// Parse an `initialize` response body.
    #[must_use]
    pub fn from_json(body: &Value) -> Self {
        let exception_filters = body
            .get("exceptionBreakpointFilters")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| {
                        let filter = f.get("filter").and_then(Value::as_str)?;
                        let label = f.get("label").and_then(Value::as_str).unwrap_or(filter);
                        let default = f.get("default").and_then(Value::as_bool).unwrap_or(false);
                        Some(ExceptionFilter::new(filter, label, default))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Capabilities {
            exception_filters,
            supports_set_variable: body
                .get("supportsSetVariable")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            supports_function_breakpoints: body
                .get("supportsFunctionBreakpoints")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            supports_data_breakpoints: body
                .get("supportsDataBreakpoints")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            supports_instruction_breakpoints: body
                .get("supportsInstructionBreakpoints")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    /// The ids of the filters the adapter marks `default`.
    #[must_use]
    pub fn default_filters(&self) -> Vec<String> {
        self.exception_filters
            .iter()
            .filter(|f| f.default)
            .map(|f| f.filter.clone())
            .collect()
    }
}

/// How a session begins once the adapter is initialized (T617).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum StartMode {
    /// Send `launch` with the adapter's `launch` table and the program.
    #[default]
    Launch,
    /// Send `attach` with the adapter's `attach` table.
    Attach,
}

/// Things the host reacts to while debugging.
#[derive(Clone, Debug)]
#[non_exhaustive]
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
    /// The adapter's `initialize` capabilities (T617).
    Capabilities(Capabilities),
    /// A `threads` response (T617).
    Threads(Vec<Thread>),
    /// A `stackTrace` response with the frames (of the selected thread).
    Stack(Vec<Frame>),
    /// A `scopes` response for `frame_id` (T617).
    Scopes {
        /// The frame the scopes belong to.
        frame_id: i64,
        /// Its scopes, in the adapter's order.
        scopes: Vec<Scope>,
    },
    /// A `variables` response: the children of `reference` (a scope or a
    /// structured variable).
    Variables {
        /// The `variablesReference` that was expanded.
        reference: i64,
        /// Its children.
        variables: Vec<Variable>,
    },
    /// A `setVariable` succeeded: `name` in container `reference` is now
    /// `value` (T617).
    VariableSet {
        /// The container's `variablesReference`.
        reference: i64,
        /// The variable's name.
        name: String,
        /// Its new rendered value.
        value: String,
    },
    /// The answer to a `dataBreakpointInfo` request (T617): whether the
    /// variable `name` can be watched, and how.
    DataBreakpointInfo {
        /// The variable asked about.
        name: String,
        /// The adapter's id for its data, or `None` when it cannot be watched.
        data_id: Option<String>,
        /// The adapter's description of the data (or why it cannot be watched).
        description: String,
        /// The access types offered (`read`, `write`, `readWrite`).
        access_types: Vec<String>,
    },
    /// A one-off `evaluate` result (for the REPL / watch), with its expression.
    Evaluated {
        /// The expression evaluated.
        expr: String,
        /// The rendered result (or error text).
        result: String,
    },
    /// The session ended.
    Terminated,
    /// A request the adapter rejected (e.g. an invalid breakpoint, a step
    /// that isn't valid in the current state): the command name and, when
    /// the adapter provided one, its own reason (Run H, T536 — every request
    /// but `evaluate`, which already reports its own failure inline, used to
    /// have this silently discarded).
    RequestFailed(String),
}

/// Messages from the reader thread.
enum Incoming {
    /// One decoded DAP message (response or event).
    Message(Value),
    /// The adapter's stdout closed (it exited or crashed).
    Exited,
}

/// What a pending request was, so its response can be interpreted.
enum Pending {
    /// `initialize`: its body carries the capabilities.
    Initialize,
    /// `threads`.
    Threads,
    /// `stackTrace` (for the selected thread).
    StackTrace,
    /// `scopes` for a frame (carries the frame id).
    Scopes(i64),
    /// `variables` for a reference.
    Variables(i64),
    /// `setVariable` of `name` in container `reference`.
    SetVariable {
        /// The container's reference.
        reference: i64,
        /// The variable's name.
        name: String,
    },
    /// `dataBreakpointInfo` for the variable of that name.
    DataBreakpointInfo(String),
    /// `evaluate` (REPL/watch), carrying the source expression.
    Evaluate(String),
    /// Any other request whose response needs no special handling on
    /// success; carries its command name so a *failure* can still name
    /// which request it was (Run H, T536).
    Other(String),
}

/// One active debug session.
struct Session {
    /// The adapter process, killed and reaped by [`Dap::stop`].
    child: Child,
    /// Framed messages to write to the adapter's stdin, drained by a dedicated
    /// writer thread so a stalled adapter can never block the UI thread.
    writer: Sender<Vec<u8>>,
    /// Decoded messages from the reader thread, drained by [`Dap::poll`].
    rx: Receiver<Incoming>,
    /// Next request sequence number (DAP `seq`, starts at 1).
    seq: i64,
    /// In-flight requests by `seq`, so a response (`request_seq`) can be
    /// matched to what it answers.
    pending: HashMap<i64, Pending>,
    /// The request (`launch` or `attach`) and its arguments, sent once the
    /// adapter emits `initialized`.
    start_request: (&'static str, Value),
    /// Breakpoints to set on `initialized`: path → breakpoints.
    breakpoints: HashMap<String, Vec<SourceBreakpoint>>,
    /// Function breakpoints to set on `initialized`.
    function_breakpoints: Vec<FunctionBreakpoint>,
    /// Instruction breakpoints to set on `initialized`.
    instruction_breakpoints: Vec<InstructionBreakpoint>,
    /// Data breakpoints (T617): this session only, as their ids are.
    data_breakpoints: Vec<DataBreakpoint>,
    /// Whether we have sent the launch (after the `initialized` event).
    configured: bool,
    /// The selected thread: the most recent stopped one unless the user
    /// picked another.
    thread_id: Option<i64>,
    /// The selected frame (scopes and `evaluate` use it).
    frame_id: Option<i64>,
    /// Whether the program is currently stopped (vs. running).
    stopped: bool,
    /// What the adapter said it supports.
    capabilities: Capabilities,
}

/// The DAP client: at most one active `Session` (private).
pub struct Dap {
    /// The active session, `None` when idle or after the adapter exited.
    session: Option<Session>,
    /// Where the adapter starts under a non-local authority (T716): the
    /// workspace root, rather than the remote home directory.
    workdir: Option<std::path::PathBuf>,
    /// The exception filters the user chose (T617); `None` until they
    /// choose, meaning the adapter's defaults. Kept across sessions.
    exception_choice: Option<Vec<String>>,
    /// The function breakpoints (T617); kept across sessions.
    function_breakpoints: Vec<FunctionBreakpoint>,
    /// The instruction breakpoints (T617); kept across sessions of this run.
    instruction_breakpoints: Vec<InstructionBreakpoint>,
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
        Dap {
            session: None,
            workdir: None,
            exception_choice: None,
            function_breakpoints: Vec::new(),
            instruction_breakpoints: Vec::new(),
        }
    }

    /// The directory a debug adapter starts in when the workspace authority
    /// is not local (T716): the remote workspace root, so a launch without
    /// its own `cwd` runs there. Under the local authority it is ignored and
    /// the adapter inherits Vix's own working directory, as before.
    pub fn set_workdir(&mut self, dir: Option<std::path::PathBuf>) {
        self.workdir = dir;
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

    /// Start `adapter` to debug `program_path` with `breakpoints` (path →
    /// breakpoints), sending `launch` or `attach` per `mode` once it is
    /// initialized. Returns whether the adapter spawned.
    pub fn start(
        &mut self,
        adapter: &DebugAdapter,
        program_path: &str,
        breakpoints: HashMap<String, Vec<SourceBreakpoint>>,
        mode: StartMode,
    ) -> bool {
        self.stop();
        let Some(mut child) = spawn_adapter(adapter, self.workdir.as_deref()) else {
            return false;
        };
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait(); // reap the just-killed child
            return false;
        };
        let (tx, rx) = channel();
        spawn_reader(stdout, tx);
        let (wtx, wrx) = channel::<Vec<u8>>();
        spawn_writer(stdin, wrx);
        let start_request = match mode {
            StartMode::Launch => ("launch", build_launch_args(adapter, program_path)),
            StartMode::Attach => ("attach", build_attach_args(adapter, program_path)),
        };
        let mut session = Session {
            child,
            writer: wtx,
            rx,
            seq: 1,
            pending: HashMap::new(),
            start_request,
            breakpoints,
            function_breakpoints: self.function_breakpoints.clone(),
            instruction_breakpoints: self.instruction_breakpoints.clone(),
            data_breakpoints: Vec::new(),
            configured: false,
            thread_id: None,
            frame_id: None,
            stopped: false,
            capabilities: Capabilities::default(),
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
        session.request("initialize", &init, Pending::Initialize);
        self.session = Some(session);
        true
    }

    /// Terminate the active session (if any).
    pub fn stop(&mut self) {
        if let Some(mut s) = self.session.take() {
            // Best-effort polite `disconnect`; the writer thread may not get to
            // flush it before the kill below, which is fine -- the kill is the
            // guarantee.
            s.request(
                "disconnect",
                &json!({ "terminateDebuggee": true }),
                Pending::Other("disconnect".to_string()),
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

    /// Send a thread-scoped execution command (`continue`, `next`, ...) for the
    /// selected thread. A no-op with no session or no selected thread.
    fn thread_command(&mut self, command: &str) {
        if let Some(s) = self.session.as_mut()
            && let Some(tid) = s.thread_id
        {
            // Optimistically mark running; the next `stopped` event flips it back.
            s.stopped = false;
            s.request(
                command,
                &json!({ "threadId": tid }),
                Pending::Other(command.to_string()),
            );
        }
    }

    /// Update the breakpoints for `path` on the running adapter.
    pub fn set_breakpoints(&mut self, path: &str, breakpoints: &[SourceBreakpoint]) {
        if let Some(s) = self.session.as_mut() {
            s.breakpoints.insert(path.to_string(), breakpoints.to_vec());
            if s.configured {
                s.send_breakpoints(path, breakpoints);
            }
        }
    }

    /// The function breakpoints (T617).
    #[must_use]
    pub fn function_breakpoints(&self) -> &[FunctionBreakpoint] {
        &self.function_breakpoints
    }

    /// Replace the function breakpoints (T617); sent at once to a configured
    /// session whose adapter supports them, and kept for later sessions.
    pub fn set_function_breakpoints(&mut self, breakpoints: Vec<FunctionBreakpoint>) {
        self.function_breakpoints = breakpoints;
        if let Some(s) = self.session.as_mut() {
            s.function_breakpoints
                .clone_from(&self.function_breakpoints);
            if s.configured {
                s.send_function_breakpoints();
            }
        }
    }

    /// The instruction breakpoints (T617).
    #[must_use]
    pub fn instruction_breakpoints(&self) -> &[InstructionBreakpoint] {
        &self.instruction_breakpoints
    }

    /// Replace the instruction breakpoints (`setInstructionBreakpoints`,
    /// T617): sent at once to a configured session, and kept for later
    /// sessions of this run. Returns `false`, changing nothing, when a session
    /// is active whose adapter does not report
    /// `supportsInstructionBreakpoints`; with no session they are only kept,
    /// and each session sends them (to an adapter that supports them) once
    /// it is configured.
    pub fn set_instruction_breakpoints(&mut self, breakpoints: Vec<InstructionBreakpoint>) -> bool {
        if let Some(s) = self.session.as_mut() {
            if !s.capabilities.supports_instruction_breakpoints {
                return false;
            }
            s.instruction_breakpoints.clone_from(&breakpoints);
            if s.configured {
                s.send_instruction_breakpoints();
            }
        }
        self.instruction_breakpoints = breakpoints;
        true
    }

    /// The data breakpoints of the active session (T617); empty when idle.
    #[must_use]
    pub fn data_breakpoints(&self) -> &[DataBreakpoint] {
        self.session
            .as_ref()
            .map_or(&[], |s| s.data_breakpoints.as_slice())
    }

    /// Ask whether the variable `name` in container `reference` can be
    /// watched (`dataBreakpointInfo`, T617); the answer arrives as
    /// [`DapEvent::DataBreakpointInfo`]. Returns `false`, sending nothing,
    /// with no session or when the adapter does not report
    /// `supportsDataBreakpoints`.
    pub fn data_breakpoint_info(&mut self, reference: i64, name: &str) -> bool {
        let Some(s) = self.session.as_mut() else {
            return false;
        };
        if !s.capabilities.supports_data_breakpoints {
            return false;
        }
        s.request(
            "dataBreakpointInfo",
            &json!({ "variablesReference": reference, "name": name }),
            Pending::DataBreakpointInfo(name.to_string()),
        );
        true
    }

    /// Replace the session's data breakpoints (`setDataBreakpoints`, T617),
    /// sent at once when configured. Returns `false`, changing nothing, with
    /// no session or when the adapter does not report
    /// `supportsDataBreakpoints`.
    pub fn set_data_breakpoints(&mut self, breakpoints: Vec<DataBreakpoint>) -> bool {
        let Some(s) = self.session.as_mut() else {
            return false;
        };
        if !s.capabilities.supports_data_breakpoints {
            return false;
        }
        s.data_breakpoints = breakpoints;
        if s.configured {
            s.send_data_breakpoints();
        }
        true
    }

    /// The active session's capabilities, if a session is active.
    #[must_use]
    pub fn capabilities(&self) -> Option<&Capabilities> {
        self.session.as_ref().map(|s| &s.capabilities)
    }

    /// The exception filters in effect (T617): the user's choice, else the
    /// active adapter's defaults (empty with no session and no choice).
    #[must_use]
    pub fn exception_filters(&self) -> Vec<String> {
        match (&self.exception_choice, self.session.as_ref()) {
            (Some(choice), _) => choice.clone(),
            (None, Some(s)) => s.capabilities.default_filters(),
            (None, None) => Vec::new(),
        }
    }

    /// Choose the exception filters (T617); sent at once to a configured
    /// session whose adapter advertised filters, and kept for later sessions.
    pub fn set_exception_filters(&mut self, filters: &[String]) {
        self.exception_choice = Some(filters.to_vec());
        if let Some(s) = self.session.as_mut()
            && s.configured
        {
            s.send_exception_filters(filters);
        }
    }

    /// The selected thread id.
    #[must_use]
    pub fn thread_id(&self) -> Option<i64> {
        self.session.as_ref().and_then(|s| s.thread_id)
    }

    /// The selected frame id.
    #[must_use]
    pub fn frame_id(&self) -> Option<i64> {
        self.session.as_ref().and_then(|s| s.frame_id)
    }

    /// Select thread `id` (T617): step commands act on it and its stack is
    /// fetched (a [`DapEvent::Stack`] follows, then its top frame's scopes).
    pub fn select_thread(&mut self, id: i64) {
        if let Some(s) = self.session.as_mut() {
            s.thread_id = Some(id);
            s.frame_id = None;
            s.request(
                "stackTrace",
                &json!({ "threadId": id, "levels": 50 }),
                Pending::StackTrace,
            );
        }
    }

    /// Select frame `id` (T617): `evaluate` runs in it and its scopes are
    /// fetched (a [`DapEvent::Scopes`] follows, then the first scope's
    /// variables).
    pub fn select_frame(&mut self, id: i64) {
        if let Some(s) = self.session.as_mut() {
            s.frame_id = Some(id);
            s.request("scopes", &json!({ "frameId": id }), Pending::Scopes(id));
        }
    }

    /// Fetch the children of `reference` (lazy tree expansion, T617); they
    /// arrive as [`DapEvent::Variables`].
    pub fn variables(&mut self, reference: i64) {
        if reference == 0 {
            return;
        }
        if let Some(s) = self.session.as_mut() {
            s.request_variables(reference);
        }
    }

    /// Set `name` in container `reference` to `value` (`setVariable`, T617).
    /// Returns `false`, sending nothing, when there is no session or the
    /// adapter does not report `supportsSetVariable`; the result arrives as
    /// [`DapEvent::VariableSet`] (or `RequestFailed`).
    pub fn set_variable(&mut self, reference: i64, name: &str, value: &str) -> bool {
        let Some(s) = self.session.as_mut() else {
            return false;
        };
        if !s.capabilities.supports_set_variable {
            return false;
        }
        s.request(
            "setVariable",
            &json!({ "variablesReference": reference, "name": name, "value": value }),
            Pending::SetVariable {
                reference,
                name: name.to_string(),
            },
        );
        true
    }

    /// Evaluate `expr` in the selected frame (REPL / watch); the result
    /// arrives as a [`DapEvent::Evaluated`].
    pub fn evaluate(&mut self, expr: &str) {
        if let Some(s) = self.session.as_mut() {
            let mut args = json!({ "expression": expr, "context": "repl" });
            if let Some(frame) = s.frame_id {
                args["frameId"] = json!(frame);
            }
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

    /// Dispatch one adapter message by its DAP `type`. Reverse requests from
    /// the adapter (e.g. `runInTerminal`) are ignored: we advertise
    /// `supportsRunInTerminalRequest: false`.
    fn handle(&mut self, msg: &Value, events: &mut Vec<DapEvent>) {
        match msg.get("type").and_then(Value::as_str) {
            Some("event") => self.handle_event(msg, events),
            Some("response") => self.handle_response(msg, events),
            _ => {}
        }
    }

    /// React to an adapter event (`initialized`, `stopped`, `output`, ...).
    fn handle_event(&mut self, msg: &Value, events: &mut Vec<DapEvent>) {
        let Some(event) = msg.get("event").and_then(Value::as_str) else {
            return;
        };
        let body = msg.get("body").cloned().unwrap_or(Value::Null);
        match event {
            "initialized" => {
                let filters = self.exception_filters();
                if let Some(s) = self.session.as_mut() {
                    s.configure(&filters);
                }
                events.push(DapEvent::Running);
            }
            "stopped" => {
                let reason = body
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                // Some adapters omit `threadId` (all-threads-stopped); 0 is then
                // a placeholder rather than a real id.
                let tid = body.get("threadId").and_then(Value::as_i64).unwrap_or(0);
                if let Some(s) = self.session.as_mut() {
                    s.thread_id = Some(tid);
                    s.frame_id = None;
                    s.stopped = true;
                    s.request("threads", &json!({}), Pending::Threads);
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
                // `stop` also kills the adapter so no process outlives its session.
                self.stop();
                events.push(DapEvent::Terminated);
            }
            _ => {}
        }
    }

    /// Match a response to its `Pending` request and turn it into events,
    /// chaining the follow-up requests (stack -> scopes -> variables).
    fn handle_response(&mut self, msg: &Value, events: &mut Vec<DapEvent>) {
        // -1 never matches a real `seq` (they start at 1), so a malformed
        // response is simply dropped below.
        let id = msg.get("request_seq").and_then(Value::as_i64).unwrap_or(-1);
        let body = msg.get("body").cloned().unwrap_or(Value::Null);
        let success = msg.get("success").and_then(Value::as_bool).unwrap_or(false);
        let Some(s) = self.session.as_mut() else {
            return;
        };
        let Some(kind) = s.pending.remove(&id) else {
            return;
        };
        // A failed request is reported uniformly, whatever it was for --
        // except `evaluate`, which already shows its own error text inline
        // in the REPL/watch result below (Run H, T536: every other kind used
        // to just drop the failure here, with no indication anything had
        // gone wrong -- a rejected breakpoint or an invalid step just
        // silently "did nothing").
        if !success && !matches!(kind, Pending::Evaluate(_)) {
            let reason = msg.get("message").and_then(Value::as_str);
            events.push(DapEvent::RequestFailed(failed_request_text(&kind, reason)));
            return;
        }
        match kind {
            Pending::Initialize => {
                s.capabilities = Capabilities::from_json(&body);
                events.push(DapEvent::Capabilities(s.capabilities.clone()));
            }
            Pending::Threads => events.push(DapEvent::Threads(parse_threads(&body))),
            Pending::StackTrace => {
                let frames = parse_frames(&body);
                // Follow up: request scopes for the top frame.
                if let Some(top) = frames.first() {
                    s.frame_id = Some(top.id);
                    s.request(
                        "scopes",
                        &json!({ "frameId": top.id }),
                        Pending::Scopes(top.id),
                    );
                }
                events.push(DapEvent::Stack(frames));
            }
            Pending::Scopes(frame_id) => {
                let scopes = parse_scopes(&body);
                // A reply for a frame the user has since moved off is stale.
                if s.frame_id != Some(frame_id) {
                    return;
                }
                if let Some(i) = auto_expand_scope(&scopes) {
                    s.request_variables(scopes[i].variables_reference);
                }
                events.push(DapEvent::Scopes { frame_id, scopes });
            }
            Pending::Variables(reference) => events.push(DapEvent::Variables {
                reference,
                variables: parse_variables(&body),
            }),
            Pending::SetVariable { reference, name } => {
                let value = body
                    .get("value")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                events.push(DapEvent::VariableSet {
                    reference,
                    name,
                    value,
                });
            }
            Pending::DataBreakpointInfo(name) => {
                let data_id = body
                    .get("dataId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let description = body
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let access_types = body
                    .get("accessTypes")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|t| t.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                events.push(DapEvent::DataBreakpointInfo {
                    name,
                    data_id,
                    description,
                    access_types,
                });
            }
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
            Pending::Other(_) => {}
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
        // Non-blocking hand-off to the writer thread; never block the UI thread
        // on a stalled adapter's stdin pipe.
        let _ = self.writer.send(frame::encode(&msg));
    }

    /// Request the children of `reference`.
    fn request_variables(&mut self, reference: i64) {
        self.request(
            "variables",
            &json!({ "variablesReference": reference }),
            Pending::Variables(reference),
        );
    }

    /// After the `initialized` event: set breakpoints and exception
    /// `filters`, mark configured, send the launch (or attach) request and
    /// `configurationDone`.
    fn configure(&mut self, filters: &[String]) {
        let paths: Vec<(String, Vec<SourceBreakpoint>)> = self
            .breakpoints
            .iter()
            .map(|(p, l)| (p.clone(), l.clone()))
            .collect();
        for (path, bps) in paths {
            self.send_breakpoints(&path, &bps);
        }
        self.send_exception_filters(filters);
        self.send_function_breakpoints();
        self.send_instruction_breakpoints();
        self.configured = true;
        let (command, args) = self.start_request.clone();
        self.request(command, &args, Pending::Other(command.to_string()));
        self.request(
            "configurationDone",
            &json!({}),
            Pending::Other("configurationDone".to_string()),
        );
    }

    /// Send `setExceptionBreakpoints` with `filters` — only to an adapter that
    /// advertised exception filters (others may reject the request). Ids the
    /// adapter does not offer are dropped.
    fn send_exception_filters(&mut self, filters: &[String]) {
        if self.capabilities.exception_filters.is_empty() {
            return;
        }
        let offered: Vec<&String> = filters
            .iter()
            .filter(|f| {
                self.capabilities
                    .exception_filters
                    .iter()
                    .any(|e| &e.filter == *f)
            })
            .collect();
        let args = json!({ "filters": offered });
        self.request(
            "setExceptionBreakpoints",
            &args,
            Pending::Other("setExceptionBreakpoints".to_string()),
        );
    }

    /// Send `setFunctionBreakpoints` — only to an adapter that advertised
    /// `supportsFunctionBreakpoints` (others may reject the request).
    fn send_function_breakpoints(&mut self) {
        if !self.capabilities.supports_function_breakpoints {
            return;
        }
        let bps: Vec<Value> = self
            .function_breakpoints
            .iter()
            .map(FunctionBreakpoint::to_json)
            .collect();
        self.request(
            "setFunctionBreakpoints",
            &json!({ "breakpoints": bps }),
            Pending::Other("setFunctionBreakpoints".to_string()),
        );
    }

    /// Send `setInstructionBreakpoints` — only to an adapter that advertised
    /// `supportsInstructionBreakpoints` (others may reject the request).
    fn send_instruction_breakpoints(&mut self) {
        if !self.capabilities.supports_instruction_breakpoints {
            return;
        }
        let bps: Vec<Value> = self
            .instruction_breakpoints
            .iter()
            .map(InstructionBreakpoint::to_json)
            .collect();
        self.request(
            "setInstructionBreakpoints",
            &json!({ "breakpoints": bps }),
            Pending::Other("setInstructionBreakpoints".to_string()),
        );
    }

    /// Send `setDataBreakpoints` — only to an adapter that advertised
    /// `supportsDataBreakpoints`. Not part of configuration: a session starts
    /// with none, as the ids are only good for one run.
    fn send_data_breakpoints(&mut self) {
        if !self.capabilities.supports_data_breakpoints {
            return;
        }
        let bps: Vec<Value> = self
            .data_breakpoints
            .iter()
            .map(DataBreakpoint::to_json)
            .collect();
        self.request(
            "setDataBreakpoints",
            &json!({ "breakpoints": bps }),
            Pending::Other("setDataBreakpoints".to_string()),
        );
    }

    /// Send a `setBreakpoints` request for `path`.
    fn send_breakpoints(&mut self, path: &str, breakpoints: &[SourceBreakpoint]) {
        let bps: Vec<Value> = breakpoints.iter().map(SourceBreakpoint::to_json).collect();
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        let args = json!({
            "source": { "path": path, "name": name },
            "breakpoints": bps,
        });
        self.request(
            "setBreakpoints",
            &args,
            Pending::Other("setBreakpoints".to_string()),
        );
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
    // The adapter's own table is applied last so it can override both defaults
    // above (`program`, `stopOnEntry`). Values are always strings here; only
    // `attach` coerces types (see `typed_value`).
    for (k, v) in &adapter.launch {
        map.insert(
            k.clone(),
            Value::String(v.replace("{program}", program_path)),
        );
    }
    Value::Object(map)
}

/// Build the `attach` arguments from the adapter's `attach` map, with
/// `{program}` expanded (T617). Nothing else is added: what to attach by is
/// the adapter's business. A value that reads as a JSON number, boolean,
/// object or array is sent as one (`processId = "4242"` sends `4242`), since
/// adapters attach by numeric ids and ports; anything else is a string.
fn build_attach_args(adapter: &DebugAdapter, program_path: &str) -> Value {
    Value::Object(
        adapter
            .attach
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    typed_value(&v.replace("{program}", program_path)),
                )
            })
            .collect(),
    )
}

/// `text` as the JSON number, boolean, object or array it spells, else as a
/// string.
fn typed_value(text: &str) -> Value {
    match serde_json::from_str::<Value>(text.trim()) {
        Ok(v @ (Value::Number(_) | Value::Bool(_) | Value::Object(_) | Value::Array(_))) => v,
        _ => Value::String(text.to_string()),
    }
}

/// Parse a `threads` body into [`Thread`]s.
fn parse_threads(body: &Value) -> Vec<Thread> {
    body.get("threads")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|t| {
                    Thread::new(
                        t.get("id").and_then(Value::as_i64).unwrap_or(0),
                        t.get("name").and_then(Value::as_str).unwrap_or(""),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse a `scopes` body into [`Scope`]s.
fn parse_scopes(body: &Value) -> Vec<Scope> {
    body.get("scopes")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|s| Scope {
                    name: s
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    variables_reference: s
                        .get("variablesReference")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    expensive: s.get("expensive").and_then(Value::as_bool).unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
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
                    variables_reference: v
                        .get("variablesReference")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The text for a failed request's [`DapEvent::RequestFailed`]: the command
/// name, plus the adapter's own `message` field when it gave one (Run H,
/// T536). Extracted as a free function (rather than inlined in
/// `handle_response`) so it's directly testable without a live `Session`.
fn failed_request_text(kind: &Pending, message: Option<&str>) -> String {
    let command = match kind {
        Pending::Other(command) => command.as_str(),
        Pending::Initialize => "initialize",
        Pending::Threads => "threads",
        Pending::StackTrace => "stackTrace",
        Pending::Scopes(_) => "scopes",
        Pending::Variables(_) => "variables",
        Pending::SetVariable { .. } => "setVariable",
        Pending::DataBreakpointInfo(_) => "dataBreakpointInfo",
        // `handle_response` never calls this for `Evaluate` (it reports its
        // own failure inline instead) -- kept here only so the match stays
        // exhaustive if that ever changes.
        Pending::Evaluate(_) => "evaluate",
    };
    match message {
        Some(reason) if !reason.is_empty() => format!("{command}: {reason}"),
        _ => command.to_string(),
    }
}

/// Background thread: drain framed messages off `wrx` and write them to the
/// adapter's stdin, so a stalled adapter can never block the UI thread. Exits
/// when the channel closes (the [`Session`] was dropped) or a write fails.
fn spawn_writer(mut stdin: ChildStdin, wrx: Receiver<Vec<u8>>) {
    std::thread::spawn(move || {
        while let Ok(buf) = wrx.recv() {
            if stdin.write_all(&buf).is_err() || stdin.flush().is_err() {
                return;
            }
        }
    });
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

/// Start the adapter process with piped stdin/stdout.
///
/// Built by the workspace authority (T710): a remote workspace runs its
/// debug adapter where its files are. There (T716) the adapter is first
/// looked up on the remote side (a missing one fails to start, as locally)
/// and starts in `workdir`.
#[cfg(feature = "process")]
fn spawn_adapter(adapter: &DebugAdapter, workdir: Option<&std::path::Path>) -> Option<Child> {
    let (prog, args) = adapter.command.split_first()?;
    let auth = vix_authority::current();
    let mut spec = vix_authority::ProcessSpec::new(prog).args(args);
    if !auth.is_local() {
        if !vix_authority::can_run(&*auth, prog) {
            return None;
        }
        if let Some(dir) = workdir {
            spec = spec.current_dir(dir);
        }
    }
    auth.command(&spec)
        .ok()?
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

/// Without the `process` feature no debug adapter is ever started.
#[cfg(not(feature = "process"))]
fn spawn_adapter(_adapter: &DebugAdapter, _workdir: Option<&std::path::Path>) -> Option<Child> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_request_text_names_the_command_and_the_adapters_reason() {
        // T536 (Run H): every DAP request but `evaluate` used to have its
        // failure silently discarded; this is the text that now reaches the
        // host as `DapEvent::RequestFailed`.
        assert_eq!(
            failed_request_text(&Pending::Other("setBreakpoints".to_string()), None),
            "setBreakpoints",
            "no adapter-provided reason: just the command name"
        );
        assert_eq!(
            failed_request_text(
                &Pending::Other("setBreakpoints".to_string()),
                Some("invalid line 42")
            ),
            "setBreakpoints: invalid line 42",
            "the adapter's own reason is included"
        );
        assert_eq!(
            failed_request_text(&Pending::Other("next".to_string()), Some("")),
            "next",
            "an empty reason string falls back to just the command name"
        );
        assert_eq!(
            failed_request_text(&Pending::StackTrace, None),
            "stackTrace"
        );
        assert_eq!(failed_request_text(&Pending::Scopes(1), None), "scopes");
        assert_eq!(
            failed_request_text(&Pending::Variables(1), None),
            "variables"
        );
    }

    #[test]
    fn launch_args_expand_program() {
        let mut adapter = DebugAdapter::new("x", vec!["rs".into()], vec!["dbg".into()]);
        adapter.launch = [("cwd".to_string(), "{program}.dir".to_string())]
            .into_iter()
            .collect();
        let args = build_launch_args(&adapter, "/tmp/a.out");
        assert_eq!(args["program"], json!("/tmp/a.out"));
        assert_eq!(args["cwd"], json!("/tmp/a.out.dir"));
    }

    #[test]
    fn attach_args_are_the_attach_table_alone() {
        let mut adapter = DebugAdapter::new("x", vec![], vec!["dbg".into()]);
        adapter.launch.insert("cwd".into(), "/w".into());
        adapter.attach.insert("processId".into(), "42".into());
        adapter.attach.insert("name".into(), "{program}".into());
        adapter
            .attach
            .insert("connect".into(), r#"{"port": 5678}"#.into());
        adapter.attach.insert("justMyCode".into(), "false".into());
        adapter.attach.insert("label".into(), "null".into());
        let args = build_attach_args(&adapter, "/a.py");
        assert_eq!(
            args,
            json!({
                "processId": 42,
                "name": "/a.py",
                "connect": { "port": 5678 },
                "justMyCode": false,
                "label": "null",
            })
        );
    }

    #[test]
    fn source_breakpoint_json_carries_only_the_set_fields() {
        assert_eq!(SourceBreakpoint::new(3).to_json(), json!({ "line": 3 }));
        assert!(SourceBreakpoint::new(3).is_plain());
        let bp = SourceBreakpoint {
            line: 4,
            condition: Some("x > 1".into()),
            hit_condition: Some(">= 2".into()),
            log_message: Some("x={x}".into()),
        };
        assert!(!bp.is_plain());
        assert_eq!(
            bp.to_json(),
            json!({ "line": 4, "condition": "x > 1", "hitCondition": ">= 2", "logMessage": "x={x}" })
        );
    }

    #[test]
    fn capabilities_parse_filters_and_flags() {
        let caps = Capabilities::from_json(&json!({
            "supportsSetVariable": true,
            "exceptionBreakpointFilters": [
                { "filter": "raised", "label": "Raised", "default": false },
                { "filter": "uncaught", "label": "Uncaught", "default": true },
                { "label": "no id: dropped" },
                { "filter": "bare" },
            ],
        }));
        assert!(caps.supports_set_variable);
        let ids: Vec<&str> = caps
            .exception_filters
            .iter()
            .map(|f| f.filter.as_str())
            .collect();
        assert_eq!(ids, ["raised", "uncaught", "bare"]);
        assert_eq!(
            caps.exception_filters[2].label, "bare",
            "label falls back to the id"
        );
        assert_eq!(caps.default_filters(), ["uncaught"]);
        assert_eq!(
            Capabilities::from_json(&Value::Null),
            Capabilities::default()
        );
    }

    #[test]
    fn auto_expand_skips_expensive_and_empty_scopes() {
        let scopes = [
            Scope::new("Registers", 1).expensive(),
            Scope::new("Empty", 0),
            Scope::new("Locals", 3),
        ];
        assert_eq!(auto_expand_scope(&scopes), Some(2));
        assert_eq!(auto_expand_scope(&scopes[..2]), None);
        let parsed = parse_scopes(&json!({ "scopes": [
            { "name": "Locals", "variablesReference": 5 },
            { "name": "Globals", "variablesReference": 6, "expensive": true },
        ]}));
        assert_eq!(
            parsed,
            vec![
                Scope::new("Locals", 5),
                Scope::new("Globals", 6).expensive()
            ]
        );
        assert_eq!(
            parse_threads(&json!({ "threads": [{ "id": 1, "name": "main" }] })),
            vec![Thread::new(1, "main")]
        );
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

        let vars = parse_variables(&json!({ "variables": [
            { "name": "n", "value": "42" },
            { "name": "p", "value": "Point", "variablesReference": 9 },
        ] }));
        assert_eq!(
            vars,
            vec![
                Variable::new("n", "42"),
                Variable::new("p", "Point").with_reference(9)
            ]
        );
    }
}
