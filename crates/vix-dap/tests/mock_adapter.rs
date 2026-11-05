//! Drives `vix-dap` against a scripted Python mock debug adapter (the DAP
//! counterpart of `tui/tests/lsp_smoke.rs`): initialize → launch → stopped →
//! threads → stackTrace → scopes → variables, plus evaluate and a failed
//! request; and (T617) breakpoint conditions, exception filters, `attach`,
//! thread/frame selection, lazy nested `variables` and `setVariable`.
//! Skips when `python3` is absent.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vix_dap::{
    Dap, DapEvent, DataBreakpoint, DebugAdapter, FunctionBreakpoint, InstructionBreakpoint,
    SourceBreakpoint, StartMode,
};

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
        ok(req, {'supportsSetVariable': True, 'supportsFunctionBreakpoints': True,
                 'supportsDataBreakpoints': True,
                 'supportsInstructionBreakpoints': True,
                 'exceptionBreakpointFilters': [
                     {'filter':'raised','label':'Raised Exceptions','default':False},
                     {'filter':'uncaught','label':'Uncaught Exceptions','default':True}]})
        send({'type':'event','event':'initialized'})
    elif cmd == 'configurationDone':
        ok(req)
        send({'type':'event','event':'output','body':{'output':'hello\n'}})
        send({'type':'event','event':'stopped','body':{'reason':'breakpoint','threadId':7}})
    elif cmd == 'threads':
        ok(req, {'threads':[{'id':7,'name':'main'},{'id':8,'name':'worker'}]})
    elif cmd == 'stackTrace':
        if req['arguments']['threadId'] == 8:
            ok(req, {'stackFrames':[{'id':21,'name':'work','line':5,
                                     'source':{'path':'/tmp/a.rs'}}]})
        else:
            ok(req, {'stackFrames':[{'id':11,'name':'main','line':3,
                                     'source':{'path':'/tmp/a.rs'}},
                                    {'id':12,'name':'caller','line':9,
                                     'source':{'path':'/tmp/a.rs'}}]})
    elif cmd == 'scopes':
        ref = {11: 5, 12: 15, 21: 25}[req['arguments']['frameId']]
        ok(req, {'scopes':[{'name':'Locals','variablesReference':ref}]})
    elif cmd == 'variables':
        ref = req['arguments']['variablesReference']
        ok(req, {'variables': {
            5: [{'name':'x','value':'1'},
                {'name':'p','value':'Point','variablesReference':9}],
            9: [{'name':'px','value':'3'}],
            15: [{'name':'y','value':'2'}],
            25: [{'name':'w','value':'4'}]}.get(ref, [])})
    elif cmd == 'dataBreakpointInfo':
        if req['arguments']['name'] == 'x':
            ok(req, {'dataId':'d-x','description':'x (int)','accessTypes':['read','write']})
        else:
            ok(req, {'dataId':None,'description':'not watchable'})
    elif cmd == 'setVariable':
        ok(req, {'value': req['arguments']['value']})
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

fn logged_requests(log: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .collect()
}

fn logged_commands(log: &std::path::Path) -> Vec<String> {
    logged_requests(log)
        .iter()
        .filter_map(|v| v["command"].as_str().map(str::to_string))
        .collect()
}

/// The arguments of every logged `command` request, in order.
fn logged_args(log: &std::path::Path, command: &str) -> Vec<serde_json::Value> {
    logged_requests(log)
        .into_iter()
        .filter(|v| v["command"] == command)
        .map(|v| v["arguments"].clone())
        .collect()
}

