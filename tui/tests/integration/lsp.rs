#![allow(
    clippy::cast_possible_truncation,
    clippy::format_collect,
    reason = "tests count and index small fixtures, and build expected text with format! over iterators"
)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

#[cfg(feature = "lsp")]
use crate::app::info_panel::InfoSlot;
use crate::integration::common::*;

#[test]
#[cfg(feature = "lsp")]
fn diagnostics_panel_empty_reports_none() {
    let mut app = app_at(Path::new("."));
    app.run_action("lsp.diagnostics");
    assert!(
        app.workspace_search.is_none(),
        "no panel without diagnostics"
    );
    assert!(
        app.status.to_lowercase().contains("diagnostic"),
        "status: {}",
        app.status
    );
}

#[test]
fn menu_hover_shows_help_tooltip() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = app_at(Path::new("."));
    // Open the File menu and highlight its first item (New).
    let file = vix::menu::menus()
        .iter()
        .position(|m| m.name == "menu.file")
        .expect("file menu");
    app.menu.open_index(file);
    app.menu.highlight_item(0);
    // The expected tooltip text is the item's own help — content-independent.
    let expected = vix::menu::menus()[file].items[0]
        .help()
        .expect("file.new item has help text");
    let prefix: String = expected.chars().take(20).collect();
    let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let screen: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(
        screen.contains(&prefix),
        "the highlighted item's help tooltip is rendered (expected prefix {prefix:?})"
    );
}

#[test]
fn lsp_navigation_actions_report_inactive_without_server() {
    // With no language server attached, the LSP nav actions are no-ops that
    // report inactivity rather than panicking.
    for action in [
        "nav.goto_implementation",
        "nav.goto_type_definition",
        "lsp.references",
        "lsp.format",
        "lsp.document_symbols",
        "lsp.workspace_symbols",
        "lsp.signature_help",
        "lsp.rename",
        "lsp.code_action",
        "lsp.expand_selection",
        "lsp.shrink_selection",
        "lsp.highlight",
        "lsp.linked_edit",
        "lsp.code_lens",
        "lsp.call_hierarchy",
        "lsp.call_hierarchy_outgoing",
        "lsp.type_hierarchy_supertypes",
        "lsp.type_hierarchy_subtypes",
    ] {
        let mut app = app_at(Path::new("."));
        type_str(&mut app, "fn main() {}\n");
        app.run_action(action);
        assert!(app.workspace_search.is_none(), "{action} opened no panel");
        assert!(
            app.code_actions.is_none(),
            "{action} opened no code-action menu"
        );
        assert!(app.choice_list.is_none(), "{action} opened no choice list");
        assert!(
            app.prompt.is_none(),
            "{action} opened no prompt without a server"
        );
    }
}

#[test]
#[cfg(feature = "lsp")]
fn open_link_without_a_link_says_so() {
    // T621: with no language server there are no links; the action reports
    // that rather than doing nothing silently.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "https://example.com\n");
    app.run_action("lsp.open_link");
    assert!(
        app.status.contains(&*t!("status.link_none")),
        "status: {}",
        app.status
    );
}

