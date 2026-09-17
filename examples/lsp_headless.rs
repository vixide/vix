//! Spawn a language server, open a document, and print its diagnostics --
//! with no editor UI at all (improvement plan T503). Uses a tiny mock
//! server (same technique as `tests/lsp_smoke.rs`) so this runs anywhere
//! Python 3 is available, with no real language server required.
//!
//! Run with: `cargo run --example lsp_headless`

#![warn(clippy::pedantic)]

use std::time::{Duration, Instant};

use vix::lsp::Lsp;
use vix::settings::LspServer;

/// A minimal LSP server: answers `initialize`, then publishes one
/// diagnostic for whatever document it's handed.
const MOCK_SERVER: &str = r"
import sys, json
def read_msg():
    headers = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line: return None
        line = line.decode('ascii').strip()
        if line == '': break
        k, _, v = line.partition(':')
        headers[k.strip().lower()] = v.strip()
    n = int(headers.get('content-length', '0'))
    return json.loads(sys.stdin.buffer.read(n))
def send(obj):
    data = json.dumps(obj).encode('utf-8')
    sys.stdout.buffer.write(b'Content-Length: %d\r\n\r\n' % len(data))
    sys.stdout.buffer.write(data)
    sys.stdout.buffer.flush()
while True:
    msg = read_msg()
    if msg is None: break
    method, mid = msg.get('method'), msg.get('id')
    if method == 'initialize':
        send({'jsonrpc':'2.0','id':mid,'result':{'capabilities':{'positionEncoding':'utf-16'}}})
    elif method == 'textDocument/didOpen':
        uri = msg['params']['textDocument']['uri']
        send({'jsonrpc':'2.0','method':'textDocument/publishDiagnostics','params':{'uri':uri,
            'diagnostics':[{'range':{'start':{'line':0,'character':0},'end':{'line':0,'character':3}},
                            'severity':1,'message':'headless example: mock diagnostic'}]}})
    elif method == 'shutdown':
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'exit':
        break
";

fn main() {
    let root = std::env::temp_dir().join(format!("vix-lsp-headless-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create scratch dir");
    let server_script = root.join("mock_lsp.py");
    std::fs::write(&server_script, MOCK_SERVER).expect("write mock server");
    let doc = root.join("example.rs");
    std::fs::write(&doc, "abc\n").expect("write scratch doc");

    let cfg = LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec![
            "python3".into(),
            server_script.to_string_lossy().into_owned(),
        ],
    };
    let mut lsp = Lsp::new(true, vec![cfg], std::slice::from_ref(&root));
    lsp.did_open(&doc, "abc\n");

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && lsp.diagnostics_for(&doc).is_empty() {
        lsp.poll(); // drains queued events; diagnostics land via publishDiagnostics
        std::thread::sleep(Duration::from_millis(20));
    }

    for diag in lsp.diagnostics_for(&doc) {
        println!("{:?}: {}", diag.severity, diag.message);
    }
    lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}
