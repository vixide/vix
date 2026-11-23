//! Round-2 behavior tests for `app.rs` (test-only module).

use super::*;
use ratatui::layout::Rect;

fn app() -> App {
    let mut app = App::new(std::env::temp_dir(), Settings::default());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

fn app_with(text: &str) -> App {
    let mut a = app();
    a.editor.new_tab_with_content(text);
    a
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        app.on_key(key(KeyCode::Char(c)));
    }
}

fn left_click(col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn cursor(app: &App) -> usize {
    app.editor.active_tab().unwrap().editor.get_cursor()
}

fn type_text(app: &mut App, text: &str) {
    let area = app.layout.editor;
    assert!(app.editor.insert_str(text, area));
}

fn set_cur(app: &mut App, off: usize) {
    app.editor.active_tab_mut().unwrap().editor.set_cursor(off);
}

fn prompt_kind(app: &App) -> Option<PromptKind> {
    app.prompt.as_ref().map(|p| p.kind)
}

// ----- tool dialogs: mouse -------------------------------------------------

#[test]
fn color_converter_click_focuses_the_clicked_field() {
    use crate::color_converter_tool::Field;
    let mut a = app();
    a.run_action("tools.color_converter");
    assert!(a.color_converter.is_some());
    a.layout.color_converter_rows = [
        Rect::new(0, 0, 10, 1),
        Rect::new(0, 1, 10, 1),
        Rect::new(0, 2, 10, 1),
    ];
    assert!(a.try_tool_dialog_mouse(left_click(3, 2)));
    assert_eq!(a.color_converter.as_ref().unwrap().focus, Field::Hsl);
    assert!(a.try_tool_dialog_mouse(left_click(3, 1)));
    assert_eq!(a.color_converter.as_ref().unwrap().focus, Field::Rgb);
    // A click outside every row leaves focus alone but still swallows the event.
    assert!(a.try_tool_dialog_mouse(left_click(50, 20)));
    assert_eq!(a.color_converter.as_ref().unwrap().focus, Field::Rgb);
}

#[test]
fn calculator_click_runs_and_inserts() {
    use crate::calculator_tool::Focus;
    let mut a = app_with("");
    a.run_action("tools.calculator");
    a.calculator.as_mut().unwrap().input = "6*7".to_string();
    a.layout.calculator_rects = [
        Rect::new(0, 0, 10, 1),
        Rect::new(0, 1, 10, 1),
        Rect::new(0, 2, 10, 1),
    ];
    // Clicking Run evaluates and focuses Run.
    assert!(a.try_tool_dialog_mouse(left_click(1, 1)));
    {
        let c = a.calculator.as_ref().unwrap();
        assert_eq!(c.focus, Focus::Run);
        assert_eq!(c.result(), Some("42"));
    }
    // Clicking the input focuses it.
    assert!(a.try_tool_dialog_mouse(left_click(1, 0)));
    assert_eq!(a.calculator.as_ref().unwrap().focus, Focus::Input);
    // Clicking Insert pastes the result into the buffer and closes the dialog.
    assert!(a.try_tool_dialog_mouse(left_click(1, 2)));
    assert!(a.calculator.is_none());
    assert_eq!(a.editor.active_tab().unwrap().text(), "42");
}

#[test]
fn regex_tester_click_focuses_the_clicked_field() {
    use crate::regex_tool::Field;
    let mut a = app();
    a.run_action("tools.regex_tester");
    assert!(a.regex_tester.is_some());
    a.layout.regex_tester_rows = [Rect::new(0, 0, 10, 1), Rect::new(0, 1, 10, 1)];
    assert!(a.try_tool_dialog_mouse(left_click(2, 1)));
    assert_eq!(a.regex_tester.as_ref().unwrap().focus, Field::Subject);
    assert!(a.try_tool_dialog_mouse(left_click(2, 0)));
    assert_eq!(a.regex_tester.as_ref().unwrap().focus, Field::Pattern);
}

#[test]
fn unit_converter_click_focuses_the_clicked_row() {
    use crate::unit_converter_tool::Focus;
    let mut a = app();
    a.run_action("tools.convert.unit");
    assert!(a.unit_converter.is_some());
    a.layout.unit_converter_rows = [
        Rect::new(0, 0, 10, 1),
        Rect::new(0, 1, 10, 1),
        Rect::new(0, 2, 10, 1),
    ];
    assert!(a.try_tool_dialog_mouse(left_click(2, 2)));
    assert_eq!(a.unit_converter.as_ref().unwrap().focus, Focus::To);
    assert!(a.try_tool_dialog_mouse(left_click(2, 1)));
    assert_eq!(a.unit_converter.as_ref().unwrap().focus, Focus::From);
}

#[test]
fn tool_dialog_mouse_is_false_when_no_dialog_is_open() {
    let mut a = app();
    assert!(!a.try_tool_dialog_mouse(left_click(1, 1)));
}

// ----- Go menu -------------------------------------------------------------

#[test]
fn go_menu_goto_actions_open_their_prompts() {
    for (action, kind) in [
        ("nav.goto_paragraph", PromptKind::GotoParagraph),
        ("nav.goto_section", PromptKind::GotoSection),
        ("nav.goto_sentence", PromptKind::GotoSentence),
        ("nav.goto_word", PromptKind::GotoWord),
        ("nav.goto_percent", PromptKind::GotoPercent),
        ("nav.goto_byte", PromptKind::GotoByte),
    ] {
        let mut a = app_with("one two\n");
        a.run_action(action);
        assert!(prompt_kind(&a) == Some(kind), "{action} opens its prompt");
        a.on_key(key(KeyCode::Esc));
        assert!(a.prompt.is_none(), "Esc closes {action}");
    }
}

#[test]
fn go_menu_word_navigation_moves_the_cursor() {
    let mut a = app_with("alpha beta gamma\n");
    set_cur(&mut a, 0);
    a.run_action("nav.word_next");
    assert_eq!(cursor(&a), 6);
    a.run_action("nav.word_next");
    assert_eq!(cursor(&a), 11);
    a.run_action("nav.word_prev");
    assert_eq!(cursor(&a), 6);
    a.run_action("nav.word_end");
    assert!(cursor(&a) >= 6);
    a.run_action("nav.word_start");
    assert_eq!(cursor(&a), 6);
}

#[test]
fn go_menu_goto_word_prompt_jumps_to_the_nth_word() {
    let mut a = app_with("alpha beta gamma\n");
    a.run_action("nav.goto_word");
    type_str(&mut a, "3");
    a.on_key(key(KeyCode::Enter));
    assert!(a.prompt.is_none());
    assert_eq!(cursor(&a), 11);
}

#[test]
fn go_menu_goto_percent_and_byte_prompts() {
    let mut a = app_with("a\nb\nc\nd\ne\n");
    a.run_action("nav.goto_percent");
    type_str(&mut a, "50%");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.editor.cursor_line(), 2, "half way down five-plus lines");
    a.run_action("nav.goto_byte");
    type_str(&mut a, "4");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(cursor(&a), 4);
    // A non-numeric answer is ignored and leaves the cursor alone.
    a.run_action("nav.goto_byte");
    type_str(&mut a, "x");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(cursor(&a), 4);
}

#[test]
fn go_menu_paragraph_section_and_sentence_navigation() {
    let mut a = app_with("one. two. three.\n\nfour\n\nfive\n");
    set_cur(&mut a, 0);
    a.run_action("nav.para_next");
    assert_eq!(a.editor.cursor_line(), 2);
    a.run_action("nav.para_next");
    assert_eq!(a.editor.cursor_line(), 4);
    a.run_action("nav.para_prev");
    assert_eq!(a.editor.cursor_line(), 2);
    a.run_action("nav.section_next");
    a.run_action("nav.section_prev");
    set_cur(&mut a, 0);
    a.run_action("nav.sentence_next");
    assert_eq!(cursor(&a), 5);
    a.run_action("nav.sentence_prev");
    assert_eq!(cursor(&a), 0);
    a.run_action("nav.sentence_end");
    a.run_action("nav.sentence_start");
    a.run_action("nav.line_next");
    a.run_action("nav.line_prev");
    // goto-by-number prompt for paragraphs.
    a.run_action("nav.goto_paragraph");
    type_str(&mut a, "3");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.editor.cursor_line(), 4);
}

#[test]
fn go_menu_lsp_actions_report_inactive_without_a_server() {
    let mut a = app_with("fn x() {}\n");
    a.run_action("nav.goto_declaration");
    assert_eq!(a.status, t!("status.lsp_inactive").to_string());
    a.status.clear();
    a.goto_type_definition();
    assert_eq!(a.status, t!("status.lsp_inactive").to_string());
    a.status.clear();
    a.find_references();
    assert_eq!(a.status, t!("status.lsp_inactive").to_string());
    a.status.clear();
    a.begin_lsp_rename();
    assert_eq!(a.status, t!("status.lsp_inactive").to_string());
    a.status.clear();
    a.request_linked_editing();
    assert_eq!(a.status, t!("status.lsp_inactive").to_string());
}

#[test]
fn lsp_rename_prompt_is_seeded_from_the_server_or_the_word_under_cursor() {
    let mut a = app_with("hello world\n");
    set_cur(&mut a, 1);
    a.open_lsp_rename_prompt(Some("placeholder".to_string()));
    assert_eq!(a.prompt.as_ref().unwrap().input, "placeholder");
    a.prompt = None;
    a.open_lsp_rename_prompt(None);
    assert!(prompt_kind(&a) == Some(PromptKind::LspRename));
    assert_eq!(a.prompt.as_ref().unwrap().input, "hello");
}

// ----- View menu toggles ---------------------------------------------------

#[test]
fn view_settings_toggles_flip_their_setting_each_time() {
    let mut a = app();
    let before = a.settings.save.trim_trailing_whitespace;
    a.run_action("view.trim_on_save");
    assert_eq!(a.settings.save.trim_trailing_whitespace, !before);
    a.run_action("view.trim_on_save");
    assert_eq!(a.settings.save.trim_trailing_whitespace, before);

    let before = a.settings.save.ensure_final_newline;
    a.run_action("view.final_newline_on_save");
    assert_eq!(a.settings.save.ensure_final_newline, !before);

    let before = a.settings.save.format_on_save;
    a.run_action("view.format_on_save");
    assert_eq!(a.settings.save.format_on_save, !before);
    assert_eq!(
        a.status,
        t!("status.format_on_save", on = !before).to_string()
    );

    let before = a.settings.completion.format_on_type;
    a.run_action("view.format_on_type");
    assert_eq!(a.settings.completion.format_on_type, !before);
    assert_eq!(
        a.status,
        t!("status.format_on_type", on = !before).to_string()
    );

    let before = a.settings.editor_behavior.sticky_scroll;
    a.run_action("view.sticky_scroll");
    assert_eq!(a.settings.editor_behavior.sticky_scroll, !before);

    let before = a.settings.viewport.show_minimap;
    a.run_action("view.minimap");
    assert_eq!(a.settings.viewport.show_minimap, !before);

    let before = a.visible.contains(Visible::BREADCRUMBS);
    a.run_action("view.breadcrumbs");
    assert_eq!(a.visible.contains(Visible::BREADCRUMBS), !before);
    assert_eq!(a.settings.secondary_panels.show_breadcrumbs, !before);
}

#[test]
fn view_auto_save_toggles_between_off_and_interval() {
    let mut a = app();
    a.settings.editor_behavior.auto_save = crate::settings::AutoSave::Off;
    a.run_action("view.auto_save");
    assert!(a.settings.editor_behavior.auto_save.is_on());
    a.run_action("view.auto_save");
    assert!(!a.settings.editor_behavior.auto_save.is_on());
}

#[test]
fn view_highlight_word_toggle_resets_the_rebuild_key() {
    let mut a = app_with("foo foo\n");
    let before = a.settings.editor_visual.highlight_word;
    a.run_action("view.highlight_word");
    assert_eq!(a.settings.editor_visual.highlight_word, !before);
    assert!(a.word_highlight_key.is_none());
    a.run_action("view.highlight_word");
    assert_eq!(a.settings.editor_visual.highlight_word, before);
}

#[test]
fn explorer_filter_actions_prefill_the_prompt_with_the_current_filter() {
    let mut a = app();
    a.explorer.include_filter = "*.rs".to_string();
    a.explorer.exclude_filter = "target".to_string();
    a.run_action("explorer.filter_include");
    assert!(prompt_kind(&a) == Some(PromptKind::ExplorerInclude));
    assert_eq!(a.prompt.as_ref().unwrap().input, "*.rs");
    a.prompt = None;
    a.run_action("explorer.filter_exclude");
    assert!(prompt_kind(&a) == Some(PromptKind::ExplorerExclude));
    assert_eq!(a.prompt.as_ref().unwrap().input, "target");
}

#[test]
fn calendar_and_clock_actions_toggle_their_panels() {
    let mut a = app();
    a.run_action("tools.calendar");
    assert!(a.visible.contains(Visible::CALENDAR));
    a.run_action("tools.calendar");
    assert!(!a.visible.contains(Visible::CALENDAR));
    a.clock.selected = 3;
    a.run_action("tools.clock");
    assert!(a.visible.contains(Visible::CLOCK));
    assert_eq!(a.clock.selected, 0);
    a.run_action("tools.clock");
    assert!(!a.visible.contains(Visible::CLOCK));
}

#[test]
fn unknown_action_warns() {
    let mut a = app();
    a.run_action("no.such.action");
    assert!(
        a.messages
            .items
            .iter()
            .any(|m| m.text == t!("msg.unknown_action", action = "no.such.action"))
    );
}

#[test]
fn help_actions_open_their_overlays() {
    let mut a = app();
    a.run_action("vix.about");
    let d = a.dialog.as_ref().expect("about dialog");
    assert!(d.body.contains(env!("CARGO_PKG_VERSION")));
    a.dialog = None;
    a.run_action("vix.website");
    assert!(
        a.dialog
            .as_ref()
            .unwrap()
            .body
            .contains("github.com/vixide/vix")
    );
    a.dialog = None;
    a.run_action("vix.email");
    assert!(a.dialog.as_ref().unwrap().body.contains('@'));
    a.run_action("help.doctor");
    assert!(a.welcome.is_some());
}

#[test]
fn project_actions_open_a_command_prompt_per_slot() {
    use crate::tasks::lifecycle::LifecycleCommands;
    let slots = [
        ProjectSlot::Configure,
        ProjectSlot::Compile,
        ProjectSlot::Test,
        ProjectSlot::Install,
        ProjectSlot::Package,
        ProjectSlot::Run,
    ];
    let mut cmds = LifecycleCommands::default();
    for (i, s) in slots.into_iter().enumerate() {
        assert_eq!(s.get(&cmds), None);
        s.set(&mut cmds, Some(format!("cmd{i}")));
        assert_eq!(s.get(&cmds), Some(format!("cmd{i}")));
        assert!(s.prompt_title_key().starts_with("prompt.project_"));
    }
    let mut h = ProjectHistory::default();
    for (i, s) in slots.into_iter().enumerate() {
        s.history_mut(&mut h).push(format!("h{i}"));
        assert_eq!(s.history_mut(&mut h).len(), 1);
    }
}

#[test]
fn keymap_ids_round_trip() {
    for id in [
        "apple",
        "emacs",
        "vi",
        "spacemacs",
        "intellij-macos",
        "intellij-windows",
        "eclipse",
        "sublime",
        "zed",
        "fresh",
        "ttt",
        "helix",
    ] {
        let km = Keymap::from_id(id).unwrap_or_else(|| panic!("{id}"));
        assert_eq!(km.id(), id);
    }
    assert_eq!(Keymap::Vscode.id(), "vscode-macos");
    assert!(Keymap::from_id("nope").is_none());
}

#[test]
fn pane_tree_session_conversion_round_trips() {
    use crate::pane_tree::Pane;
    use crate::session::PaneNode;
    let node = PaneNode::Split {
        dir: "horizontal".to_string(),
        ratio: 500,
        first: Box::new(PaneNode::Leaf(0)),
        second: Box::new(PaneNode::Split {
            dir: "vertical".to_string(),
            ratio: 1,
            first: Box::new(PaneNode::Leaf(1)),
            second: Box::new(PaneNode::Leaf(9)),
        }),
    };
    let pane = node_to_pane(&node, 2);
    let Pane::Split {
        dir, ratio, second, ..
    } = &pane
    else {
        panic!("split");
    };
    assert_eq!(*dir, crate::editor::SplitDir::Horizontal);
    assert_eq!(*ratio, 90, "ratio clamped to 10..=90");
    let Pane::Split {
        ratio: r2,
        second: leaf,
        ..
    } = &**second
    else {
        panic!("inner split");
    };
    assert_eq!(*r2, 10);
    assert!(matches!(**leaf, Pane::Leaf(1)), "leaf clamped to tab count");

    // Back to a node: tabs map to file indices; an unmapped tab aborts.
    let back = pane_to_node(&pane, &[Some(0), Some(5)]).expect("all mapped");
    let PaneNode::Split { dir, second, .. } = back else {
        panic!("split");
    };
    assert_eq!(dir, "horizontal");
    let PaneNode::Split { dir, second, .. } = *second else {
        panic!("split");
    };
    assert_eq!(dir, "vertical");
    assert!(matches!(*second, PaneNode::Leaf(5)));
    assert!(pane_to_node(&pane, &[Some(0), None]).is_none());
}

// ----- fixtures -------------------------------------------------------------

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("vix-r2a-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn app_in(dir: &Path) -> App {
    let mut a = App::new(dir.to_path_buf(), Settings::default());
    a.layout.editor = Rect::new(0, 0, 80, 24);
    a
}

fn active_text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
}

// ----- discovered tasks ---------------------------------------------------

