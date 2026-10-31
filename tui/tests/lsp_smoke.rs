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

#![cfg(feature = "lsp")]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vix::app::App;
use vix::lsp::{EditReply, EditReplyKind, Lsp, LspEvent, RenamePrepared};
use vix::settings::{LspServer, Settings};
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

fn tool_available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
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
        send({'jsonrpc':'2.0','id':mid,'result':{'capabilities':{
            'positionEncoding':'utf-16',
            'completionProvider':{'triggerCharacters':['.']},
            'semanticTokensProvider':{'legend':{
                'tokenTypes':['keyword','variable'],'tokenModifiers':[]},
                'full':True}}}})
        send({'jsonrpc':'2.0','method':'$/progress','params':{
            'token':'t1','value':{'kind':'begin','title':'Indexing','percentage':0}}})
    elif method == 'workspace/diagnostic':
        send({'jsonrpc':'2.0','id':mid,'result':{'items':[
            {'uri':'file:///other.rs','kind':'full','items':[
                {'range':{'start':{'line':2,'character':0},'end':{'line':2,'character':5}},
                 'severity':1,'message':'workspace-wide finding'}]}]}})
    elif method == 'textDocument/semanticTokens/full':
        # One token: line 0, char 0, length 3, type 0 ("keyword"), no modifiers.
        send({'jsonrpc':'2.0','id':mid,'result':{'data':[0, 0, 3, 0, 0]}})
    elif method == 'textDocument/didOpen':
        uri = msg['params']['textDocument']['uri']
        send({'jsonrpc':'2.0','method':'textDocument/publishDiagnostics','params':{
            'uri': uri,
            'diagnostics': [{'range':{'start':{'line':0,'character':0},
                                      'end':{'line':0,'character':3}},
                             'severity':1,'message':'mock error'}]}})
    elif method == 'textDocument/hover':
        send({'jsonrpc':'2.0','id':mid,'result':{'contents':{'kind':'plaintext','value':'mock hover'}}})
    elif method == 'textDocument/completion':
        # Echo the request's CompletionContext so a test can check it (T612).
        ctx = msg['params'].get('context', {})
        send({'jsonrpc':'2.0','id':mid,'result':{'isIncomplete':False,'items':[
            {'label':'len','detail':'kind=%s char=%s' % (
                ctx.get('triggerKind'), ctx.get('triggerCharacter'))}]}})
    elif method == 'textDocument/signatureHelp':
        send({'jsonrpc':'2.0','id':mid,'error':{'code':-32603,'message':'mock signature help failure'}})
    elif method == 'textDocument/prepareRename':
        pos = msg['params']['position']
        if pos['character'] == 0:
            send({'jsonrpc':'2.0','id':mid,'result':{
                'range':{'start':pos,'end':pos},'placeholder':'mock_symbol'}})
        else:
            send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'shutdown':
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'exit':
        break
"#;

/// A variant of `MOCK_SERVER` whose `textDocument/signatureHelp` succeeds
/// (with a minimal but real result) instead of erroring, for the T123
/// auto-trigger test -- the shared `MOCK_SERVER` above deliberately errors on
/// that method for `mock_server_error_response_surfaces_as_a_request_failed_event`.
const MOCK_SERVER_SIGNATURE_HELP_OK: &str = r"
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
    elif method == 'textDocument/signatureHelp':
        send({'jsonrpc':'2.0','id':mid,'result':{
            'signatures': [{'label': 'fn f(a: i32)', 'parameters': [{'label': 'a: i32'}]}],
            'activeSignature': 0, 'activeParameter': 0}})
    elif method == 'shutdown':
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'exit':
        break
";

/// A mock server that completes `initialize` and answers exactly one
/// `didOpen` (publishing a diagnostic so the test can tell it got there),
/// then exits immediately -- simulating a crash, deterministically, on
/// *every* launch (including each respawn), for the T134 crash-recovery
/// tests below.
const MOCK_SERVER_CRASHES_AFTER_DID_OPEN: &str = r#"
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
                                      'end':{'line':0,'character':1}},
                             'severity':1,'message':'mock'}]}})
        sys.exit(1)  # "crash" right after handling the first didOpen
"#;

/// T123b: a second, independent mock server for the same extension as
/// `MOCK_SERVER` (e.g. a separate linter alongside a type-checker), so a
/// test can drive both against one file at once. Publishes its own,
/// distinctly-worded diagnostic on `didOpen` -- a different message than
/// `MOCK_SERVER`'s "mock error", so a test can tell whether both reached
/// the client (correct, T123b) or the second clobbered the first (the bug
/// T123b fixed).
const MOCK_SERVER_SECOND_LINTER: &str = r"
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
            'diagnostics': [{'range':{'start':{'line':1,'character':0},
                                      'end':{'line':1,'character':3}},
                             'severity':2,'message':'mock lint warning'}]}})
    elif method == 'shutdown':
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'exit':
        break
";

/// T123f: logs every message with a `method` (as one JSON line each) to the
/// log file path given as `argv[1]`, so the Rust test can inspect exactly
/// what `initialize` and later notifications actually carried -- opts into
/// `workspace/didChangeWorkspaceFolders` via `changeNotifications: true` so
/// the client should send one when a folder is added after startup.
const MOCK_SERVER_LOGS_WORKSPACE_FOLDERS: &str = r"
import sys, json

LOG = sys.argv[1]

def log(entry):
    with open(LOG, 'a') as f:
        f.write(json.dumps(entry) + '\n')

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
    if method:
        log(msg)
    if method == 'initialize':
        send({'jsonrpc':'2.0','id':mid,'result':{'capabilities':{
            'positionEncoding':'utf-16',
            'workspace': {'workspaceFolders': {'supported': True, 'changeNotifications': True}}
        }}})
    elif method == 'textDocument/didOpen':
        uri = msg['params']['textDocument']['uri']
        send({'jsonrpc':'2.0','method':'textDocument/publishDiagnostics','params':{
            'uri': uri,
            'diagnostics': [{'range':{'start':{'line':0,'character':0},
                                      'end':{'line':0,'character':1}},
                             'severity':1,'message':'mock'}]}})
    elif method == 'shutdown':
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'exit':
        break
";

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
    let mut lsp = Lsp::new(true, vec![cfg], std::slice::from_ref(&root));
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