#[test]
fn goto_definition_single_jumps() {
    let dir = unique_dir("gotodef");
    fs::write(dir.join("lib.rs"), "fn target() {}\n").unwrap();
    fs::write(dir.join("main.rs"), "target()\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("main.rs")); // cursor at offset 0 → on "target"

    app.on_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));
    let tab = app.editor.active_tab().unwrap();
    assert!(
        tab.path.as_ref().unwrap().ends_with("lib.rs"),
        "jumped to the definition file"
    );
    assert_eq!(app.editor.cursor_1based().0, 1);
    assert!(
        app.workspace_search.is_none(),
        "single match jumps directly"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn goto_definition_multiple_opens_panel() {
    let dir = unique_dir("gotodef2");
    fs::write(dir.join("a.rs"), "fn dup() {}\n").unwrap();
    fs::write(dir.join("b.rs"), "fn dup() {}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("a.rs"));
    app.editor.goto(1, Some(4), Rect::new(0, 0, 80, 24)); // cursor on "dup"

    app.on_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));
    let ps = app.workspace_search.as_ref().expect("panel of candidates");
    assert!(ps.flags.contains(WorkspaceFlags::STATIC_RESULTS));
    assert_eq!(ps.hits.len(), 2, "two definitions of dup");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn hover_moves_menu_dropdown_selection() {
    let mut app = app_at(Path::new("."));
    app.layout.menu = Rect::new(0, 0, 100, 1);
    app.on_mouse(click(2, 0)); // open the Vix menu (index 0)
    assert_eq!(app.menu.open, Some(0));
    let dd = vix::ui::menu_dropdown_rect(
        app.menu.defs(),
        Rect::new(0, 0, 100, 40),
        app.layout.menu,
        0,
    );
    app.layout.menu_dropdown = dd;

    // Hover (no button) over the third item; the highlight follows the pointer
    // without committing or closing.
    app.on_mouse(mouse(MouseEventKind::Moved, dd.x + 2, dd.y + 1 + 2));
    assert_eq!(
        app.menu.item,
        Some(2),
        "hover highlights the item under the pointer"
    );
    assert!(app.menu.is_open(), "hover must not commit or close");
    // Move back up to the first item.
    app.on_mouse(mouse(MouseEventKind::Moved, dd.x + 2, dd.y + 1));
    assert_eq!(app.menu.item, Some(0));
}

#[test]
fn hover_switches_open_top_menu() {
    let mut app = app_at(Path::new("."));
    app.layout.menu = Rect::new(0, 0, 100, 1);
    app.on_mouse(click(2, 0)); // open Vix (index 0)
    assert_eq!(app.menu.open, Some(0));
    // Hover the File menu name (index 1); the open menu follows the pointer.
    let file_col = top_menu_col(&app, 1);
    app.on_mouse(mouse(MouseEventKind::Moved, file_col, 0));
    assert_eq!(
        app.menu.open,
        Some(1),
        "hovering another name switches menus"
    );
}

#[test]
fn hover_over_pane_does_not_steal_focus() {
    let mut app = app_at(Path::new("."));
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.focus = Focus::Explorer;
    // With no menu open, plain motion must be ignored everywhere.
    app.on_mouse(mouse(MouseEventKind::Moved, 10, 5));
    assert_eq!(
        app.focus,
        Focus::Explorer,
        "plain hover must not change focus"
    );
}

/// T732: View → LSP Status opens a live panel over every configured server
/// (here one whose command cannot start, so it reads Failed with a spawn
/// error); Up/Down stay in bounds, `r` retries the spawn, Esc closes.
#[test]
#[cfg(feature = "lsp")]
fn lsp_status_panel_lists_servers_and_closes() {
    use ratatui::{Terminal, backend::TestBackend};
    use vix::lsp::{Lsp, ServerError, ServerState};
    use vix::settings::LspServer;
    let mut app = app_at(Path::new("."));
    let ghost = LspServer {
        language_id: "ghost".into(),
        extensions: vec!["gh".into()],
        command: vec!["vix-no-such-language-server-t732".into()],
    };
    let mut other = ghost.clone();
    other.language_id = "other".into();
    other.extensions = vec!["ot".into()];
    app.lsp = Lsp::new(true, vec![ghost, other], &[PathBuf::from("/proj")]);
    app.lsp.did_open(Path::new("/proj/a.gh"), "x");

    app.run_action("lsp.status");
    assert!(app.info.lsp_status().is_some(), "the panel opens");
    let statuses = app.lsp.statuses();
    assert_eq!(statuses.len(), 2);
    assert_eq!(statuses[0].state, ServerState::Failed);
    assert_eq!(statuses[0].last_error, Some(ServerError::SpawnFailed));
    assert_eq!(statuses[1].state, ServerState::NotStarted);

    let mut term = Terminal::new(TestBackend::new(100, 40)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let screen: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(screen.contains("ghost") && screen.contains("other"));
    assert!(screen.contains("vix-no-such-language-server-t732"));

    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Down));
    assert_eq!(app.info.lsp_status().unwrap().selected, 1, "clamped");
    app.on_key(keycode(KeyCode::Up));
    app.on_key(keycode(KeyCode::Up));
    assert_eq!(app.info.lsp_status().unwrap().selected, 0, "clamped");
    app.on_key(key('r'));
    assert!(
        app.info.lsp_status().is_some(),
        "restart keeps the panel open"
    );
    assert_eq!(app.lsp.statuses()[0].state, ServerState::Failed);
    app.on_key(esc());
    assert!(app.info.lsp_status().is_none(), "Esc closes");
}

/// T620: a mock language server for call and type hierarchy. `argv[1]` is a
/// tag the server stamps into every item it prepares (`data`) and into the
/// paths of its results (`file:///<tag>/...`). It has no subtypes.
#[cfg(feature = "lsp")]
const MOCK_HIERARCHY_SERVER: &str = r"
import sys, json

TAG = sys.argv[1]

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

def rng(line, ch):
    return {'start': {'line': line, 'character': ch}, 'end': {'line': line, 'character': ch + 1}}

def item(name, uri, line):
    return {'name': name, 'kind': 12, 'uri': uri, 'data': TAG,
            'range': rng(line, 0), 'selectionRange': rng(line, 3)}

def at(name):
    return 'file:///%s/%s.rs' % (TAG, name)

