#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

#[test]
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
    ] {
        let mut app = app_at(Path::new("."));
        type_str(&mut app, "fn main() {}\n");
        app.run_action(action);
        assert!(app.workspace_search.is_none(), "{action} opened no panel");
        assert!(
            app.code_actions.is_none(),
            "{action} opened no code-action menu"
        );
        assert!(app.code_lens.is_none(), "{action} opened no code-lens menu");
        assert!(
            app.prompt.is_none(),
            "{action} opened no prompt without a server"
        );
    }
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
    let dd = vix::ui::menu_dropdown_rect(Rect::new(0, 0, 100, 40), app.layout.menu, 0);
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