/// A scratch directory holding the mock script, and the adapter that runs it
/// logging to `requests.log` there.
fn mock_setup(tag: &str) -> (std::path::PathBuf, std::path::PathBuf, DebugAdapter) {
    let root = std::env::temp_dir().join(format!("vix-dap-{tag}-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&root) {
        panic!("create {}: {e}", root.display());
    }
    let mock = root.join("mock_dap.py");
    let log = root.join("requests.log");
    if let Err(e) = std::fs::write(&mock, MOCK_ADAPTER) {
        panic!("write {}: {e}", mock.display());
    }
    let adapter = DebugAdapter::new(
        "mock",
        vec!["rs".into()],
        vec![
            "python3".into(),
            mock.to_string_lossy().into_owned(),
            log.to_string_lossy().into_owned(),
        ],
    );
    (root, log, adapter)
}

/// The variables of the first `Variables` event for `reference`.
fn variables_for(events: &[DapEvent], reference: i64) -> Vec<(String, String)> {
    first(
        events,
        |e| match e {
            DapEvent::Variables {
                reference: r,
                variables,
            } if *r == reference => Some(
                variables
                    .iter()
                    .map(|v| (v.name.clone(), v.value.clone()))
                    .collect(),
            ),
            _ => None,
        },
        "variables event",
    )
}

fn has_variables(events: &[DapEvent], reference: i64) -> bool {
    events
        .iter()
        .any(|e| matches!(e, DapEvent::Variables { reference: r, .. } if *r == reference))
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
    let (root, log, adapter) = mock_setup("mock");
    let mut dap = Dap::new();
    let mut bps = HashMap::new();
    bps.insert("/tmp/a.rs".to_string(), vec![SourceBreakpoint::new(3)]);
    assert!(dap.start(&adapter, "/tmp/a.rs", bps, StartMode::Launch));
    assert!(dap.is_active());

    // initialize → initialized → launch/configurationDone → stopped →
    // stackTrace → scopes → variables.
    let mut events = Vec::new();
    poll_until(&mut dap, &mut events, |e| has_variables(e, 5));
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
    assert_eq!(frames.len(), 2);
    assert_eq!((frames[0].id, frames[0].name.as_str()), (11, "main"));
    assert_eq!(frames[0].path.as_deref(), Some("/tmp/a.rs"));
    assert_eq!(variables_for(&events, 5)[0], ("x".into(), "1".into()));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, DapEvent::Threads(t) if t.len() == 2))
    );
    assert!(dap.is_stopped());
    assert_eq!((dap.thread_id(), dap.frame_id()), (Some(7), Some(11)));

    // Request order on the wire: the handshake, breakpoints before launch.
    let cmds = logged_commands(&log);
    let pos = |c: &str| cmds.iter().position(|x| x == c).unwrap_or(usize::MAX);
    assert_eq!(cmds[0], "initialize");
    assert!(pos("setBreakpoints") < pos("launch"));
    assert!(pos("launch") < pos("configurationDone"));
    assert!(pos("stackTrace") < pos("scopes") && pos("scopes") < pos("variables"));
    assert!(pos("threads") < pos("stackTrace"));
    assert!(pos("setExceptionBreakpoints") < pos("launch"));
    assert_eq!(cmds.iter().filter(|c| *c == "attach").count(), 0);

    check_evaluate(&mut dap);
    check_rejected_step(&mut dap);
    check_terminated(&mut dap);

    let _ = std::fs::remove_dir_all(&root);
}

fn check_threads_frames_and_nested_variables(dap: &mut Dap) {
    // Lazy expansion of a structured variable (T617).
    dap.variables(9);
    let mut ev = Vec::new();
    poll_until(dap, &mut ev, |e| has_variables(e, 9));
    assert_eq!(variables_for(&ev, 9), vec![("px".into(), "3".into())]);

    // Another frame: its scopes, then its first scope's variables.
    dap.select_frame(12);
    let mut ev = Vec::new();
    poll_until(dap, &mut ev, |e| has_variables(e, 15));
    assert!(ev.iter().any(|e| matches!(
        e,
        DapEvent::Scopes { frame_id: 12, scopes } if scopes[0].variables_reference == 15
    )));
    assert_eq!(dap.frame_id(), Some(12));

    // Another thread: its stack, its top frame, its variables.
    dap.select_thread(8);
    let mut ev = Vec::new();
    poll_until(dap, &mut ev, |e| has_variables(e, 25));
    assert!(
        ev.iter()
            .any(|e| matches!(e, DapEvent::Stack(f) if f.len() == 1 && f[0].id == 21))
    );
    assert_eq!((dap.thread_id(), dap.frame_id()), (Some(8), Some(21)));
}