while True:
    msg = read_msg()
    if msg is None:
        break
    method = msg.get('method')
    mid = msg.get('id')
    params = msg.get('params') or {}
    if method == 'initialize':
        send({'jsonrpc': '2.0', 'id': mid, 'result': {'capabilities': {
            'positionEncoding': 'utf-16', 'callHierarchyProvider': True,
            'typeHierarchyProvider': True}}})
    elif method in ('textDocument/prepareCallHierarchy', 'textDocument/prepareTypeHierarchy'):
        uri = params['textDocument']['uri']
        send({'jsonrpc': '2.0', 'id': mid, 'result': [item('f', uri, 0)]})
    elif method == 'callHierarchy/incomingCalls':
        send({'jsonrpc': '2.0', 'id': mid, 'result': [
            {'from': item('caller', at('caller'), 5), 'fromRanges': [rng(6, 2)]}]})
    elif method == 'callHierarchy/outgoingCalls':
        send({'jsonrpc': '2.0', 'id': mid, 'result': [
            {'to': item('callee', at('callee'), 3), 'fromRanges': [rng(1, 4)]}]})
    elif method == 'typeHierarchy/supertypes':
        send({'jsonrpc': '2.0', 'id': mid, 'result': [item('Base', at('base'), 7)]})
    elif method == 'typeHierarchy/subtypes':
        send({'jsonrpc': '2.0', 'id': mid, 'result': []})
    elif method == 'shutdown':
        send({'jsonrpc': '2.0', 'id': mid, 'result': None})
    elif method == 'exit':
        break
";

/// The open result list's title and the paths of its hits.
#[cfg(feature = "lsp")]
fn hierarchy_list(app: &App) -> Option<(String, Vec<PathBuf>)> {
    let ps = app.workspace_search.as_ref()?;
    Some((
        ps.title.clone()?,
        ps.hits.iter().map(|h| h.path.clone()).collect(),
    ))
}

/// T620: the call- and type-hierarchy actions in a live `App` against a mock
/// server: the result list is titled by direction, `Alt+D` flips callers and
/// callees against the same prepared item, an empty reply says so instead of
/// opening a list (and a flip with no results keeps the list it had), and
/// closing the list forgets the hierarchy.
#[test]
#[cfg(feature = "lsp")]
fn hierarchy_actions_list_results_and_alt_d_flips_the_direction() {
    let has_python = std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !has_python {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root = unique_dir("lsp-hierarchy");
    let mock = root.join("mock_hierarchy.py");
    fs::write(&mock, MOCK_HIERARCHY_SERVER).unwrap();
    let file = root.join("a.rs");
    fs::write(&file, "fn f() {}\n").unwrap();
    let mut settings = Settings::default();
    settings.lsp_servers.push(vix::settings::LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec![
            "python3".into(),
            mock.to_string_lossy().into_owned(),
            "alpha".into(),
        ],
    });
    let mut app = app_at_with(&root, settings);
    app.open_initial(&file);
    app.poll_lsp(); // spawns the server and opens the document

    let timeout = Duration::from_secs(15);
    app.run_action("lsp.call_hierarchy");
    wait_until(
        &mut app,
        App::poll_lsp,
        |a| hierarchy_list(a).is_some(),
        timeout,
    );
    let (title, paths) = hierarchy_list(&app).unwrap();
    assert_eq!(title, "Callers");
    assert_eq!(paths, [PathBuf::from("/alpha/caller.rs")]);
    let panel_status = &app.workspace_search.as_ref().unwrap().status;
    assert!(panel_status.contains("Alt+D"), "status: {panel_status}");

    app.on_key(alt(KeyCode::Char('d')));
    wait_until(
        &mut app,
        App::poll_lsp,
        |a| hierarchy_list(a).is_some_and(|(t, _)| t == "Callees"),
        timeout,
    );
    assert_eq!(
        hierarchy_list(&app).unwrap().1,
        [PathBuf::from("/alpha/callee.rs")],
        "the callee itself, from the same server"
    );
    app.on_key(alt(KeyCode::Char('d')));
    wait_until(
        &mut app,
        App::poll_lsp,
        |a| hierarchy_list(a).is_some_and(|(t, _)| t == "Callers"),
        timeout,
    );
    app.on_key(esc());
    assert!(app.workspace_search.is_none());

    // Type hierarchy: no subtypes, so a status line instead of a list.
    app.run_action("lsp.type_hierarchy_subtypes");
    wait_until(
        &mut app,
        App::poll_lsp,
        |a| a.status.contains("Subtypes"),
        timeout,
    );
    assert!(
        app.workspace_search.is_none(),
        "an empty reply opens no list"
    );
    app.run_action("lsp.type_hierarchy_supertypes");
    wait_until(
        &mut app,
        App::poll_lsp,
        |a| hierarchy_list(a).is_some(),
        timeout,
    );
    let (title, paths) = hierarchy_list(&app).unwrap();
    assert_eq!(title, "Supertypes");
    assert_eq!(paths, [PathBuf::from("/alpha/base.rs")]);
    // Alt+D asks for subtypes; there are none, so the supertypes list stays.
    app.status.clear();
    app.on_key(alt(KeyCode::Char('d')));
    wait_until(
        &mut app,
        App::poll_lsp,
        |a| a.status.contains("Subtypes"),
        timeout,
    );
    assert_eq!(hierarchy_list(&app).unwrap().0, "Supertypes");

    app.lsp.shutdown();
    let _ = fs::remove_dir_all(&root);
}