/// T123 audit: a JSON-RPC `error` response (as opposed to a `result`) used to
/// be silently dropped -- a failed request just appeared to do nothing, with
/// no feedback. `LspEvent::RequestFailed` should surface the server's own
/// error message instead.
#[test]
fn mock_server_error_response_surfaces_as_a_request_failed_event() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }

    let root = std::env::temp_dir().join(format!("vix-lsp-mock-error-{}", std::process::id()));
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
    let mut lsp = Lsp::new(true, vec![cfg], std::slice::from_ref(&root));
    lsp.did_open(&file, "abc\n");

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut failure: Option<String> = None;
    let mut asked = false;
    while Instant::now() < deadline && failure.is_none() {
        for ev in lsp.poll() {
            if let LspEvent::RequestFailed(message) = ev {
                failure = Some(message);
            }
        }
        // Wait for the diagnostics from `didOpen` first, same as the hover
        // test -- proof the handshake and doc sync already completed.
        if !asked && !lsp.diagnostics_for(&file).is_empty() {
            lsp.request_signature_help(&file, 0, 0);
            asked = true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(
        failure.as_deref(),
        Some("mock signature help failure"),
        "the server's JSON-RPC error message should reach the client as a RequestFailed event"
    );
}

/// T134 audit: `prepareRename` at a renameable position surfaces the
/// server's own placeholder text, not just a bare "go ahead".
#[test]
fn mock_server_prepare_rename_surfaces_the_servers_placeholder() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("vix-lsp-mock-rename-ok-{}", std::process::id()));
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
    let mut lsp = Lsp::new(true, vec![cfg], std::slice::from_ref(&root));
    lsp.did_open(&file, "abc\n");

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut outcome: Option<RenamePrepared> = None;
    let mut asked = false;
    while Instant::now() < deadline && outcome.is_none() {
        for ev in lsp.poll() {
            if let LspEvent::RenamePrepared(o) = ev {
                outcome = Some(o);
            }
        }
        if !asked && !lsp.diagnostics_for(&file).is_empty() {
            // character 0 -- the mock replies with an explicit placeholder.
            lsp.request_prepare_rename(&file, 0, 0);
            asked = true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(
        outcome,
        Some(RenamePrepared::Placeholder("mock_symbol".to_string()))
    );
}

/// T134 audit: `prepareRename` at a non-renameable position (a bare `null`
/// result) surfaces as `NotRenameable`, not silently nothing.
#[test]
fn mock_server_prepare_rename_null_surfaces_as_not_renameable() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root =
        std::env::temp_dir().join(format!("vix-lsp-mock-rename-null-{}", std::process::id()));
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
    let mut lsp = Lsp::new(true, vec![cfg], std::slice::from_ref(&root));
    lsp.did_open(&file, "abc\n");

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut outcome: Option<RenamePrepared> = None;
    let mut asked = false;
    while Instant::now() < deadline && outcome.is_none() {
        for ev in lsp.poll() {
            if let LspEvent::RenamePrepared(o) = ev {
                outcome = Some(o);
            }
        }
        if !asked && !lsp.diagnostics_for(&file).is_empty() {
            // character 1 -- the mock replies with a bare `null`.
            lsp.request_prepare_rename(&file, 0, 1);
            asked = true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(outcome, Some(RenamePrepared::NotRenameable));
}

/// T123d: `$/progress` (sent unprompted right after `initialize` here)
/// surfaces as an event, and a `workspace/diagnostic` pull (triggered
/// explicitly, unlike push) merges into the same diagnostics map as
/// `publishDiagnostics` -- for a file (`other.rs`) that was never opened or
/// synced at all, proving this is genuinely whole-project, not just a
/// second way to learn about files already tracked.
#[test]
fn mock_server_progress_and_pull_diagnostics_round_trip() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("vix-lsp-mock-pull-{}", std::process::id()));
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
    let mut lsp = Lsp::new(true, vec![cfg], std::slice::from_ref(&root));
    lsp.did_open(&file, "abc\n");

    // The mock hardcodes this as a bare absolute path, not root-relative --
    // matched here exactly, not derived from `root`.
    let other = PathBuf::from("/other.rs");
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut progress: Option<String> = None;
    let mut pulled = false;
    let mut asked_pull = false;
    while Instant::now() < deadline && !pulled {
        for ev in lsp.poll() {
            match ev {
                LspEvent::Progress(text) => progress = Some(text),
                LspEvent::Diagnostics(path) if path.ends_with("other.rs") => pulled = true,
                _ => {}
            }
        }
        if !asked_pull && !lsp.diagnostics_for(&file).is_empty() {
            lsp.request_workspace_diagnostics();
            asked_pull = true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(progress.as_deref(), Some("Indexing (0%)"));
    assert!(pulled, "the pull's own file should get its diagnostics too");
    assert_eq!(
        lsp.diagnostics_for(&other)[0].message,
        "workspace-wide finding"
    );
}

/// T123a: `textDocument/semanticTokens/full` round-trips through a real
/// server -- the legend captured at `initialize` resolves the response's
/// numeric type index back into the name (`"keyword"`) the rest of the
/// pipeline understands.
#[test]
fn mock_server_semantic_tokens_resolve_against_the_initialize_legend() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("vix-lsp-mock-semtok-{}", std::process::id()));
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
    let mut lsp = Lsp::new(true, vec![cfg], std::slice::from_ref(&root));
    lsp.did_open(&file, "abc\n");

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut tokens: Option<Vec<vix::lsp_core::SemanticToken>> = None;
    let mut asked = false;
    while Instant::now() < deadline && tokens.is_none() {
        for ev in lsp.poll() {
            if let LspEvent::SemanticTokens(t) = ev {
                tokens = Some(t);
            }
        }
        if !asked && !lsp.diagnostics_for(&file).is_empty() {
            lsp.request_semantic_tokens(&file, None);
            asked = true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);

    let tokens = tokens.expect("a SemanticTokens event should have arrived");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].line, 0);
    assert_eq!(tokens[0].character, 0);
    assert_eq!(tokens[0].length, 3);
    assert_eq!(tokens[0].token_type, "keyword");
}

