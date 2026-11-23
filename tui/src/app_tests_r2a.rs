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