fn check_set_variable_and_evaluate_frame(dap: &mut Dap, log: &std::path::Path) {
    assert!(dap.set_variable(25, "w", "40"));
    dap.evaluate("w");
    let mut ev = Vec::new();
    poll_until(dap, &mut ev, |e| {
        e.iter().any(|x| matches!(x, DapEvent::VariableSet { .. }))
            && e.iter().any(|x| matches!(x, DapEvent::Evaluated { .. }))
    });
    assert!(ev.iter().any(|e| matches!(
        e,
        DapEvent::VariableSet { reference: 25, name, value } if name == "w" && value == "40"
    )));
    let set = logged_args(log, "setVariable");
    assert_eq!(set[0]["variablesReference"], 25);
    assert_eq!(
        (set[0]["name"].as_str(), set[0]["value"].as_str()),
        (Some("w"), Some("40"))
    );
    // evaluate runs in the selected frame.
    assert_eq!(logged_args(log, "evaluate")[0]["frameId"], 21);
}

#[test]
fn mock_adapter_attach_breakpoint_fields_exception_filters_and_tree() {
    if !python_available() {
        eprintln!("python3 not available; skipping");
        return;
    }
    let (root, log, mut adapter) = mock_setup("attach");
    adapter.attach.insert("processId".into(), "4242".into());
    adapter.launch.insert("cwd".into(), "/never/sent".into());
    let mut dap = Dap::new();
    let mut bp = SourceBreakpoint::new(3);
    bp.condition = Some("x > 1".into());
    bp.hit_condition = Some(">= 2".into());
    bp.log_message = Some("x is {x}".into());
    let bps = HashMap::from([("/tmp/a.rs".to_string(), vec![bp, SourceBreakpoint::new(9)])]);
    assert!(dap.start(&adapter, "/tmp/a.rs", bps, StartMode::Attach));
    let mut events = Vec::new();
    poll_until(&mut dap, &mut events, |e| has_variables(e, 5));
    let caps = first(
        &events,
        |e| match e {
            DapEvent::Capabilities(c) => Some(c.clone()),
            _ => None,
        },
        "capabilities event",
    );
    assert!(caps.supports_set_variable);
    assert_eq!(caps.exception_filters.len(), 2);
    assert_eq!(
        dap.exception_filters(),
        ["uncaught"],
        "the adapter's defaults"
    );

    // attach, with the attach table alone, instead of launch.
    let cmds = logged_commands(&log);
    assert!(!cmds.iter().any(|c| c == "launch"), "{cmds:?}");
    assert_eq!(
        logged_args(&log, "attach"),
        vec![serde_json::json!({ "processId": 4242 })]
    );
    // The breakpoint fields on the wire, and a plain one with none.
    let set = logged_args(&log, "setBreakpoints");
    assert_eq!(
        set[0]["breakpoints"],
        serde_json::json!([
            { "line": 3, "condition": "x > 1", "hitCondition": ">= 2", "logMessage": "x is {x}" },
            { "line": 9 },
        ])
    );
    // The default exception filters during configuration...
    let exc = logged_args(&log, "setExceptionBreakpoints");
    assert_eq!(exc, vec![serde_json::json!({ "filters": ["uncaught"] })]);
    // ...and a changed choice sent at once (an unknown id dropped).
    dap.set_exception_filters(&["raised".into(), "bogus".into()]);
    dap.set_breakpoints("/tmp/a.rs", &[SourceBreakpoint::new(4)]);
    let deadline = Instant::now() + Duration::from_secs(15);
    while logged_args(&log, "setBreakpoints").len() < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        logged_args(&log, "setExceptionBreakpoints")[1],
        serde_json::json!({ "filters": ["raised"] })
    );
    assert_eq!(dap.exception_filters(), ["raised", "bogus"]);

    // Function breakpoints: sent during configuration (none yet), then at
    // once on change.
    assert_eq!(
        logged_args(&log, "setFunctionBreakpoints"),
        vec![serde_json::json!({ "breakpoints": [] })]
    );
    assert!(caps.supports_function_breakpoints);
    let mut conditional = FunctionBreakpoint::new("parse");
    conditional.condition = Some("n > 2".into());
    dap.set_function_breakpoints(vec![FunctionBreakpoint::new("main"), conditional]);
    let deadline = Instant::now() + Duration::from_secs(15);
    while logged_args(&log, "setFunctionBreakpoints").len() < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        logged_args(&log, "setFunctionBreakpoints")[1],
        serde_json::json!({ "breakpoints": [
            { "name": "main" },
            { "name": "parse", "condition": "n > 2" },
        ] })
    );
    assert_eq!(dap.function_breakpoints().len(), 2);

    check_threads_frames_and_nested_variables(&mut dap);
    check_set_variable_and_evaluate_frame(&mut dap, &log);
    check_data_breakpoints(&mut dap, &log);
    check_instruction_breakpoints(&mut dap, &log);
    dap.stop();
    let _ = std::fs::remove_dir_all(&root);
}