/// T134 audit: a crashed server for an already-open file used to just
/// vanish -- every LSP feature for that file went silently dead until the
/// user closed and reopened it. It now respawns (up to `MAX_RESTART_ATTEMPTS`
/// times) and emits `ServerRestarted` naming the files that need a fresh
/// `didOpen` replayed with their real content -- this test drives that
/// replay itself (exactly what `App::poll_lsp` does), proving the full
/// respawn chain and that it eventually gives up with `ServerCrashed`
/// rather than looping forever against a server that crash-loops.
#[test]
fn mock_server_crash_respawns_then_gives_up_after_max_attempts() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("vix-lsp-mock-crash-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mock = root.join("mock_lsp.py");
    std::fs::write(&mock, MOCK_SERVER_CRASHES_AFTER_DID_OPEN).unwrap();
    let file = root.join("a.rs");
    std::fs::write(&file, "abc\n").unwrap();

    let cfg = LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec!["python3".into(), mock.to_string_lossy().into_owned()],
    };
    let mut lsp = Lsp::new(true, vec![cfg], std::slice::from_ref(&root));
    lsp.did_open(&file, "abc\n");

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut restarts = 0;
    let mut crashed_language: Option<String> = None;
    while Instant::now() < deadline && crashed_language.is_none() {
        for ev in lsp.poll() {
            match ev {
                LspEvent::ServerRestarted(paths) => {
                    restarts += 1;
                    assert_eq!(
                        paths,
                        vec![file.clone()],
                        "names the crashed file for replay"
                    );
                    // Exactly what App::poll_lsp does on this event: replay
                    // didOpen with the file's real (here, unchanged) content.
                    lsp.did_open(&file, "abc\n");
                }
                LspEvent::ServerCrashed(language) => crashed_language = Some(language),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(
        crashed_language.as_deref(),
        Some("rust"),
        "should eventually give up rather than respawn forever"
    );
    assert_eq!(
        restarts, 3,
        "exactly MAX_RESTART_ATTEMPTS respawns before giving up"
    );
}

/// T123b: two servers configured for the same extension (a type-checker and
/// a separate linter, the motivating example in the task's own text) both
/// see `didOpen` for the same file, and both publish their own diagnostics
/// without one clobbering the other -- the real bug fan-out was needed to
/// fix, not just registering two servers.
#[test]
fn mock_server_two_servers_on_one_file_publish_independently() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("vix-lsp-mock-multi-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let type_checker = root.join("type_checker.py");
    std::fs::write(&type_checker, MOCK_SERVER).unwrap();
    let linter = root.join("linter.py");
    std::fs::write(&linter, MOCK_SERVER_SECOND_LINTER).unwrap();
    let file = root.join("a.rs");
    std::fs::write(&file, "abc\n").unwrap();

    let cfgs = vec![
        LspServer {
            language_id: "rust-type-checker".into(),
            extensions: vec!["rs".into()],
            command: vec![
                "python3".into(),
                type_checker.to_string_lossy().into_owned(),
            ],
        },
        LspServer {
            language_id: "rust-linter".into(),
            extensions: vec!["rs".into()],
            command: vec!["python3".into(), linter.to_string_lossy().into_owned()],
        },
    ];
    let mut lsp = Lsp::new(true, cfgs, std::slice::from_ref(&root));
    lsp.did_open(&file, "abc\n");

    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline && lsp.diagnostics_for(&file).len() < 2 {
        for _ in lsp.poll() {}
        std::thread::sleep(Duration::from_millis(20));
    }
    lsp.shutdown();

    let diags = lsp.diagnostics_for(&file);
    let messages: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(
        diags.len(),
        2,
        "both servers' diagnostics for the same file must coexist, got {messages:?}"
    );
    assert!(
        messages.contains(&"mock error"),
        "the type-checker's diagnostic should still be present: {messages:?}"
    );
    assert!(
        messages.contains(&"mock lint warning"),
        "the linter's diagnostic should also be present: {messages:?}"
    );
}

/// T123f: `initialize` carries every open workspace folder (not just a
/// single `rootUri`), and adding a folder later sends
/// `workspace/didChangeWorkspaceFolders` to a server whose own `initialize`
/// response asked for it.
#[test]
fn mock_server_gets_every_folder_at_initialize_and_a_change_notification_when_one_is_added() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("vix-lsp-mock-wsfolders-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let extra = std::env::temp_dir().join(format!(
        "vix-lsp-mock-wsfolders-extra-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&extra).unwrap();
    let mock = root.join("mock_lsp.py");
    std::fs::write(&mock, MOCK_SERVER_LOGS_WORKSPACE_FOLDERS).unwrap();
    let log = root.join("received.jsonl");
    let file = root.join("a.rs");
    std::fs::write(&file, "abc\n").unwrap();

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

    // Wait for the didOpen diagnostic -- proof initialize/initialized (and
    // so the logged initialize params) already completed.
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline && lsp.diagnostics_for(&file).is_empty() {
        for _ in lsp.poll() {}
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !lsp.diagnostics_for(&file).is_empty(),
        "server should be ready before checking the logged initialize params"
    );

    lsp.add_workspace_folder(&extra);
    // Give the notification a moment to actually reach the mock server and
    // get flushed to the log file before shutdown tears the pipe down.
    let flush_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < flush_deadline
        && !std::fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("didChangeWorkspaceFolders")
    {
        for _ in lsp.poll() {}
        std::thread::sleep(Duration::from_millis(20));
    }
    lsp.shutdown();

    let log_text = std::fs::read_to_string(&log).unwrap_or_default();
    let messages: Vec<serde_json::Value> = log_text
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&extra);

    let init = messages
        .iter()
        .find(|m| m["method"] == "initialize")
        .expect("initialize should have been logged");
    let root_uri = vix::lsp::path_to_uri(&root);
    assert_eq!(
        init["params"]["workspaceFolders"].as_array().unwrap().len(),
        1,
        "only the one folder open at spawn time: {init}"
    );
    assert_eq!(init["params"]["workspaceFolders"][0]["uri"], root_uri);
    assert_eq!(init["params"]["rootUri"], root_uri);

    let change = messages
        .iter()
        .find(|m| m["method"] == "workspace/didChangeWorkspaceFolders")
        .expect("didChangeWorkspaceFolders should have been sent and logged");
    let added = change["params"]["event"]["added"].as_array().unwrap();
    assert_eq!(added.len(), 1, "exactly the one added folder: {change}");
    assert_eq!(added[0]["uri"], vix::lsp::path_to_uri(&extra));
    assert_eq!(
        change["params"]["event"]["removed"],
        serde_json::json!([]),
        "nothing was removed"
    );
}

/// T123: typing `(` inside a call should auto-trigger signature help (no
/// explicit `lsp.signature_help` action needed) -- exercised end-to-end
/// through a real `App`, not just the `Lsp` client directly, so the whole
/// `on_key` -> `editor_key` -> `maybe_auto_signature_help` ->
/// `Lsp::request_signature_help` -> `poll_lsp` -> `App::hover` path is
/// covered, matching how a user would actually see it.
#[test]
fn typing_an_open_paren_auto_triggers_signature_help_in_a_live_app() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }

    let root = std::env::temp_dir().join(format!("vix-lsp-app-sighelp-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mock = root.join("mock_lsp.py");
    std::fs::write(&mock, MOCK_SERVER_SIGNATURE_HELP_OK).unwrap();
    let file = root.join("a.rs");
    std::fs::write(&file, "abc\n").unwrap();

    let mut settings = Settings::default();
    settings.lsp_servers.push(LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec!["python3".into(), mock.to_string_lossy().into_owned()],
    });

    let mut app = App::new(root.clone(), settings).with_backup_dir(root.join(".vix-test-backup"));
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
    app.open_initial(&file);

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut asked = false;
    while Instant::now() < deadline && app.hover.is_none() {
        app.poll_lsp();
        if !asked && !app.lsp.diagnostics_for(&file).is_empty() {
            // Move to the end of the line, then type `(` as a user would --
            // this is the real trigger path, not a direct `Lsp` call.
            app.on_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
            app.on_key(KeyEvent::new(KeyCode::Char('('), KeyModifiers::NONE));
            asked = true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    app.lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);

    let hover_text = app.hover.map(|h| h.text);
    assert_eq!(
        hover_text.as_deref(),
        Some("fn f(a: i32)\n→ a: i32"),
        "typing `(` should have auto-triggered signature help and populated the hover popup"
    );
}

/// T612: typing one of the server's `initialize`-advertised trigger
/// characters (the mock lists `.`) in a live `App` auto-requests completion
/// after the debounce, reporting `triggerKind` 2 and the character, and the
/// reply opens the popup -- the whole `on_key` -> `completion_after_key` ->
/// `poll_completion_due` -> `Lsp::request_completion_with` -> `poll_lsp`
/// path, as a user would hit it.
#[test]
fn typing_a_trigger_character_auto_opens_the_completion_popup() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }

    let root = std::env::temp_dir().join(format!("vix-lsp-app-complete-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mock = root.join("mock_lsp.py");
    std::fs::write(&mock, MOCK_SERVER).unwrap();
    let file = root.join("a.rs");
    std::fs::write(&file, "abc\n").unwrap();

    let mut settings = Settings::default();
    settings.lsp_servers.push(LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec!["python3".into(), mock.to_string_lossy().into_owned()],
    });

    let mut app = App::new(root.clone(), settings).with_backup_dir(root.join(".vix-test-backup"));
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
    app.open_initial(&file);

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut typed = false;
    while Instant::now() < deadline && app.completion_popup().is_none() {
        app.poll_lsp();
        if !typed && !app.lsp.diagnostics_for(&file).is_empty() {
            app.on_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
            app.on_key(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::NONE));
            typed = true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    app.lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);

    let popup = app
        .completion_popup()
        .expect("typing `.` should have auto-opened the completion popup");
    let item = popup.selected_item().expect("one candidate shown");
    assert_eq!(item.label, "len");
    assert_eq!(item.detail.as_deref(), Some("kind=2 char=."));
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
    let mut lsp = Lsp::new(true, vec![cfg], std::slice::from_ref(&root));
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

/// T652: answers `initialize` and publishes one diagnostic per opened document
/// (so a test can tell the handshake finished), and answers whole-document
/// formatting with a single edit inserting `// fmt\n` at the top.
const MOCK_SERVER_FORMATTING: &str = r"
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
                                      'end':{'line':0,'character':1}},
                             'severity':1,'message':'mock'}]}})
    elif method == 'textDocument/formatting':
        pos = {'line':0,'character':0}
        send({'jsonrpc':'2.0','id':mid,'result':[
            {'range':{'start':pos,'end':pos},'newText':'// fmt\n'}]})
    elif method == 'shutdown':
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'exit':
        break
