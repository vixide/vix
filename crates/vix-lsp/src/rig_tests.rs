//! A harness for driving [`Lsp`] with no real language server.
//!
//! A [`Rig`] installs a hand-built [`Server`] whose "process" is an idle
//! `sleep` (never spoken to) and whose two channels are held by the test:
//! everything the client writes arrives on `out` as framed bytes, decoded
//! back to JSON by [`Rig::sent`], and everything the "server" says is pushed
//! through `tx` and delivered by [`Lsp::poll`]. Nothing is timing dependent.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};

use serde_json::{Value, json};
use vix_lsp_core::file_ops;
use vix_lsp_core::sync::SyncKind;
use vix_lsp_core::{Encoding, frame, message};

use super::{
    INITIALIZE_ID, Incoming, LensSupport, Lsp, LspEvent, Server, ServerConfig, path_to_uri,
};
use std::path::{Path, PathBuf};

/// The language id and extension the rig serves.
pub(crate) const LANG: &str = "rs";

/// The file the rig opens.
pub(crate) fn file() -> PathBuf {
    PathBuf::from("/proj/a.rs")
}

/// A client wired to one scripted server.
pub(crate) struct Rig {
    pub(crate) lsp: Lsp,
    out: Receiver<Vec<u8>>,
    pub(crate) tx: Sender<Incoming>,
    decoder: frame::Decoder,
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.lsp.shutdown();
    }
}

fn blank_server(wtx: Sender<Vec<u8>>, rx: Receiver<Incoming>, ready: bool) -> Server {
    let child = Command::new("sleep")
        .arg("60")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sleep");
    Server {
        child,
        writer: wtx,
        rx,
        next_id: INITIALIZE_ID + 1,
        pending: HashMap::new(),
        targets: HashMap::new(),
        docs: HashMap::new(),
        texts: HashMap::new(),
        sync: SyncKind::Full,
        semantic_support: message::SemanticTokensSupport::default(),
        semantic_results: HashMap::new(),
        encoding: Encoding::Utf16,
        semantic_tokens_legend: Vec::new(),
        completion_triggers: Vec::new(),
        on_type_triggers: Vec::new(),
        workspace_folders_change_support: false,
        code_lens: LensSupport::None,
        execute_commands: Vec::new(),
        diagnostic_provider: false,
        diagnostic_result_ids: HashMap::new(),
        file_ops: file_ops::FileOpCaps::default(),
        watchers: HashMap::new(),
        ready,
        queue: Vec::new(),
        ready_since: ready.then(std::time::Instant::now),
        capabilities: Vec::new(),
        progress: Vec::new(),
    }
}

impl Rig {
    /// A rig whose server has (`ready`) or has not yet finished `initialize`.
    pub(crate) fn new(ready: bool) -> Self {
        let config = ServerConfig {
            language_id: LANG.into(),
            extensions: vec!["rs".into()],
            command: vec!["sleep".into(), "60".into()],
        };
        let mut lsp = Lsp::new(true, vec![config], &[PathBuf::from("/proj")]);
        let (wtx, out) = channel();
        let (tx, rx) = channel();
        lsp.servers
            .insert(LANG.into(), blank_server(wtx, rx, ready));
        Rig {
            lsp,
            out,
            tx,
            decoder: frame::Decoder::new(),
        }
    }

    /// A ready rig with `/proj/a.rs` open and its `didOpen` already read.
    pub(crate) fn open() -> Self {
        let mut rig = Rig::new(true);
        rig.lsp.did_open(&file(), "fn main() {}\n");
        assert_eq!(rig.methods(), ["textDocument/didOpen"]);
        rig
    }

    /// The server under test.
    pub(crate) fn server(&mut self) -> &mut Server {
        self.lsp.servers.get_mut(LANG).expect("rig server")
    }

    /// Everything written to the server since the last call.
    pub(crate) fn sent(&mut self) -> Vec<Value> {
        while let Ok(bytes) = self.out.try_recv() {
            self.decoder.push(&bytes);
        }
        let mut all = Vec::new();
        while let Some(v) = self.decoder.pop() {
            all.push(v);
        }
        all
    }

    /// Assert that nothing was written since the last call.
    pub(crate) fn quiet(&mut self, why: &str) {
        let sent = self.sent();
        assert_eq!(sent, Vec::<Value>::new(), "{why}");
    }

    /// The `method` of everything written since the last call.
    pub(crate) fn methods(&mut self) -> Vec<String> {
        self.sent()
            .iter()
            .map(|m| m["method"].as_str().unwrap_or("<response>").to_string())
            .collect()
    }

    /// The single request written since the last call: its id and the whole message.
    pub(crate) fn one_request(&mut self, method: &str) -> (i64, Value) {
        let mut sent = self.sent();
        assert_eq!(sent.len(), 1, "expected exactly one message: {sent:?}");
        let msg = sent.remove(0);
        assert_eq!(msg["method"], method);
        (msg["id"].as_i64().expect("a request has an id"), msg)
    }

    /// Deliver `msg` as if the server sent it, and poll.
    pub(crate) fn deliver(&mut self, msg: Value) -> Vec<LspEvent> {
        self.tx.send(Incoming::Message(msg)).expect("server alive");
        self.lsp.poll()
    }

    /// Answer request `id` with `result`.
    pub(crate) fn answer(&mut self, id: i64, result: Value) -> Vec<LspEvent> {
        let mut msg = json!({"jsonrpc": "2.0", "id": id});
        msg["result"] = result;
        self.deliver(msg)
    }

    /// Answer request `id` with a JSON-RPC error.
    pub(crate) fn fail(&mut self, id: i64, message: Option<&str>) -> Vec<LspEvent> {
        let error = message.map_or_else(
            || json!({"code": -1}),
            |m| json!({"code": -1, "message": m}),
        );
        self.deliver(json!({"jsonrpc": "2.0", "id": id, "error": error}))
    }

    /// The server's process dies: its stdout reaches EOF, which the reader
    /// thread reports as [`Incoming::Exited`] (the process is really killed, as
    /// [`Lsp::poll`] reaps it).
    pub(crate) fn crash(&mut self) {
        let (tx, rx) = channel();
        tx.send(Incoming::Exited).expect("receiver is held below");
        let server = self.server();
        let _ = server.child.kill();
        server.rx = rx;
    }

    /// Finish `initialize` with a server answering `result` (the `initialize` result object).
    pub(crate) fn initialize(&mut self, result: Value) -> Vec<LspEvent> {
        self.answer(INITIALIZE_ID, result)
    }
}

/// An LSP range object.
pub(crate) fn rng(l1: u32, c1: u32, l2: u32, c2: u32) -> Value {
    json!({"start": {"line": l1, "character": c1}, "end": {"line": l2, "character": c2}})
}

/// A `Location` in `/proj/<name>`.
pub(crate) fn loc(name: &str, line: u32, character: u32) -> Value {
    json!({"uri": path_to_uri(Path::new("/proj").join(name).as_path()),
           "range": rng(line, character, line, character + 1)})
}

/// A `WorkspaceEdit` replacing `range` in `a.rs` with `text`.
pub(crate) fn edit_a(text: &str) -> Value {
    json!({"changes": {path_to_uri(&file()): [{"range": rng(0, 0, 0, 2), "newText": text}]}})
}
