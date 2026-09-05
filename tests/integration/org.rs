#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

#[test]
fn org_capture_inserts_todo_and_time_report_tabulates() {
    // Capture → Anything… opens a `%^{Task}` field prompt; submitting inserts
    // a TODO headline immediately (its built-in template is `immediate_finish`).
    let mut app = app_at(Path::new("."));
    app.run_action("org.capture");
    assert!(app.prompt.is_some(), "Org → Capture opens a prompt");
    for c in "Buy milk".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("* TODO Buy milk")
    );

    // Capture → Task… has no `%^{}` fields, so it goes straight to the
    // multiline review buffer, pre-filled from the template.
    let mut app = app_at(Path::new("."));
    app.run_action("org.capture.task");
    let p = app.prompt.as_ref().expect("Org → Capture → Task… prompts");
    assert_eq!(p.input, "* TODO ", "pre-filled from the built-in template");
    for c in "Plan trip".chars() {
        app.on_key(key(c));
    }
    // Alt+Enter inserts a newline; plain Enter submits verbatim.
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
    for c in "  details".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("* TODO Plan trip\n  details\n"),
        "multiline capture inserted verbatim"
    );

    // A custom template body pre-fills the multiline review editor.
    let mut app = app_at(Path::new("."));
    let task_template = app
        .settings
        .org_capture_templates
        .iter_mut()
        .find(|t| t.key == "t")
        .expect("built-in Task template");
    task_template.template = "* TODO %\n  SCHEDULED:".to_string();
    app.run_action("org.capture.task");
    assert_eq!(
        app.prompt.as_ref().unwrap().input,
        "* TODO %\n  SCHEDULED:",
        "a custom org_capture_templates entry pre-fills the editing area"
    );

    // Time Tracker builds a clock report in a new tab.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "* Task\nCLOCK: [a]--[b] =>  1:00\n");
    let before = app.editor.tabs.len();
    app.run_action("org.time_report");
    assert_eq!(app.editor.tabs.len(), before + 1);
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("| Task | 1:00 |")
    );

    // Agenda Tracker runs and opens a buffer (no .org files → just the header).
    app.run_action("org.agenda");
    assert!(app.editor.active_tab().unwrap().text().contains("Agenda"));

    // Clock In inserts an open CLOCK entry; Clock Out completes it.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "* Task\n");
    app.run_action("org.clock_in");
    let t = app.editor.active_tab().unwrap().text();
    assert!(t.contains("CLOCK: ["), "clock-in line: {t:?}");
    assert!(!t.contains("--"), "still open");
    app.run_action("org.clock_out");
    let t = app.editor.active_tab().unwrap().text();
    assert!(t.contains("--[") && t.contains("=>"), "clocked out: {t:?}");
}

#[test]
fn org_capture_babel_and_note_templates() {
    // Capture → Babel… prompts for a language, then opens a review buffer
    // with a `#+begin_src <language>` / `#+end_src` block (the `%?` marker
    // between them is dropped, leaving a blank line). Prompt input has no
    // mid-text cursor — typed characters append at the end, like every other
    // multiline capture prompt — so editing that blank line isn't possible
    // here; submitting verbatim still files a well-formed block.
    let mut app = app_at(Path::new("."));
    app.run_action("org.capture.babel");
    for c in "python".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        app.prompt.as_ref().unwrap().input,
        "#+begin_src python\n\n#+end_src",
        "Babel capture opens a review buffer around the language's src block"
    );
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("#+begin_src python\n\n#+end_src\n"),
        "Babel capture files the block verbatim"
    );

    // Capture → Note… prompts once for the note text, then files immediately
    // (its `immediate_finish` is set) as a plain headline with a creation
    // timestamp (`%U`, an inactive `[...]` timestamp so it doesn't count
    // toward the agenda).
    let mut app = app_at(Path::new("."));
    app.run_action("org.capture.note");
    assert!(app.prompt.is_some(), "Org → Capture → Note… opens a prompt");
    for c in "Buy stamps".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let text = app.editor.active_tab().unwrap().text();
    assert!(
        text.starts_with("* Buy stamps\n  ["),
        "Note capture files a plain headline with a timestamp: {text:?}"
    );
    assert!(
        !text.contains("TODO"),
        "Note capture is a plain note, not a task: {text:?}"
    );
}

