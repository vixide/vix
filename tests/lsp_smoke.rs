//! End-to-end smoke tests for the LSP client.
//!
//! `mock_server_round_trips_diagnostics_and_hover` drives the client against a
//! tiny Python mock server, exercising the full path — spawn, `initialize`
//! handshake, queued `didOpen` flush, `publishDiagnostics`, and a hover
//! request/response — with no external toolchain (skips if `python3` is absent).
//!
//! `rust_analyzer_publishes_diagnostics_for_a_broken_file` is ignored by default;
//! run it with `cargo test --test lsp_smoke -- --ignored` when rust-analyzer is
//! installed and on PATH.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vix::lsp::{Lsp, LspEvent};
use vix::settings::LspServer;

fn tool_available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// A minimal LSP server: answers `initialize`, publishes one diagnostic for the
/// opened document, and replies to `hover`.
const MOCK_SERVER: &str = r#"
import sys, json

def read_msg():
    headers = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        line = line.decode('ascii').strip()
        if line == '':
            break
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
    if msg is None:
        break
    method = msg.get('method')
    mid = msg.get('id')
    if method == 'initialize':
        send({'jsonrpc':'2.0','id':mid,'result':{'capabilities':{'positionEncoding':'utf-16'}}})
    elif method == 'textDocument/didOpen':
        uri = msg['params']['textDocument']['uri']
        send({'jsonrpc':'2.0','method':'textDocument/publishDiagnostics','params':{
            'uri': uri,
            'diagnostics': [{'range':{'start':{'line':0,'character':0},
                                      'end':{'line':0,'character':3}},
                             'severity':1,'message':'mock error'}]}})
    elif method == 'textDocument/hover':
        send({'jsonrpc':'2.0','id':mid,'result':{'contents':{'kind':'plaintext','value':'mock hover'}}})
    elif method == 'shutdown':
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'exit':
        break
"#;

#[test]
fn mock_server_round_trips_diagnostics_and_hover() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }

    let root = std::env::temp_dir().join(format!("vix-lsp-mock-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mock = root.join("mock_lsp.py");
    std::fs::write(&mock, MOCK_SERVER).unwrap();
    let file = root.join("a.rs");
    std::fs::write(&file, "abc\n").unwrap();

    let cfg = LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec!["python3".into(), mock.to_string_lossy().into_owned()],
    };
    let mut lsp = Lsp::new(true, vec![cfg], &root);
    lsp.did_open(&file, "abc\n");

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut hover: Option<String> = None;
    let mut asked_hover = false;
    while Instant::now() < deadline && hover.is_none() {
        for ev in lsp.poll() {
            if let LspEvent::Hover(text) = ev {
                hover = Some(text);
            }
        }
        if !asked_hover && !lsp.diagnostics_for(&file).is_empty() {
            lsp.request_hover(&file, 0, 0);
            asked_hover = true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    lsp.shutdown();

    let diag_ok = !lsp.diagnostics_for(&file).is_empty();
    let _ = std::fs::remove_dir_all(&root);

    assert!(
        diag_ok,
        "the mock server's diagnostic should reach the client"
    );
    assert_eq!(
        hover.as_deref(),
        Some("mock hover"),
        "hover should round-trip"
    );
}

#[test]
#[ignore = "spawns rust-analyzer; run with --ignored"]
fn rust_analyzer_publishes_diagnostics_for_a_broken_file() {
    if !tool_available("rust-analyzer") {
        eprintln!("rust-analyzer not installed; skipping");
        return;
    }

    let root: PathBuf = std::env::temp_dir().join(format!("vix-lsp-ra-{}", std::process::id()));
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"smoke\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[[bin]]\nname = \"smoke\"\npath = \"src/main.rs\"\n",
    )
    .unwrap();
    let main_rs = src.join("main.rs");
    std::fs::write(
        &main_rs,
        "fn main() {\n    let x: u32 = \"nope\";\n    let _ = x;\n}\n",
    )
    .unwrap();

    let cfg = LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec!["rust-analyzer".into()],
    };
    let mut lsp = Lsp::new(true, vec![cfg], &root);
    let text = std::fs::read_to_string(&main_rs).unwrap();
    lsp.did_open(&main_rs, &text);

    let deadline = Instant::now() + Duration::from_secs(90);
    let mut found = false;
    while Instant::now() < deadline {
        let _ = lsp.poll();
        if !lsp.diagnostics_for(&main_rs).is_empty() {
            found = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        found,
        "rust-analyzer should publish a diagnostic for the broken file"
    );
}