";

/// A scratch workspace with the formatting mock and two Rust files, plus an
/// `App` (with `settings` tweaked by `tweak`) that has `a.rs` open and synced.
fn formatting_app(tag: &str, tweak: impl FnOnce(&mut Settings)) -> Option<(App, PathBuf)> {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return None;
    }
    let root = std::env::temp_dir().join(format!("vix-lsp-fmt-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).ok()?;
    let root = root.canonicalize().ok()?;
    let mock = root.join("mock_lsp.py");
    std::fs::write(&mock, MOCK_SERVER_FORMATTING).ok()?;
    std::fs::write(root.join("a.rs"), "a\n").ok()?;
    std::fs::write(root.join("b.rs"), "b\n").ok()?;

    let mut settings = Settings::default();
    settings.lsp_servers.push(LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec!["python3".into(), mock.to_string_lossy().into_owned()],
    });
    tweak(&mut settings);
    let mut app = App::new(root.clone(), settings).with_backup_dir(root.join(".vix-test-backup"));
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
    app.open_initial(&root.join("b.rs"));
    app.open_initial(&root.join("a.rs"));
    let a = root.join("a.rs");
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline && app.lsp.diagnostics_for(&a).is_empty() {
        app.poll_lsp();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !app.lsp.diagnostics_for(&a).is_empty(),
        "the mock server should have opened a.rs"
    );
    Some((app, root))
}

/// The text of the tab open on `path` (empty if none is).
fn tab_text(app: &App, path: &std::path::Path) -> String {
    app.editor
        .tabs
        .iter()
        .find(|t| t.path.as_deref() == Some(path))
        .map(vix::editor::Tab::text)
        .unwrap_or_default()
}