/// `dataBreakpointInfo` for a watchable and an unwatchable variable, then
/// `setDataBreakpoints` with the access type, and clearing them.
fn check_data_breakpoints(dap: &mut Dap, log: &std::path::Path) {
    let mut events = Vec::new();
    assert!(dap.data_breakpoints().is_empty());
    assert!(dap.data_breakpoint_info(5, "x"));
    assert!(dap.data_breakpoint_info(5, "p"));
    poll_until(dap, &mut events, |e| {
        e.iter()
            .filter(|e| matches!(e, DapEvent::DataBreakpointInfo { .. }))
            .count()
            == 2
    });
    let infos: Vec<(String, Option<String>, Vec<String>)> = events
        .iter()
        .filter_map(|e| match e {
            DapEvent::DataBreakpointInfo {
                name,
                data_id,
                access_types,
                ..
            } => Some((name.clone(), data_id.clone(), access_types.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        infos,
        vec![
            (
                "x".to_string(),
                Some("d-x".to_string()),
                vec!["read".to_string(), "write".to_string()]
            ),
            ("p".to_string(), None, Vec::new()),
        ]
    );
    let mut bp = DataBreakpoint::new("d-x", "x");
    bp.access_type = Some("write".into());
    assert!(dap.set_data_breakpoints(vec![bp]));
    assert_eq!(dap.data_breakpoints().len(), 1);
    assert!(dap.set_data_breakpoints(Vec::new()));
    let deadline = Instant::now() + Duration::from_secs(15);
    while logged_args(log, "setDataBreakpoints").len() < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        logged_args(log, "setDataBreakpoints"),
        vec![
            serde_json::json!({ "breakpoints": [{ "dataId": "d-x", "accessType": "write" }] }),
            serde_json::json!({ "breakpoints": [] }),
        ]
    );
}

/// `setInstructionBreakpoints` on a live session, sent at once.
fn check_instruction_breakpoints(dap: &mut Dap, log: &std::path::Path) {
    let mut bp = InstructionBreakpoint::new("0x4005d0");
    bp.hit_condition = Some("2".into());
    assert!(dap.set_instruction_breakpoints(vec![bp]));
    let deadline = Instant::now() + Duration::from_secs(15);
    while logged_args(log, "setInstructionBreakpoints").len() < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    // One empty list during configuration, then the new one.
    assert_eq!(
        logged_args(log, "setInstructionBreakpoints")[1],
        serde_json::json!({ "breakpoints": [
            { "instructionReference": "0x4005d0", "hitCondition": "2" }
        ] })
    );
    assert_eq!(dap.instruction_breakpoints().len(), 1);
}

#[test]
fn start_fails_cleanly_when_the_adapter_cannot_spawn() {
    let adapter = DebugAdapter::new(
        "none",
        vec![],
        vec!["/nonexistent/vix-no-such-adapter".into()],
    );
    let mut dap = Dap::new();
    assert!(!dap.start(&adapter, "x", HashMap::new(), StartMode::Launch));
    assert!(!dap.is_active());
    assert!(dap.poll().is_empty());
}