#[test]
fn org_capture_field_prompts_preview_the_template_in_progress() {
    // Capture → Contact… has five fields (Name/Email/Phone/Address/Birthday);
    // each field's prompt should preview the whole template, with earlier
    // fields substituted, the current one marked `‹Label›`, and later ones
    // still `[Label]`.
    let mut app = app_at(Path::new("."));
    app.run_action("org.capture.contact");
    let preview = app
        .prompt
        .as_ref()
        .expect("Org → Capture → Contact… prompts")
        .preview
        .as_ref()
        .expect("field prompts carry a template preview");
    assert!(preview.contains("* \u{2039}Name\u{203a}"), "{preview:?}");
    assert!(preview.contains(":EMAIL: [Email]"), "{preview:?}");
    assert!(preview.contains(":PHONE: [Phone]"), "{preview:?}");

    for c in "Ada Lovelace".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let preview = app.prompt.as_ref().unwrap().preview.as_ref().unwrap();
    assert!(
        preview.contains("* Ada Lovelace"),
        "answered field substituted for real: {preview:?}"
    );
    assert!(
        preview.contains("\u{2039}Email\u{203a}"),
        "next field now marked current: {preview:?}"
    );
    assert!(preview.contains(":PHONE: [Phone]"), "{preview:?}");
}

#[test]
fn org_node_nodeify_extract_and_dead_links() {
    let dir = unique_dir("orgnode");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut app = app_at(&dir);

    // Nodeify: the headline at the cursor gains an :ID: drawer.
    type_str(&mut app, "* My Heading\nsome body\n");
    for _ in 0..10 {
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    }
    app.run_action("node.nodeify");
    let t = app.editor.active_tab().unwrap().text();
    assert!(
        t.contains("* My Heading\n:PROPERTIES:\n:ID:"),
        "nodeified: {t:?}"
    );
    // A second nodeify is a no-op (already a node).
    app.run_action("node.nodeify");
    assert!(app.status.contains("cursor on a headline") || t.contains(":ID:"));

    // Insert a transclusion for a new node into a fresh buffer.
    let mut app = app_at(&dir);
    app.run_action("node.insert_transclusion");
    type_str(&mut app, "Shared Block");
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("#+transclude: [[id:")
    );

    // Extract subtree: the subtree moves to a new file, a link stays behind.
    let mut app = app_at(&dir);
    type_str(&mut app, "* Parent\n** Child\nchild body\n");
    for _ in 0..10 {
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    }
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // onto "** Child"
    app.run_action("node.extract_subtree");
    assert!(
        dir.join("child.org").exists(),
        "extracted node file written"
    );

    // Dead-links report opens a buffer.
    app.run_action("node.dead_links");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("Dead Links")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn org_checkbox_toggle_updates_parents_and_cookies() {
    // Move the cursor to a 0-based line by going to the top, then down.
    fn goto(app: &mut App, line: usize) {
        for _ in 0..40 {
            app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        }
        for _ in 0..line {
            app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
    }

    let mut app = app_at(Path::new("."));
    type_str(
        &mut app,
        "* Tasks [/]\n- [ ] call people\n  - [ ] Peter\n  - [ ] Sarah\n",
    );

    // Toggle the "Peter" child (line 2): parent becomes partial, child checked.
    goto(&mut app, 2);
    app.run_action("org.toggle_checkbox");
    let t = app.editor.active_tab().unwrap().text();
    assert!(t.contains("- [-] call people"), "parent partial: {t:?}");
    assert!(t.contains("  - [x] Peter"), "child checked: {t:?}");

    // Toggle "Sarah" (line 3): parent and the top-level item become fully checked.
    goto(&mut app, 3);
    app.run_action("org.toggle_checkbox");
    let t = app.editor.active_tab().unwrap().text();
    assert!(t.contains("- [X] call people"), "parent checked: {t:?}");
    assert!(
        t.contains("* Tasks [1/1]"),
        "headline cookie counts the top-level checkbox: {t:?}"
    );
}

#[test]
fn org_menu_edits_headlines_and_exports() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "* Task\nbody");
    // Cursor is on the body line; move to the headline (line 0).
    app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    app.run_action("org.cycle_todo");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .starts_with("* TODO Task")
    );
    app.run_action("org.demote");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .starts_with("** TODO Task")
    );

    // Priority Up sets the default cookie (0 = highest, per settings
    // defaults); Priority Down steps it toward the lowest bound.
    app.run_action("org.priority.up");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .starts_with("** TODO [#0] Task")
    );
    app.run_action("org.priority.down");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .starts_with("** TODO [#1] Task")
    );

    // Export opens a new buffer containing Markdown.
    let before = app.editor.tabs.len();
    app.run_action("org.export_markdown");
    assert_eq!(app.editor.tabs.len(), before + 1);
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("## TODO [#1] Task")
    );
}