/// Poll the app for a while so any in-flight reply is processed.
fn settle(app: &mut App, until: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !until(app) {
        app.poll_lsp();
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// T652: a formatting reply that arrives after the user switched tabs edits
/// the file it was requested for, not the newly active one.
#[test]
fn formatting_reply_after_a_tab_switch_edits_the_requested_file() {
    let Some((mut app, root)) = formatting_app("switch", |_| {}) else {
        return;
    };
    let (a, b) = (root.join("a.rs"), root.join("b.rs"));
    app.run_action("lsp.format");
    app.open_initial(&b); // switch before the reply is polled
    settle(&mut app, |app| tab_text(app, &a).starts_with("// fmt"));
    app.lsp.shutdown();
    assert_eq!(tab_text(&app, &a), "// fmt\na\n", "a.rs got its edits");
    assert_eq!(tab_text(&app, &b), "b\n", "b.rs was left alone");
    let _ = std::fs::remove_dir_all(&root);
}

/// T652: a formatting reply computed against text the user has since edited
/// is dropped, not applied at stale offsets.
#[test]
fn formatting_reply_for_an_edited_document_is_dropped() {
    let Some((mut app, root)) = formatting_app("stale", |_| {}) else {
        return;
    };
    let a = root.join("a.rs");
    app.run_action("lsp.format");
    // Type before the reply is polled: the next sync bumps the version.
    app.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    // Give the reply time to arrive and be judged.
    let deadline = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < deadline {
        app.poll_lsp();
        std::thread::sleep(Duration::from_millis(20));
    }
    app.lsp.shutdown();
    let text = tab_text(&app, &a);
    assert!(
        !text.contains("// fmt"),
        "stale edits were dropped: {text:?}"
    );
    assert!(text.contains('x'), "the typed edit stays: {text:?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// T652: format-on-save followed by a tab switch formats and re-saves the
/// saved file; the other file is neither edited nor written.
#[test]
fn format_on_save_then_tab_switch_formats_and_saves_the_saved_file() {
    let Some((mut app, root)) = formatting_app("on-save", |s| s.save.format_on_save = true) else {
        return;
    };
    let (a, b) = (root.join("a.rs"), root.join("b.rs"));
    app.run_action("file.save");
    app.open_initial(&b);
    settle(&mut app, |_| {
        std::fs::read_to_string(&a).is_ok_and(|t| t.starts_with("// fmt"))
    });
    app.lsp.shutdown();
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "// fmt\na\n");
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "b\n");
    assert_eq!(tab_text(&app, &b), "b\n");
    let a_tab = app
        .editor
        .tabs
        .iter()
        .find(|t| t.path.as_deref() == Some(a.as_path()))
        .unwrap();
    assert!(!a_tab.dirty, "the formatted a.rs was written back");
    assert_eq!(
        app.editor.active_tab().and_then(|t| t.path.clone()),
        Some(b.clone()),
        "b.rs stays the active tab"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// T652, at the client layer: a formatting reply is handed back by
/// `take_edit_replies` naming its document -- its edits while the document is
/// unchanged, `Stale` once the version moved on -- and never as an `LspEvent`.
#[test]
fn mock_server_formatting_replies_name_their_document_and_go_stale() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("vix-lsp-fmt-client-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mock = root.join("mock_lsp.py");
    std::fs::write(&mock, MOCK_SERVER_FORMATTING).unwrap();
    let file = root.join("a.rs");
    std::fs::write(&file, "a\n").unwrap();
    let cfg = LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec!["python3".into(), mock.to_string_lossy().into_owned()],
    };
    let mut lsp = Lsp::new(true, vec![cfg], std::slice::from_ref(&root));
    lsp.did_open(&file, "a\n");

    // Poll until a reply arrives (asking once the handshake is done).
    let next_reply = |lsp: &mut Lsp, ask: &dyn Fn(&mut Lsp)| -> Option<EditReply> {
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut asked = false;
        while Instant::now() < deadline {
            let _ = lsp.poll();
            if let Some(reply) = lsp.take_edit_replies().into_iter().next() {
                return Some(reply);
            }
            if !asked && !lsp.diagnostics_for(&file).is_empty() {
                ask(lsp);
                asked = true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    };
    let fresh = next_reply(&mut lsp, &|lsp| lsp.request_formatting(&file, 4));
    let stale = next_reply(&mut lsp, &|lsp| {
        lsp.request_formatting(&file, 4);
        lsp.did_change(&file, "ab\n"); // the document moves on
    });
    lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);

    let fresh = fresh.expect("a reply for the unchanged document");
    assert_eq!(fresh.path, file);
    assert!(
        matches!(&fresh.kind, EditReplyKind::Edits(e) if e.len() == 1),
        "its edits are delivered: {fresh:?}"
    );
    let stale = stale.expect("a reply for the changed document");
    assert_eq!(stale.path, file);
    assert_eq!(
        stale.kind,
        EditReplyKind::Stale,
        "edits for an old version are dropped"
    );
}

/// T726: provides code lenses (one complete, one to resolve), runs the
/// advertised `mock.run` command, and after the first lens request asks the
/// client to refresh. Every request's method (and an executed command's
/// name) is appended to the log file `argv[1]`.
const MOCK_SERVER_CODE_LENS: &str = r"
import sys, json

LOG = sys.argv[1]

def log(text):
    with open(LOG, 'a') as f:
        f.write(text + '\n')

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

def rng(line):
    return {'start':{'line':line,'character':0},'end':{'line':line,'character':2}}

lens_requests = 0
while True:
    msg = read_msg()
    if msg is None:
        break
    method = msg.get('method')
    mid = msg.get('id')
    if method:
        log(method)
    if method == 'initialize':
        send({'jsonrpc':'2.0','id':mid,'result':{'capabilities':{
            'positionEncoding':'utf-16',
            'codeLensProvider':{'resolveProvider':True},
            'executeCommandProvider':{'commands':['mock.run']}}}})
    elif method == 'textDocument/codeLens':
        lens_requests += 1
        send({'jsonrpc':'2.0','id':mid,'result':[
            {'range':rng(0),'command':{'title':'$(play) Run mock','command':'mock.run','arguments':[1]}},
            {'range':rng(1),'data':{'n':1}}]})
        if lens_requests == 1:
            send({'jsonrpc':'2.0','id':9001,'method':'workspace/codeLens/refresh'})
    elif method == 'codeLens/resolve':
        lens = msg['params']
        lens['command'] = {'title':'2 references','command':'mock.refs'}
        send({'jsonrpc':'2.0','id':mid,'result':lens})
    elif method == 'workspace/executeCommand':
        log('exec ' + msg['params']['command'])
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'shutdown':
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'exit':
        break
";

/// T726 end to end through a real `App`: lenses are requested on open, drawn
/// above their lines once the unresolved one is resolved, re-requested on the
/// server's `workspace/codeLens/refresh`, and `lsp.code_lens_run` sends an
/// advertised command back as `workspace/executeCommand` while refusing an
/// unadvertised one.
#[test]
fn code_lenses_are_requested_resolved_refreshed_and_run_in_a_live_app() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("vix-lsp-lens-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mock = root.join("mock_lsp.py");
    std::fs::write(&mock, MOCK_SERVER_CODE_LENS).unwrap();
    let log = root.join("requests.log");
    let file = root.join("a.rs");
    std::fs::write(&file, "fn a() {}\nfn b() {}\n").unwrap();

    let mut settings = Settings::default();
    settings.lsp_servers.push(LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec![
            "python3".into(),
            mock.to_string_lossy().into_owned(),
            log.to_string_lossy().into_owned(),
        ],
    });
    let mut app = App::new(root.clone(), settings).with_backup_dir(root.join(".vix-test-backup"));
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
    app.open_initial(&file);

    let logged = |n: &str| {
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .filter(|l| *l == n)
            .count()
    };
    let lens = |app: &App, line: usize| {
        app.editor
            .active_tab()
            .and_then(|t| t.editor.code_lens_at(line).map(str::to_string))
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline
        && !(lens(&app, 1).is_some() && logged("textDocument/codeLens") >= 2)
    {
        app.tick();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        lens(&app, 0).as_deref(),
        Some("Run mock"),
        "icon markup gone"
    );
    assert_eq!(lens(&app, 1).as_deref(), Some("2 references"), "resolved");
    assert!(logged("codeLens/resolve") >= 1);
    assert!(
        logged("textDocument/codeLens") >= 2,
        "re-requested on workspace/codeLens/refresh"
    );

    // Line 0's lens: an advertised server command.
    app.run_action("lsp.code_lens_run");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && logged("exec mock.run") == 0 {
        app.tick();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        logged("exec mock.run"),
        1,
        "sent as workspace/executeCommand"
    );

    // Line 1's lens: a command the server never advertised.
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.run_action("lsp.code_lens_run");
    assert!(app.status.contains("mock.refs"), "refused: {}", app.status);

    app.lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

/// T732: a live app shows the mock server's `$/progress` task in the status
/// bar, and View → LSP Status lists the server as running with the
/// capabilities it advertised; `r` in the panel restarts it.
#[test]
fn lsp_status_panel_and_status_bar_progress_in_a_live_app() {
    use ratatui::{Terminal, backend::TestBackend};
    use vix::lsp::ServerState;
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("vix-lsp-status-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mock = root.join("mock_lsp.py");
    std::fs::write(&mock, MOCK_SERVER).unwrap();
    let file = root.join("a.rs");
    std::fs::write(&file, "abc\n").unwrap();

    let mut settings = Settings::default();
    settings.lsp_servers.push(LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec!["python3".into(), mock.to_string_lossy().into_owned()],
    });
    let mut app = App::new(root.clone(), settings).with_backup_dir(root.join(".vix-test-backup"));
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
    app.open_initial(&file);

    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline && app.lsp.progress_line().is_none() {
        app.tick();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        app.lsp.progress_line().as_deref(),
        Some("rust: Indexing (0%)")
    );
    let status = &app.lsp.statuses()[0];
    assert_eq!(status.state, ServerState::Running);
    assert!(
        status.capabilities.iter().any(|c| c == "semanticTokens"),
        "{:?}",
        status.capabilities
    );
    assert_eq!(status.open_documents, 1);

    let screen = |app: &mut App| {
        let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
        term.draw(|f| vix::ui::draw(app, f)).unwrap();
        term.backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>()
    };
    assert!(
        screen(&mut app).contains("rust: Indexing (0%)"),
        "progress shows in the status bar"
    );
    app.run_action("lsp.status");
    let shown = screen(&mut app);
    assert!(shown.contains("semanticTokens"), "capabilities listed");
    assert!(shown.contains(&mock.to_string_lossy().into_owned()) || shown.contains("python3"));

    // `r` restarts the highlighted server: its progress goes with it.
    app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert_eq!(app.lsp.progress_line(), None);
    assert_ne!(app.lsp.statuses()[0].state, ServerState::Failed);
    app.lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

/// A mock server for T614 that publishes an error on line 0 and a warning on
/// line 2 of every opened document.
const MOCK_SERVER_TWO_DIAGNOSTICS: &str = r"
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

def at(line):
    return {'start':{'line':line,'character':0},'end':{'line':line,'character':1}}

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
            'diagnostics': [
                {'range':at(0),'severity':1,'message':'first error'},
                {'range':at(2),'severity':2,'message':'second warning\nmore detail'}]}})
    elif method == 'shutdown':
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'exit':
        break
";

/// The lines of the active tab's inline-diagnostic virtual text.
fn inline_diagnostic_lines(app: &App) -> Vec<(usize, String)> {
    use vix::editor_core::virtual_text::VirtualTextLayer;
    app.editor
        .active_tab()
        .map(|t| {
            t.editor
                .eol_texts(VirtualTextLayer::Diagnostics)
                .iter()
                .map(|e| (e.line, e.text.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// T614: the three `inline_diagnostics` modes, the cursor's diagnostic in the
/// status line, goto landing on (and showing) the next diagnostic, and the
/// inline text on screen.
#[test]
fn inline_diagnostics_follow_the_setting_the_cursor_and_goto() {
    use ratatui::{Terminal, backend::TestBackend};
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("vix-lsp-inline-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let mock = root.join("mock_lsp.py");
    std::fs::write(&mock, MOCK_SERVER_TWO_DIAGNOSTICS).unwrap();
    let file = root.join("a.rs");
    std::fs::write(&file, "abc\nx\nlet y\n").unwrap();

    let mut settings = Settings::default();
    settings.lsp_servers.push(LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec!["python3".into(), mock.to_string_lossy().into_owned()],
    });
    assert_eq!(settings.inline_diagnostics, "cursor-line", "the default");
    let mut app = App::new(root.clone(), settings).with_backup_dir(root.join(".vix-test-backup"));
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
    app.open_initial(&file);
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline && app.lsp.diagnostics_for(&file).len() < 2 {
        app.poll_lsp();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        app.lsp.diagnostics_for(&file).len(),
        2,
        "the mock published"
    );
    app.poll_lsp();

    // cursor-line: only the cursor's line (0), and the status line shows it.
    assert_eq!(
        inline_diagnostic_lines(&app),
        vec![(0, "first error".into())]
    );
    assert!(app.status.contains("first error"), "status: {}", app.status);

    // Goto lands on line 2's diagnostic and shows its (first-line) message.
    app.run_action("nav.next_issue");
    assert_eq!(app.editor.cursor_line(), 2, "goto lands on the diagnostic");
    assert!(
        app.status.contains("second warning"),
        "status: {}",
        app.status
    );
    assert!(!app.status.contains("more detail"), "first line only");
    app.poll_lsp();
    assert_eq!(
        inline_diagnostic_lines(&app),
        vec![(2, "second warning".into())]
    );

    // On screen: dimmed text after `let y`.
    let mut term = Terminal::new(TestBackend::new(100, 20)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let screen: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(
        screen.contains("let y  second warning"),
        "inline text drawn"
    );

    // Leaving the line clears the status message it showed.
    app.run_action("nav.prev_issue");
    assert_eq!(app.editor.cursor_line(), 0);
    let area = app.layout.editor;
    app.editor.goto(2, None, area); // 1-based: line index 1, no diagnostic
    app.poll_lsp();
    assert!(app.status.is_empty(), "status: {}", app.status);

    // all: every line with a diagnostic; off: none.
    app.settings.inline_diagnostics = "all".into();
    app.poll_lsp();
    assert_eq!(
        inline_diagnostic_lines(&app),
        vec![(0, "first error".into()), (2, "second warning".into())]
    );
    app.settings.inline_diagnostics = "off".into();
    app.poll_lsp();
    assert_eq!(inline_diagnostic_lines(&app), Vec::new());

    app.lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

/// T615: a mock server that keeps its own copy of every open document,
/// applying each `didChange` the way a real server does — a ranged change
/// with UTF-16 columns and every LSP line break, or a whole-text one. Its
/// sync kind (`argv[1]`: 1 full, 2 incremental) and whether it answers
/// `semanticTokens/range` (`argv[2]`: `range` or `full`) come from the
/// command line. Hover reports `incremental|full|last range|text|end`, so a test
/// can check what the server ended up with and how it got there.
const MOCK_SERVER_SYNC: &str = r"
import sys, json

SYNC = int(sys.argv[1])
RANGE = sys.argv[2] == 'range'

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

def offset(text, pos):
    i, n = 0, len(text)
    for _ in range(pos['line']):
        while i < n and text[i] not in '\r\n':
            i += 1
        if i >= n:
            return n
        i += 2 if text[i] == '\r' and i + 1 < n and text[i + 1] == '\n' else 1
    units = 0
    while i < n and text[i] not in '\r\n' and units < pos['character']:
        units += 2 if ord(text[i]) > 0xFFFF else 1
        i += 1
    return i

docs = {}
stats = {'inc': 0, 'full': 0, 'range': '-'}

while True:
    msg = read_msg()
    if msg is None:
        break
    method = msg.get('method')
    mid = msg.get('id')
    params = msg.get('params') or {}
    if method == 'initialize':
        provider = {'legend': {'tokenTypes': ['keyword'], 'tokenModifiers': []}, 'full': True}
        if RANGE:
            provider['range'] = True
        send({'jsonrpc':'2.0','id':mid,'result':{'capabilities':{
            'positionEncoding':'utf-16',
            'textDocumentSync':{'openClose':True,'change':SYNC},
            'hoverProvider':True,
            'semanticTokensProvider':provider}}})
    elif method == 'textDocument/didOpen':
        doc = params['textDocument']
        docs[doc['uri']] = doc['text']
        send({'jsonrpc':'2.0','method':'textDocument/publishDiagnostics','params':{
            'uri': doc['uri'], 'diagnostics': [{'range':{'start':{'line':0,'character':0},
            'end':{'line':0,'character':1}},'severity':3,'message':'opened'}]}})
    elif method == 'textDocument/didChange':
        uri = params['textDocument']['uri']
        for change in params['contentChanges']:
            if 'range' in change:
                stats['inc'] += 1
                text = docs[uri]
                start = offset(text, change['range']['start'])
                end = offset(text, change['range']['end'])
                docs[uri] = text[:start] + change['text'] + text[end:]
            else:
                stats['full'] += 1
                docs[uri] = change['text']
    elif method == 'textDocument/hover':
        uri = params['textDocument']['uri']
        value = '%d|%d|%s|%s|end' % (stats['inc'], stats['full'], stats['range'], docs.get(uri, ''))
        send({'jsonrpc':'2.0','id':mid,'result':{'contents':{'kind':'plaintext','value':value}}})
    elif method == 'textDocument/semanticTokens/range':
        r = params['range']
        stats['range'] = '%d:%d-%d:%d' % (r['start']['line'], r['start']['character'],
                                          r['end']['line'], r['end']['character'])
        send({'jsonrpc':'2.0','id':mid,'result':{'data':[r['start']['line'], 0, 1, 0, 0]}})
    elif method == 'textDocument/semanticTokens/full':
        send({'jsonrpc':'2.0','id':mid,'result':{'data':[0, 0, 1, 0, 0]}})
    elif method == 'shutdown':
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'exit':
        break
";

/// What [`MOCK_SERVER_SYNC`]'s hover reports.
#[derive(Debug)]
struct SyncReport {
    incremental: usize,
    full: usize,
    range: String,
    text: String,
}

/// The client trims hover text, so the mock ends it with `|end` to keep a
/// trailing newline of the document.
fn parse_sync_report(hover: &str) -> Option<SyncReport> {
    let mut parts = hover.strip_suffix("|end")?.splitn(4, '|');
    Some(SyncReport {
        incremental: parts.next()?.parse().ok()?,
        full: parts.next()?.parse().ok()?,
        range: parts.next()?.to_string(),
        text: parts.next()?.to_string(),
    })
}

/// A scratch dir with [`MOCK_SERVER_SYNC`] and `a.rs` holding `text`, plus
/// the server config running it with `sync` and `range`.
fn sync_fixture(tag: &str, text: &str, sync: u8, range: &str) -> Option<(PathBuf, LspServer)> {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return None;
    }
    let root = std::env::temp_dir().join(format!("vix-lsp-sync-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).ok()?;
    let root = root.canonicalize().ok()?;
    let mock = root.join("mock_lsp.py");
    std::fs::write(&mock, MOCK_SERVER_SYNC).ok()?;
    std::fs::write(root.join("a.rs"), text).ok()?;
    let cfg = LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec![
            "python3".into(),
            mock.to_string_lossy().into_owned(),
            sync.to_string(),
            range.into(),
        ],
    };
    Some((root, cfg))
}

/// Ask the `Lsp` for the mock's report on `file`.
fn lsp_sync_report(lsp: &mut Lsp, file: &std::path::Path) -> Option<SyncReport> {
    lsp.request_hover(file, 0, 0);
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        for ev in lsp.poll() {
            if let LspEvent::Hover(text) = ev {
                return parse_sync_report(&text);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

/// Wait until the mock has opened `file` (it publishes a diagnostic then).
fn lsp_wait_open(lsp: &mut Lsp, file: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline && lsp.diagnostics_for(file).is_empty() {
        lsp.poll();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !lsp.diagnostics_for(file).is_empty(),
        "the mock opened the file"
    );
}

/// T615: against a server that negotiated incremental sync, every change is
/// sent as a ranged change (UTF-16 columns past multi-byte and astral chars,
/// across lines and CRLF breaks), and the server's copy matches the buffer
/// exactly, as a full re-sync would leave it.
#[test]
fn incremental_did_change_keeps_the_servers_copy_exact() {
    let start = "fn main() {\n    let s = \"héllo 💡\";\n}\n";
    let Some((root, cfg)) = sync_fixture("inc", start, 2, "full") else {
        return;
    };
    let file = root.join("a.rs");
    let mut lsp = Lsp::new(true, vec![cfg], std::slice::from_ref(&root));
    lsp.did_open(&file, start);
    lsp_wait_open(&mut lsp, &file);

    let steps = [
        // insert after an astral char
        "fn main() {\n    let s = \"héllo 💡!\";\n}\n",
        // delete across lines
        "fn main() {\n}\n",
        // multi-line insert with astral chars and CRLF
        "fn main() {\r\n    let 𝔵 = 1;\r\n    let y = 𝔵;\r\n}\n",
        // a change at both ends (several carets) of a CRLF document
        "// top\r\nfn main() {\r\n    let 𝔵 = 1;\r\n    let y = 𝔵;\r\n}\n// end\n",
        // undo back to the start
        start,
        start, // unchanged: nothing is sent
    ];
    for text in steps {
        lsp.did_change(&file, text);
    }
    let report = lsp_sync_report(&mut lsp, &file).unwrap();
    lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(report.text, start);
    assert_eq!(report.full, 0, "{report:?}");
    assert_eq!(report.incremental, steps.len() - 1, "{report:?}");
}

/// T615: a server that only takes full sync still gets the whole text.
#[test]
fn full_sync_server_still_gets_the_whole_text() {
    let Some((root, cfg)) = sync_fixture("full", "abc\n", 1, "full") else {
        return;
    };
    let file = root.join("a.rs");
    let mut lsp = Lsp::new(true, vec![cfg], std::slice::from_ref(&root));
    lsp.did_open(&file, "abc\n");
    lsp_wait_open(&mut lsp, &file);
    lsp.did_change(&file, "abxc\n");
    lsp.did_change(&file, "💡\n");
    let report = lsp_sync_report(&mut lsp, &file).unwrap();
    lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(report.text, "💡\n");
    assert_eq!((report.incremental, report.full), (0, 2), "{report:?}");
}

/// Poll `app` until its hover popup shows the mock's report.
fn app_sync_report(app: &mut App, file: &std::path::Path) -> Option<SyncReport> {
    app.hover = None;
    app.lsp.request_hover(file, 0, 0);
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline && app.hover.is_none() {
        app.poll_lsp();
        std::thread::sleep(Duration::from_millis(20));
    }
    parse_sync_report(&app.hover.take()?.text)
}

/// Sync `app`, then check the mock's copy of `file` matches the buffer and
/// came by incremental changes only; returns how many it has received.
fn app_check_sync(app: &mut App, file: &std::path::Path, what: &str) -> usize {
    app.poll_lsp(); // the sync tick
    let Some(report) = app_sync_report(app, file) else {
        return 0; // no reply: the caller's count check fails
    };
    assert_eq!(report.text, tab_text(app, file), "after {what}");
    assert_eq!(report.full, 0, "after {what}: {report:?}");
    report.incremental
}

/// T615 end to end: in a live `App`, typing, several carets, undo/redo and a
/// Cowork peer's remote edit each reach an incremental server as ranged
/// changes, and its copy always matches the buffer.
#[test]
fn app_edits_of_every_kind_sync_incrementally() {
    let start = "let a = \"💡\";\nlet b = 2;\n";
    let Some((root, cfg)) = sync_fixture("app", start, 2, "full") else {
        return;
    };
    let file = root.join("a.rs");
    let mut settings = Settings::default();
    settings.lsp_servers.push(cfg);
    let mut app = App::new(root.clone(), settings);
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
    app.open_initial(&file);
    settle(&mut app, |app| !app.lsp.diagnostics_for(&file).is_empty());

    // Typing at the end of the first line, after the astral char.
    app.on_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    for c in "// 💡é".chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    let mut sent = app_check_sync(&mut app, &file, "typing");
    assert!(sent > 0);

    // Two carets typing at once, on two lines.
    app.editor.active_tab_mut().unwrap().editor.add_caret_at(0);
    app.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    let n = app_check_sync(&mut app, &file, "multi-cursor typing");
    assert!(n > sent);
    sent = n;

    // Undo, then redo.
    app.run_action("edit.undo");
    let n = app_check_sync(&mut app, &file, "undo");
    assert!(n > sent);
    sent = n;
    app.run_action("edit.redo");
    let n = app_check_sync(&mut app, &file, "redo");
    assert!(n > sent);
    sent = n;

    // A Cowork peer's edit, replayed without an undo step.
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .apply_remote_edits(&[vix::editor_core::code::Splice {
            offset: 0,
            delete: 3,
            insert: "const\n".into(),
        }]);
    let n = app_check_sync(&mut app, &file, "a remote edit");
    assert!(n > sent);

    app.lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}

/// T615: a server that answers `semanticTokens/range` is asked for the
/// window around the visible lines, not the whole document, and again once
/// the view scrolls past that window.
#[test]
fn app_requests_ranged_semantic_tokens_for_the_view() {
    use std::fmt::Write as _;
    let text = (0..200).fold(String::new(), |mut s, i| {
        let _ = writeln!(s, "let v{i} = {i};");
        s
    });
    let Some((root, cfg)) = sync_fixture("range", &text, 2, "range") else {
        return;
    };
    let file = root.join("a.rs");
    let mut settings = Settings::default();
    settings.lsp_servers.push(cfg);
    let mut app = App::new(root.clone(), settings);
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 20);
    app.open_initial(&file);
    settle(&mut app, |app| !app.lsp.diagnostics_for(&file).is_empty());
    let mut report = app_sync_report(&mut app, &file).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && report.range == "-" {
        report = app_sync_report(&mut app, &file).unwrap();
    }
    assert_eq!(report.range, "0:0-40:0", "a view above and below line 0");

    // Scroll far down: the window follows, after the debounce.
    let area = app.layout.editor;
    app.editor.goto(150, None, area);
    let top = app.editor.active_tab().unwrap().editor.top_line();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && report.range.starts_with("0:") {
        std::thread::sleep(Duration::from_millis(50));
        report = app_sync_report(&mut app, &file).unwrap();
    }
    app.lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);

    let (first, rest) = report.range.split_once(':').unwrap();
    let first: usize = first.parse().unwrap();
    let end: usize = rest
        .split_once('-')
        .unwrap()
        .1
        .split_once(':')
        .unwrap()
        .0
        .parse()
        .unwrap();
    assert!(
        first <= top && top + 20 <= end,
        "window {} covers top {top}",
        report.range
    );
}
