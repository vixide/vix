//! End-to-end test for T623: `willRenameFiles`, `didRenameFiles`, and
//! `didChangeWatchedFiles` against a tiny Python mock language server
//! (skips if `python3` is absent).

#![cfg(feature = "lsp")]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vix::lsp::{FileEdits, Lsp, LspEvent};
use vix::lsp_core::file_ops::FileChangeType;
use vix::settings::LspServer;

fn tool_available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Offers `willRename`/`didRename` for `*.rs`, registers a `Cargo.toml`
/// watcher dynamically, answers `willRenameFiles` with one edit, and logs
/// every file notification it receives to `argv[1]`.
const MOCK_SERVER_FILE_OPS: &str = r"
import sys, json

log = open(sys.argv[1], 'a')

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
        filt = {'filters': [{'pattern': {'glob': '**/*.rs'}}]}
        send({'jsonrpc':'2.0','id':mid,'result':{'capabilities':{
            'workspace': {'fileOperations': {'willRename': filt, 'didRename': filt}}}}})
        send({'jsonrpc':'2.0','id':900,'method':'client/registerCapability','params':{
            'registrations':[{'id':'w1','method':'workspace/didChangeWatchedFiles',
                'registerOptions':{'watchers':[{'globPattern':'**/Cargo.toml'}]}}]}})
    elif method == 'textDocument/didOpen':
        uri = msg['params']['textDocument']['uri']
        send({'jsonrpc':'2.0','method':'textDocument/publishDiagnostics','params':{
            'uri': uri, 'diagnostics': [{'range':{'start':{'line':0,'character':0},
                'end':{'line':0,'character':1}},'severity':1,'message':'ready'}]}})
    elif method == 'workspace/willRenameFiles':
        f = msg['params']['files'][0]
        log.write('will %s %s\n' % (f['oldUri'], f['newUri'])); log.flush()
        send({'jsonrpc':'2.0','id':mid,'result':{'changes':{
            'file:///imports.rs':[{'range':{'start':{'line':0,'character':0},
                'end':{'line':0,'character':3}},'newText':'use'}]}}})
    elif method == 'workspace/didRenameFiles':
        f = msg['params']['files'][0]
        log.write('did %s %s\n' % (f['oldUri'], f['newUri'])); log.flush()
    elif method == 'workspace/didChangeWatchedFiles':
        for c in msg['params']['changes']:
            log.write('watch %d %s\n' % (c['type'], c['uri'])); log.flush()
    elif method == 'shutdown':
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'exit':
        break
";

/// A rename asks the server for `willRenameFiles` edits, then tells it
/// `didRenameFiles`; a registered `Cargo.toml` watcher gets
/// `didChangeWatchedFiles`, and an unwatched file does not.
#[test]
fn mock_server_gets_rename_and_watched_file_notifications() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("vix-lsp-mock-fileops-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mock = root.join("mock_lsp.py");
    std::fs::write(&mock, MOCK_SERVER_FILE_OPS).unwrap();
    let log = root.join("log.txt");
    let file = root.join("a.rs");
    std::fs::write(&file, "abc\n").unwrap();
    let renamed = root.join("b.rs");

    let cfg = LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec![
            "python3".into(),
            mock.to_string_lossy().into_owned(),
            log.to_string_lossy().into_owned(),
        ],
    };
    let mut lsp = Lsp::new(true, vec![cfg], std::slice::from_ref(&root));
    lsp.did_open(&file, "abc\n");

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut asked = false;
    let mut reply: Option<(Vec<FileEdits>, bool)> = None;
    while Instant::now() < deadline && reply.is_none() {
        for ev in lsp.poll() {
            if let LspEvent::WillRenameFiles { edits, last } = ev {
                reply = Some((edits, last));
            }
        }
        if !asked && !lsp.diagnostics_for(&file).is_empty() {
            assert!(lsp.watches_files(), "the dynamic registration was kept");
            assert!(lsp.will_rename_files(&file, &renamed));
            asked = true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let (edits, last) = reply.expect("willRenameFiles was answered");
    assert!(last);
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].0, PathBuf::from("/imports.rs"));

    lsp.did_rename_files(&file, &renamed);
    lsp.did_change_watched_files(&[
        (root.join("Cargo.toml"), FileChangeType::Changed),
        (root.join("notes.txt"), FileChangeType::Changed),
    ]);
    let mut text = String::new();
    while Instant::now() < deadline && !text.contains("Cargo.toml") {
        text = std::fs::read_to_string(&log).unwrap_or_default();
        std::thread::sleep(Duration::from_millis(20));
    }
    lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);

    assert!(text.contains("will file://"), "{text}");
    assert!(text.contains("did file://"), "{text}");
    assert!(
        text.contains("watch 2 file://") && text.contains("Cargo.toml"),
        "{text}"
    );
    assert!(!text.contains("notes.txt"), "{text}");
    // The rename is also a watched-file create/delete, but this watcher only
    // matches Cargo.toml, so no `watch 1` line appears for b.rs.
    assert!(!text.contains("watch 1"), "{text}");
}