#[test]
fn org_insert_and_marker_block_toggles() {
    let mut app = app_at(Path::new("."));
    // Org snippet insertion.
    app.run_action("tools.insert.org.title");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "#+title: Hello World\n"
    );

    // Marker toggle wraps, then unwraps, the selection.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "bold");
    app.on_key(ctrl('a'));
    app.run_action("tools.insert.marker.bold");
    assert_eq!(app.editor.active_tab().unwrap().text(), "*bold*");
    app.on_key(ctrl('a'));
    app.run_action("tools.insert.marker.bold");
    assert_eq!(app.editor.active_tab().unwrap().text(), "bold");

    // Begin-End block toggle wraps the selection.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "hi");
    app.on_key(ctrl('a'));
    app.run_action("tools.insert.block.quote");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "#+BEGIN_QUOTE\nhi\n#+END_QUOTE"
    );

    // The Tag marker wraps the selection with ':'.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "work");
    app.on_key(ctrl('a'));
    app.run_action("tools.insert.marker.tag");
    assert_eq!(app.editor.active_tab().unwrap().text(), ":work:");

    // The Properties snippet inserts a property drawer.
    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.org.properties");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.starts_with(":PROPERTIES:"));
    assert!(text.contains(":Composer:  J.S. Bach"));
    assert!(text.trim_end().ends_with(":END:"));
}

