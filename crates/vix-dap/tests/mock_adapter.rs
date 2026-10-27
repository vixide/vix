//! Drives `vix-dap` against a scripted Python mock debug adapter (the DAP
//! counterpart of `tui/tests/lsp_smoke.rs`): initialize → launch → stopped →
//! stackTrace → scopes → variables, plus evaluate and a failed request.
//! Skips when `python3` is absent.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vix_dap::{Dap, DapEvent, DebugAdapter};

const MOCK_ADAPTER: &str = r"
import sys, json

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

def fail(req, why):
    send({'type':'response','request_seq':req['seq'],'success':False,
          'command':req['command'],'message':why})

log = open(sys.argv[1], 'a')
while True:
    req = read()
    if req is None:
        break
    cmd = req['command']
    log.write(json.dumps(req) + '\n')
    log.flush()
    if cmd == 'initialize':
        ok(req, {})
        send({'type':'event','event':'initialized'})
    elif cmd == 'configurationDone':
        ok(req)
        send({'type':'event','event':'output','body':{'output':'hello\n'}})
        send({'type':'event','event':'stopped','body':{'reason':'breakpoint','threadId':7}})
    elif cmd == 'stackTrace':
        ok(req, {'stackFrames':[{'id':11,'name':'main','line':3,
                                 'source':{'path':'/tmp/a.rs'}}]})
    elif cmd == 'scopes':
        ok(req, {'scopes':[{'name':'Locals','variablesReference':5}]})
    elif cmd == 'variables':
        ok(req, {'variables':[{'name':'x','value':'1'}]})
    elif cmd == 'evaluate':
        if req['arguments']['expression'] == 'boom':
            fail(req, 'no such symbol')
        else:
            ok(req, {'result':'42'})
    elif cmd == 'next':
        fail(req, 'cannot step here')
    elif cmd == 'stepIn':
        ok(req)
        send({'type':'event','event':'terminated'})
    elif cmd == 'disconnect':
        ok(req)
        break
    else:
        ok(req)
";

