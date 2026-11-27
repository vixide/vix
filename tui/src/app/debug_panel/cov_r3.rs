//! Behavior tests for the debugger panel glue that need no adapter: DAP
//! events applied to the panel, the panel's keys, and the breakpoint,
//! exception-filter and variable prompts when no session is running.

use super::*;
use crate::app::cov4::{app_at as base_app_at, code, content, key, temp};
use crate::dap::{Scope, Thread, Variable};

fn app_at(d: &Path) -> App {
    let mut app = base_app_at(d);
    app.settings_path = Some(d.join("vix-test-config.toml"));
    app
}

fn with_file(tag: &str) -> (App, PathBuf) {
    let d = temp(tag);
    let file = d.join("prog.txt");
    std::fs::write(&file, "a = 1\nb = 2\ny = x\nprint(y)\n").unwrap();
    let mut app = app_at(&d);
    app.open_initial(&file);
    let path = app.editor.active_tab().unwrap().path.clone().unwrap();
    (app, path)
}

#[test]
fn simple_events_update_the_status_the_console_and_watches() {
    let d = temp("dbg-events");
    let mut app = app_at(&d);
    app.apply_dap_event(DapEvent::Running);
    assert_eq!(app.status, t!("status.debug_running").to_string());
    app.apply_dap_event(DapEvent::Stopped {
        reason: "breakpoint".to_string(),
        thread_id: 3,
    });
    assert_eq!(
        app.status,
        t!("status.debug_stopped_at", reason = "breakpoint").to_string()
    );
    let before = app.bottom_dock.lines.len();
    app.apply_dap_event(DapEvent::Output("hello from the program\n".to_string()));
    assert_eq!(app.bottom_dock.lines.len(), before + 1);
    // A result with no matching watch is echoed to the console...
    app.apply_dap_event(DapEvent::Evaluated {
        expr: "1 + 1".to_string(),
        result: "2".to_string(),
    });
    assert_eq!(
        app.bottom_dock.lines.last().map(String::as_str),
        Some("1 + 1 = 2")
    );
    // ...while a watched expression takes the value instead.
    app.dap_view.watches.push(("x".to_string(), String::new()));
    let lines = app.bottom_dock.lines.len();
    app.apply_dap_event(DapEvent::Evaluated {
        expr: "x".to_string(),
        result: "42".to_string(),
    });
    assert_eq!(app.dap_view.watches[0].1, "42");
    assert_eq!(app.bottom_dock.lines.len(), lines);
    app.apply_dap_event(DapEvent::RequestFailed("bad line".to_string()));
    assert_eq!(
        app.status,
        t!("status.dap_request_failed", message = "bad line").to_string()
    );
}

#[test]
fn threads_stack_scopes_and_variables_build_the_tree() {
    let (mut app, path) = with_file("dbg-tree");
    app.apply_dap_event(DapEvent::Stopped {
        reason: "step".to_string(),
        thread_id: 1,
    });
    app.apply_dap_event(DapEvent::Threads(vec![Thread::new(1, "main")]));
    app.apply_dap_event(DapEvent::Stack(vec![Frame::new(
        10,
        "main",
        Some(path.to_string_lossy().into_owned()),
        3,
    )]));
    assert_eq!(app.dap_stopped, Some((path.clone(), 3)));
    assert_eq!(app.editor.cursor_line(), 2, "the editor follows the frame");
    app.apply_dap_event(DapEvent::Scopes {
        frame_id: 10,
        scopes: vec![Scope::new("Locals", 100)],
    });
    app.apply_dap_event(DapEvent::Variables {
        reference: 100,
        variables: vec![Variable::new("x", "7")],
    });
    assert!(app.dap_view.rows().iter().any(|r| r.label.contains('x')));
    assert_eq!(app.dap_view.current_frame().map(|f| f.id), Some(10));
    // A top frame without a source leaves the stop marker alone.
    app.dap_stopped = None;
    app.apply_dap_event(DapEvent::Stack(vec![Frame::new(11, "native", None, 0)]));
    assert_eq!(app.dap_stopped, None);
    // Setting a variable reports the new value.
    app.apply_dap_event(DapEvent::VariableSet {
        reference: 100,
        name: "x".to_string(),
        value: "9".to_string(),
    });
    assert_eq!(
        app.status,
        t!("status.debug_variable_set", name = "x", value = "9").to_string()
    );
    // Not stopped (no live session): inline values are empty.
    assert_eq!(app.debug_inline_values(&path), []);
    // Termination clears the tree, the marker and the panel focus.
    app.focus = Focus::Debug;
    app.apply_dap_event(DapEvent::Terminated);
    assert_eq!(app.focus, Focus::Editor);
    assert_eq!(app.dap_stopped, None);
    assert_eq!(app.status, t!("status.debug_terminated").to_string());
    assert!(app.dap_view.current_frame().is_none());
}