#[test]
fn discovered_tasks_merge_every_manifest_found_in_the_directory() {
    let dir = tmp("tasks");
    let w = |n: &str, body: &str| {
        let p = dir.join(n);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    };
    w("package.json", r#"{"scripts":{"build":"tsc"}}"#);
    w("deno.json", r#"{"tasks":{"serve":"deno run x.ts"}}"#);
    w("composer.json", r#"{"scripts":{"lint":"phpcs"}}"#);
    w("justfile", "jb:\n\techo hi\n");
    w(
        "Taskfile.yml",
        "version: '3'\ntasks:\n  tf:\n    cmds: [echo]\n",
    );
    w("Rakefile", "task :rk do\nend\n");
    w("extra.rake", "task :ex do\nend\n");
    w("rakelib/lib.rake", "task :lb do\nend\n");
    w("Gemfile", "source 'https://rubygems.org'\n");
    w("Makefile", "mk:\n\techo\n");
    let tasks = App::discovered_tasks_at(&dir);
    let find = |n: &str| tasks.iter().find(|t| t.name == n);
    assert!(tasks.iter().any(|t| t.name.ends_with(":build")), "npm");
    assert_eq!(find("deno:serve").unwrap().command, "deno task serve");
    assert_eq!(
        find("composer:lint").unwrap().command,
        "composer run-script lint"
    );
    assert_eq!(find("just:jb").unwrap().command, "just jb");
    assert_eq!(find("task:tf").unwrap().command, "task tf");
    assert_eq!(find("rake:rk").unwrap().command, "bundle exec rake rk");
    assert!(find("rake:ex").is_some(), "top-level .rake file");
    assert!(find("rake:lb").is_some(), "rakelib .rake file");
    assert_eq!(find("make:mk").unwrap().command, "make mk");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn discovered_tasks_use_alternate_manifest_spellings() {
    let dir = tmp("tasks-alt");
    std::fs::write(dir.join("deno.jsonc"), r#"{"tasks":{"d":"x"}}"#).unwrap();
    std::fs::write(dir.join(".justfile"), "jj:\n\techo\n").unwrap();
    std::fs::write(dir.join("Taskfile.yaml"), "tasks:\n  ty:\n    cmds: [x]\n").unwrap();
    std::fs::write(dir.join("GNUmakefile"), "gm:\n\techo\n").unwrap();
    let names: Vec<String> = App::discovered_tasks_at(&dir)
        .into_iter()
        .map(|t| t.name)
        .collect();
    for n in ["deno:d", "just:jj", "task:ty", "make:gm"] {
        assert!(names.iter().any(|x| x == n), "{n} in {names:?}");
    }
    assert_eq!(App::discovered_tasks_at(&tmp("tasks-empty")).len(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_task_without_any_tasks_says_so() {
    let dir = tmp("no-tasks");
    let mut a = app_in(&dir);
    a.run_action("project.run_task");
    assert_eq!(a.status, t!("status.no_tasks").to_string());
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- text conversion actions ----------------------------------------------

fn converts<E: std::fmt::Display>(
    action: &str,
    input: &str,
    expected: impl Fn(&str) -> Result<String, E>,
) {
    let mut a = app_with(input);
    a.run_action(action);
    if let Ok(out) = expected(input) {
        assert_eq!(active_text(&a), out, "{action}");
    } else {
        assert_eq!(
            active_text(&a),
            input,
            "{action} leaves the buffer on error"
        );
        assert_ne!(a.status, "");
    }
}

#[test]
fn convert_and_format_actions_apply_the_matching_tool() {
    use crate::{base_tool as bt, base64_tool as b64, format_tool as ft, url_tool as ut};
    converts("tools.convert.base64.encode", "hello", b64::encode);
    converts("tools.convert.base64.decode", "aGVsbG8=", b64::decode);
    converts("tools.convert.url.encode", "a b&c", ut::encode);
    converts("tools.convert.url.decode", "a%20b", ut::decode);
    converts("tools.convert.number.dec", "0xff", bt::to_dec);
    converts("tools.convert.number.hex", "255", bt::to_hex);
    converts("tools.convert.number.bin", "5", bt::to_bin);
    converts("tools.convert.number.oct", "8", bt::to_oct);
    converts(
        "tools.convert.csv.json",
        "a,b\n1,2\n",
        crate::convert_from_csv_into_json_tool::convert,
    );
    converts(
        "tools.convert.csv.tsv",
        "a,b\n1,2\n",
        crate::convert_from_csv_into_tsv_tool::convert,
    );
    converts(
        "tools.convert.tsv.csv",
        "a\tb\n1\t2\n",
        crate::convert_from_tsv_into_csv_tool::convert,
    );
    converts(
        "tools.convert.tsv.json",
        "a\tb\n1\t2\n",
        crate::convert_from_tsv_into_json_tool::convert,
    );
    converts(
        "tools.convert.json.csv",
        r#"[{"a":1,"b":2}]"#,
        crate::convert_from_json_into_csv_tool::convert,
    );
    converts(
        "tools.convert.json.tsv",
        r#"[{"a":1,"b":2}]"#,
        crate::convert_from_json_into_tsv_tool::convert,
    );
    converts(
        "tools.convert.json.yaml",
        r#"{"a":1}"#,
        crate::convert_from_json_into_yaml_tool::convert,
    );
    converts(
        "tools.convert.yaml.json",
        "a: 1\n",
        crate::convert_from_yaml_into_json_tool::convert,
    );
    converts(
        "tools.convert.json.toml",
        r#"{"a":1}"#,
        crate::convert_from_json_into_toml_tool::convert,
    );
    converts(
        "tools.convert.toml.json",
        "a = 1\n",
        crate::convert_from_toml_into_json_tool::convert,
    );
    converts(
        "tools.convert.markdown.html",
        "# hi\n",
        crate::convert_from_markdown_into_html_tool::convert,
    );
    converts(
        "tools.convert.html.markdown",
        "<h1>hi</h1>",
        crate::convert_from_html_into_markdown_tool::convert,
    );
    converts(
        "tools.convert.jwt",
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.c2ln",
        crate::jwt_tool::decode,
    );
    converts("tools.format.json_pretty", r#"{"a":1}"#, ft::json_pretty);
    converts(
        "tools.format.json_minify",
        "{\n \"a\": 1\n}",
        ft::json_minify,
    );
    converts("tools.format.yaml", "a:   1\n", ft::yaml_format);
    converts("tools.format.toml", "a   =  1\n", ft::toml_format);
}

#[test]
fn checksum_actions_replace_the_buffer_with_the_digest() {
    for (action, f) in [
        (
            "tools.checksum.sha256",
            crate::checksum_tool::sha256_hex as fn(&str) -> String,
        ),
        ("tools.checksum.sha512", crate::checksum_tool::sha512_hex),
        ("tools.checksum.md5", crate::checksum_tool::md5_hex),
        ("tools.checksum.crc32", crate::checksum_tool::crc32_hex),
    ] {
        let mut a = app_with("abc");
        a.run_action(action);
        assert_eq!(active_text(&a), f("abc"), "{action}");
    }
}

#[test]
fn convert_action_failure_reports_and_keeps_the_buffer() {
    let mut a = app_with("%%%");
    a.run_action("tools.convert.base64.decode");
    assert_eq!(active_text(&a), "%%%");
    assert_ne!(a.status, "");
}

#[test]
fn convert_action_on_an_empty_buffer_reports_no_selection() {
    let mut a = app_with("");
    a.run_action("tools.convert.base64.encode");
    assert_eq!(a.status, t!("status.no_selection").to_string());
}

// ----- prompts accepted ---------------------------------------------------------

fn submit(app: &mut App, kind: PromptKind, input: &str) {
    app.prompt = Some(Prompt::new(kind, "t".to_string()).with_input(input.to_string()));
    app.accept_prompt();
    assert!(app.prompt.is_none());
}

#[test]
fn open_prompt_accepts_a_path_with_a_line_and_column_target() {
    let dir = tmp("open-prompt");
    std::fs::write(dir.join("f.txt"), "one\ntwo\nthree\n").unwrap();
    let mut a = app_in(&dir);
    submit(&mut a, PromptKind::Open, "f.txt:3:2");
    assert_eq!(a.editor.cursor_line(), 2);
    assert_eq!(a.focus, Focus::Editor);
    // An empty path is ignored.
    let tabs = a.editor.tabs.len();
    submit(&mut a, PromptKind::Open, "  ");
    assert_eq!(a.editor.tabs.len(), tabs);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_as_prompt_writes_a_new_file_and_confirms_before_overwriting() {
    let dir = tmp("save-as");
    let mut a = app_in(&dir);
    a.editor.new_tab_with_content("fresh");
    submit(&mut a, PromptKind::SaveAs, "new.txt");
    assert_eq!(
        std::fs::read_to_string(dir.join("new.txt")).unwrap(),
        "fresh\n"
    );
    assert_eq!(
        a.editor.active_tab().unwrap().path.as_deref(),
        Some(dir.join("new.txt").as_path())
    );
    // Saving a second buffer over an existing file asks first.
    std::fs::write(dir.join("old.txt"), "old").unwrap();
    a.editor.new_tab_with_content("other");
    submit(&mut a, PromptKind::SaveAs, "old.txt");
    assert!(a.confirm.is_some(), "overwrite needs confirmation");
    assert_eq!(std::fs::read_to_string(dir.join("old.txt")).unwrap(), "old");
    a.on_key(key(KeyCode::Char('n')));
    assert!(a.confirm.is_none());
    assert_eq!(a.status, t!("status.save_as_cancelled").to_string());
    assert_eq!(std::fs::read_to_string(dir.join("old.txt")).unwrap(), "old");
    submit(&mut a, PromptKind::SaveAs, "old.txt");
    a.on_key(key(KeyCode::Char('y')));
    assert_eq!(
        std::fs::read_to_string(dir.join("old.txt")).unwrap(),
        "other\n"
    );
    // Empty input does nothing.
    submit(&mut a, PromptKind::SaveAs, "");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_as_refuses_a_dangling_symlink() {
    let dir = tmp("save-as-link");
    let link = dir.join("dangling");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.join("nowhere"), &link).unwrap();
        let mut a = app_in(&dir);
        a.editor.new_tab_with_content("x");
        a.request_save_as(link.clone());
        assert!(a.confirm.is_none());
        assert!(
            a.messages
                .items
                .iter()
                .any(|m| m.text == t!("msg.save_as_dangling_symlink", path = link.display()))
        );
        assert!(!dir.join("nowhere").exists());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rename_prompt_renames_the_active_file_on_disk() {
    let dir = tmp("rename");
    std::fs::write(dir.join("a.txt"), "hi").unwrap();
    std::fs::write(dir.join("taken.txt"), "x").unwrap();
    let mut a = app_in(&dir);
    a.open_path(&dir.join("a.txt"), false);
    a.run_action("file.rename");
    assert!(prompt_kind(&a) == Some(PromptKind::Rename));
    assert_eq!(a.prompt.as_ref().unwrap().input, "a.txt");
    a.prompt = None;
    // Same name: a no-op. Existing target: refused. Empty: ignored.
    submit(&mut a, PromptKind::Rename, "a.txt");
    submit(&mut a, PromptKind::Rename, "  ");
    submit(&mut a, PromptKind::Rename, "taken.txt");
    assert_eq!(
        a.status,
        t!(
            "status.rename_exists",
            name = dir.join("taken.txt").display()
        )
        .to_string()
    );
    assert!(dir.join("a.txt").exists());
    submit(&mut a, PromptKind::Rename, "b.txt");
    assert!(!dir.join("a.txt").exists());
    assert!(dir.join("b.txt").exists());
    assert_eq!(
        a.editor.active_tab().unwrap().path.as_deref(),
        Some(dir.join("b.txt").as_path())
    );
    assert_eq!(
        a.status,
        t!("status.renamed", path = dir.join("b.txt").display()).to_string()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rename_without_a_file_backed_tab_reports_it() {
    let mut a = app_with("scratch");
    a.run_action("file.rename");
    assert_eq!(a.status, t!("status.rename_no_file").to_string());
    submit(&mut a, PromptKind::Rename, "x.txt");
    assert_eq!(a.status, t!("status.rename_no_file").to_string());
}

#[test]
fn explorer_filter_prompts_set_include_and_exclude() {
    let mut a = app();
    submit(&mut a, PromptKind::ExplorerInclude, " *.rs ");
    assert_eq!(a.explorer.include_filter, "*.rs");
    submit(&mut a, PromptKind::ExplorerExclude, "target");
    assert_eq!(a.explorer.exclude_filter, "target");
    assert_eq!(a.explorer.include_filter, "*.rs", "include survives");
}

#[test]
fn insert_file_prompt_inserts_the_file_at_the_cursor() {
    let dir = tmp("insert-file");
    std::fs::write(dir.join("snip.txt"), "SNIP").unwrap();
    let mut a = app_in(&dir);
    a.editor.new_tab_with_content("ab");
    set_cur(&mut a, 1);
    submit(&mut a, PromptKind::InsertFile, "snip.txt");
    assert_eq!(active_text(&a), "aSNIPb");
    submit(&mut a, PromptKind::InsertFile, "");
    assert_eq!(active_text(&a), "aSNIPb");
    submit(&mut a, PromptKind::InsertFile, "missing.txt");
    assert!(
        a.messages
            .items
            .iter()
            .any(|m| m.text == t!("msg.open_failed", error = dir.join("missing.txt").display()))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_settings_prompt_writes_a_bundle_file() {
    let dir = tmp("export-settings");
    let mut a = app_in(&dir);
    submit(&mut a, PromptKind::ExportSettings, "bundle.toml");
    assert!(dir.join("bundle.toml").exists());
    assert_eq!(
        a.status,
        t!(
            "status.settings_exported",
            path = dir.join("bundle.toml").display()
        )
        .to_string()
    );
    submit(&mut a, PromptKind::ExportSettings, "");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn import_settings_of_a_missing_bundle_reports_the_failure() {
    let dir = tmp("import-settings");
    let mut a = app_in(&dir);
    submit(&mut a, PromptKind::ImportSettings, "absent.toml");
    assert!(a.messages.items.iter().any(|m| {
        m.text.starts_with(
            t!("msg.settings_import_failed", error = "")
                .to_string()
                .trim_end_matches(|c: char| !c.is_alphanumeric()),
        )
    }));
    submit(&mut a, PromptKind::ImportSettings, "");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lsp_rename_prompt_without_a_server_clears_the_pending_rename() {
    let mut a = app_with("x");
    a.rename_at = Some((PathBuf::from("/x.rs"), 0, 0));
    submit(&mut a, PromptKind::LspRename, "newname");
    assert!(a.rename_at.is_none());
    // WorkspaceSymbol with no server is a quiet no-op.
    submit(&mut a, PromptKind::WorkspaceSymbol, "sym");
}

#[test]
fn workspace_search_prompt_lists_hits_in_the_bottom_dock() {
    let dir = tmp("dock-search");
    std::fs::write(dir.join("a.txt"), "needle here\nnothing\n").unwrap();
    std::fs::write(dir.join("b.txt"), "no match\n").unwrap();
    let mut a = app_in(&dir);
    a.prompt = Some(
        Prompt::new(PromptKind::SearchToDock, "s".to_string()).with_input("NEEDLE".to_string()),
    );
    a.accept_prompt();
    assert!(a.visible.contains(Visible::BOTTOM_DOCK));
    let joined = a.bottom_dock.lines.join("\n");
    assert!(joined.contains("a.txt:1:1"), "{joined}");
    assert!(joined.contains("[1 matches in"), "{joined}");
    // Case-sensitive literal search misses the lower-case text.
    a.bottom_dock.lines.clear();
    let mut p =
        Prompt::new(PromptKind::SearchToDock, "s".to_string()).with_input("NEEDLE".to_string());
    p.case_sensitive = true;
    a.prompt = Some(p);
    a.accept_prompt();
    assert!(a.bottom_dock.lines.join("\n").contains("[0 matches in"));
    // A bad regex is reported, not run.
    let mut p = Prompt::new(PromptKind::SearchToDock, "s".to_string()).with_input("(".to_string());
    p.regex = true;
    a.prompt = Some(p);
    a.accept_prompt();
    assert!(
        a.bottom_dock
            .lines
            .iter()
            .any(|l| l.starts_with("[bad regex"))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- linked editing / symbols ------------------------------------------------

#[test]
fn linked_edit_rewrites_every_captured_range() {
    let mut a = app_with("<div></div>");
    a.linked_ranges = Some(vec![(1, 4), (7, 10)]);
    submit(&mut a, PromptKind::LinkedEdit, "span");
    assert_eq!(active_text(&a), "<span></span>");
    assert!(a.linked_ranges.is_none());
    assert_eq!(a.status, t!("status.linked_edited").to_string());
    assert!(a.editor.active_tab().unwrap().dirty);
    // No captured ranges: nothing happens.
    submit(&mut a, PromptKind::LinkedEdit, "x");
    assert_eq!(active_text(&a), "<span></span>");
}

#[test]
fn workspace_symbols_open_a_sorted_static_result_panel() {
    let dir = tmp("symbols");
    let mut a = app_in(&dir);
    a.show_workspace_symbols(&[]);
    assert_eq!(a.status, t!("status.no_symbols").to_string());
    assert!(a.workspace_search.is_none());
    a.show_workspace_symbols(&[
        (dir.join("b.rs"), 4, 2, "zeta".to_string()),
        (dir.join("a.rs"), 0, 0, "alpha".to_string()),
    ]);
    let ws = a.workspace_search.as_ref().expect("panel");
    assert!(ws.flags.contains(WorkspaceFlags::STATIC_RESULTS));
    assert_eq!(ws.hits.len(), 2);
    assert_eq!(ws.hits[0].text, "alpha");
    assert_eq!(ws.hits[0].display, "alpha  a.rs:1");
    assert_eq!((ws.hits[1].line, ws.hits[1].col), (5, 3));
    assert_eq!(ws.status, t!("status.symbols_n", n = 2).to_string());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn references_and_document_symbols_open_static_result_panels() {
    let dir = tmp("refs");
    std::fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
    let mut a = app_in(&dir);
    a.show_references(&[]);
    assert_eq!(a.status, t!("status.no_references").to_string());
    a.show_references(&[(dir.join("a.rs"), 2, 1), (dir.join("a.rs"), 0, 0)]);
    let ws = a.workspace_search.take().unwrap();
    assert_eq!(ws.hits.len(), 2);
    assert_eq!(ws.hits[0].display, "a.rs:1");
    assert_eq!(ws.status, t!("status.references_n", n = 2).to_string());

    // Document symbols need an active file.
    a.show_document_symbols(&[(1, 0, "x".to_string())]);
    assert!(a.workspace_search.is_none(), "no active path: ignored");
    a.open_path(&dir.join("a.rs"), false);
    a.show_document_symbols(&[]);
    assert_eq!(a.status, t!("status.no_symbols").to_string());
    a.show_document_symbols(&[(5, 0, "later".to_string()), (0, 3, "first".to_string())]);
    let ws = a.workspace_search.take().unwrap();
    assert_eq!(ws.hits[0].text, "first", "sorted by line");
    assert_eq!(ws.hits[0].display, "first  :1");
    assert_eq!(ws.hits[1].line, 6);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn request_document_symbols_without_a_server_reports_inactive() {
    let mut a = app_with("x");
    a.request_document_symbols();
    assert_eq!(a.status, t!("status.lsp_inactive").to_string());
}

#[test]
fn root_relative_strips_the_root_even_through_a_symlinked_prefix() {
    let dir = tmp("relroot");
    let a = app_in(&dir);
    assert_eq!(a.root_relative(&dir.join("x/y.rs")), "x/y.rs");
    let other = PathBuf::from("/definitely/elsewhere/z.rs");
    assert_eq!(a.root_relative(&other), "/definitely/elsewhere/z.rs");
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- bookmarks -------------------------------------------------------------

#[test]
fn bookmarks_toggle_and_navigate_across_files_with_wrapping() {
    let dir = tmp("bookmarks");
    std::fs::write(dir.join("a.txt"), "1\n2\n3\n").unwrap();
    std::fs::write(dir.join("b.txt"), "1\n2\n3\n").unwrap();
    let mut a = app_in(&dir);
    a.run_action("bookmark.next");
    assert_eq!(a.status, t!("status.no_bookmarks").to_string());
    a.run_action("bookmark.list");
    assert_eq!(a.status, t!("status.no_bookmarks").to_string());

    // A scratch buffer has no location: toggling does nothing.
    a.editor.new_tab_with_content("x");
    a.run_action("bookmark.toggle");
    assert!(a.bookmarks.is_empty());

    a.open_path(&dir.join("a.txt"), false);
    a.run_action("bookmark.toggle");
    assert_eq!(a.status, t!("status.bookmark_added").to_string());
    a.open_path(&dir.join("b.txt"), false);
    let area = a.layout.editor;
    a.editor.goto(2, None, area);
    a.run_action("bookmark.toggle");
    assert_eq!(a.bookmarks.len(), 2);

    // From b.txt:2, next wraps to a.txt:1; next again goes to b.txt:2.
    a.run_action("bookmark.next");
    assert!(a.active_path().unwrap().ends_with("a.txt"));
    a.run_action("bookmark.next");
    assert!(a.active_path().unwrap().ends_with("b.txt"));
    assert_eq!(a.editor.cursor_line(), 1);
    a.run_action("bookmark.prev");
    assert!(a.active_path().unwrap().ends_with("a.txt"));
    a.run_action("bookmark.prev");
    assert!(a.active_path().unwrap().ends_with("b.txt"));

    a.run_action("bookmark.list");
    assert_eq!(a.chooser.location().map(|c| c.entries.len()), Some(2));
    a.chooser.take_location();

    // Toggling again on the same line removes it.
    a.run_action("bookmark.toggle");
    assert_eq!(a.status, t!("status.bookmark_removed").to_string());
    assert_eq!(a.bookmarks.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- open failures -------------------------------------------------------------

#[test]
fn opening_a_binary_file_asks_then_opens_the_byte_editor() {
    let dir = tmp("binary");
    let p = dir.join("blob.bin");
    std::fs::write(&p, [0u8, 1, 2, 3, 0xff, 0, 0, 9]).unwrap();
    let mut a = app_in(&dir);
    a.open_path(&p, false);
    let c = a.confirm.as_ref().expect("asks first");
    assert_eq!(c.kind, ConfirmKind::OpenBinary);
    assert_eq!(c.paths, vec![p.clone()]);
    a.on_key(key(KeyCode::Char('n')));
    assert!(a.confirm.is_none());
    assert_eq!(a.status, t!("status.open_cancelled").to_string());
    assert!(a.edit_bytes_file.is_none());
    // An arrow-key preview only reports in the status line.
    a.open_path(&p, true);
    assert!(a.confirm.is_none());
    a.open_path(&p, false);
    a.on_key(key(KeyCode::Char('y')));
    assert_eq!(a.edit_bytes_file.as_deref(), Some(p.as_path()));
    assert!(a.confirm.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn opening_a_huge_file_offers_the_large_file_viewer() {
    let dir = tmp("huge");
    let p = dir.join("big.log");
    let f = std::fs::File::create(&p).unwrap();
    f.set_len(crate::fileops::MAX_TEXT_BYTES + 1024).unwrap();
    drop(f);
    let mut a = app_in(&dir);
    a.open_path(&p, false);
    let c = a.confirm.as_ref().expect("asks first");
    assert_eq!(c.kind, ConfirmKind::OpenLarge);
    a.on_key(key(KeyCode::Char('x')));
    assert!(a.confirm.is_some(), "an unrelated key leaves the question");
    a.on_key(key(KeyCode::Char('v')));
    assert!(a.confirm.is_none());
    assert!(a.large_file.is_some(), "viewer opened");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn opening_a_directory_or_missing_file_reports_an_error() {
    let dir = tmp("open-fail");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let mut a = app_in(&dir);
    let before = a.messages.items.len();
    a.open_path(&dir.join("missing.txt"), false);
    assert!(a.messages.items.len() > before);
    assert!(a.confirm.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- explorer clipboard / paste ---------------------------------------------

fn paste_fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let base = tmp(name);
    let src = base.join("src");
    let dst = base.join("dst");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    for n in ["f1.txt", "f2.txt", "f3.txt"] {
        std::fs::write(src.join(n), format!("new-{n}")).unwrap();
        std::fs::write(dst.join(n), format!("old-{n}")).unwrap();
    }
    (base, src, dst)
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap()
}

fn paste_op(src: &Path, dst: &Path, cut: bool) -> PasteOp {
    PasteOp {
        target: dst.to_path_buf(),
        cut,
        queue: ["f1.txt", "f2.txt", "f3.txt"]
            .iter()
            .map(|n| src.join(n))
            .collect(),
        overwrite_all: false,
        skip_all: false,
        conflict: None,
    }
}

#[test]
fn paste_conflicts_are_resolved_one_key_at_a_time() {
    let (base, src, dst) = paste_fixture("paste-conflicts");
    let mut a = app_in(&base);
    a.paste = Some(paste_op(&src, &dst, false));
    a.process_paste();
    assert_eq!(a.paste.as_ref().unwrap().conflict, Some(src.join("f1.txt")));
    a.on_key(key(KeyCode::Char('s'))); // skip f1
    assert_eq!(a.paste.as_ref().unwrap().conflict, Some(src.join("f2.txt")));
    assert_eq!(read(&dst.join("f1.txt")), "old-f1.txt");
    a.on_key(key(KeyCode::Char('o'))); // overwrite f2 only
    assert_eq!(read(&dst.join("f2.txt")), "new-f2.txt");
    assert_eq!(a.paste.as_ref().unwrap().conflict, Some(src.join("f3.txt")));
    a.on_key(key(KeyCode::Char('x'))); // unrelated key: still waiting
    assert!(a.paste.is_some());
    a.on_key(key(KeyCode::Char('O'))); // overwrite the rest
    assert_eq!(read(&dst.join("f3.txt")), "new-f3.txt");
    assert!(a.paste.is_none());
    assert_eq!(a.status, t!("status.paste_complete").to_string());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn paste_skip_all_and_cancel() {
    let (base, src, dst) = paste_fixture("paste-skip");
    let mut a = app_in(&base);
    a.paste = Some(paste_op(&src, &dst, false));
    a.process_paste();
    a.on_key(key(KeyCode::Char('S')));
    assert!(a.paste.is_none(), "skip-all drains the queue");
    for n in ["f1.txt", "f2.txt", "f3.txt"] {
        assert_eq!(read(&dst.join(n)), format!("old-{n}"));
    }
    a.paste = Some(paste_op(&src, &dst, false));
    a.process_paste();
    a.on_key(key(KeyCode::Esc));
    assert!(a.paste.is_none());
    assert_eq!(a.status, t!("status.paste_cancelled").to_string());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn paste_overwrite_all_flag_replaces_without_asking() {
    let (base, src, dst) = paste_fixture("paste-overwrite-all");
    let mut a = app_in(&base);
    let mut op = paste_op(&src, &dst, false);
    op.overwrite_all = true;
    a.paste = Some(op);
    a.process_paste();
    assert!(a.paste.is_none());
    assert_eq!(read(&dst.join("f2.txt")), "new-f2.txt");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn paste_cut_moves_files_and_repoints_open_buffers() {
    let (base, src, dst) = paste_fixture("paste-cut");
    let mut a = app_in(&base);
    a.open_path(&src.join("f1.txt"), false);
    let mut op = paste_op(&src, &dst, true);
    op.overwrite_all = true;
    a.clip = vec![src.join("f1.txt")];
    a.flags.insert(AppFlags::CLIP_CUT);
    a.paste = Some(op);
    a.process_paste();
    assert!(!src.join("f1.txt").exists());
    assert_eq!(read(&dst.join("f1.txt")), "new-f1.txt");
    assert!(a.clip.is_empty(), "a finished cut clears the clipboard");
    assert!(!a.flags.contains(AppFlags::CLIP_CUT));
    assert_eq!(
        a.editor.active_tab().unwrap().path.as_deref(),
        Some(dst.join("f1.txt").as_path()),
        "the open buffer follows the move"
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn paste_into_the_same_directory_copies_under_a_new_name_and_drops_a_cut() {
    let (base, src, _dst) = paste_fixture("paste-same");
    let mut a = app_in(&base);
    let mut op = paste_op(&src, &src, false);
    op.queue.truncate(1);
    a.paste = Some(op);
    a.process_paste();
    let names: Vec<String> = std::fs::read_dir(&src)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 4, "a copy appeared: {names:?}");
    let copy = names
        .iter()
        .find(|n| n.contains("copy"))
        .expect("copy name");
    assert_eq!(read(&src.join(copy)), "new-f1.txt");
    // Cutting into its own directory is a no-op.
    let mut op = paste_op(&src, &src, true);
    op.queue.truncate(1);
    a.paste = Some(op);
    a.process_paste();
    assert!(src.join("f1.txt").exists());
    assert_eq!(std::fs::read_dir(&src).unwrap().count(), 4);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn explorer_keys_copy_cut_and_cancel_the_clipboard() {
    let dir = tmp("explorer-keys");
    std::fs::write(dir.join("a.txt"), "a").unwrap();
    std::fs::write(dir.join("b.txt"), "b").unwrap();
    let mut a = app_in(&dir);
    a.explorer.rebuild();
    a.focus = Focus::Explorer;
    a.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(a.clip.len(), 1);
    assert_eq!(a.status, t!("status.copied_n", n = 1).to_string());
    assert!(!a.flags.contains(AppFlags::CLIP_CUT));
    a.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
    assert!(a.flags.contains(AppFlags::CLIP_CUT));
    assert_eq!(a.status, t!("status.cut_n", n = 1).to_string());
    a.on_key(key(KeyCode::Esc));
    assert_eq!(a.clip.len(), 0);
    assert_eq!(a.status, t!("status.cut_cancelled").to_string());
    // With nothing marked, Esc hands focus back to the editor.
    a.on_key(key(KeyCode::Esc));
    assert_eq!(a.focus, Focus::Editor);
    // Pasting an empty clipboard does nothing.
    a.focus = Focus::Explorer;
    a.on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));
    assert!(a.paste.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn explorer_navigation_keys_move_the_selection() {
    let dir = tmp("explorer-nav");
    for n in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(dir.join(n), n).unwrap();
    }
    let mut a = app_in(&dir);
    a.explorer.rebuild();
    a.focus = Focus::Explorer;
    a.on_key(key(KeyCode::End));
    let last = a.explorer.selected_node().unwrap().path.clone();
    a.on_key(key(KeyCode::Home));
    let first = a.explorer.selected_node().unwrap().path.clone();
    assert_ne!(first, last);
    a.on_key(key(KeyCode::Down));
    assert_ne!(a.explorer.selected_node().unwrap().path, first);
    a.on_key(key(KeyCode::Up));
    assert_eq!(a.explorer.selected_node().unwrap().path, first);
    a.on_key(key(KeyCode::PageDown));
    assert_eq!(a.explorer.selected_node().unwrap().path, last);
    a.on_key(key(KeyCode::PageUp));
    assert_eq!(a.explorer.selected_node().unwrap().path, first);
    // Shift+Down extends a multi-selection, which Esc clears.
    a.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    a.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT));
    a.on_key(key(KeyCode::Esc));
    assert!(a.explorer.marked.is_empty());
    // Enter opens the selected file in a tab.
    a.explorer.first();
    a.on_key(key(KeyCode::Enter));
    assert!(a.editor.active_tab().unwrap().path.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- timers and polling ---------------------------------------------------------

fn seconds_ago(n: u64) -> std::time::Instant {
    std::time::Instant::now()
        .checked_sub(std::time::Duration::from_secs(n))
        .expect("monotonic clock far enough from its epoch")
}

#[test]
fn pomodoro_dialog_starts_counts_down_and_finishes() {
    use crate::pomodoro_tool::Phase;
    let mut a = app();
    assert!(!a.pomodoro_running());
    a.poll_pomodoro(); // idle: nothing to do
    a.run_action("tools.pomodoro");
    assert!(a.visible.contains(Visible::POMODORO));
    let base = a.pomodoro.as_ref().unwrap().work_minutes();
    a.on_key(key(KeyCode::Up));
    assert_eq!(a.pomodoro.as_ref().unwrap().work_minutes(), base + 1);
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    assert_eq!(a.pomodoro.as_ref().unwrap().work_minutes(), base - 1);
    a.on_key(key(KeyCode::Enter));
    assert!(a.pomodoro_running());
    assert!(!a.visible.contains(Visible::POMODORO), "runs in background");
    // Less than a second elapsed: no tick yet.
    a.poll_pomodoro();
    assert!(a.pomodoro.as_ref().unwrap().remaining > 0);
    // Work ends: the break starts and the dialog resurfaces.
    a.pomodoro.as_mut().unwrap().remaining = 30;
    a.pomodoro_last_tick = Some(seconds_ago(60));
    a.poll_pomodoro();
    assert!(a.pomodoro.as_ref().unwrap().is_break());
    assert!(a.visible.contains(Visible::POMODORO));
    assert_eq!(a.status, t!("ui.pomodoro_break_label").to_string());
    // Break ends: the timer is dropped.
    a.pomodoro.as_mut().unwrap().remaining = 30;
    a.pomodoro_last_tick = Some(seconds_ago(60));
    a.poll_pomodoro();
    assert!(a.pomodoro.is_none());
    assert_eq!(a.status, t!("status.pomodoro_done").to_string());
    assert!(!a.visible.contains(Visible::POMODORO));
    // Opening and pressing Esc while idle drops the timer; Enter while
    // running stops it.
    a.run_action("tools.pomodoro");
    a.on_key(key(KeyCode::Char(' ')));
    assert!(a.pomodoro_running());
    a.run_action("tools.pomodoro");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.pomodoro.as_ref().unwrap().phase, Phase::Idle);
    a.on_key(key(KeyCode::Esc));
    assert!(a.pomodoro.is_none());
    assert!(!a.visible.contains(Visible::POMODORO));
}

#[test]
fn pomodoro_adjust_out_of_range_reports_an_error() {
    let mut a = app();
    a.run_action("tools.pomodoro");
    for _ in 0..200 {
        a.on_key(key(KeyCode::Up));
    }
    assert!(a.pomodoro.as_ref().unwrap().work_minutes() <= 180);
    assert!(!a.messages.items.is_empty(), "the refused step is reported");
}

#[test]
fn auto_save_writes_a_dirty_file_backed_buffer_once_per_interval() {
    let dir = tmp("autosave");
    let p = dir.join("n.txt");
    std::fs::write(&p, "old\n").unwrap();
    let mut a = app_in(&dir);
    a.open_path(&p, false);
    a.settings.editor_behavior.auto_save = crate::settings::AutoSave::Off;
    type_text(&mut a, "new ");
    a.poll_auto_save();
    assert_eq!(read(&p), "old\n", "off: nothing written");
    a.settings.editor_behavior.auto_save = crate::settings::AutoSave::Interval;
    a.poll_auto_save();
    assert!(read(&p).contains("new "));
    assert_eq!(a.status, t!("status.auto_saved").to_string());
    // Throttled: a second edit right away is not written.
    type_text(&mut a, "more ");
    a.poll_auto_save();
    assert!(!read(&p).contains("more "));
    a.last_auto_save = Some(seconds_ago(10));
    a.poll_auto_save();
    assert!(read(&p).contains("more "));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn external_changes_reload_clean_buffers_and_warn_about_dirty_ones() {
    let dir = tmp("extchange");
    let clean = dir.join("clean.txt");
    let dirty = dir.join("dirty.txt");
    std::fs::write(&clean, "one\n").unwrap();
    std::fs::write(&dirty, "one\n").unwrap();
    let mut a = app_in(&dir);
    a.open_path(&clean, false);
    a.open_path(&dirty, false);
    type_text(&mut a, "edit ");
    a.poll_file_changes(); // first sighting records mtimes
    let bump = |p: &Path, body: &str| {
        std::fs::write(p, body).unwrap();
        let f = std::fs::OpenOptions::new().write(true).open(p).unwrap();
        f.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(30))
            .unwrap();
    };
    bump(&clean, "two\n");
    bump(&dirty, "two\n");
    // Throttled to once a second.
    a.poll_file_changes();
    assert!(a.editor.tabs[0].text().contains("one"));
    a.last_disk_poll = Some(seconds_ago(5));
    a.poll_file_changes();
    assert_eq!(a.editor.tabs[0].text(), "two\n", "clean buffer reloaded");
    assert_eq!(
        a.status,
        t!("status.reloaded_external", count = 1).to_string()
    );
    assert!(a.editor.tabs[1].text().contains("edit "), "dirty kept");
    assert!(
        a.messages
            .items
            .iter()
            .any(|m| m.text == t!("msg.external_change_unsaved", path = dirty.display()))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- find / replace box -----------------------------------------------------

fn alt(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)
}

fn flags(a: &App) -> SearchFlags {
    a.search.as_ref().expect("find box open").flags
}

#[test]
fn find_box_alt_keys_toggle_options_and_typing_previews_the_match() {
    let mut a = app_with("xx foo yy foo\n");
    a.run_action("edit.find");
    assert!(a.search.is_some());
    assert!(!flags(&a).contains(SearchFlags::REPLACING));
    type_str(&mut a, "foo");
    assert_eq!(cursor(&a), 3, "live preview lands on the first match");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(cursor(&a), 10, "Enter steps to the next match");
    a.on_key(key(KeyCode::F(3)));
    a.on_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::SHIFT));
    a.on_key(key(KeyCode::Backspace));
    assert_eq!(a.search.as_ref().unwrap().query, "fo");
    for (c, f) in [
        ('c', SearchFlags::CASE_SENSITIVE),
        ('s', SearchFlags::SMART_CASE),
        ('w', SearchFlags::WHOLE_WORD),
        ('r', SearchFlags::REGEX),
    ] {
        let before = flags(&a).contains(f);
        a.on_key(alt(c));
        assert_eq!(flags(&a).contains(f), !before, "Alt+{c}");
    }
    a.on_key(alt('h'));
    assert!(flags(&a).contains(SearchFlags::REPLACING));
    a.on_key(alt('h'));
    assert!(!flags(&a).contains(SearchFlags::REPLACING));
    // Tab only switches fields while replacing.
    a.on_key(key(KeyCode::Tab));
    assert_eq!(a.search.as_ref().unwrap().field, Field::Query);
    a.on_key(key(KeyCode::Esc));
    assert!(a.search.is_none());
}

#[test]
fn find_box_reports_a_bad_regex() {
    let mut a = app_with("abc\n");
    a.run_action("edit.find");
    a.on_key(alt('r'));
    type_str(&mut a, "(");
    let s = &a.search.as_ref().unwrap().status;
    assert!(!s.is_empty(), "bad regex shows in the box: {s:?}");
    a.on_key(key(KeyCode::Esc));
}

#[test]
fn replace_box_replaces_all_matches_from_the_replace_field() {
    let mut a = app_with("foo bar foo\n");
    a.run_action("edit.replace");
    assert!(flags(&a).contains(SearchFlags::REPLACING));
    type_str(&mut a, "foo");
    a.on_key(key(KeyCode::Tab));
    assert_eq!(a.search.as_ref().unwrap().field, Field::Replace);
    type_str(&mut a, "baz");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(active_text(&a), "baz bar baz\n");
    assert_eq!(
        a.search.as_ref().unwrap().status,
        t!("status.replaced", count = 2).to_string()
    );
}

#[test]
fn replace_box_once_replaces_a_single_match_and_reports_when_none_left() {
    let mut a = app_with("foo foo\n");
    a.run_action("edit.replace");
    a.layout.search_once = Rect::new(0, 5, 4, 1);
    a.replace_once(); // empty query: nothing to do
    type_str(&mut a, "foo");
    a.on_key(key(KeyCode::Tab));
    type_str(&mut a, "x");
    set_cur(&mut a, 0);
    a.search_mouse(left_click(1, 5));
    assert_eq!(active_text(&a), "x foo\n");
    assert_eq!(
        a.search.as_ref().unwrap().status,
        t!("status.replaced", count = 1).to_string()
    );
    a.replace_once();
    assert_eq!(active_text(&a), "x x\n");
    a.replace_once();
    assert_eq!(
        a.search.as_ref().unwrap().status,
        t!("status.qr_no_matches").to_string()
    );
}

#[test]
fn query_replace_asks_per_match() {
    let mut a = app_with("foo foo foo\n");
    a.run_action("edit.query_replace");
    assert!(flags(&a).contains(SearchFlags::INTERACTIVE));
    type_str(&mut a, "foo");
    assert_eq!(cursor(&a), 0, "interactive mode does not preview");
    a.on_key(key(KeyCode::Tab));
    type_str(&mut a, "bar");
    a.on_key(key(KeyCode::Enter));
    assert!(a.search.is_none());
    assert!(a.query_replace.is_some());
    assert_eq!(a.status, t!("status.qr_keys").to_string());
    a.on_key(key(KeyCode::Char('y'))); // replace first
    a.on_key(key(KeyCode::Char('n'))); // skip second
    a.on_key(key(KeyCode::Char('z'))); // ignored
    a.on_key(key(KeyCode::Char('y'))); // replace third
    assert_eq!(active_text(&a), "bar foo bar\n");
    assert!(
        a.query_replace.is_none(),
        "session ends after the last match"
    );
}

#[test]
fn query_replace_bang_replaces_the_rest_and_q_quits() {
    let mut a = app_with("foo foo foo foo\n");
    a.run_action("edit.query_replace");
    type_str(&mut a, "foo");
    a.on_key(key(KeyCode::Tab));
    type_str(&mut a, "o");
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Char('!')));
    assert_eq!(active_text(&a), "o o o o\n");
    assert!(a.query_replace.is_none());

    let mut a = app_with("foo foo\n");
    a.run_action("edit.query_replace");
    type_str(&mut a, "foo");
    a.on_key(key(KeyCode::Tab));
    type_str(&mut a, "q");
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Char('q')));
    assert_eq!(active_text(&a), "foo foo\n");
    assert!(a.query_replace.is_none());
}

#[test]
fn query_replace_with_no_match_or_no_query_says_so() {
    let mut a = app_with("abc\n");
    a.run_action("edit.query_replace");
    a.begin_query_replace();
    assert_eq!(
        a.search.as_ref().unwrap().status,
        t!("status.type_to_find").to_string()
    );
    type_str(&mut a, "zzz");
    a.on_key(key(KeyCode::Tab));
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.status, t!("status.qr_no_matches").to_string());
    // A bad regex stays in the box.
    a.run_action("edit.query_replace");
    a.on_key(alt('r'));
    type_str(&mut a, "(");
    a.begin_query_replace();
    assert!(a.search.is_some());
    assert_ne!(a.search.as_ref().unwrap().status, "");
}

#[test]
fn find_box_mouse_clicks_toggle_options_and_focus_fields() {
    let mut a = app_with("foo\n");
    a.run_action("edit.replace");
    type_str(&mut a, "foo");
    a.layout.search = Rect::new(0, 10, 40, 4);
    a.layout.search_case = Rect::new(0, 20, 2, 1);
    a.layout.search_smartcase = Rect::new(3, 20, 2, 1);
    a.layout.search_word = Rect::new(6, 20, 2, 1);
    a.layout.search_regex = Rect::new(9, 20, 2, 1);
    a.layout.search_replace_toggle = Rect::new(12, 20, 2, 1);
    a.layout.search_scope = Rect::new(15, 20, 2, 1);
    a.layout.search_once = Rect::new(18, 20, 2, 1);
    a.layout.search_ask = Rect::new(21, 20, 2, 1);
    a.layout.search_all = Rect::new(24, 20, 2, 1);
    // Non-click events are ignored.
    a.search_mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: 0,
        row: 20,
        modifiers: KeyModifiers::NONE,
    });
    for (col, f) in [
        (0, SearchFlags::CASE_SENSITIVE),
        (3, SearchFlags::SMART_CASE),
        (6, SearchFlags::WHOLE_WORD),
        (9, SearchFlags::REGEX),
    ] {
        let before = flags(&a).contains(f);
        a.search_mouse(left_click(col, 20));
        assert_eq!(flags(&a).contains(f), !before, "button at {col}");
    }
    // Field rows: row 0 is Find, row 2 is Replace while replacing.
    a.search_mouse(left_click(2, 12));
    assert_eq!(a.search.as_ref().unwrap().field, Field::Replace);
    a.search_mouse(left_click(2, 10));
    assert_eq!(a.search.as_ref().unwrap().field, Field::Query);
    // A click outside everything leaves the box open.
    a.search_mouse(left_click(60, 2));
    assert!(a.search.is_some());
    // The replace toggle flips replace mode.
    a.search_mouse(left_click(12, 20));
    assert!(!flags(&a).contains(SearchFlags::REPLACING));
    a.search_mouse(left_click(12, 20));
    assert!(flags(&a).contains(SearchFlags::REPLACING));
    // Replace-all button.
    a.search.as_mut().unwrap().replace = "z".to_string();
    a.search.as_mut().unwrap().flags.remove(SearchFlags::REGEX);
    a.search_mouse(left_click(24, 20));
    assert_eq!(active_text(&a), "z\n");
    // Ask button starts query-replace (no more matches -> reports).
    a.search_mouse(left_click(21, 20));
    assert!(a.search.is_none());
}

#[test]
fn widening_the_find_scope_hands_the_query_to_the_workspace_surfaces() {
    let dir = tmp("widen");
    std::fs::write(dir.join("a.txt"), "needle\n").unwrap();
    let mut a = app_in(&dir);
    a.editor.new_tab_with_content("needle\n");
    a.run_action("edit.find");
    type_str(&mut a, "needle");
    a.on_key(alt('i')); // buffer -> files
    assert!(a.search.is_none());
    let ws = a.workspace_search.as_ref().expect("files panel");
    assert_eq!(ws.query, "needle");
    a.workspace_search = None;
    a.run_action("edit.find");
    a.search.as_mut().unwrap().scope = Scope::Files;
    type_str(&mut a, "needle");
    a.on_key(alt('I')); // files -> workspace
    assert!(a.search.is_none());
    assert!(a.bottom_dock.lines.join("\n").contains("a.txt"));
    // Empty query asks for the pattern instead.
    a.run_action("edit.find");
    a.search.as_mut().unwrap().scope = Scope::Files;
    a.widen_search_scope();
    assert!(prompt_kind(&a) == Some(PromptKind::SearchToDock));
    a.prompt = None;
    // Workspace -> buffer returns to the box.
    a.run_action("edit.find");
    a.search.as_mut().unwrap().scope = Scope::Workspace;
    a.widen_search_scope();
    assert!(a.search.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- choosers: clipboard history, macros, encodings -----------------------

fn chooser_click(app: &mut App, row: u16) -> MouseEvent {
    app.layout.chooser = Rect::new(2, 2, 30, 10);
    left_click(5, 2 + row)
}

#[test]
fn clipboard_history_chooser_navigates_and_pastes() {
    let mut a = app_with("");
    a.run_action("edit.paste_from_history");
    assert_eq!(a.status, t!("status.clipboard_empty").to_string());
    a.record_clipboard(None);
    a.record_clipboard(Some(String::new()));
    a.record_clipboard(Some("one".to_string()));
    a.record_clipboard(Some("two".to_string()));
    a.record_clipboard(Some("one".to_string()));
    assert_eq!(
        a.clipboard_ring,
        ["one", "two"],
        "most recent first, deduplicated"
    );
    a.run_action("edit.paste_from_history");
    assert!(a.chooser.clipboard().is_some());
    a.on_key(key(KeyCode::Down));
    assert_eq!(a.chooser.clipboard().unwrap().selected, 1);
    a.on_key(key(KeyCode::Down));
    assert_eq!(a.chooser.clipboard().unwrap().selected, 0, "wraps");
    a.on_key(key(KeyCode::Up));
    assert_eq!(a.chooser.clipboard().unwrap().selected, 1);
    a.on_key(key(KeyCode::Char('x')));
    a.on_key(key(KeyCode::Enter));
    assert!(a.chooser.is_none());
    assert_eq!(active_text(&a), "two");
    assert_eq!(
        a.clipboard_ring[0], "two",
        "pasted entry moves to the front"
    );
    // Esc closes; a click on a row pastes it.
    a.run_action("edit.paste_from_history");
    a.on_key(key(KeyCode::Esc));
    assert!(a.chooser.is_none());
    a.run_action("edit.paste_from_history");
    let m = chooser_click(&mut a, 1);
    a.clipboard_mouse(m);
    assert!(a.chooser.is_none());
    assert_eq!(active_text(&a), "twoone");
    // A click past the last entry does nothing.
    a.run_action("edit.paste_from_history");
    let m = chooser_click(&mut a, 8);
    a.clipboard_mouse(m);
    assert!(a.chooser.clipboard().is_some());
}

#[test]
fn macro_chooser_plays_the_selected_macro() {
    let mut a = app_with("");
    let mk = |name: &str, text: &str| crate::macros::Macro {
        name: name.to_string(),
        keys: crate::macros::encode(
            &text
                .chars()
                .map(|c| key(KeyCode::Char(c)))
                .collect::<Vec<_>>(),
        ),
    };
    let open = |a: &mut App| {
        a.chooser.open_macros(MacroChooser {
            macros: vec![mk("first", "ab"), mk("second", "xyz")],
            selected: 0,
        });
    };
    open(&mut a);
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Up));
    assert_eq!(a.chooser.macros().unwrap().selected, 1);
    a.on_key(key(KeyCode::Char('q'))); // ignored
    a.on_key(key(KeyCode::Enter));
    assert!(a.chooser.is_none());
    assert_eq!(active_text(&a), "xyz");
    assert_eq!(a.status, t!("status.macro_played").to_string());
    open(&mut a);
    a.on_key(key(KeyCode::Esc));
    assert!(a.chooser.is_none());
    open(&mut a);
    let m = chooser_click(&mut a, 0);
    a.macro_mouse(m);
    assert_eq!(active_text(&a), "xyzab");
    open(&mut a);
    let m = chooser_click(&mut a, 7);
    a.macro_mouse(m);
    assert!(
        a.chooser.macros().is_some(),
        "click past the list is ignored"
    );
}

#[test]
fn recording_and_replaying_a_macro_repeats_the_keystrokes() {
    let mut a = app_with("");
    let before = a.status.clone();
    a.run_action("macro.play");
    assert_eq!(a.status, before, "nothing recorded: playing is a no-op");
    a.run_action("macro.save");
    assert_eq!(a.status, t!("status.macro_empty").to_string());
    a.run_action("macro.record");
    assert_eq!(a.macro_state, MacroState::Recording);
    assert_eq!(a.status, t!("status.macro_recording").to_string());
    type_str(&mut a, "hi");
    a.run_action("macro.play"); // refused while recording
    a.run_action("macro.record");
    assert_eq!(a.macro_state, MacroState::Idle);
    assert_eq!(
        a.status,
        t!("status.macro_recorded", count = a.macro_keys.len()).to_string()
    );
    assert_eq!(active_text(&a), "hi");
    a.run_action("macro.play");
    assert_eq!(active_text(&a), "hihi");
    a.run_action("macro.save");
    assert!(prompt_kind(&a) == Some(PromptKind::SaveMacro));
    a.on_key(key(KeyCode::Esc));
    assert!(a.prompt.is_none());
    // A blank name never reaches the config file.
    a.save_macro("   ");
}

#[test]
fn encoding_chooser_reopens_a_latin1_file_and_saves_back_in_it() {
    let dir = tmp("encoding");
    let p = dir.join("cafe.txt");
    std::fs::write(&p, b"caf\xe9\n").unwrap();
    let mut a = app_in(&dir);
    a.run_action("file.reopen_with_encoding");
    assert_eq!(a.status, t!("status.encoding_no_file").to_string());
    a.open_path(&p, false);
    a.run_action("file.reopen_with_encoding");
    assert!(a.chooser.encoding().is_some());
    let idx = crate::encoding::CHOICES
        .iter()
        .position(|c| *c == "windows-1252")
        .unwrap();
    for _ in 0..idx {
        a.on_key(key(KeyCode::Down));
    }
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Char('?')));
    a.on_key(key(KeyCode::Enter));
    assert!(a.chooser.is_none());
    assert_eq!(active_text(&a), "caf\u{e9}\n");
    // Reopening with unsaved edits is refused.
    type_text(&mut a, "!");
    a.run_action("file.reopen_with_encoding");
    a.on_key(key(KeyCode::Enter));
    assert_eq!(a.status, t!("status.reopen_encoding_unsaved").to_string());
    // Save As windows-1252 writes a single byte for é.
    a.run_action("file.save_with_encoding");
    for _ in 0..idx {
        a.on_key(key(KeyCode::Down));
    }
    a.on_key(key(KeyCode::Enter));
    let bytes = std::fs::read(&p).unwrap();
    assert!(bytes.starts_with(b"caf\xe9"), "{bytes:?}");
    // Esc and mouse paths.
    a.run_action("file.save_with_encoding");
    a.on_key(key(KeyCode::Esc));
    assert!(a.chooser.is_none());
    a.run_action("file.reopen_with_encoding");
    let m = chooser_click(&mut a, 0);
    a.encoding_mouse(m);
    assert!(a.chooser.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_with_encoding_is_refused_on_a_read_only_buffer() {
    let dir = tmp("encoding-ro");
    let p = dir.join("ro.txt");
    std::fs::write(&p, "x\n").unwrap();
    let mut a = app_in(&dir);
    a.open_path(&p, false);
    a.run_action("view.read_only");
    a.save_with_encoding(crate::encoding::TextEncoding::from_label("UTF-8").unwrap());
    assert_eq!(a.status, t!("status.read_only_blocked").to_string());
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- keybinding editor (navigation only: rebinding writes the user config) -----

#[test]
fn keybinding_editor_filters_scrolls_and_sorts() {
    use vix_keybinding_editor_panel::Column;
    let mut a = app();
    a.run_action("keybindings.editor");
    assert!(a.keybinding_editor.is_some());
    a.layout.keybinding_editor_body = Rect::new(0, 2, 60, 5);
    a.layout.keybinding_editor_headers = [Rect::new(0, 1, 20, 1), Rect::new(20, 1, 20, 1)];
    let rows = a.keybinding_editor.as_ref().unwrap().len();
    assert!(rows > 10, "built-in bindings are listed");
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    assert_eq!(a.keybinding_editor.as_ref().unwrap().selected, 2);
    a.on_key(key(KeyCode::Up));
    assert_eq!(a.keybinding_editor.as_ref().unwrap().selected, 1);
    a.on_key(key(KeyCode::PageDown));
    assert_eq!(a.keybinding_editor.as_ref().unwrap().selected, 6);
    a.on_key(key(KeyCode::PageUp));
    assert_eq!(a.keybinding_editor.as_ref().unwrap().selected, 1);
    // Typing filters; Backspace widens again.
    type_str(&mut a, "zzzzqqqq");
    assert_eq!(a.keybinding_editor.as_ref().unwrap().len(), 0);
    for _ in 0..8 {
        a.on_key(key(KeyCode::Backspace));
    }
    assert_eq!(a.keybinding_editor.as_ref().unwrap().len(), rows);
    // Wheel and header clicks.
    let wheel = |kind| MouseEvent {
        kind,
        column: 3,
        row: 3,
        modifiers: KeyModifiers::NONE,
    };
    let start = a.keybinding_editor.as_ref().unwrap().selected;
    a.keybinding_editor_mouse(wheel(MouseEventKind::ScrollDown));
    assert_eq!(a.keybinding_editor.as_ref().unwrap().selected, start + 3);
    a.keybinding_editor_mouse(wheel(MouseEventKind::ScrollUp));
    assert_eq!(a.keybinding_editor.as_ref().unwrap().selected, start);
    let before = a.keybinding_editor.as_ref().unwrap().rows.clone();
    a.keybinding_editor_mouse(left_click(2, 1));
    a.keybinding_editor_mouse(left_click(25, 1));
    a.keybinding_editor_mouse(left_click(50, 1));
    let _ = (Column::Action, before);
    // Enter starts a rebind prompt; an unparseable token is refused without
    // touching the config file.
    a.on_key(key(KeyCode::Enter));
    assert!(prompt_kind(&a) == Some(PromptKind::RebindKey));
    assert!(a.pending_rebind_action_id.is_some());
    submit(&mut a, PromptKind::RebindKey, "not a key at all");
    assert!(
        a.messages
            .items
            .iter()
            .any(|m| m.text == t!("msg.rebind_invalid_token", token = "not a key at all"))
    );
    // Delete on a built-in (non-resettable) row does nothing.
    a.on_key(key(KeyCode::Delete));
    a.on_key(key(KeyCode::Esc));
    assert!(a.keybinding_editor.is_none());
    // Keys with no editor open are ignored.
    a.keybinding_editor_key(key(KeyCode::Down));
}

// ----- spell checking (in-memory dictionary; nothing touches the user config) -----

fn spelling_app(text: &str) -> App {
    let mut a = app();
    a.editor.new_tab_with_content_as(text, Some("x.rs"));
    a.flags.insert(AppFlags::SPELLCHECK);
    a.speller = Some(
        crate::spellcheck::SpellChecker::from_strings(
            "SET UTF-8\n",
            "4\nhello\nworld\nhelp\nhold\n",
        )
        .unwrap(),
    );
    a.speller_locale = Some(rust_i18n::locale().to_string());
    a
}

#[test]
fn spellcheck_marks_misspelled_words_in_comments_only() {
    let mut a = spelling_app("// helo world\nlet helo = 1;\n");
    a.refresh_spellcheck();
    let marks = a
        .editor
        .active_tab()
        .unwrap()
        .editor
        .spell_marks()
        .cloned()
        .unwrap_or_default();
    assert_eq!(marks, vec![(3, 7)], "only the comment's helo is underlined");
    // The per-frame hook does nothing until the buffer changes.
    a.refresh_spellcheck_if_stale();
    type_text(&mut a, "zzz ");
    a.refresh_spellcheck_if_stale();
    assert!(a.spell_key.is_some());
    // Turning spell-checking off clears the marks.
    a.speller = None;
    a.flags.remove(AppFlags::SPELLCHECK);
    a.refresh_spellcheck();
    assert!(
        a.editor
            .active_tab()
            .unwrap()
            .editor
            .spell_marks()
            .is_none_or(Vec::is_empty)
    );
}

#[test]
fn spell_suggest_popup_offers_corrections_and_applies_one() {
    let mut a = spelling_app("// helo world\n");
    set_cur(&mut a, 4);
    a.run_action("spell.suggest");
    let p = a.spell_suggest.as_ref().expect("popup");
    assert_eq!(p.word, "helo");
    assert_eq!(p.span, (3, 7));
    assert!(p.suggestions.iter().any(|s| s == "hello"));
    let n = p.suggestions.len();
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Up));
    assert_eq!(a.spell_suggest.as_ref().unwrap().selected, 0);
    for _ in 0..n + 2 {
        a.on_key(key(KeyCode::Down));
    }
    assert_eq!(a.spell_suggest.as_ref().unwrap().selected, n - 1, "clamped");
    a.spell_suggest.as_mut().unwrap().selected = a
        .spell_suggest
        .as_ref()
        .unwrap()
        .suggestions
        .iter()
        .position(|s| s == "hello")
        .unwrap();
    a.on_key(key(KeyCode::Enter));
    assert!(a.spell_suggest.is_none());
    assert_eq!(active_text(&a), "// hello world\n");
}

#[test]
fn spell_suggest_click_applies_the_clicked_suggestion() {
    let mut a = spelling_app("// helo world\n");
    set_cur(&mut a, 4);
    a.run_action("spell.suggest");
    a.layout.spell_suggest = Rect::new(5, 5, 20, 6);
    let idx = a
        .spell_suggest
        .as_ref()
        .unwrap()
        .suggestions
        .iter()
        .position(|s| s == "hello")
        .unwrap();
    // A click past the list is ignored; a click on the row applies it.
    a.spell_suggest_mouse(left_click(6, 5 + 10));
    assert!(a.spell_suggest.is_some());
    a.spell_suggest_mouse(left_click(6, 5 + u16::try_from(idx).unwrap()));
    assert_eq!(active_text(&a), "// hello world\n");
}

#[test]
fn spell_suggest_ignore_word_and_esc() {
    let mut a = spelling_app("// helo world\n");
    set_cur(&mut a, 4);
    a.run_action("spell.suggest");
    a.on_key(key(KeyCode::Char('i')));
    assert!(a.spell_suggest.is_none());
    assert_eq!(
        a.status,
        t!("status.spell_ignored", word = "helo").to_string()
    );
    assert!(a.speller.as_ref().unwrap().check("helo"));
    // Now the word is fine.
    a.run_action("spell.suggest");
    assert_eq!(a.status, t!("status.spell_ok").to_string());
    // Off a word, the popup says so; Esc closes a popup.
    set_cur(&mut a, 2);
    a.run_action("spell.suggest");
    assert_eq!(a.status, t!("status.spell_no_word").to_string());
    set_cur(&mut a, 11);
    a.run_action("spell.suggest");
    a.spell_suggest = Some(SpellSuggest {
        word: "x".to_string(),
        span: (0, 1),
        suggestions: vec![],
        selected: 0,
    });
    a.on_key(key(KeyCode::Esc));
    assert!(a.spell_suggest.is_none());
}

#[test]
fn spell_suggest_without_a_dictionary_says_it_is_unavailable() {
    let mut a = app_with("// helo\n");
    a.open_spell_suggest();
    assert_eq!(a.status, t!("status.spell_unavailable").to_string());
}

// ----- welcome / help / html overlays ---------------------------------------------

#[test]
fn welcome_overlay_scrolls_and_closes_on_the_documented_keys() {
    let mut a = app();
    a.run_action("help.privacy");
    a.layout.welcome = Rect::new(0, 0, 60, 3);
    let len = a.welcome.as_ref().unwrap().len();
    assert!(len > 3, "needs more lines than the viewport");
    a.on_key(key(KeyCode::Down));
    assert_eq!(a.welcome.as_ref().unwrap().scroll, 1);
    a.on_key(key(KeyCode::Up));
    assert_eq!(a.welcome.as_ref().unwrap().scroll, 0);
    a.on_key(key(KeyCode::PageDown));
    assert_eq!(a.welcome.as_ref().unwrap().scroll, 3);
    a.on_key(key(KeyCode::PageUp));
    assert_eq!(a.welcome.as_ref().unwrap().scroll, 0);
    a.on_key(key(KeyCode::Char(' ')));
    assert_eq!(a.welcome.as_ref().unwrap().scroll, 3);
    a.on_key(key(KeyCode::End));
    let end = a.welcome.as_ref().unwrap().scroll;
    assert_eq!(end, len - 3, "End shows the last page");
    a.on_key(key(KeyCode::Home));
    assert_eq!(a.welcome.as_ref().unwrap().scroll, 0);
    a.on_key(key(KeyCode::Char('z')));
    assert!(a.welcome.is_some());
    for close in [
        KeyCode::Esc,
        KeyCode::Enter,
        KeyCode::F(1),
        KeyCode::Char('q'),
    ] {
        a.run_action("help.privacy");
        a.on_key(key(close));
        assert!(a.welcome.is_none(), "{close:?} closes");
    }
}

#[test]
fn welcome_overlay_wheel_scrolls() {
    let mut a = app();
    a.run_action("help.privacy");
    a.layout.welcome = Rect::new(0, 0, 60, 3);
    let wheel = |kind| MouseEvent {
        kind,
        column: 1,
        row: 1,
        modifiers: KeyModifiers::NONE,
    };
    a.on_mouse(wheel(MouseEventKind::ScrollDown));
    assert!(a.welcome.as_ref().unwrap().scroll > 0);
    a.on_mouse(wheel(MouseEventKind::ScrollUp));
    assert_eq!(a.welcome.as_ref().unwrap().scroll, 0);
}

#[test]
fn shortcut_help_overlay_scrolls_and_sorts_by_header() {
    let mut a = app();
    a.run_action("help.shortcuts");
    assert!(a.help.is_some());
    a.layout.help_headers = [Rect::new(0, 1, 20, 1), Rect::new(20, 1, 20, 1)];
    let wheel = |kind| MouseEvent {
        kind,
        column: 3,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    a.help_mouse(wheel(MouseEventKind::ScrollDown));
    a.help_mouse(wheel(MouseEventKind::ScrollUp));
    let first_before = a
        .help
        .as_ref()
        .unwrap()
        .rows
        .first()
        .map(|r| r.action.clone());
    a.help_mouse(left_click(2, 1));
    a.help_mouse(left_click(2, 1));
    a.help_mouse(left_click(25, 1));
    a.help_mouse(left_click(60, 1));
    let first_after = a
        .help
        .as_ref()
        .unwrap()
        .rows
        .first()
        .map(|r| r.action.clone());
    assert!(first_before.is_some() && first_after.is_some());
}

#[test]
fn html_palette_inserts_glyph_name_or_code_by_clicked_column() {
    let mut a = app_with("");
    a.run_action("tools.html_chars");
    assert!(a.html_panel.is_some());
    a.layout.html_panel = Rect::new(0, 5, 70, 8);
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    let glyph = active_text(&a);
    assert_eq!(
        glyph.chars().count(),
        1,
        "Enter inserts the glyph: {glyph:?}"
    );
    assert!(a.html_panel.is_some(), "panel stays open");
    assert_eq!(
        a.status,
        t!("status.html_inserted", name = glyph).to_string()
    );
    // Clicking the name column inserts the entity name.
    a.html_mouse(left_click(10, 5));
    let after_name = active_text(&a);
    assert!(after_name.len() > glyph.len());
    // Clicking the code column inserts the code.
    a.html_mouse(left_click(40, 5));
    assert!(active_text(&a).len() > after_name.len());
    // A click outside the list does nothing.
    let before = active_text(&a);
    a.html_mouse(left_click(40, 0));
    assert_eq!(active_text(&a), before);
    a.on_key(key(KeyCode::Esc));
    assert!(a.html_panel.is_none());
}

// ----- context menu ---------------------------------------------------------------

#[test]
fn right_click_opens_the_context_menu_and_keys_skip_separators() {
    let mut a = app_with("hello\n");
    a.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: 3,
        row: 2,
        modifiers: KeyModifiers::NONE,
    });
    let cm = a.context_menu.as_ref().expect("menu opens");
    assert_eq!((cm.x, cm.y, cm.selected), (3, 2, 0));
    // Down x3 skips the separator after Paste.
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    assert_eq!(
        CONTEXT_ITEMS[a.context_menu.as_ref().unwrap().selected].1,
        "edit.select_all"
    );
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Up));
    assert_eq!(a.context_menu.as_ref().unwrap().selected, 0);
    a.on_key(key(KeyCode::Up)); // wraps to the last row
    assert_eq!(
        a.context_menu.as_ref().unwrap().selected,
        CONTEXT_ITEMS.len() - 1
    );
    a.on_key(key(KeyCode::Esc));
    assert!(a.context_menu.is_none());
}

#[test]
fn context_menu_click_runs_the_row_and_outside_click_dismisses() {
    let mut a = app_with("hello\n");
    a.open_context_menu(3, 2);
    a.layout.context_menu = Rect::new(3, 2, 20, 13);
    // A click on a separator row is inert.
    a.context_menu_mouse(left_click(5, 2 + 1 + 3));
    assert!(a.context_menu.is_some());
    // A click on "Select All" selects the whole buffer.
    a.context_menu_mouse(left_click(5, 2 + 1 + 4));
    assert!(a.context_menu.is_none());
    assert_eq!(
        a.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .get_selection_text()
            .as_deref(),
        Some("hello\n")
    );
    a.open_context_menu(3, 2);
    a.context_menu_mouse(left_click(60, 20));
    assert!(a.context_menu.is_none(), "outside click dismisses");
    // Enter runs the highlighted item (Cut/Copy/Paste are harmless here).
    a.open_context_menu(3, 2);
    a.context_menu_key(key(KeyCode::Down));
    a.context_menu_key(key(KeyCode::Enter));
    assert!(a.context_menu.is_none());
}

// ----- jump mode, docks, scrollbars ---------------------------------------------------

#[test]
fn jump_mode_labels_visible_lines_and_jumps_on_a_typed_label() {
    let body = "line\n".repeat(30);
    let mut a = app_with(&body);
    a.run_action("nav.jump");
    let jm = a.jump.as_ref().expect("jump mode");
    assert_eq!(jm.labels.len(), 24);
    let (label, line) = jm.labels[5].clone();
    // A non-character key cancels.
    a.on_key(key(KeyCode::Esc));
    assert!(a.jump.is_none());
    a.run_action("nav.jump");
    for c in label.chars() {
        a.on_key(key(KeyCode::Char(c)));
    }
    assert_eq!(a.editor.cursor_line(), line);
    assert!(a.jump.is_none());
    // A character that matches no label ends the mode.
    a.run_action("nav.jump");
    a.on_key(key(KeyCode::Char('~')));
    assert!(a.jump.is_none());
    // A zero-height editor never opens jump mode.
    a.layout.editor = Rect::new(0, 0, 80, 0);
    a.run_action("nav.jump");
    assert!(a.jump.is_none());
}

#[test]
fn horizontal_scrollbar_drag_sets_each_views_offset() {
    let mut a = app_with(&"x".repeat(300));
    a.layout.editor_hscrollbar = Rect::new(0, 23, 11, 1);
    a.layout.explorer_hscrollbar = Rect::new(0, 23, 11, 1);
    a.layout.messages_hscrollbar = Rect::new(0, 23, 11, 1);
    a.layout.bottom_hscrollbar = Rect::new(0, 23, 11, 1);
    a.editor_hmax = 100;
    a.explorer_hmax = 50;
    a.messages_hmax = 20;
    a.bottom_hmax = 10;
    a.hbar_drag(HBar::Explorer, 10);
    assert_eq!(a.explorer_hscroll, 50);
    a.hbar_drag(HBar::Messages, 5);
    assert_eq!(a.messages_hscroll, 10);
    a.hbar_drag(HBar::Bottom, 0);
    assert_eq!(a.bottom_hscroll, 0);
    a.hbar_drag(HBar::Editor, 10);
    assert_eq!(a.editor.active_tab().unwrap().editor.get_offset_x(), 100);
}

#[test]
fn bottom_dock_click_jumps_to_a_path_line_hit_and_wheel_scrolls() {
    let dir = tmp("dock-click");
    std::fs::write(dir.join("a.txt"), "1\n2\n3\n4\n").unwrap();
    let mut a = app_in(&dir);
    a.layout.bottom_dock = Rect::new(0, 10, 40, 6);
    a.bottom_dock.push("a.txt:3:1: hit text");
    a.bottom_dock.push("not a location");
    // Row 11 is the first content line (row 10 is the border).
    a.bottomdock_mouse(left_click(2, 11));
    assert_eq!(a.focus, Focus::Editor);
    assert_eq!(a.editor.cursor_line(), 2);
    assert!(a.active_path().unwrap().ends_with("a.txt"));
    // A line that is not a location only focuses the dock.
    a.bottomdock_mouse(left_click(2, 12));
    assert_eq!(a.focus, Focus::BottomDock);
    // The border row does nothing; wheel scrolls.
    a.bottomdock_open_at(10);
    for i in 0..30 {
        a.bottom_dock.push(format!("line {i}"));
    }
    let wheel = |kind| MouseEvent {
        kind,
        column: 2,
        row: 12,
        modifiers: KeyModifiers::NONE,
    };
    a.bottom_dock.scroll = 0;
    a.bottomdock_mouse(wheel(MouseEventKind::ScrollDown));
    assert_eq!(a.bottom_dock.scroll, 3);
    a.bottomdock_mouse(wheel(MouseEventKind::ScrollUp));
    assert_eq!(a.bottom_dock.scroll, 0);
    // Clicking the scrollbar column jumps the scroll position.
    a.settings.viewport.show_scrollbar = true;
    a.bottomdock_mouse(left_click(39, 14));
    assert!(a.bottom_dock.scroll > 0);
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- recent files / locations / file browser -------------------------------------

#[test]
fn recent_files_chooser_lists_existing_files_and_opens_one() {
    let dir = tmp("recent");
    std::fs::write(dir.join("a.txt"), "A").unwrap();
    std::fs::write(dir.join("b.txt"), "B").unwrap();
    let mut a = app_in(&dir);
    a.run_action("file.open_recent");
    assert_eq!(a.status, t!("status.no_recent").to_string());
    a.settings.recent_files = vec![
        dir.join("a.txt").to_string_lossy().into_owned(),
        dir.join("gone.txt").to_string_lossy().into_owned(),
        dir.join("b.txt").to_string_lossy().into_owned(),
    ];
    a.run_action("file.open_recent");
    assert_eq!(
        a.chooser.recent().unwrap().entries.len(),
        2,
        "missing file dropped"
    );
    a.on_key(key(KeyCode::Up));
    assert_eq!(a.chooser.recent().unwrap().selected, 1, "wraps up");
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Char('x')));
    a.on_key(key(KeyCode::Esc));
    assert!(a.chooser.is_none());
    a.run_action("file.open_recent");
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    assert_eq!(active_text(&a), "B");
    a.run_action("file.open_recent");
    let m = chooser_click(&mut a, 1); // opening B moved it to the front
    a.recent_mouse(m);
    assert_eq!(active_text(&a), "A");
    a.run_action("file.open_recent");
    let m = chooser_click(&mut a, 9);
    a.recent_mouse(m);
    assert!(a.chooser.recent().is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn recent_locations_chooser_follows_the_jump_history() {
    let dir = tmp("locations");
    std::fs::write(dir.join("a.txt"), "1\n2\n3\n").unwrap();
    std::fs::write(dir.join("b.txt"), "1\n2\n3\n").unwrap();
    let mut a = app_in(&dir);
    a.run_action("nav.recent_locations");
    assert_eq!(a.status, t!("status.no_recent_locations").to_string());
    a.open_path(&dir.join("a.txt"), false);
    a.with_jump(|s| {
        s.open_path(&dir.join("b.txt"), false);
        let area = s.layout.editor;
        s.editor.goto(3, None, area);
    });
    a.run_action("nav.recent_locations");
    assert!(a.chooser.location().unwrap().entries.len() >= 2);
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Char('x')));
    a.on_key(key(KeyCode::Esc));
    assert!(a.chooser.is_none());
    a.run_action("nav.recent_locations");
    let last = a.chooser.location().unwrap().entries.len() - 1;
    let m = chooser_click(&mut a, u16::try_from(last).unwrap());
    a.location_mouse(m);
    assert!(a.chooser.is_none());
    assert!(a.active_path().unwrap().ends_with("a.txt"));
    a.run_action("nav.recent_locations");
    a.on_key(key(KeyCode::Enter));
    assert!(a.chooser.is_none());
    a.run_action("nav.recent_locations");
    let m = chooser_click(&mut a, 9);
    a.location_mouse(m);
    assert!(a.chooser.location().is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn file_browser_wheel_and_click_navigate_and_open() {
    let dir = tmp("browser");
    std::fs::write(dir.join("a.txt"), "A").unwrap();
    std::fs::write(dir.join("b.txt"), "B").unwrap();
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let mut a = app_in(&dir);
    a.run_action("file.open");
    assert!(a.file_browser.is_some());
    a.layout.file_browser = Rect::new(0, 5, 40, 10);
    let wheel = |kind| MouseEvent {
        kind,
        column: 3,
        row: 6,
        modifiers: KeyModifiers::NONE,
    };
    let start = a.file_browser.as_ref().unwrap().selected;
    a.file_browser_mouse(wheel(MouseEventKind::ScrollDown));
    assert_eq!(a.file_browser.as_ref().unwrap().selected, start + 1);
    a.file_browser_mouse(wheel(MouseEventKind::ScrollUp));
    assert_eq!(a.file_browser.as_ref().unwrap().selected, start);
    // A click outside the panel, and past its last row, does nothing.
    a.file_browser_mouse(left_click(60, 0));
    a.file_browser_mouse(left_click(3, 14));
    assert!(a.file_browser.is_some());
    // Find the row of a.txt and click it.
    let fb = a.file_browser.as_ref().unwrap();
    let row = fb
        .matches()
        .iter()
        .position(|&i| fb.entries[i].name == "a.txt")
        .expect("a.txt listed");
    a.file_browser_mouse(left_click(3, 5 + u16::try_from(row).unwrap()));
    assert!(a.file_browser.is_none());
    assert_eq!(active_text(&a), "A");
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- test runner results ---------------------------------------------------------

#[test]
fn finished_test_run_parses_output_and_jumps_to_the_failing_test() {
    let dir = tmp("testrun");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "a\nb\nc\nd\n").unwrap();
    let mut a = app_in(&dir);
    a.flags.insert(AppFlags::TEST_CAPTURE);
    a.test_buffer = "running 3 tests
test mod_a::works ... ok
test mod_a::ignored_one ... ignored
test mod_b::breaks ... FAILED

failures:

---- mod_b::breaks stdout ----
thread 'mod_b::breaks' panicked at src/lib.rs:3:5:
assertion failed

test result: FAILED. 1 passed; 1 failed; 1 ignored;
"
    .lines()
    .map(str::to_string)
    .collect();
    a.finish_test_run();
    assert!(!a.flags.contains(AppFlags::TEST_CAPTURE));
    assert_eq!(a.test_results.len(), 3);
    assert_eq!(
        a.status,
        t!("status.tests_done", pass = 1, fail = 1, ignore = 1).to_string()
    );
    assert!(a.messages.items.iter().any(|m| m.text == a.status));
    let idx = a
        .test_results
        .iter()
        .position(|r| r.name == "mod_b::breaks")
        .unwrap();
    a.jump_to_test(idx);
    assert_eq!(a.test_selected, idx);
    assert!(a.active_path().unwrap().ends_with("src/lib.rs"));
    assert_eq!(a.editor.cursor_line(), 2);
    // A test with no location, or a missing file, does not jump.
    let tabs = a.editor.tabs.len();
    a.jump_to_test(0);
    assert_eq!(a.editor.tabs.len(), tabs);
    a.test_results[idx].location = Some(("nope.rs".to_string(), 1));
    a.jump_to_test(idx);
    assert_eq!(a.editor.tabs.len(), tabs);
    // An all-passing run reports through an info message.
    a.test_buffer = vec!["test x ... ok".to_string()];
    a.finish_test_run();
    assert_eq!(
        a.status,
        t!("status.tests_done", pass = 1, fail = 0, ignore = 0).to_string()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_panel_toggles_with_its_action() {
    let mut a = app();
    a.run_action("tools.test_panel");
    assert!(a.visible.contains(Visible::TEST_PANEL));
    a.run_action("tools.test_panel");
    assert!(!a.visible.contains(Visible::TEST_PANEL));
}

#[test]
fn cancelling_with_nothing_running_does_nothing() {
    let mut a = app();
    assert!(!a.command_running());
    a.run_action("tools.cancel_command");
    assert_eq!(a.bottom_dock.lines.len(), 0);
}

// ----- language-server notices -----------------------------------------------------

#[test]
fn language_server_down_events_become_error_messages() {
    let mut a = app();
    a.report_lsp_server_down(&crate::lsp::LspEvent::ServerCrashed("rust".to_string()));
    a.report_lsp_server_down(&crate::lsp::LspEvent::ServerNotInstalled {
        language_id: "go".to_string(),
        install: "go install gopls".to_string(),
    });
    // Any other event is not a "server down" notice.
    let before = a.messages.items.len();
    a.report_lsp_server_down(&crate::lsp::LspEvent::CodeLensRefresh);
    assert_eq!(a.messages.items.len(), before);
    let texts: Vec<&str> = a.messages.items.iter().map(|m| m.text.as_str()).collect();
    assert!(texts.contains(&t!("msg.lsp_server_crashed", language = "rust").as_ref()));
    assert!(
        texts.contains(
            &t!(
                "msg.lsp_server_not_installed",
                language = "go",
                install = "go install gopls"
            )
            .as_ref()
        )
    );
}

#[test]
fn polling_the_language_server_with_none_running_is_a_quiet_no_op() {
    let mut a = app_with("x");
    a.completion_due = Some((std::time::Instant::now(), None));
    a.poll_lsp();
    assert!(a.completion_due.is_none());
}

// ----- word autocomplete -------------------------------------------------------------

#[test]
fn autocomplete_cycles_buffer_words_sharing_the_prefix() {
    let mut a = app_with("foobar foobaz fooqux fo");
    let end = active_text(&a).chars().count();
    set_cur(&mut a, end);
    a.run_action("autocomplete");
    assert_eq!(active_text(&a), "foobar foobaz fooqux foobar");
    a.run_action("autocomplete");
    assert_eq!(active_text(&a), "foobar foobaz fooqux foobaz");
    a.run_action("autocomplete");
    assert_eq!(active_text(&a), "foobar foobaz fooqux fooqux");
    a.run_action("autocomplete"); // wraps
    assert_eq!(active_text(&a), "foobar foobaz fooqux foobar");
    a.run_action("cycle_autocomplete_back");
    assert_eq!(active_text(&a), "foobar foobaz fooqux fooqux");
}

#[test]
fn autocomplete_reports_when_there_is_nothing_to_complete() {
    let mut a = app_with("zzz ");
    let end = active_text(&a).chars().count();
    set_cur(&mut a, end);
    a.run_action("autocomplete");
    assert_eq!(a.status, t!("status.no_completions").to_string());
    let mut a = app_with("zzz zz");
    let end = active_text(&a).chars().count();
    set_cur(&mut a, end);
    a.run_action("autocomplete");
    assert_eq!(active_text(&a), "zzz zzz");
    // Nothing else shares the prefix `abc`.
    let mut a = app_with("abc");
    set_cur(&mut a, 3);
    a.status.clear();
    a.run_action("autocomplete");
    assert_eq!(a.status, t!("status.no_completions").to_string());
    // No active text tab: ignored.
    let mut a = app();
    a.run_action("autocomplete");
}

// ----- message / dock / explorer panes ------------------------------------------------

#[test]
fn messages_pane_keys_select_close_and_return_focus() {
    let mut a = app();
    a.messages.info("one".to_string());
    a.messages.info("two".to_string());
    a.messages.info("three".to_string());
    a.focus = Focus::Messages;
    let n = a.messages.items.len();
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Char('x')));
    assert_eq!(a.messages.items.len(), n - 1);
    a.on_key(key(KeyCode::Delete));
    assert_eq!(a.messages.items.len(), n - 2);
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Char('?')));
    a.on_key(key(KeyCode::Esc));
    assert_eq!(a.focus, Focus::Editor);
}

#[test]
fn messages_pane_mouse_selects_and_closes_by_the_right_edge() {
    let mut a = app();
    for t in ["one", "two", "three", "four"] {
        a.messages.info(t.to_string());
    }
    a.layout.messages = Rect::new(60, 2, 20, 10);
    a.settings.viewport.show_scrollbar = true;
    let n = a.messages.items.len();
    let wheel = |kind| MouseEvent {
        kind,
        column: 62,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    a.messages_mouse(wheel(MouseEventKind::ScrollDown));
    a.messages_mouse(wheel(MouseEventKind::ScrollUp));
    // Click on a row selects it and focuses the pane.
    a.messages_mouse(left_click(62, 4));
    assert_eq!(a.messages.selected, 1);
    assert_eq!(a.focus, Focus::Messages);
    assert_eq!(a.messages.items.len(), n);
    // Click near the right edge closes it.
    a.messages_mouse(left_click(78, 4));
    assert_eq!(a.messages.items.len(), n - 1);
    // Below the last message: nothing.
    a.messages_mouse(left_click(62, 11));
    assert_eq!(a.messages.items.len(), n - 1);
}

#[test]
fn bottom_dock_keys_scroll_and_return_focus() {
    let mut a = app();
    a.layout.bottom_dock = Rect::new(0, 10, 40, 6);
    for i in 0..40 {
        a.bottom_dock.push(format!("l{i}"));
    }
    a.focus = Focus::BottomDock;
    a.on_key(key(KeyCode::Home));
    assert_eq!(a.bottom_dock.scroll, 0);
    a.on_key(key(KeyCode::Down));
    assert_eq!(a.bottom_dock.scroll, 1);
    a.on_key(key(KeyCode::PageDown));
    assert_eq!(a.bottom_dock.scroll, 6);
    a.on_key(key(KeyCode::PageUp));
    assert_eq!(a.bottom_dock.scroll, 1);
    a.on_key(key(KeyCode::Up));
    assert_eq!(a.bottom_dock.scroll, 0);
    a.on_key(key(KeyCode::End));
    assert!(a.bottom_dock.scroll > 20);
    a.on_key(key(KeyCode::Esc));
    assert_eq!(a.focus, Focus::Editor);
}

#[test]
fn explorer_mouse_previews_opens_and_scrolls() {
    let dir = tmp("explorer-mouse");
    std::fs::write(dir.join("a.txt"), "A").unwrap();
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/inner.txt"), "I").unwrap();
    let mut a = app_in(&dir);
    a.explorer.rebuild();
    a.layout.explorer = Rect::new(0, 2, 30, 12);
    let wheel = |kind| MouseEvent {
        kind,
        column: 3,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    a.explorer_mouse(wheel(MouseEventKind::ScrollDown));
    let after_down = a.explorer.selected;
    a.explorer_mouse(wheel(MouseEventKind::ScrollUp));
    assert!(a.explorer.selected < after_down || after_down == 0);
    let file_row = a
        .explorer
        .nodes
        .iter()
        .position(|n| n.path.ends_with("a.txt"))
        .unwrap();
    let dir_row = a
        .explorer
        .nodes
        .iter()
        .position(|n| n.path.ends_with("sub"))
        .unwrap();
    // The row above the first node is the border: focuses only.
    a.explorer_mouse(left_click(3, 2));
    assert_eq!(a.focus, Focus::Explorer);
    // First click previews, second click on the same file opens a real tab.
    a.explorer_mouse(left_click(3, 3 + u16::try_from(file_row).unwrap()));
    assert_eq!(a.explorer.selected, file_row);
    a.explorer_mouse(left_click(3, 3 + u16::try_from(file_row).unwrap()));
    assert!(a.editor.tabs.iter().any(|t| !t.preview && t.text() == "A"));
    // A click on a directory expands it.
    let nodes_before = a.explorer.nodes.len();
    a.explorer_mouse(left_click(3, 3 + u16::try_from(dir_row).unwrap()));
    assert!(a.explorer.nodes.len() > nodes_before);
    // A click below the last node does nothing.
    let sel = a.explorer.selected;
    a.explorer_mouse(left_click(3, 12));
    assert_eq!(a.explorer.selected, sel);
    // Dragging the scrollbar column jumps the selection.
    a.settings.viewport.show_scrollbar = true;
    a.layout.explorer = Rect::new(0, 2, 30, 3); // tiny viewport: scrollbar shows
    a.explorer_mouse(left_click(28, 3));
    assert_eq!(a.focus, Focus::Explorer);
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- workspace files ------------------------------------------------------------------

#[test]
fn workspace_prompts_seed_a_default_save_path() {
    let dir = tmp("ws-prompt");
    let mut a = app_in(&dir);
    a.prompt_workspace(PromptKind::WorkspaceSave);
    let seed = a.prompt.as_ref().unwrap().input.clone();
    assert!(seed.starts_with(&dir.to_string_lossy().into_owned()));
    assert!(seed.ends_with(&format!(".{}", crate::workspace::EXTENSION)));
    a.prompt_workspace(PromptKind::WorkspaceOpen);
    assert!(prompt_kind(&a) == Some(PromptKind::WorkspaceOpen));
    assert_eq!(a.prompt.as_ref().unwrap().input, "");
    a.prompt_workspace(PromptKind::WorkspaceAddFolder);
    assert!(prompt_kind(&a) == Some(PromptKind::WorkspaceAddFolder));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn workspace_open_of_a_missing_or_broken_file_reports_an_error() {
    let dir = tmp("ws-open-bad");
    std::fs::write(dir.join("broken.vix-workspace"), "{{{ not a workspace").unwrap();
    let mut a = app_in(&dir);
    let before = a.messages.items.len();
    a.workspace_open(&dir.join("missing.vix-workspace").to_string_lossy());
    assert!(a.messages.items.len() > before);
    let before = a.messages.items.len();
    a.workspace_open(&dir.join("broken.vix-workspace").to_string_lossy());
    assert!(a.messages.items.len() > before);
    a.workspace_add_folder(&dir.join("not-a-dir").to_string_lossy());
    assert!(!a.workspace_folders.contains(&dir.join("not-a-dir")));
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- project-wide replace / editable results --------------------------------------

fn search_fixture(name: &str) -> (PathBuf, App) {
    let dir = tmp(name);
    std::fs::write(dir.join("a.txt"), "foo here\nfoo again\n").unwrap();
    std::fs::write(dir.join("b.txt"), "nothing\n").unwrap();
    std::fs::write(dir.join("c.txt"), "foo there\n").unwrap();
    let mut a = app_in(&dir);
    a.build_file_index();
    (dir, a)
}

fn start_replace(a: &mut App, query: &str, with: &str) {
    a.run_action("search.workspace_replace");
    type_str(a, query);
    a.on_key(key(KeyCode::Tab));
    type_str(a, with);
}

#[test]
fn workspace_replace_previews_then_writes_every_file_on_confirm() {
    let (dir, mut a) = search_fixture("ws-replace");
    start_replace(&mut a, "foo", "BAR");
    assert_eq!(a.workspace_search.as_ref().unwrap().hits.len(), 3);
    a.on_key(key(KeyCode::Enter));
    let rc = a.replace_confirm.as_ref().expect("preview opens");
    assert_eq!(rc.replaced, 3);
    assert_eq!(rc.lines.len(), 2);
    assert!(rc.lines.contains(&"a.txt (2)".to_string()));
    // Scrolling stays within the list.
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Down));
    assert_eq!(a.replace_confirm.as_ref().unwrap().scroll, 1);
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Up));
    assert_eq!(a.replace_confirm.as_ref().unwrap().scroll, 0);
    a.on_key(key(KeyCode::Char('?')));
    // Nothing has been written yet; n cancels.
    a.on_key(key(KeyCode::Char('n')));
    assert!(a.replace_confirm.is_none());
    assert_eq!(read(&dir.join("a.txt")), "foo here\nfoo again\n");
    // Enter on the Replace field again, then confirm with y.
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Char('y')));
    assert!(a.replace_confirm.is_none());
    assert_eq!(read(&dir.join("a.txt")), "BAR here\nBAR again\n");
    assert_eq!(read(&dir.join("c.txt")), "BAR there\n");
    assert_eq!(read(&dir.join("b.txt")), "nothing\n");
    assert_eq!(
        a.workspace_search.as_ref().unwrap().status,
        t!("status.replaced_in_files", replaced = 3, files = 2).to_string()
    );
    assert_eq!(a.workspace_search.as_ref().unwrap().hits.len(), 0);
    assert!(
        a.messages
            .items
            .iter()
            .any(|m| m.text == t!("status.replaced_in_files", replaced = 3, files = 2))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn workspace_replace_keeps_dirty_buffers_unsaved_and_skips_read_only_ones() {
    let (dir, mut a) = search_fixture("ws-replace-open");
    a.open_path(&dir.join("a.txt"), false);
    type_text(&mut a, "x");
    a.open_path(&dir.join("c.txt"), false);
    a.run_action("view.read_only");
    start_replace(&mut a, "foo", "BAR");
    a.on_key(key(KeyCode::Enter));
    a.on_key(key(KeyCode::Enter));
    // The dirty buffer changed in memory only; the read-only one is refused.
    assert_eq!(read(&dir.join("a.txt")), "foo here\nfoo again\n");
    assert!(a.editor.tabs[0].text().contains("BAR"));
    assert!(a.editor.tabs[0].dirty);
    assert_eq!(read(&dir.join("c.txt")), "foo there\n");
    assert!(a.messages.items.iter().any(|m| {
        m.text
            == t!(
                "msg.replace_read_only_skipped",
                path = dir.join("c.txt").display()
            )
    }));
    assert!(
        a.messages
            .items
            .iter()
            .any(|m| m.text == t!("msg.replace_buffer_only", count = 1))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn workspace_replace_handles_no_hits_and_bad_patterns() {
    let (dir, mut a) = search_fixture("ws-replace-none");
    // Not in replace mode: Enter just opens the (absent) selected hit.
    a.run_action("search.workspace");
    a.workspace_replace_all();
    assert!(a.replace_confirm.is_none());
    a.on_key(key(KeyCode::Esc));
    // A pattern with no matches plans nothing.
    start_replace(&mut a, "zzzzz", "q");
    a.on_key(key(KeyCode::Enter));
    assert!(a.replace_confirm.is_none());
    // A pattern matching only text equal to its replacement plans nothing.
    a.workspace_search.as_mut().unwrap().query = "foo".to_string();
    a.workspace_search.as_mut().unwrap().replace = "foo".to_string();
    a.run_workspace_search();
    a.workspace_replace_all();
    assert!(a.replace_confirm.is_none());
    assert_eq!(
        a.workspace_search.as_ref().unwrap().status,
        t!("status.replaced_in_files", replaced = 0, files = 0).to_string()
    );
    // Empty query clears the hits; a bad regex is reported.
    a.workspace_search.as_mut().unwrap().query.clear();
    a.run_workspace_search();
    assert_eq!(a.workspace_search.as_ref().unwrap().hits.len(), 0);
    assert_eq!(
        a.workspace_search.as_ref().unwrap().status,
        t!("status.workspace_search_prompt").to_string()
    );
    a.on_key(alt('r'));
    a.workspace_search.as_mut().unwrap().query = "(".to_string();
    a.run_workspace_search();
    assert_ne!(a.workspace_search.as_ref().unwrap().status, "");
    a.workspace_replace_all();
    assert!(a.replace_confirm.is_none());
    // Static result lists are never re-searched.
    a.workspace_search
        .as_mut()
        .unwrap()
        .flags
        .insert(WorkspaceFlags::STATIC_RESULTS);
    a.run_workspace_search();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn editable_search_results_write_changed_lines_back_after_confirmation() {
    let (dir, mut a) = search_fixture("wgrep");
    a.run_action("search.edit_results"); // no search open: nothing
    assert!(a.wgrep_baseline.is_none());
    a.run_action("search.workspace");
    type_str(&mut a, "foo");
    a.on_key(alt('e'));
    assert!(a.workspace_search.is_none());
    assert_eq!(
        a.editor.active_tab().unwrap().path.as_deref(),
        Some(Path::new(App::WGREP_RESULTS_PATH))
    );
    assert_eq!(a.status, t!("status.wgrep_opened", count = 3).to_string());
    // Saving unchanged results reports that nothing changed.
    a.run_action("file.save");
    assert_eq!(a.status, t!("status.wgrep_no_changes").to_string());
    // Edit one result line and save: a confirmation lists the file.
    let edited = active_text(&a).replace("foo again", "baz again");
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content(&edited);
    a.run_action("file.save");
    let wc = a.wgrep_confirm.as_ref().expect("confirm");
    assert_eq!(wc.lines, vec!["a.txt (1)".to_string()]);
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Up));
    a.on_key(key(KeyCode::Char('n')));
    assert!(a.wgrep_confirm.is_none());
    assert_eq!(read(&dir.join("a.txt")), "foo here\nfoo again\n");
    a.run_action("file.save");
    a.on_key(key(KeyCode::Char('y')));
    assert_eq!(read(&dir.join("a.txt")), "foo here\nbaz again\n");
    assert_eq!(
        a.status,
        t!("status.wgrep_applied", edited = 1, files = 1).to_string()
    );
    // The edited line is the new baseline: saving again is a no-op.
    a.run_action("file.save");
    assert_eq!(a.status, t!("status.wgrep_no_changes").to_string());
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- outline -----------------------------------------------------------------------

const RUST_SRC: &str = "fn alpha() {}\n\nfn beta() {}\n\nfn gamma() {}\n";

#[test]
fn outline_panel_lists_symbols_and_jumps() {
    let mut a = app();
    a.run_action("nav.outline");
    assert_eq!(a.status, t!("status.outline_empty").to_string());
    a.editor.new_tab_with_content_as(RUST_SRC, Some("x.rs"));
    set_cur(&mut a, 20);
    a.run_action("nav.outline");
    let o = a.outline.as_ref().expect("outline");
    assert_eq!(o.entries.len(), 3);
    a.layout.outline = Rect::new(0, 3, 40, 10);
    a.on_key(key(KeyCode::Down));
    a.on_key(key(KeyCode::Enter));
    assert!(a.outline.is_none());
    assert_eq!(a.editor.cursor_line(), 4, "jumped to gamma");
    // A click on a row jumps; Esc closes without moving.
    a.run_action("nav.outline");
    a.outline_mouse(left_click(2, 3));
    assert!(a.outline.is_none());
    assert_eq!(a.editor.cursor_line(), 0);
    a.run_action("nav.outline");
    a.on_key(key(KeyCode::Esc));
    assert!(a.outline.is_none());
    // A click past the last row is ignored.
    a.run_action("nav.outline");
    a.outline_mouse(left_click(2, 12));
    assert!(a.outline.is_some());
}

#[test]
fn outline_dock_tracks_the_cursor_and_clicks_jump() {
    let mut a = app();
    a.editor.new_tab_with_content_as(RUST_SRC, Some("x.rs"));
    a.run_action("view.outline_dock");
    assert!(a.settings.secondary_panels.show_outline_dock);
    let o = a.outline_dock.as_ref().expect("dock outline");
    assert_eq!(o.entries.len(), 3);
    assert_eq!(o.selected, 0);
    set_cur(&mut a, 30);
    a.refresh_outline_dock();
    assert_eq!(
        a.outline_dock.as_ref().unwrap().selected,
        2,
        "nearest symbol"
    );
    a.outline_dock_click(1);
    assert_eq!(a.editor.cursor_line(), 2);
    assert_eq!(a.focus, Focus::Editor);
    a.outline_dock_click(50); // past the end: ignored
    assert_eq!(a.editor.cursor_line(), 2);
    // A buffer with no symbols empties the dock; turning it off clears it.
    a.editor.new_tab_with_content("plain words\n");
    a.refresh_outline_dock();
    assert!(a.outline_dock.is_none());
    a.run_action("view.outline_dock");
    assert!(a.outline_dock.is_none());
}

// ----- edit surfaces: open, save through the buffer, close ---------------------------------

fn ctrl_s() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
}

fn surface_app(name: &str, file: &str, body: &str) -> (PathBuf, App) {
    let dir = tmp(name);
    std::fs::write(dir.join(file), body).unwrap();
    let mut a = app_in(&dir);
    a.layout.edit_table = Rect::new(0, 0, 60, 12);
    a.layout.edit_outline = Rect::new(0, 0, 60, 12);
    a.layout.edit_value = Rect::new(0, 0, 60, 12);
    a.layout.edit_bytes = Rect::new(0, 0, 60, 12);
    a.open_path(&dir.join(file), false);
    (dir, a)
}

#[test]
fn table_editor_saves_the_grid_back_to_the_file_and_closes() {
    let (dir, mut a) = surface_app("surf-table", "t.csv", "a,b\n1,2\n");
    a.run_action("tools.edit_table");
    assert!(a.edit.edit_table().is_some());
    a.on_key(ctrl_s());
    assert_eq!(read(&dir.join("t.csv")), "a,b\n1,2\n");
    assert!(!a.editor.active_tab().unwrap().dirty);
    a.on_key(key(KeyCode::Char('q')));
    assert!(a.edit.edit_table().is_none());
    // A TSV file opens as tab-separated.
    std::fs::write(dir.join("t.tsv"), "x\ty\n").unwrap();
    a.open_path(&dir.join("t.tsv"), false);
    a.run_action("tools.edit_table");
    a.on_key(ctrl_s());
    assert_eq!(read(&dir.join("t.tsv")), "x\ty\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn outline_editor_saves_and_closes() {
    let (dir, mut a) = surface_app("surf-outline", "o.txt", "top\n  child\n");
    a.run_action("tools.edit_outline");
    assert!(a.edit.edit_outline().is_some());
    a.on_key(ctrl_s());
    assert_eq!(read(&dir.join("o.txt")), "top\n  child\n");
    a.on_key(key(KeyCode::Esc));
    assert!(a.edit.edit_outline().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn value_editor_round_trips_json_and_yaml_and_rejects_bad_input() {
    let (dir, mut a) = surface_app("surf-json", "v.json", "{\"a\": 1}\n");
    a.run_action("tools.edit_json");
    assert!(a.edit.edit_value().is_some());
    a.on_key(ctrl_s());
    let saved = read(&dir.join("v.json"));
    assert!(saved.contains("\"a\"") && saved.contains('1'), "{saved}");
    a.on_key(key(KeyCode::Char('q')));
    assert!(a.edit.edit_value().is_none());
    std::fs::write(dir.join("v.yaml"), "a: 1\n").unwrap();
    a.open_path(&dir.join("v.yaml"), false);
    a.run_action("tools.edit_yaml");
    assert!(a.edit.edit_value().is_some());
    a.on_key(ctrl_s());
    assert!(read(&dir.join("v.yaml")).contains("a:"));
    a.on_key(key(KeyCode::Esc));
    // Text that does not parse is reported and nothing opens.
    a.editor.new_tab_with_content("{ not json");
    a.run_action("tools.edit_json");
    assert!(a.edit.edit_value().is_none());
    assert!(a.messages.items.iter().any(|m| {
        m.text.starts_with(
            t!("msg.edit_value_parse", error = "")
                .to_string()
                .trim_end_matches(|c: char| !c.is_alphanumeric()),
        )
    }));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn edit_surfaces_warn_when_there_is_no_buffer() {
    let mut a = app();
    a.editor.tabs.clear();
    a.run_action("tools.edit_table");
    a.run_action("tools.edit_outline");
    a.run_action("tools.edit_json");
    a.run_action("tools.edit_bytes");
    let warned: Vec<&str> = a.messages.items.iter().map(|m| m.text.as_str()).collect();
    for key in [
        "msg.edit_table_no_buffer",
        "msg.edit_outline_no_buffer",
        "msg.edit_value_no_buffer",
        "msg.edit_bytes_no_buffer",
    ] {
        assert!(warned.contains(&t!(key).as_ref()), "{key}");
    }
    assert!(a.edit.edit_table().is_none());
}

#[test]
fn byte_editor_saves_a_text_buffer_and_a_binary_file_byte_for_byte() {
    let (dir, mut a) = surface_app("surf-bytes", "b.txt", "hello\n");
    a.run_action("tools.edit_bytes");
    assert!(a.edit.edit_bytes().is_some());
    a.on_key(ctrl_s());
    assert_eq!(read(&dir.join("b.txt")), "hello\n");
    a.on_key(key(KeyCode::Char('q')));
    assert!(a.edit.edit_bytes().is_none());

    // A binary file opened straight into the byte editor is rewritten as bytes.
    let bin = dir.join("raw.bin");
    std::fs::write(&bin, [0u8, 159, 146, 150, 0]).unwrap();
    a.open_path(&bin, false);
    a.on_key(key(KeyCode::Char('y')));
    assert_eq!(a.edit_bytes_file.as_deref(), Some(bin.as_path()));
    a.layout.edit_bytes = Rect::new(0, 0, 60, 12);
    a.on_key(ctrl_s());
    assert_eq!(std::fs::read(&bin).unwrap(), [0u8, 159, 146, 150, 0]);
    assert_eq!(
        a.status,
        t!("status.saved", path = bin.display()).to_string()
    );
    a.on_key(key(KeyCode::Esc));
    assert!(a.edit.edit_bytes().is_none());
    assert!(a.edit_bytes_file.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "sql")]
#[test]
fn sql_editor_saves_and_closes() {
    let (dir, mut a) = surface_app("surf-sql", "q.sql", "select 1;\n");
    a.run_action("tools.edit_sql");
    assert!(a.edit.edit_sql().is_some());
    a.layout.edit_sql = Rect::new(0, 0, 60, 12);
    a.on_key(ctrl_s());
    assert!(read(&dir.join("q.sql")).to_lowercase().contains("select 1"));
    a.on_key(key(KeyCode::Char('q')));
    assert!(a.edit.edit_sql().is_none());
    a.editor.tabs.clear();
    a.run_action("tools.edit_sql");
    assert!(a.edit.edit_sql().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- Edit menu: motions, selections, transforms -------------------------------------------

fn sel_text(a: &mut App) -> Option<String> {
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .get_selection_text()
}

#[test]
fn edit_motion_actions_move_the_cursor_to_documented_places() {
    let mut a = app_with("alpha beta\ngamma delta\n\nepsilon (zeta)\n");
    set_cur(&mut a, 13);
    a.run_action("edit.line_start");
    assert_eq!(cursor(&a), 11);
    a.run_action("edit.line_end");
    assert_eq!(cursor(&a), 22);
    a.run_action("edit.go_last");
    assert_eq!(cursor(&a), 39);
    a.run_action("edit.go_first");
    assert_eq!(cursor(&a), 0);
    a.run_action("edit.para_end");
    let after_para_end = cursor(&a);
    assert!(after_para_end > 0);
    a.run_action("edit.para_start");
    assert!(cursor(&a) < after_para_end);
    a.run_action("edit.section_end");
    let after_section_end = cursor(&a);
    a.run_action("edit.section_start");
    assert!(cursor(&a) <= after_section_end);
    // The motion.* wrappers behave like the arrow keys.
    set_cur(&mut a, 0);
    a.run_action("motion.char_right");
    assert_eq!(cursor(&a), 1);
    a.run_action("motion.char_left");
    assert_eq!(cursor(&a), 0);
    a.run_action("motion.line_down");
    assert_eq!(a.editor.cursor_line(), 1);
    a.run_action("motion.line_up");
    assert_eq!(a.editor.cursor_line(), 0);
    a.run_action("motion.end");
    assert_eq!(cursor(&a), 10);
    a.run_action("motion.home");
    assert_eq!(cursor(&a), 0);
    a.run_action("motion.page_down");
    assert!(a.editor.cursor_line() > 0);
    a.run_action("motion.page_up");
    assert_eq!(a.editor.cursor_line(), 0);
    a.run_action("motion.delete_forward");
    assert!(active_text(&a).starts_with("lpha"));
    a.run_action("edit.keyboard_quit");
    assert_eq!(a.status, t!("status.emacs_quit").to_string());
}

#[test]
fn edit_selection_actions_select_words_lines_and_blocks() {
    let mut a = app_with("alpha beta\ngamma delta\n\nepsilon (zeta)\n");
    set_cur(&mut a, 0);
    a.run_action("edit.select_more");
    assert_eq!(sel_text(&mut a).as_deref(), Some("alpha"));
    a.run_action("edit.select_less");
    a.run_action("edit.select_line");
    assert_eq!(sel_text(&mut a).as_deref(), Some("alpha beta\n"));
    set_cur(&mut a, 2);
    a.run_action("edit.select_paragraph");
    assert!(sel_text(&mut a).unwrap().starts_with("alpha beta"));
    a.run_action("edit.select_section");
    assert!(sel_text(&mut a).is_some());
    a.run_action("edit.select_all");
    assert_eq!(sel_text(&mut a).unwrap(), active_text(&a));
    // Matching bracket.
    set_cur(&mut a, 32);
    a.run_action("edit.match_bracket");
    assert!(
        cursor(&a) >= 37,
        "jumped to the closing paren: {}",
        cursor(&a)
    );
    // Every occurrence of the word under the cursor, and column selection.
    let mut a = app_with("ab ab\nab\n");
    set_cur(&mut a, 0);
    a.run_action("edit.select_more");
    a.run_action("edit.select_all_occurrences");
    a.run_action("edit.column_select_down");
    a.run_action("edit.column_select_up");
}

#[test]
fn edit_move_line_swaps_neighbouring_lines() {
    let mut a = app_with("one\ntwo\nthree\n");
    set_cur(&mut a, 4);
    a.run_action("edit.move_line_up");
    assert_eq!(active_text(&a), "two\none\nthree\n");
    a.run_action("edit.move_line_down");
    assert_eq!(active_text(&a), "one\ntwo\nthree\n");
}

#[test]
fn goto_actions_seed_the_palette_with_their_prefix() {
    for (action, seed) in [
        ("nav.goto_line", ":"),
        ("nav.goto_symbol", "@"),
        ("nav.goto_workspace_symbol", "@@"),
        ("nav.switch_buffer", "#"),
    ] {
        let mut a = app_with("x\n");
        a.run_action(action);
        assert_eq!(a.palette.as_ref().expect(action).input, seed, "{action}");
    }
}

#[test]
fn undo_and_redo_walk_the_edit_history() {
    let mut a = app_with("");
    type_text(&mut a, "abc");
    a.run_action("edit.undo");
    assert_eq!(active_text(&a), "");
    a.run_action("edit.redo");
    assert_eq!(active_text(&a), "abc");
    assert!(a.editor.active_tab().unwrap().dirty);
}

#[test]
fn cut_and_copy_record_the_selection_in_the_clipboard_history() {
    let mut a = app_with("hello world");
    a.run_action("edit.select_all");
    a.run_action("edit.copy");
    assert_eq!(
        a.clipboard_ring.first().map(String::as_str),
        Some("hello world")
    );
    assert_eq!(active_text(&a), "hello world");
    a.run_action("edit.cut");
    assert_eq!(active_text(&a), "");
    assert!(a.editor.active_tab().unwrap().dirty);
    a.run_action("edit.paste");
}

#[test]
fn case_actions_rewrite_the_selection() {
    for (action, f) in [
        ("edit.case_upper", crate::case::upper as fn(&str) -> String),
        ("edit.case_lower", crate::case::lower),
        ("edit.case_title", crate::case::title),
        ("edit.case_kebab", crate::case::kebab),
        ("edit.case_snake", crate::case::snake),
        ("edit.case_camel", crate::case::camel),
        ("edit.case_pascal", crate::case::pascal),
    ] {
        let mut a = app_with("Hello big_World");
        a.run_action("edit.select_all");
        a.run_action(action);
        assert_eq!(active_text(&a), f("Hello big_World"), "{action}");
    }
}

#[test]
fn line_transform_actions_reorder_and_clean_lines() {
    let cases: &[(&str, &str, &str)] = &[
        ("edit.sort_lines", "b\na\nc\n", "a\nb\nc\n"),
        ("edit.reverse_lines", "a\nb\nc\n", "c\nb\na\n"),
        ("edit.remove_duplicate_lines", "a\nb\na\n", "a\nb\n"),
        ("edit.sort_unique", "b\na\nb\n", "a\nb\n"),
        ("edit.trim_trailing_whitespace", "a  \nb\t\n", "a\nb\n"),
        ("edit.squeeze_blank_lines", "a\n\n\n\nb\n", "a\n\nb\n"),
        ("edit.eol_crlf", "a\nb\n", "a\r\nb\r\n"),
        ("edit.eol_lf", "a\r\nb\r\n", "a\nb\n"),
        ("tools.convert.rot13", "abc", "nop"),
        (
            "edit.align.equals",
            "a = 1\nbbb = 2\n",
            "a   = 1\nbbb = 2\n",
        ),
    ];
    for (action, input, expected) in cases {
        let mut a = app_with(input);
        a.run_action("edit.select_all");
        a.run_action(action);
        assert_eq!(&active_text(&a), expected, "{action}");
    }
    let mut a = app_with("x\ny\n");
    set_cur(&mut a, 0);
    a.run_action("edit.duplicate_line");
    assert_eq!(active_text(&a), "x\nx\ny\n");
    let mut a = app_with("x\ny\n");
    a.run_action("edit.select_all");
    a.run_action("edit.join_lines");
    assert_eq!(active_text(&a).trim_end(), "x y");
    let mut a = app_with("c\nb\na\n");
    a.run_action("edit.select_all");
    a.run_action("edit.shuffle");
    let mut got: Vec<char> = active_text(&a)
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect();
    got.sort_unstable();
    assert_eq!(got, ['a', 'b', 'c'], "shuffling keeps every line");
}

#[test]
fn transpose_delete_and_number_actions_edit_around_the_cursor() {
    let mut a = app_with("ab");
    set_cur(&mut a, 1);
    a.run_action("edit.transpose_chars");
    assert_eq!(active_text(&a), "ba");
    let mut a = app_with("one two");
    set_cur(&mut a, 3);
    a.run_action("edit.transpose_words");
    assert_eq!(active_text(&a), "two one");
    let mut a = app_with("a\nb\n");
    set_cur(&mut a, 2);
    a.run_action("edit.transpose_lines");
    assert_eq!(active_text(&a), "b\na\n");
    let mut a = app_with("abc");
    set_cur(&mut a, 1);
    a.run_action("edit.delete.character");
    assert_eq!(active_text(&a), "ac");
    let mut a = app_with("one two three");
    set_cur(&mut a, 4);
    a.run_action("edit.delete.word");
    assert!(!active_text(&a).contains("two"));
    let mut a = app_with("Hello. World.");
    set_cur(&mut a, 2);
    a.run_action("edit.delete.sentence");
    assert!(!active_text(&a).contains("Hello"));
    let mut a = app_with("para one\n\npara two\n");
    set_cur(&mut a, 2);
    a.run_action("edit.delete.paragraph");
    assert!(!active_text(&a).contains("para one"));
    let mut a = app_with("x 41 y");
    set_cur(&mut a, 3);
    a.run_action("edit.increment_number");
    assert_eq!(active_text(&a), "x 42 y");
    a.run_action("edit.decrement_number");
    a.run_action("edit.decrement_number");
    assert_eq!(active_text(&a), "x 40 y");
}

#[test]
fn read_only_buffers_refuse_mutating_edit_actions_but_allow_selection() {
    let mut a = app_with("keep");
    a.run_action("view.read_only");
    a.run_action("edit.case_upper");
    assert_eq!(a.status, t!("status.read_only_blocked").to_string());
    assert_eq!(active_text(&a), "keep");
    a.status.clear();
    a.run_action("edit.select_all");
    assert_eq!(a.status, "");
    assert_eq!(sel_text(&mut a).as_deref(), Some("keep"));
}

#[test]
fn toggle_comment_comments_and_uncomments_the_line() {
    let mut a = app();
    a.editor
        .new_tab_with_content_as("let x = 1;\n", Some("x.rs"));
    a.run_action("edit.toggle_comment");
    assert!(active_text(&a).starts_with("//"));
    a.run_action("edit.toggle_comment");
    assert_eq!(active_text(&a), "let x = 1;\n");
}

// ----- menu bar keyboard ---------------------------------------------------------------------

#[test]
fn menu_keys_navigate_switch_and_close() {
    let mut a = app();
    assert!(!a.menu.is_open());
    // Alt+<letter> opens a menu; naming it again closes it; another letter switches.
    a.on_key(alt('e'));
    assert!(a.menu.is_open());
    let edit = a.menu.open;
    a.on_key(alt('e'));
    assert!(!a.menu.is_open());
    a.on_key(alt('e'));
    a.on_key(alt('t'));
    assert!(a.menu.is_open());
    assert_ne!(a.menu.open, edit);
    // Arrow keys move within and across menus.
    let top = a.menu.open;
    a.on_key(key(KeyCode::Right));
    assert_ne!(a.menu.open, top);
    a.on_key(key(KeyCode::Left));
    assert_eq!(a.menu.open, top);
    let first = a.menu.item;
    a.on_key(key(KeyCode::Down));
    assert_ne!(a.menu.item, first);
    a.on_key(key(KeyCode::Up));
    assert_eq!(a.menu.item, first);
    // A plain letter type-aheads without closing.
    a.on_key(key(KeyCode::Char('c')));
    assert!(a.menu.is_open());
    // Esc and F10 close; a stray key is ignored.
    a.on_key(key(KeyCode::Char('?')));
    a.on_key(key(KeyCode::Esc));
    assert!(!a.menu.is_open());
    a.on_key(alt('h'));
    assert!(a.menu.is_open());
    a.on_key(key(KeyCode::F(10)));
    assert!(!a.menu.is_open());
}

#[test]
fn menu_enter_runs_the_highlighted_item_and_closes_the_menu() {
    let mut a = app();
    a.on_key(alt('h')); // Help
    let items = a.menu.defs()[a.menu.open.unwrap()].items;
    let want = items
        .iter()
        .position(|i| i.action == "help.license")
        .expect("Help lists License");
    for _ in 0..items.len() {
        if a.menu.item == Some(want) {
            break;
        }
        a.on_key(key(KeyCode::Down));
    }
    assert_eq!(a.menu.item, Some(want));
    a.on_key(key(KeyCode::Enter));
    assert!(!a.menu.is_open());
    assert!(a.welcome.is_some(), "the License overlay opened");
}

// ----- explorer delete -------------------------------------------------------------------------

#[test]
fn explorer_delete_asks_then_removes_the_file_and_closes_its_buffer() {
    let dir = tmp("delete");
    std::fs::write(dir.join("gone.txt"), "x").unwrap();
    std::fs::write(dir.join("stay.txt"), "y").unwrap();
    let mut a = app_in(&dir);
    a.settings.explorer_delete = "hard".to_string();
    a.explorer.rebuild();
    a.focus = Focus::Explorer;
    a.on_key(key(KeyCode::Delete));
    let c = a.confirm.as_ref().expect("asks first");
    assert_eq!(c.message, t!("confirm.delete_hard", n = 1).to_string());
    let target = c.paths[0].clone();
    a.on_key(key(KeyCode::Char('x'))); // unrelated key keeps asking
    assert!(a.confirm.is_some());
    a.on_key(key(KeyCode::Char('n')));
    assert!(a.confirm.is_none());
    assert_eq!(a.status, t!("status.delete_cancelled").to_string());
    assert!(target.exists());
    // Open it, delete it for real, and its tab goes away.
    a.open_path(&target, false);
    a.on_key(key(KeyCode::Delete));
    a.on_key(key(KeyCode::Char('y')));
    assert!(!target.exists());
    assert_eq!(a.status, t!("status.deleted_n", n = 1).to_string());
    assert!(
        a.editor
            .tabs
            .iter()
            .all(|t| t.path.as_deref() != Some(target.as_path()))
    );
    assert!(dir.join("stay.txt").exists() || dir.join("gone.txt").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- mouse dispatch --------------------------------------------------------------------------

fn mouse(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn clicking_the_menu_bar_opens_and_closes_a_menu() {
    let mut a = app();
    a.layout.menu = Rect::new(0, 0, 80, 1);
    a.on_mouse(left_click(2, 0));
    assert!(a.menu.is_open());
    a.on_mouse(left_click(2, 0));
    assert!(
        !a.menu.is_open(),
        "clicking the open menu's title closes it"
    );
    // Hover / wheel follow the highlight while a menu is open.
    a.on_mouse(left_click(2, 0));
    let item = a.menu.item;
    a.on_mouse(mouse(MouseEventKind::ScrollDown, 5, 5));
    assert_ne!(a.menu.item, item);
    a.on_mouse(mouse(MouseEventKind::ScrollUp, 5, 5));
    assert_eq!(a.menu.item, item);
    a.on_mouse(mouse(MouseEventKind::Moved, 5, 5));
    a.on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 5, 5));
    assert!(a.menu.is_open());
    // A click far away from every dropdown closes it.
    a.layout.menu_dropdown = Rect::new(0, 1, 20, 10);
    a.on_mouse(left_click(70, 20));
    assert!(!a.menu.is_open());
}

#[test]
fn tab_strip_click_activates_the_clicked_tab() {
    let mut a = app_with("one");
    a.editor.new_tab_with_content("two");
    a.layout.tabs = Rect::new(0, 1, 80, 1);
    let last = a.editor.tabs.len() - 1;
    assert_eq!(a.editor.active, last);
    let origin = crate::ui::tabs_origin(&a, a.layout.tabs);
    a.on_mouse(left_click(origin, 1));
    assert_eq!(a.editor.active, 0);
    assert_eq!(a.focus, Focus::Editor);
    // Past the last tab does nothing.
    a.on_mouse(left_click(79, 1));
    assert_eq!(a.editor.active, 0);
    let second = origin + u16::try_from(a.editor.tabs[0].title().chars().count()).unwrap() + 3;
    a.on_mouse(left_click(second, 1));
    assert_eq!(a.editor.active, 1);
    // Non-click events on the tab strip are ignored.
    a.on_mouse(mouse(MouseEventKind::ScrollDown, origin, 1));
    assert_eq!(a.editor.active, 1);
}

#[test]
fn clicking_the_editor_moves_the_cursor_and_the_side_panes_route_their_own_clicks() {
    let dir = tmp("mouse-routes");
    std::fs::write(dir.join("a.txt"), "A").unwrap();
    let mut a = app_in(&dir);
    a.editor.new_tab_with_content("abc\ndef\nghi\n");
    a.layout.editor = Rect::new(20, 3, 50, 15);
    a.layout.explorer = Rect::new(0, 3, 20, 15);
    a.layout.messages = Rect::new(70, 3, 10, 15);
    a.visible.set(Visible::EXPLORER, true);
    a.visible.set(Visible::MESSAGES, true);
    a.on_mouse(left_click(30, 4));
    assert_eq!(a.editor.cursor_line(), 1, "clicked the second text row");
    // Explorer and messages clicks focus their panes.
    a.explorer.rebuild();
    a.on_mouse(left_click(3, 5));
    assert_eq!(a.focus, Focus::Explorer);
    a.messages.info("hi".to_string());
    a.on_mouse(left_click(72, 4));
    assert_eq!(a.focus, Focus::Messages);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_panel_and_outline_dock_clicks_jump_to_their_targets() {
    let dir = tmp("mouse-panels");
    std::fs::write(dir.join("t.rs"), "fn a() {}\n\nfn b() {}\n").unwrap();
    let mut a = app_in(&dir);
    a.open_path(&dir.join("t.rs"), false);
    a.visible.set(Visible::TEST_PANEL, true);
    a.layout.editor = Rect::new(0, 3, 50, 10);
    a.layout.test_panel = Rect::new(0, 20, 40, 4);
    a.test_results = crate::test_runner::parse(
        "test t::a ... FAILED\n\nfailures:\n\n---- t::a stdout ----\nthread 't::a' panicked at t.rs:3:1:\nboom\n",
    );
    assert_eq!(a.test_results.len(), 1);
    a.on_mouse(left_click(2, 20));
    assert_eq!(a.test_selected, 0);
    assert_eq!(a.editor.cursor_line(), 2);
    // A click past the last result does nothing.
    set_cur(&mut a, 0);
    a.on_mouse(left_click(2, 22));
    assert_eq!(a.editor.cursor_line(), 0);
    // The outline dock.
    a.settings.secondary_panels.show_outline_dock = true;
    a.refresh_outline_dock();
    a.layout.outline_dock = Rect::new(60, 3, 20, 10);
    a.on_mouse(left_click(62, 4));
    assert_eq!(a.editor.cursor_line(), 2, "jumped to the second symbol");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bottom_dock_clicks_route_through_dispatch() {
    let mut a = app();
    a.visible.set(Visible::BOTTOM_DOCK, true);
    a.layout.editor = Rect::new(0, 3, 80, 10);
    a.layout.bottom_dock = Rect::new(0, 18, 80, 6);
    a.bottom_dock.push("plain output");
    a.on_mouse(left_click(3, 19));
    assert_eq!(a.focus, Focus::BottomDock);
}

#[test]
fn folding_actions_report_when_there_is_nothing_to_fold() {
    let mut a = app_with("plain\n");
    a.fold_at_cursor(true);
    assert_eq!(a.status, t!("status.no_fold").to_string());
    a.status.clear();
    a.fold_at_cursor(false);
    assert_eq!(a.status, t!("status.no_fold").to_string());
    let mut a = app();
    a.editor.new_tab_with_content_as(
        "fn f() {\n    let x = 1;\n    let y = 2;\n}\n",
        Some("x.rs"),
    );
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_fold_ranges(vec![(0, 3)]);
    set_cur(&mut a, 12);
    a.status.clear();
    a.fold_at_cursor(true);
    assert_eq!(a.status, "", "a real fold leaves the status alone");
    assert!(a.editor.active_tab().unwrap().editor.is_line_hidden(1));
    a.fold_at_cursor(false);
    assert_eq!(a.status, "");
    assert!(!a.editor.active_tab().unwrap().editor.is_line_hidden(1));
}