fn python_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Poll until `done` accepts the accumulated events (or 15 s pass).
fn poll_until(dap: &mut Dap, events: &mut Vec<DapEvent>, done: impl Fn(&[DapEvent]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline && !done(events) {
        events.extend(dap.poll());
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The first event `pick` accepts; panics (naming `what`) when none does.
fn first<T>(events: &[DapEvent], pick: impl Fn(&DapEvent) -> Option<T>, what: &str) -> T {
    match events.iter().find_map(pick) {
        Some(v) => v,
        None => panic!("no {what} among {events:?}"),
    }
}

fn logged_commands(log: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v["command"].as_str().map(str::to_string))
        .collect()
}

fn check_evaluate(dap: &mut Dap) {
    // evaluate: success, and failure reported inline (not as RequestFailed).
    dap.evaluate("1+1");
    dap.evaluate("boom");
    let mut more = Vec::new();
    poll_until(dap, &mut more, |e| {
        e.iter()
            .filter(|x| matches!(x, DapEvent::Evaluated { .. }))
            .count()
            == 2
    });
    assert!(more.iter().any(
        |e| matches!(e, DapEvent::Evaluated { expr, result } if expr == "1+1" && result == "42")
    ));
    assert!(more.iter().any(|e| matches!(
        e,
        DapEvent::Evaluated { expr, result } if expr == "boom" && result == "no such symbol"
    )));
    assert!(!more.iter().any(|e| matches!(e, DapEvent::RequestFailed(_))));
}

fn check_rejected_step(dap: &mut Dap) {
    // A rejected step surfaces as RequestFailed naming the command + reason,
    // and clears `stopped` optimistically.
    dap.step_over();
    assert!(!dap.is_stopped());
    let mut failed = Vec::new();
    poll_until(dap, &mut failed, |e| {
        e.iter().any(|x| matches!(x, DapEvent::RequestFailed(_)))
    });
    let text = first(
        &failed,
        |e| match e {
            DapEvent::RequestFailed(t) => Some(t.clone()),
            _ => None,
        },
        "RequestFailed event",
    );
    assert!(
        text.contains("next") && text.contains("cannot step here"),
        "{text}"
    );
}

fn check_terminated(dap: &mut Dap) {
    // A `terminated` event ends the session.
    dap.step_into(); // thread id is still known; adapter replies then terminates
    let mut end = Vec::new();
    poll_until(dap, &mut end, |e| {
        e.iter().any(|x| matches!(x, DapEvent::Terminated))
    });
    assert!(end.iter().any(|e| matches!(e, DapEvent::Terminated)));
    assert!(!dap.is_active());
}

#[test]
fn mock_adapter_drives_a_full_debug_session() {
    if !python_available() {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root = std::env::temp_dir().join(format!("vix-dap-mock-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mock = root.join("mock_dap.py");
    let log = root.join("requests.log");
    std::fs::write(&mock, MOCK_ADAPTER).unwrap();

    let adapter = DebugAdapter {
        adapter_id: "mock".into(),
        extensions: vec!["rs".into()],
        command: vec![
            "python3".into(),
            mock.to_string_lossy().into_owned(),
            log.to_string_lossy().into_owned(),
        ],
        launch: std::collections::BTreeMap::new(),
    };
    let mut dap = Dap::new();
    let mut bps = HashMap::new();
    bps.insert("/tmp/a.rs".to_string(), vec![3]);
    assert!(dap.start(&adapter, "/tmp/a.rs", bps));
    assert!(dap.is_active());

    // initialize → initialized → launch/configurationDone → stopped →
    // stackTrace → scopes → variables.
    let mut events = Vec::new();
    poll_until(&mut dap, &mut events, |e| {
        e.iter().any(|x| matches!(x, DapEvent::Variables(_)))
    });
    assert!(events.iter().any(|e| matches!(e, DapEvent::Running)));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, DapEvent::Output(t) if t == "hello"))
    );
    assert!(events.iter().any(|e| matches!(
        e,
        DapEvent::Stopped { reason, thread_id: 7 } if reason == "breakpoint"
    )));
    let frames = first(
        &events,
        |e| match e {
            DapEvent::Stack(f) => Some(f.clone()),
            _ => None,
        },
        "stack event",
    );
    assert_eq!(frames.len(), 1);
    assert_eq!((frames[0].id, frames[0].name.as_str()), (11, "main"));
    assert_eq!(frames[0].path.as_deref(), Some("/tmp/a.rs"));
    let vars = first(
        &events,
        |e| match e {
            DapEvent::Variables(v) => Some(v.clone()),
            _ => None,
        },
        "variables event",
    );
    assert_eq!((vars[0].name.as_str(), vars[0].value.as_str()), ("x", "1"));
    assert!(dap.is_stopped());

    // Request order on the wire: the handshake, breakpoints before launch.
    let cmds = logged_commands(&log);
    let pos = |c: &str| cmds.iter().position(|x| x == c).unwrap_or(usize::MAX);
    assert_eq!(cmds[0], "initialize");
    assert!(pos("setBreakpoints") < pos("launch"));
    assert!(pos("launch") < pos("configurationDone"));
    assert!(pos("stackTrace") < pos("scopes") && pos("scopes") < pos("variables"));

    check_evaluate(&mut dap);
    check_rejected_step(&mut dap);
    check_terminated(&mut dap);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn start_fails_cleanly_when_the_adapter_cannot_spawn() {
    let adapter = DebugAdapter {
        adapter_id: "none".into(),
        extensions: vec![],
        command: vec!["/nonexistent/vix-no-such-adapter".into()],
        launch: std::collections::BTreeMap::new(),
    };
    let mut dap = Dap::new();
    assert!(!dap.start(&adapter, "x", HashMap::new()));
    assert!(!dap.is_active());
    assert!(dap.poll().is_empty());
}
