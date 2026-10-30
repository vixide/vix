//! T716: language servers, debug adapters, run configurations and the
//! integrated terminal under a non-local authority — a [`FakeAuthority`]
//! whose paths don't exist on this machine, and an `SshAuthority` over a fake
//! `ssh` (a shell script named by explicit path that runs the remote command
//! locally, as `sshd` would). No real host is contacted.
//!
//! The authority slot is process-wide, so every test here holds [`LOCK`]
//! while its authority is installed, and this file is its own test binary.
//! The mock servers are Python; without `python3` those parts are skipped.
#![cfg(all(unix, feature = "tasks", feature = "lsp"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;
use vix::app::App;
use vix::settings::{DebugAdapter, LspServer, Settings};
use vix_authority::ssh::SshOptions;
use vix_authority::{FakeAuthority, LocalAuthority};
use vix_input::{KeyCode, KeyEvent};

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A mock language server. It logs `initialize`'s params and its own
/// working directory to `argv[1]` (one JSON object per line), publishes a
/// diagnostic for every opened document, answers `definition` with
/// `<root>/src/b.t716` line 2, and after the first `didOpen` asks the
/// client to apply an edit to `<root>/src/c.t716` (a file no tab has open).
const MOCK_LSP: &str = r"
import sys, json, os
log = open(sys.argv[1], 'a')
def note(obj):
    log.write(json.dumps(obj) + '\n'); log.flush()
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
    return json.loads(sys.stdin.buffer.read(int(headers.get('content-length', '0'))))
def send(obj):
    data = json.dumps(obj).encode('utf-8')
    sys.stdout.buffer.write(b'Content-Length: %d\r\n\r\n' % len(data) + data)
    sys.stdout.buffer.flush()
root = None
edited = False
while True:
    msg = read_msg()
    if msg is None:
        break
    method = msg.get('method'); mid = msg.get('id')
    if method == 'initialize':
        root = msg['params']['rootUri']
        note({'init': msg['params'], 'cwd': os.getcwd()})
        send({'jsonrpc':'2.0','id':mid,'result':{'capabilities':{'positionEncoding':'utf-16'}}})
    elif method == 'textDocument/didOpen':
        uri = msg['params']['textDocument']['uri']
        note({'open': uri})
        send({'jsonrpc':'2.0','method':'textDocument/publishDiagnostics','params':{
            'uri': uri, 'diagnostics': [{'range':{'start':{'line':0,'character':0},
            'end':{'line':0,'character':1}},'severity':1,'message':'remote finding'}]}})
        if not edited:
            edited = True
            send({'jsonrpc':'2.0','id':900,'method':'workspace/applyEdit','params':{'edit':{
                'changes':{root + '/src/c.t716':[{'range':{'start':{'line':0,'character':0},
                'end':{'line':0,'character':0}},'newText':'# edited\n'}]}}}})
    elif method == 'textDocument/definition':
        send({'jsonrpc':'2.0','id':mid,'result':{'uri': root + '/src/b.t716',
            'range':{'start':{'line':1,'character':0},'end':{'line':1,'character':1}}}})
    elif method == 'shutdown':
        send({'jsonrpc':'2.0','id':mid,'result':None})
    elif method == 'exit':
        break
";

/// A mock debug adapter: logs every request and its working directory to
/// `argv[1]`, stops right after `configurationDone`, and reports one frame
/// in the launched program at line 2.
const MOCK_DAP: &str = r"
import sys, json, os
log = open(sys.argv[1], 'a')
log.write(json.dumps({'cwd': os.getcwd()}) + '\n'); log.flush()
def send(m):
    b = json.dumps(m).encode()
    sys.stdout.buffer.write(b'Content-Length: %d\r\n\r\n' % len(b) + b)
    sys.stdout.buffer.flush()
def read():
    n = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        line = line.strip()
        if not line:
            break
        if line.lower().startswith(b'content-length:'):
            n = int(line.split(b':')[1])
    return json.loads(sys.stdin.buffer.read(n))
def ok(req, body=None):
    send({'type':'response','request_seq':req['seq'],'success':True,
          'command':req['command'],'body':body or {}})
