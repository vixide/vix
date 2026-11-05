//! Debugger depth (T617, `crates/vix-dap/spec`) through the real `App`: Run →
//! Edit Breakpoint…, the exception-filter checklist, Run → Attach, and the
//! Debug panel's keys (thread/frame selection, lazy variables, set value,
//! inline values) — against a scripted Python mock adapter, never a real one.
//! Built only with the `dap` feature: without it the `run.*` actions are
//! refused by the capability check.

use super::common::*;

/// A file with a few lines, opened in an app rooted at its directory.
fn app_with_file(tag: &str, settings: Settings) -> (App, PathBuf, PathBuf) {
    let dir = unique_dir(tag);
    let file = dir.join("prog.mock");
    fs::write(&file, "a = 1\nb = 2\ny = x + p.px\nprint(y)\n").unwrap();
    let mut app = app_at_with(&dir, settings);
    app.open_initial(&file);
    let path = app.editor.active_tab().unwrap().path.clone().unwrap();
    (app, dir, path)
}

/// Answer the open prompt with `text` (replacing its seeded input).
fn answer(app: &mut App, text: &str) {
    app.prompt.as_mut().expect("a prompt is open").input = text.to_string();
    app.on_key(keycode(KeyCode::Enter));
}

#[test]
fn edit_breakpoint_sets_condition_hit_count_and_log_message() {
    let (mut app, dir, _) = app_with_file("t617-edit-bp", Settings::default());
    app.run_action("cursor_down"); // line 2
    app.run_action("run.edit_breakpoint");
    assert!(
        app.prompt
            .as_ref()
            .is_some_and(|p| p.kind == PromptKind::DebugBreakpointCondition)
    );
    answer(&mut app, "b > 1");
    assert!(
        app.prompt
            .as_ref()
            .is_some_and(|p| p.kind == PromptKind::DebugBreakpointHit)
    );
    answer(&mut app, ">= 3");
    answer(&mut app, "b is {b}");
    assert!(app.prompt.is_none());
    assert_eq!(
        app.active_breakpoints(),
        vec![2],
        "the line gained a breakpoint"
    );
    let bp = app.active_breakpoint(2).unwrap();
    assert_eq!(bp.condition.as_deref(), Some("b > 1"));
    assert_eq!(bp.hit_condition.as_deref(), Some(">= 3"));
    assert_eq!(bp.log_message.as_deref(), Some("b is {b}"));

    // Editing again seeds each prompt with the current value; an empty
    // answer clears that field.
    app.run_action("run.edit_breakpoint");
    assert_eq!(app.prompt.as_ref().unwrap().input, "b > 1");
    answer(&mut app, "");
    assert_eq!(app.prompt.as_ref().unwrap().input, ">= 3");
    answer(&mut app, ">= 3");
    answer(&mut app, "");
    let bp = app.active_breakpoint(2).unwrap();
    assert_eq!(
        (bp.condition, bp.hit_condition.as_deref(), bp.log_message),
        (None, Some(">= 3"), None)
    );

    // Esc abandons the edit: nothing changes.
    app.run_action("run.edit_breakpoint");
    answer(&mut app, "never");
    app.on_key(esc());
    assert_eq!(app.active_breakpoint(2).unwrap().condition, None);

    // Toggling it off removes it, fields and all.
    app.run_action("run.toggle_breakpoint");
    assert!(app.active_breakpoint(2).is_none());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn exception_filters_and_the_panel_need_a_session() {
    let (mut app, dir, _) = app_with_file("t617-idle", Settings::default());
    app.run_action("run.exception_filters");
    assert!(app.choice_list.is_none());
    assert!(!app.status.is_empty(), "the status line says why");
    app.run_action("run.focus_panel");
    assert_eq!(app.focus, Focus::Debug);
    // A plain key stays in the panel; Esc gives the keyboard back.
    app.on_key(key('x'));
    assert_eq!(
        app.editor
            .active_tab()
            .unwrap()
            .editor
            .get_content()
            .chars()
            .next(),
        Some('a')
    );
    app.on_key(esc());
    assert_eq!(app.focus, Focus::Editor);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn function_breakpoint_prompt_toggles_a_named_function() {
    let (mut app, dir, _) = app_with_file("t617-func", Settings::default());
    app.run_action("run.function_breakpoint");
    assert!(app.prompt.is_some());
    answer(&mut app, "parse");
    assert_eq!(app.dap.function_breakpoints().len(), 1);
    assert_eq!(app.dap.function_breakpoints()[0].name, "parse");
    // An empty answer changes nothing; the same name again removes it.
    app.run_action("run.function_breakpoint");
    answer(&mut app, "");
    assert_eq!(app.dap.function_breakpoints().len(), 1);
    app.run_action("run.function_breakpoint");
    answer(&mut app, "parse");
    assert_eq!(app.dap.function_breakpoints(), []);
    fs::remove_dir_all(&dir).ok();
}

mod with_adapter {
    use super::*;
    use vix::dap::view::RowKind;
    use vix::editor_core::virtual_text::VirtualTextLayer;

    const MOCK: &str = r"
import sys, json
log = open(sys.argv[1], 'a')
path = sys.argv[2]

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

def frame(i, name, line):
    return {'id':i,'name':name,'line':line,'source':{'path':path}}

VARS = {5: [{'name':'x','value':'1'},
            {'name':'p','value':'Point','variablesReference':9}],
        9: [{'name':'px','value':'3'}],
        15: [{'name':'b','value':'2'}],
        25: [{'name':'w','value':'4'}]}

while True:
    req = read()
    if req is None:
        break
    cmd = req['command']
    args = req.get('arguments', {})
    log.write(json.dumps(req) + '\n')
    log.flush()
    if cmd == 'initialize':
        ok(req, {'supportsSetVariable': True, 'supportsFunctionBreakpoints': True,
                 'supportsDataBreakpoints': True,
                 'supportsInstructionBreakpoints': True,
                 'exceptionBreakpointFilters': [
            {'filter':'raised','label':'Raised','default':False},
            {'filter':'uncaught','label':'Uncaught','default':True}]})
        send({'type':'event','event':'initialized'})
    elif cmd == 'configurationDone':
        ok(req)
        send({'type':'event','event':'stopped','body':{'reason':'breakpoint','threadId':7}})
    elif cmd == 'threads':
        ok(req, {'threads':[{'id':7,'name':'main'},{'id':8,'name':'worker'}]})
    elif cmd == 'stackTrace':
        if args['threadId'] == 8:
            ok(req, {'stackFrames':[frame(21, 'work', 4)]})
        else:
            ok(req, {'stackFrames':[frame(11, 'main', 3), frame(12, 'caller', 2)]})
    elif cmd == 'scopes':
        ref = {11: 5, 12: 15, 21: 25}[args['frameId']]
        ok(req, {'scopes':[{'name':'Locals','variablesReference':ref}]})
    elif cmd == 'variables':
        ok(req, {'variables': VARS.get(args['variablesReference'], [])})
    elif cmd == 'dataBreakpointInfo':
        if args['name'] == 'x':
            ok(req, {'dataId':'d-x','description':'x','accessTypes':['read','write']})
        else:
            ok(req, {'dataId':None,'description':'a struct'})
    elif cmd == 'setVariable':
        ok(req, {'value': args['value']})
    elif cmd == 'evaluate':
        ok(req, {'result': 'r'})
    elif cmd == 'disconnect':
        ok(req)
        break
    else:
        ok(req)
";

    fn python_available() -> bool {
        std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    fn requests(log: &Path, command: &str) -> Vec<serde_json::Value> {
        fs::read_to_string(log)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter(|v| v["command"] == command)
            .map(|v| v["arguments"].clone())
            .collect()
    }

    fn wait(app: &mut App, pred: impl FnMut(&App) -> bool) {
        wait_until(app, App::poll_dap, pred, Duration::from_secs(10));
    }

    fn row_index(app: &App, kind: &RowKind) -> usize {
        app.dap_view
            .rows()
            .iter()
            .position(|r| &r.kind == kind)
            .unwrap_or_else(|| panic!("no row {kind:?} in {:?}", app.dap_view.rows()))
    }

    fn has_label(app: &App, label: &str) -> bool {
        app.dap_view.rows().iter().any(|r| r.label == label)
    }

    /// An app debugging a mock-adapter session, stopped with its variables in.
    fn started(tag: &str) -> Option<(App, PathBuf, PathBuf)> {
        if !python_available() {
            eprintln!("python3 not available; skipping");
            return None;
        }
        let dir = unique_dir(tag);
        let mock = dir.join("mock_dap.py");
        let log = dir.join("requests.log");
        fs::write(&mock, MOCK).unwrap();
        // The adapter is configured after opening, once the tab's path (the
        // one stack frames must name) is known.
        let (mut app, _, path) = app_with_file(tag, Settings::default());
        let mut adapter = vix::dap::DebugAdapter::new(
            "mock",
            vec!["mock".into()],
            vec![
                "python3".into(),
                mock.display().to_string(),
                log.display().to_string(),
                path.display().to_string(),
            ],
        );
        adapter.attach.insert("processId".into(), "4242".into());
        app.settings.debug_adapters = vec![adapter];
        Some((app, dir, log))
    }

    #[test]
    fn panel_tree_exception_filters_set_variable_and_inline_values() {
        let Some((mut app, dir, log)) = started("t617-session") else {
            return;
        };
        app.run_action("run.start");
        wait(&mut app, |a| has_label(a, "p"));
        assert!(has_label(&app, "worker"), "threads are listed");
        assert_eq!(app.dap_view.frame_id, Some(11));

        // Inline values on the stopped line 3 (`y = x + p.px`).
        let texts = app
            .editor
            .active_tab()
            .unwrap()
            .editor
            .eol_texts(VirtualTextLayer::DebugValues)
            .to_vec();
        assert_eq!(texts.len(), 1);
        assert_eq!(
            (texts[0].line, texts[0].text.as_str()),
            (2, "x = 1  p = Point")
        );

        // The exception checklist: the adapter's default is on.
        app.run_action("run.exception_filters");
        let rows = app.choice_list.as_ref().unwrap().panel.rows.clone();
        assert_eq!(
            rows,
            vec!["[ ] Raised".to_string(), "[x] Uncaught".to_string()]
        );
        app.on_key(keycode(KeyCode::Enter)); // toggle Raised on
        let rows = app.choice_list.as_ref().unwrap().panel.rows.clone();
        assert_eq!(rows[0], "[x] Raised", "the list stays open, updated");
        app.on_key(esc());
        assert!(app.choice_list.is_none());
        wait(&mut app, |_| {
            requests(&log, "setExceptionBreakpoints").len() == 2
        });
        assert_eq!(
            requests(&log, "setExceptionBreakpoints")[1]["filters"],
            serde_json::json!(["raised", "uncaught"])
        );

        // The panel: keys move, Enter expands a structured variable lazily.
        app.run_action("run.focus_panel");
        assert_eq!(app.focus, Focus::Debug);
        app.dap_view.selected = row_index(&app, &RowKind::Node(vec![0, 0])); // x
        app.on_key(keycode(KeyCode::Down)); // p
        app.on_key(keycode(KeyCode::Enter));
        wait(&mut app, |a| has_label(a, "px"));

        // `e` sets a variable's value.
        app.dap_view.selected = row_index(&app, &RowKind::Node(vec![0, 0]));
        app.on_key(key('e'));
        assert_eq!(app.prompt.as_ref().unwrap().input, "1");
        answer(&mut app, "10");
        wait(&mut app, |a| {
            a.dap_view
                .rows()
                .iter()
                .any(|r| r.label == "x" && r.detail == "10")
        });
        assert_eq!(requests(&log, "setVariable")[0]["value"], "10");

        // Another frame: its scopes and variables, and the editor goes there.
        app.dap_view.selected = row_index(&app, &RowKind::Frame(12));
        app.on_key(keycode(KeyCode::Enter));
        wait(&mut app, |a| has_label(a, "b"));
        assert_eq!(app.focus, Focus::Debug, "the panel keeps the keyboard");
        assert_eq!(app.editor.active_tab().unwrap().editor.cursor_line(), 1);

        // Another thread: its stack and variables.
        app.dap_view.selected = row_index(&app, &RowKind::Thread(8));
        app.on_key(keycode(KeyCode::Enter));
        wait(&mut app, |a| has_label(a, "w"));
        assert_eq!(app.dap.thread_id(), Some(8));

        // Continue clears the inline values.
        app.run_action("run.continue");
        let left = app
            .editor
            .active_tab()
            .unwrap()
            .editor
            .eol_texts(VirtualTextLayer::DebugValues)
            .len();
        assert_eq!(left, 0, "no inline values while running");
        app.run_action("run.stop");
        assert_eq!(app.focus, Focus::Editor);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn attach_sends_the_attach_table_and_breakpoint_fields() {
        let Some((mut app, dir, log)) = started("t617-attach") else {
            return;
        };
        app.run_action("run.edit_breakpoint");
        answer(&mut app, "a > 0");
        answer(&mut app, "");
        answer(&mut app, "");
        app.run_action("run.attach");
        wait(&mut app, |a| has_label(a, "p"));
        assert_eq!(requests(&log, "launch").len(), 0, "attach, not launch");
        assert_eq!(
            requests(&log, "attach"),
            vec![serde_json::json!({ "processId": 4242 })]
        );
        assert_eq!(
            requests(&log, "setBreakpoints")[0]["breakpoints"],
            serde_json::json!([{ "line": 1, "condition": "a > 0" }])
        );
        app.run_action("run.stop");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn breakpoints_persist_across_runs_and_reach_the_next_session() {
        let Some((app, dir, log)) = started("t617-persist") else {
            return;
        };
        let session = dir.join("session.toml");
        let settings = dir.join("settings.toml");
        let mut app = app
            .with_session_path(&session)
            .with_settings_path(&settings);
        let adapters = app.settings.debug_adapters.clone();
        app.run_action("run.toggle_breakpoint"); // line 1
        app.run_action("cursor_down");
        app.run_action("run.edit_breakpoint"); // line 2
        answer(&mut app, "b > 1");
        answer(&mut app, "");
        answer(&mut app, "b is {b}");
        app.run_action("run.function_breakpoint");
        answer(&mut app, "parse");
        app.on_exit();
        drop(app);

        // A fresh run: nothing is set until the saved ones are restored.
        let file = dir.join("prog.mock");
        let mut again = app_at_with(&dir, Settings::default())
            .with_session_path(&session)
            .with_settings_path(&settings);
        again.open_initial(&file);
        assert!(again.active_breakpoints().is_empty());
        again.restore_breakpoints();
        assert_eq!(again.active_breakpoints(), vec![1, 2]);
        let bp = again.active_breakpoint(2).unwrap();
        assert_eq!(bp.condition.as_deref(), Some("b > 1"));
        assert_eq!(bp.log_message.as_deref(), Some("b is {b}"));
        assert_eq!(again.dap.function_breakpoints()[0].name, "parse");

        // They go to the adapter of the next session as ordinary breakpoints.
        again.settings.debug_adapters = adapters;
        again.run_action("run.start");
        wait(&mut again, |a| has_label(a, "p"));
        let sent = requests(&log, "setBreakpoints");
        assert_eq!(
            sent[0]["breakpoints"],
            serde_json::json!([
                { "line": 1 },
                { "line": 2, "condition": "b > 1", "logMessage": "b is {b}" }
            ])
        );
        assert_eq!(
            requests(&log, "setFunctionBreakpoints")[0]["breakpoints"],
            serde_json::json!([{ "name": "parse" }])
        );
        again.run_action("run.stop");

        // Removing them all is saved too.
        again.run_action("run.toggle_breakpoint");
        again.run_action("cursor_down");
        again.run_action("run.toggle_breakpoint");
        again.run_action("run.function_breakpoint");
        answer(&mut again, "parse");
        again.on_exit();
        let mut third = app_at_with(&dir, Settings::default()).with_session_path(&session);
        third.restore_breakpoints();
        assert!(third.dap.function_breakpoints().is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn data_breakpoints_are_set_from_a_panel_variable() {
        let Some((mut app, dir, log)) = started("t617-data") else {
            return;
        };
        // Without a session or a highlighted variable nothing is asked.
        app.run_action("run.data_breakpoint");
        assert!(app.dap.data_breakpoints().is_empty());
        app.run_action("run.start");
        wait(&mut app, |a| has_label(a, "p"));
        app.run_action("run.focus_panel");

        // `b` on a variable that cannot be watched changes nothing.
        app.dap_view.selected = row_index(&app, &RowKind::Node(vec![0, 1])); // p
        app.on_key(key('b'));
        wait(&mut app, |_| requests(&log, "dataBreakpointInfo").len() == 1);
        assert_eq!(
            requests(&log, "dataBreakpointInfo")[0],
            serde_json::json!({ "variablesReference": 5, "name": "p" })
        );
        std::thread::sleep(Duration::from_millis(200));
        app.poll_dap();
        assert!(app.dap.data_breakpoints().is_empty());

        // `b` on x sets a write data breakpoint; the action toggles it off.
        app.dap_view.selected = row_index(&app, &RowKind::Node(vec![0, 0])); // x
        app.on_key(key('b'));
        wait(&mut app, |a| a.dap.data_breakpoints().len() == 1);
        assert_eq!(app.dap.data_breakpoints()[0].data_id, "d-x");
        wait(&mut app, |_| requests(&log, "setDataBreakpoints").len() == 1);
        assert_eq!(
            requests(&log, "setDataBreakpoints")[0],
            serde_json::json!({ "breakpoints": [{ "dataId": "d-x", "accessType": "write" }] })
        );
        app.run_action("run.data_breakpoint");
        wait(&mut app, |a| a.dap.data_breakpoints().is_empty());
        wait(&mut app, |_| requests(&log, "setDataBreakpoints").len() == 2);
        app.run_action("run.stop");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn data_breakpoints_need_adapter_support() {
        let Some((mut app, dir, log)) = started("t617-data-unsupported") else {
            return;
        };
        // An adapter that does not advertise `supportsDataBreakpoints`.
        let mock = dir.join("mock_dap.py");
        let script = fs::read_to_string(&mock)
            .unwrap()
            .replace("'supportsDataBreakpoints': True,", "");
        fs::write(&mock, script).unwrap();
        app.run_action("run.start");
        wait(&mut app, |a| has_label(a, "p"));
        app.dap_view.selected = row_index(&app, &RowKind::Node(vec![0, 0]));
        app.run_action("run.data_breakpoint");
        assert!(requests(&log, "dataBreakpointInfo").is_empty());
        assert!(app.dap.data_breakpoints().is_empty());
        app.run_action("run.stop");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn instruction_breakpoints_are_sent_by_memory_reference() {
        let Some((mut app, dir, log)) = started("t617-instr") else {
            return;
        };
        // Set before the session: kept, and sent while it configures.
        app.run_action("run.instruction_breakpoint");
        assert!(
            app.prompt
                .as_ref()
                .is_some_and(|p| p.kind == PromptKind::DebugInstructionBreakpoint)
        );
        answer(&mut app, "0x4005d0");
        assert_eq!(app.dap.instruction_breakpoints().len(), 1);
        app.run_action("run.start");
        wait(&mut app, |a| has_label(a, "p"));
        assert_eq!(
            requests(&log, "setInstructionBreakpoints")[0],
            serde_json::json!({ "breakpoints": [{ "instructionReference": "0x4005d0" }] })
        );

        // Live: another one is added, then the first toggled off.
        app.run_action("run.instruction_breakpoint");
        answer(&mut app, "0x400600");
        app.run_action("run.instruction_breakpoint");
        answer(&mut app, "0x4005d0");
        wait(&mut app, |_| {
            requests(&log, "setInstructionBreakpoints").len() == 3
        });
        assert_eq!(
            requests(&log, "setInstructionBreakpoints")[2],
            serde_json::json!({ "breakpoints": [{ "instructionReference": "0x400600" }] })
        );
        // An empty answer changes nothing.
        app.run_action("run.instruction_breakpoint");
        answer(&mut app, "");
        assert_eq!(app.dap.instruction_breakpoints().len(), 1);
        app.run_action("run.stop");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn instruction_breakpoints_need_adapter_support() {
        let Some((mut app, dir, log)) = started("t617-instr-unsupported") else {
            return;
        };
        let mock = dir.join("mock_dap.py");
        let script = fs::read_to_string(&mock)
            .unwrap()
            .replace("'supportsInstructionBreakpoints': True,", "");
        fs::write(&mock, script).unwrap();
        app.run_action("run.start");
        wait(&mut app, |a| has_label(a, "p"));
        app.run_action("run.instruction_breakpoint");
        assert!(app.prompt.is_none(), "refused before asking");
        assert!(requests(&log, "setInstructionBreakpoints").is_empty());
        app.run_action("run.stop");
        fs::remove_dir_all(&dir).ok();
    }
}