#[test]
fn an_adapter_dropped_for_its_framing_is_reported() {
    // T848: `ConnectionDropped` explains, the `Terminated` after it resets.
    let mut app = app_at(&temp("dbg-frame"));
    app.apply_dap_event(DapEvent::ConnectionDropped(
        crate::lsp_core::frame::FrameError::HeaderTooLarge { limit: 8192 },
    ));
    assert_eq!(
        app.messages.items.last().map(|m| m.text.clone()),
        Some(
            t!(
                "msg.dap_connection_dropped",
                reason = t!("msg.frame_invalid")
            )
            .to_string()
        )
    );
}

#[test]
fn data_breakpoint_answers_without_a_live_session_report_why() {
    let d = temp("dbg-data");
    let mut app = app_at(&d);
    app.apply_dap_event(DapEvent::DataBreakpointInfo {
        name: "x".to_string(),
        data_id: None,
        description: "a struct".to_string(),
        access_types: Vec::new(),
    });
    assert_eq!(
        app.status,
        t!(
            "status.debug_data_breakpoint_unavailable",
            name = "x",
            reason = "a struct"
        )
        .to_string()
    );
    // With an id but no adapter to send them to, the list cannot be set.
    app.apply_dap_event(DapEvent::DataBreakpointInfo {
        name: "x".to_string(),
        data_id: Some("id-1".to_string()),
        description: String::new(),
        access_types: vec!["read".to_string(), "write".to_string()],
    });
    assert_eq!(
        app.status,
        t!("status.debug_data_breakpoint_unsupported").to_string()
    );
    // The keys: with nothing selected, `e` does nothing and `b` explains.
    app.visible.set(Visible::DEBUG_PANEL, true);
    app.focus = Focus::Debug;
    assert!(app.debug_panel_key(key('e')));
    assert!(app.prompt.is_none());
    assert!(app.debug_panel_key(key('b')));
    assert_eq!(app.status, t!("status.debug_data_no_variable").to_string());
    app.begin_data_breakpoint();
    assert_eq!(app.status, t!("status.debug_data_no_variable").to_string());
}

#[test]
fn the_panel_keys_move_the_selection_and_hand_back_focus() {
    let d = temp("dbg-keys");
    let mut app = app_at(&d);
    // Hidden panel: the key is refused and focus returns to the editor.
    app.focus = Focus::Debug;
    assert!(!app.debug_panel_key(code(KeyCode::Down)));
    assert_eq!(app.focus, Focus::Editor);

    app.focus_debug_panel();
    assert!(app.visible.contains(Visible::DEBUG_PANEL));
    assert_eq!(app.focus, Focus::Debug);
    app.apply_dap_event(DapEvent::Threads(vec![
        Thread::new(1, "main"),
        Thread::new(2, "worker"),
    ]));
    app.layout.debug_panel = ratatui::layout::Rect::new(0, 0, 40, 5);
    // Chords with Ctrl/Alt/Super and function keys belong to the app.
    for refused in [
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT),
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::SUPER),
        code(KeyCode::F(5)),
    ] {
        assert!(!app.debug_panel_key(refused));
    }
    let top = app.dap_view.selected;
    for k in [
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::PageDown,
        KeyCode::PageUp,
        KeyCode::End,
        KeyCode::Home,
        KeyCode::Right,
        KeyCode::Left,
        KeyCode::Enter,
        KeyCode::Tab,
    ] {
        assert!(app.debug_panel_key(code(k)), "{k:?}");
    }
    app.debug_panel_key(code(KeyCode::Home));
    let first = app
        .dap_view
        .rows()
        .iter()
        .position(vix_dap::view::Row::selectable);
    assert_eq!(Some(app.dap_view.selected), first);
    let _ = top;
    app.debug_panel_key(code(KeyCode::End));
    let last = app
        .dap_view
        .rows()
        .iter()
        .rposition(vix_dap::view::Row::selectable);
    assert_eq!(Some(app.dap_view.selected), last);
    app.debug_panel_key(code(KeyCode::Up));
    assert_eq!(Some(app.dap_view.selected), last.map(|l| l - 1));
    assert!(app.debug_panel_key(code(KeyCode::Esc)));
    assert_eq!(app.focus, Focus::Editor);
}