program = None
while True:
    req = read()
    if req is None:
        break
    log.write(json.dumps(req) + '\n'); log.flush()
    cmd = req['command']
    if cmd == 'initialize':
        ok(req); send({'type':'event','event':'initialized'})
    elif cmd == 'launch':
        program = req['arguments']['program']; ok(req)
    elif cmd == 'configurationDone':
        ok(req)
        send({'type':'event','event':'stopped','body':{'reason':'breakpoint','threadId':1}})
    elif cmd == 'stackTrace':
        ok(req, {'stackFrames':[{'id':1,'name':'main','line':2,'source':{'path':program}}]})
    elif cmd == 'disconnect':
        ok(req); break
    else:
        ok(req)
";

fn python() -> bool {
    Command::new("python3")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A fresh scratch directory (canonical, so paths compare with `pwd -P`).
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-t716-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// The project tree every test works on, under `proj` (a real directory).
fn write_project(proj: &Path) {
    std::fs::create_dir_all(proj.join("src")).unwrap();
    std::fs::create_dir_all(proj.join(".vix")).unwrap();
    std::fs::write(proj.join("src/a.t716"), "alpha\n").unwrap();
    std::fs::write(proj.join("src/b.t716"), "one\ntwo\nthree\n").unwrap();
    std::fs::write(proj.join("src/c.t716"), "gamma\n").unwrap();
    std::fs::write(proj.join("src/x.nolsp"), "x\n").unwrap();
    std::fs::write(proj.join("marker-t716-proj"), "").unwrap();
    std::fs::write(
        proj.join(".vix/run.toml"),
        "[[config]]\nname='where'\ncommand='pwd -P'\n",
    )
    .unwrap();
}

/// Settings with the mock server for `.t716`, a server that isn't
/// installed anywhere for `.nolsp`, and the mock adapter for `.t716`.
fn settings(tools: &Path) -> Settings {
    let lsp = tools.join("mock_lsp.py");
    let dap = tools.join("mock_dap.py");
    std::fs::write(&lsp, MOCK_LSP).unwrap();
    std::fs::write(&dap, MOCK_DAP).unwrap();
    let mut settings = Settings::default();
    settings.lsp_servers = vec![
        LspServer {
            language_id: "t716".into(),
            extensions: vec!["t716".into()],
            command: vec![
                "python3".into(),
                lsp.display().to_string(),
                tools.join("lsp.log").display().to_string(),
            ],
        },
        LspServer {
            language_id: "nolsp".into(),
            extensions: vec!["nolsp".into()],
            command: vec!["vix-t716-no-such-server".into()],
        },
    ];
    settings.debug_adapters = vec![DebugAdapter {
        adapter_id: "mock".into(),
        extensions: vec!["t716".into()],
        command: vec![
            "python3".into(),
            dap.display().to_string(),
            tools.join("dap.log").display().to_string(),
        ],
        launch: std::collections::BTreeMap::new(),
    }];
    settings
}

/// The JSON lines a mock logged.
fn log_lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Tick `app` until `done` holds, failing after 15 s with `what`.
fn until(app: &mut App, what: &str, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done(app) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        app.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn type_line(app: &mut App, text: &str) {
    for c in text.chars() {
        app.on_key(KeyEvent::from(KeyCode::Char(c)));
    }
    app.on_key(KeyEvent::from(KeyCode::Enter));
}

/// The language server, as the App drives it: `initialize` names the
/// authority's paths and no local process id, the server runs in the
/// workspace, diagnostics key the authority path, a definition opens the
/// authority file, and a server edit to an unopened file is written through
/// the authority.
fn check_lsp(app: &mut App, root: &Path, tools: &Path, server_cwd: &Path) {
    let a = root.join("src/a.t716");
    app.open_initial(&a);
    until(app, "diagnostics", |app| {
        !app.lsp.diagnostics_for(&a).is_empty()
    });
    let lines = log_lines(&tools.join("lsp.log"));
    let init = lines.iter().find_map(|l| l.get("init")).unwrap();
    let root_uri = vix::lsp::path_to_uri(root);
    assert_eq!(init["rootUri"], Value::from(root_uri.clone()));
    assert_eq!(init["workspaceFolders"][0]["uri"], Value::from(root_uri));
    assert_eq!(init["processId"], Value::Null, "no local pid on a remote");
    let cwd = lines.iter().find_map(|l| l.get("cwd")).unwrap();
    assert_eq!(cwd.as_str().map(Path::new), Some(server_cwd));
    assert!(
        lines
            .iter()
            .any(|l| l.get("open") == Some(&Value::from(vix::lsp::path_to_uri(&a))))
    );

    // The server's edit to c.t716 (no tab) lands through the authority.
    let c = root.join("src/c.t716");
    until(app, "the workspace edit", |_| {
        vix_authority::current()
            .read_to_string(&c)
            .is_ok_and(|t| t == "# edited\ngamma\n")
    });

    // Go to definition opens b.t716 through the authority, at line 2.
    app.run_action("nav.goto_definition");
    let b = root.join("src/b.t716");
    until(app, "the definition jump", |app| {
        app.editor
            .active_tab()
            .is_some_and(|t| t.path.as_deref() == Some(b.as_path()))
    });
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.text(), "one\ntwo\nthree\n");
    assert_eq!(tab.editor.cursor_line(), 1);

    // A server that isn't installed where the workspace is fails to start.
    let x = root.join("src/x.nolsp");
    app.open_initial(&x);
    app.tick();
    let status = app
        .lsp
        .statuses()
        .into_iter()
        .find(|s| s.language_id == "nolsp")
        .unwrap();
    assert_eq!(status.state, vix::lsp::ServerState::Failed);
    assert_eq!(status.last_error, Some(vix::lsp::ServerError::SpawnFailed));
}

/// Run the one run configuration (`pwd -P`) and return what it printed.
fn run_config_output(app: &mut App) -> String {
    app.bottom_dock.lines.clear();
    app.run_action("project.run_config");
    // The picker of run configurations: run the only one.
    app.on_key(KeyEvent::from(KeyCode::Enter));
    until(app, "the run configuration", |app| {
        app.bottom_dock.lines.iter().any(|l| l.starts_with('/'))
    });
    app.bottom_dock
        .lines
        .iter()
        .find(|l| l.starts_with('/'))
        .cloned()
        .unwrap()
}

/// Open the terminal, list its directory, and return the screen once
/// `want` shows, or the computed end marker after the listing (`T716-716`,
/// which only the shell itself can print, not the echo of the typed line).
fn terminal_listing(app: &mut App, want: &str) -> String {
    app.run_action("tools.terminal");
    assert!(app.terminal_running(), "the terminal opened");
    type_line(app, "ls -a; echo T716-$((700 + 16))");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let screen = app.terminal_screen_text().unwrap_or_default();
        if screen.contains(want) || screen.contains("T716-716") {
            app.run_action("tools.terminal");
            return screen;
        }
        assert!(Instant::now() < deadline, "terminal timed out:\n{screen}");
        app.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_fake_authority_runs_language_servers_tasks_and_the_terminal_on_its_side() {
    let _guard = lock();
    let dir = scratch("fake");
    let fake = Arc::new(FakeAuthority::new(dir.join("remote")).unwrap());
    let backing = fake.backing().to_path_buf();
    write_project(&backing.join("proj"));
    let tools = dir.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let previous = vix_authority::install(Arc::<FakeAuthority>::clone(&fake));

    // `/proj` exists only inside the fake.
    let root = PathBuf::from("/proj");
    let mut app =
        App::new(root.clone(), settings(&tools)).with_session_path(dir.join("session.toml"));
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 100, 30);

    if python() {
        check_lsp(&mut app, &root, &tools, &backing.join("proj"));
        assert!(
            fake.calls()
                .iter()
                .any(|c| c.op == "which" && c.path == Path::new("python3")),
            "the server was looked up through the authority"
        );
        assert!(
            fake.calls()
                .iter()
                .any(|c| c.op == "which" && c.path == Path::new("vix-t716-no-such-server"))
        );
        app.lsp.shutdown();
    }

    // The run configuration runs in the workspace, on the authority's side.
    assert_eq!(
        run_config_output(&mut app),
        backing.join("proj").display().to_string()
    );

    // The terminal opens in the workspace on the authority's side.
    let screen = terminal_listing(&mut app, "marker-t716-proj");
    assert!(screen.contains("marker-t716-proj"), "{screen}");
    assert!(
        fake.calls()
            .iter()
            .any(|c| c.op == "terminal" && c.path == root)
    );

    vix_authority::install(previous);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A fake `ssh` in `dir`: log every argument to `dir/argv`, then skip the
/// options and host and run the remote command with `/bin/sh -c`.
fn fake_ssh(dir: &Path) -> PathBuf {
    let script = format!(
        "#!/bin/sh\n\
         for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{argv}'; done\n\
         while [ $# -gt 0 ]; do\n\
           case $1 in --) shift; break ;; -o|-p|-l) shift 2 ;; *) shift ;; esac\n\
         done\n\
         shift\n\
         exec /bin/sh -c \"$*\"\n",
        argv = dir.join("argv").display(),
    );
    let path = dir.join("fake-ssh");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn an_ssh_workspace_runs_its_tools_remotely_once_trusted() {
    let _guard = lock();
    let dir = scratch("ssh");
    let proj = dir.join("proj");
    write_project(&proj);
    let tools = dir.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let options = SshOptions::default()
        .with_program(fake_ssh(&dir))
        .with_multiplex(false);
    let mut app = App::new(dir.clone(), settings(&tools))
        .with_session_path(dir.join("session.toml"))
        .with_ssh_options(options);
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 100, 30);
    app.open_remote(&format!("ssh://me@example.invalid{}", proj.display()));
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.remote_busy() && Instant::now() < deadline {
        app.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(app.root, proj);
    assert!(app.remote_untrusted());
    // The trust prompt that opens on adopt (keys typed within 250 ms are
    // ignored): decline it for now.
    std::thread::sleep(Duration::from_millis(300));
    app.on_key(KeyEvent::from(KeyCode::Char('n')));

    // Untrusted: the terminal opens in the remote home, not the workspace,
    // and the run configuration is refused.
    let screen = terminal_listing(&mut app, "\u{0}");
    assert!(!screen.contains("marker-t716-proj"), "{screen}");
    app.bottom_dock.lines.clear();
    app.run_action("project.run_config");
    for _ in 0..20 {
        app.tick();
    }
    assert!(app.bottom_dock.lines.is_empty(), "nothing ran");

    // Trust it.
    app.run_action("file.trust_remote");
    std::thread::sleep(Duration::from_millis(300));
    app.on_key(KeyEvent::from(KeyCode::Char('y')));
    assert!(!app.remote_untrusted());

    let argv = || std::fs::read_to_string(dir.join("argv")).unwrap_or_default();
    if python() {
        check_lsp(&mut app, &proj, &tools, &proj);
        check_dap(&mut app, &proj, &tools);
        app.lsp.shutdown();
    }
    assert_eq!(run_config_output(&mut app), proj.display().to_string());

    let before = argv().matches("\n-t\n").count();
    let screen = terminal_listing(&mut app, "marker-t716-proj");
    assert!(screen.contains("marker-t716-proj"), "{screen}");
    assert_eq!(argv().matches("\n-t\n").count(), before + 1, "ssh -t");

    vix_authority::install(Arc::new(LocalAuthority));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The debugger over SSH: the adapter starts in the workspace, is told the
/// remote program path, and its stop opens that file.
fn check_dap(app: &mut App, proj: &Path, tools: &Path) {
    let a = proj.join("src/a.t716");
    app.open_initial(&a);
    app.run_action("run.start");
    let log = tools.join("dap.log");
    until(app, "the debugger to stop in a.t716", |app| {
        log_lines(&log).iter().any(|l| l["command"] == "stackTrace")
            && app.editor.active_tab().is_some_and(|t| {
                t.path.as_deref() == Some(a.as_path()) && t.editor.cursor_line() == 1
            })
    });
    let lines = log_lines(&log);
    let cwd = lines.iter().find_map(|l| l.get("cwd")).unwrap();
    assert_eq!(cwd.as_str().map(Path::new), Some(proj));
    let launch = lines.iter().find(|l| l["command"] == "launch").unwrap();
    assert_eq!(
        launch["arguments"]["program"],
        Value::from(a.display().to_string())
    );
    app.run_action("run.stop");
}