#[test]
fn org_tab_folds_drawer_under_cursor() {
    use ratatui::{Terminal, backend::TestBackend};
    let dir = unique_dir("org-drawer-fold");
    let file = dir.join("notes.org");
    fs::write(&file, "* Name\n:properties:\n:foo: 123\n:end:\nbody\n").unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&file.clone());

    // Put the caret on the `:properties:` drawer header (line 1).
    app.run_action("edit.go_first");
    app.on_key(keycode(KeyCode::Down));

    // Tab folds the drawer: its body (`:foo:` and `:end:`) is hidden, the header
    // stays visible.
    app.on_key(keycode(KeyCode::Tab));
    {
        let ed = &app.editor.active_tab().unwrap().editor;
        assert!(ed.has_folds(), "drawer folded");
        assert!(!ed.is_line_hidden(1), "header line stays visible");
        assert!(ed.is_line_hidden(2) && ed.is_line_hidden(3), "body hidden");
        assert!(!ed.is_line_hidden(4), "line after the drawer visible");
    }

    // The folded header renders `:properties:...`; its body text is off-screen.
    // Hide the side docks so the editor pane is wide enough to show the marker.
    app.show_explorer = false;
    app.show_messages = false;
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let screen: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(
        screen.contains(":properties:..."),
        "folded drawer header shows the ... marker"
    );
    assert!(!screen.contains(":foo: 123"), "drawer body is hidden");

    // The caret stayed on the visible header line, so Tab again unfolds it, and
    // the buffer text is never mutated by folding.
    app.on_key(keycode(KeyCode::Tab));
    assert!(
        !app.editor.active_tab().unwrap().editor.has_folds(),
        "drawer unfolded"
    );
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "* Name\n:properties:\n:foo: 123\n:end:\nbody\n",
        "folding never edits the buffer text"
    );

    // Folding is view-only, so it works even in a read-only buffer (where Tab
    // would otherwise be blocked as an edit key).
    app.run_action("view.read_only");
    assert!(
        app.editor.active_tab().unwrap().read_only,
        "buffer read-only"
    );
    app.run_action("edit.go_first");
    app.on_key(keycode(KeyCode::Down)); // onto the `:properties:` header
    app.on_key(keycode(KeyCode::Tab));
    assert!(
        app.editor.active_tab().unwrap().editor.has_folds(),
        "drawer folds even while read-only"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn emacs_org_ctrl_c_chords_cycle_todo_and_close_with_note() {
    let dir = unique_dir("emacs-org");
    let file = dir.join("todo.org");
    fs::write(&file, "* Task\n").unwrap();
    let mut app = app_at(&dir);
    app.settings.keymap = "emacs".to_string();
    app.open_initial(&file.clone());
    app.run_action("edit.go_first"); // cursor on the headline

    // C-c C-t cycles none -> TODO -> DONE.
    app.on_key(ctrl('c'));
    app.on_key(ctrl('t'));
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .starts_with("* TODO Task"),
        "C-c C-t adds TODO"
    );
    app.on_key(ctrl('c'));
    app.on_key(ctrl('t'));
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .starts_with("* DONE Task"),
        "C-c C-t advances to DONE"
    );

    // C-u C-c C-t opens a closing-note prompt; submitting logs CLOSED + LOGBOOK.
    app.on_key(ctrl('u'));
    app.on_key(ctrl('c'));
    app.on_key(ctrl('t'));
    assert!(
        app.prompt.is_some(),
        "C-u C-c C-t opens the close-note prompt"
    );
    type_str(&mut app, "shipped");
    app.on_key(keycode(KeyCode::Enter));
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("CLOSED: ["), "closed stamp added: {text}");
    assert!(text.contains(":LOGBOOK:"), "logbook drawer added: {text}");
    assert!(text.contains("shipped"), "note body logged: {text}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn org_ctrl_c_ctrl_c_toggles_checkbox() {
    let dir = unique_dir("org-ccc");
    let file = dir.join("list.org");
    fs::write(&file, "- [ ] a\n- [ ] b\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file.clone());
    app.run_action("edit.go_first"); // line 0: - [ ] a

    app.run_action("org.ctrl_c_ctrl_c");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .starts_with("- [x] a"),
        "C-c C-c on a checkbox line marks it done"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn org_agenda_t_cycles_task_in_its_source_file() {
    let dir = unique_dir("org-agenda-t");
    let file = dir.join("work.org");
    fs::write(&file, "* TODO Ship it\n").unwrap();
    let mut app = app_at(&dir);

    app.run_action("org.agenda");
    let text = app.editor.active_tab().unwrap().text();
    assert!(
        text.contains("#+title: Agenda"),
        "agenda buffer opened: {text}"
    );
    assert!(
        text.contains("- TODO Ship it (work.org)"),
        "agenda lists the unscheduled task: {text}"
    );

    // Move the cursor onto the task line and press `t`.
    let line = text
        .split('\n')
        .position(|l| l.contains("Ship it"))
        .unwrap();
    app.run_action("edit.go_first");
    for _ in 0..line {
        app.on_key(keycode(KeyCode::Down));
    }
    app.on_key(key('t'));

    // The source file was cycled to DONE on disk.
    let disk = fs::read_to_string(&file).unwrap();
    assert!(
        disk.contains("* DONE Ship it"),
        "source file cycled to DONE: {disk}"
    );
    // The agenda rebuilt itself; a DONE headline is no longer an unscheduled TODO.
    let text2 = app.editor.active_tab().unwrap().text();
    assert!(
        !text2.contains("Ship it"),
        "DONE task drops from the agenda: {text2}"
    );

    // The agenda buffer is read-only, so a stray letter does not edit it.
    let before = app.editor.active_tab().unwrap().text();
    app.on_key(key('z'));
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        before,
        "agenda buffer stays read-only"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn org_agenda_views_list_todos_matches_searches_and_stuck_projects() {
    let dir = unique_dir("org-agenda-views");
    fs::write(
        dir.join("work.org"),
        "* TODO Ship it :urgent:\n* DONE Old thing\n* Project A\n** TODO next step\n\
         * Project B\n** DONE finished\n** notes only\n",
    )
    .unwrap();
    fs::write(dir.join("notes.org"), "* Meeting\nbudget review here\n").unwrap();
    let mut app = app_at(&dir);

    // Global TODO list: only not-DONE TODO headlines.
    app.run_action("org.agenda.todo");
    let text = app.editor.active_tab().unwrap().text();
    assert!(
        text.contains("#+title: Global TODO List"),
        "todo view title: {text}"
    );
    assert!(
        text.contains("- TODO Ship it :urgent: (work.org)"),
        "lists TODO: {text}"
    );
    assert!(text.contains("- TODO next step (work.org)"));
    assert!(!text.contains("Old thing"), "DONE excluded: {text}");

    // Stuck projects: a project (has children) with no not-DONE child.
    app.run_action("org.agenda.stuck");
    let text = app.editor.active_tab().unwrap().text();
    assert!(
        text.contains("#+title: Stuck Projects"),
        "stuck view title: {text}"
    );
    assert!(
        text.contains("- Project B (work.org)"),
        "Project B is stuck: {text}"
    );
    assert!(
        !text.contains("Project A"),
        "Project A has a next action: {text}"
    );

    // Match view (tags): prompt-driven.
    app.run_action("org.agenda.match");
    assert!(app.prompt.is_some(), "match opens a query prompt");
    type_str(&mut app, "urgent");
    app.on_key(keycode(KeyCode::Enter));
    let text = app.editor.active_tab().unwrap().text();
    assert!(
        text.contains("#+title: Match: urgent"),
        "match view title: {text}"
    );
    assert!(
        text.contains("Ship it"),
        "urgent-tagged headline matched: {text}"
    );

    // Search view (text): prompt-driven, matches entry body.
    app.run_action("org.agenda.search");
    assert!(app.prompt.is_some(), "search opens a query prompt");
    type_str(&mut app, "budget");
    app.on_key(keycode(KeyCode::Enter));
    let text = app.editor.active_tab().unwrap().text();
    assert!(
        text.contains("#+title: Search: budget"),
        "search view title: {text}"
    );
    assert!(
        text.contains("- Meeting (notes.org)"),
        "entry body matched: {text}"
    );

    // `t` in a list view cycles the source task and rebuilds the SAME view.
    app.run_action("org.agenda.todo");
    let text = app.editor.active_tab().unwrap().text();
    let line = text
        .split('\n')
        .position(|l| l.contains("Ship it"))
        .unwrap();
    app.run_action("edit.go_first");
    for _ in 0..line {
        app.on_key(keycode(KeyCode::Down));
    }
    app.on_key(key('t')); // TODO -> DONE
    let rebuilt = app.editor.active_tab().unwrap().text();
    assert!(
        rebuilt.contains("#+title: Global TODO List"),
        "rebuilds the todo view, not the weekly agenda: {rebuilt}"
    );
    assert!(
        !rebuilt.contains("Ship it"),
        "the now-DONE task drops from the list: {rebuilt}"
    );
    assert!(
        fs::read_to_string(dir.join("work.org"))
            .unwrap()
            .contains("* DONE Ship it"),
        "source file updated on disk"
    );

    fs::remove_dir_all(&dir).ok();
}