#[test]
fn edit_breakpoint_walks_condition_hit_count_and_log_message() {
    let d = temp("dbg-edit-none");
    let mut app = app_at(&d);
    app.editor.tabs.clear();
    app.begin_edit_breakpoint();
    assert_eq!(app.status, t!("status.debug_no_file").to_string());
    // A stale answer with nothing being edited is ignored.
    app.accept_breakpoint_prompt(PromptKind::DebugBreakpointCondition, "x");
    assert!(app.prompt.is_none());

    let (mut app, path) = with_file("dbg-edit");
    app.begin_edit_breakpoint();
    assert!(matches!(
        app.prompt.as_ref().unwrap().kind,
        PromptKind::DebugBreakpointCondition
    ));
    app.accept_breakpoint_prompt(PromptKind::DebugBreakpointCondition, "a > 0");
    assert!(matches!(
        app.prompt.as_ref().unwrap().kind,
        PromptKind::DebugBreakpointHit
    ));
    app.accept_breakpoint_prompt(PromptKind::DebugBreakpointHit, "");
    assert!(matches!(
        app.prompt.as_ref().unwrap().kind,
        PromptKind::DebugBreakpointLog
    ));
    app.accept_breakpoint_prompt(PromptKind::DebugBreakpointLog, "a is {a}");
    assert_eq!(
        app.status,
        t!("status.debug_breakpoint_edited", line = 1).to_string()
    );
    let bp = app.breakpoints.get(&path).and_then(|m| m.get(&1)).unwrap();
    assert_eq!(bp.condition.as_deref(), Some("a > 0"));
    assert_eq!(bp.hit_condition, None);
    assert_eq!(bp.log_message.as_deref(), Some("a is {a}"));
    // Editing again seeds the first prompt with the stored condition.
    app.begin_edit_breakpoint();
    assert_eq!(app.prompt.as_ref().unwrap().input, "a > 0");
}

#[test]
fn function_and_instruction_breakpoints_toggle_and_explain_when_unsupported() {
    let d = temp("dbg-fn");
    let mut app = app_at(&d);
    app.begin_function_breakpoint();
    assert!(matches!(
        app.prompt.as_ref().unwrap().kind,
        PromptKind::DebugFunctionBreakpoint
    ));
    app.accept_function_breakpoint("");
    assert_eq!(app.dap.function_breakpoints(), []);
    app.accept_function_breakpoint("main");
    assert_eq!(app.dap.function_breakpoints().len(), 1);
    assert_eq!(
        app.status,
        t!("status.debug_function_breakpoint_added", name = "main").to_string()
    );
    app.accept_function_breakpoint("main");
    assert_eq!(app.dap.function_breakpoints(), []);
    assert_eq!(
        app.status,
        t!("status.debug_function_breakpoint_removed", name = "main").to_string()
    );

    app.begin_instruction_breakpoint();
    assert!(matches!(
        app.prompt.as_ref().unwrap().kind,
        PromptKind::DebugInstructionBreakpoint
    ));
    app.accept_instruction_breakpoint("  ");
    assert_eq!(app.dap.instruction_breakpoints(), []);
    app.accept_instruction_breakpoint("0x10");
    app.accept_instruction_breakpoint("0x10");
    assert_eq!(app.dap.instruction_breakpoints(), []);
}

#[test]
fn exception_filters_and_variable_edits_need_a_session() {
    let d = temp("dbg-filters");
    let mut app = app_at(&d);
    app.open_exception_filters();
    assert_eq!(
        app.status,
        t!("status.debug_no_exception_filters").to_string()
    );
    assert!(app.choice_list.is_none());
    // Toggling a row that does not exist does nothing.
    app.toggle_exception_filter(&["raised".to_string()], 3);
    assert!(app.choice_list.is_none());
    // A set-variable answer with nothing being edited is ignored.
    app.accept_set_variable("5");
    assert!(app.prompt.is_none());
    app.dap_edit.variable = Some((100, "x".to_string()));
    app.accept_set_variable("");
    assert!(app.dap_edit.variable.is_none());
    app.dap_edit.variable = Some((100, "x".to_string()));
    app.accept_set_variable("5");
    assert_eq!(
        app.status,
        t!("status.debug_set_variable_unsupported").to_string()
    );
    let _ = content(&mut app);
}
