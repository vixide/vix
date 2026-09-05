#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

// ----- vix-script host wiring (improvement plan T103) ----------------------

#[test]
fn project_script_registers_and_runs_a_command() {
    let dir = unique_dir("script-run");
    fs::create_dir_all(dir.join(".vix/scripts")).unwrap();
    fs::write(
        dir.join(".vix/scripts/greet.rhai"),
        r#"
        register_command("greet", "Greet", "on_greet");
        fn on_greet() { set_buffer_text("hello from script"); }
        "#,
    )
    .unwrap();
    let mut app = app_at(&dir);
    load_scripts_trusted(&mut app);

    // Tools → Scripts → Run… lists every loaded script's commands.
    app.run_action("script.run");
    let chooser = app.script_chooser.as_ref().expect("chooser should open");
    assert_eq!(chooser.commands.len(), 1);
    assert_eq!(chooser.commands[0].0, "greet"); // script file stem
    assert_eq!(chooser.commands[0].1.label, "Greet"); // shown verbatim

    app.on_key(keycode(KeyCode::Enter)); // run the highlighted command
    assert!(app.script_chooser.is_none());
    assert_eq!(app.editor.active_tab().unwrap().text(), "hello from script");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn script_command_appears_in_the_command_palette() {
    let dir = unique_dir("script-palette");
    fs::create_dir_all(dir.join(".vix/scripts")).unwrap();
    fs::write(
        dir.join(".vix/scripts/upper.rhai"),
        r#"
        register_command("uppercase_selection", "Uppercase Selection", "on_up");
        fn on_up() {}
        "#,
    )
    .unwrap();
    let mut app = app_at(&dir);
    load_scripts_trusted(&mut app);

    app.run_action("tools.palette");
    app.on_key(key('>')); // switch to Commands mode
    let p = app.palette.as_ref().expect("palette open");
    let found = p
        .entries
        .iter()
        .find(|e| e.label.contains("Uppercase Selection"))
        .expect("script command should appear in the command palette");
    match &found.action {
        vix::palette::Action::RunCommand(action) => {
            assert_eq!(action, "script:upper:uppercase_selection");
        }
        _ => panic!("expected a RunCommand action"),
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn script_reload_picks_up_a_script_added_after_startup() {
    let dir = unique_dir("script-reload");
    fs::create_dir_all(dir.join(".vix/scripts")).unwrap();
    let mut app = app_at(&dir);
    load_scripts_trusted(&mut app); // no-op: no scripts exist yet
    app.run_action("script.run");
    assert!(
        app.script_chooser.is_none(),
        "no scripts yet: chooser shouldn't open"
    );

    fs::write(
        dir.join(".vix/scripts/late.rhai"),
        r#"register_command("late", "Late", "on_late"); fn on_late() {}"#,
    )
    .unwrap();
    app.run_action("script.reload"); // itself re-checks trust and queues the prompt
    trust_scripts_if_prompted(&mut app);
    app.run_action("script.run");
    let chooser = app
        .script_chooser
        .as_ref()
        .expect("chooser should open now");
    assert_eq!(chooser.commands[0].1.id, "late");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn script_edit_is_blocked_on_a_read_only_buffer() {
    let dir = unique_dir("script-readonly");
    fs::create_dir_all(dir.join(".vix/scripts")).unwrap();
    fs::write(
        dir.join(".vix/scripts/edit.rhai"),
        r#"
        register_command("edit", "Edit", "on_edit");
        fn on_edit() { set_buffer_text("should not apply"); }
        "#,
    )
    .unwrap();
    let mut app = app_at(&dir);
    load_scripts_trusted(&mut app);
    app.run_action("view.read_only");

    app.run_action("script:edit:edit");
    assert_eq!(app.editor.active_tab().unwrap().text(), "");
    assert!(
        app.messages
            .items
            .iter()
            .any(|m| matches!(m.level, vix::messages::Level::Error)),
        "blocked edit should report an error"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn script_prompt_round_trips_to_a_fresh_handler_call() {
    let dir = unique_dir("script-prompt");
    fs::create_dir_all(dir.join(".vix/scripts")).unwrap();
    fs::write(
        dir.join(".vix/scripts/rename.rhai"),
        r#"
        register_command("rename", "Rename", "on_rename");
        fn on_rename() { prompt("New name:", "on_rename_answer"); }
        fn on_rename_answer(answer) { set_buffer_text(answer); }
        "#,
    )
    .unwrap();
    let mut app = app_at(&dir);
    load_scripts_trusted(&mut app);

    app.run_action("script:rename:rename");
    let p = app
        .prompt
        .as_ref()
        .expect("script's prompt() should open App::prompt");
    assert_eq!(p.title, "New name:"); // the script's own message, shown verbatim
    assert!(matches!(p.kind, PromptKind::Script));

    for c in "picked".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter)); // submits -> a fresh call to on_rename_answer
    assert!(app.prompt.is_none());
    assert_eq!(app.editor.active_tab().unwrap().text(), "picked");
    fs::remove_dir_all(&dir).ok();
}

// ----- script trust (T132) -------------------------------------------------

#[test]
fn script_trust_prompt_shows_the_count_and_defers_loading() {
    let dir = unique_dir("script-trust-prompt");
    fs::create_dir_all(dir.join(".vix/scripts")).unwrap();
    fs::write(
        dir.join(".vix/scripts/a.rhai"),
        r#"register_command("a", "A", "h"); fn h() {}"#,
    )
    .unwrap();
    fs::write(
        dir.join(".vix/scripts/b.rhai"),
        r#"register_command("b", "B", "h"); fn h() {}"#,
    )
    .unwrap();
    let mut app = app_at(&dir);
    app.load_scripts();
    assert_eq!(
        loaded_script_command_count(&mut app),
        0,
        "an untrusted workspace's project scripts stay unloaded"
    );
    app.maybe_prompt_script_trust();
    assert_eq!(
        app.script_trust.as_ref().map(|p| p.count),
        Some(2),
        "the prompt names how many scripts are waiting"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn no_project_scripts_means_no_trust_prompt() {
    let dir = unique_dir("script-trust-empty");
    fs::create_dir_all(dir.join(".vix/scripts")).unwrap(); // exists, but empty
    let mut app = app_at(&dir);
    app.load_scripts();
    app.maybe_prompt_script_trust();
    assert!(
        app.script_trust.is_none(),
        "nothing to trust, so nothing to ask about"
    );
    fs::remove_dir_all(&dir).ok();
}

// ----- wiring vix-script's bind_key into the choke point (T104j) ----------
// App::resolve_key_overrides now also builds an Override from every loaded
// script's KeyBinding (action id "script:<stem>:<command_id>", exactly what
// App::run_script_command already parses), merged with the persisted ones
// into one App::apply_key_overrides call -- the *original* T104 ask.

#[test]
fn a_scripts_bind_key_request_actually_fires_through_on_key() {
    let dir = unique_dir("script-bind-key");
    fs::create_dir_all(dir.join(".vix/scripts")).unwrap();
    fs::write(
        dir.join(".vix/scripts/greet.rhai"),
        r#"
        register_command("greet", "Greet", "on_greet");
        bind_key("C-j", "greet");
        fn on_greet() { set_buffer_text("hello from script"); }
        "#,
    )
    .unwrap();
    let mut app = app_at(&dir);
    load_scripts_trusted(&mut app);
    app.resolve_key_overrides();

    // Ctrl+J is unbound in the built-in Apple table, so this can only run
    // through the script's own bind_key request.
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
    assert_eq!(app.editor.active_tab().unwrap().text(), "hello from script");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn script_reload_re_resolves_key_overrides() {
    let dir = unique_dir("script-bind-key-reload");
    fs::create_dir_all(dir.join(".vix/scripts")).unwrap();
    let script_path = dir.join(".vix/scripts/greet.rhai");
    fs::write(
        &script_path,
        r#"
        register_command("greet", "Greet", "on_greet");
        fn on_greet() { set_buffer_text("v1"); }
        "#,
    )
    .unwrap();
    let mut app = app_at(&dir);
    load_scripts_trusted(&mut app);
    app.resolve_key_overrides();
    // F9: claimed by no built-in binding and never falls through to the
    // editor's own literal-character insert when unclaimed, unlike a bare
    // Ctrl+letter -- a clean no-op to assert against before the binding
    // exists.
    app.on_key(KeyEvent::new(KeyCode::F(9), KeyModifiers::NONE));
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "",
        "not bound yet, so F9 is a no-op"
    );

    // Hand-edit the script to add the binding, then reload -- mirrors
    // `script_reload_picks_up_a_script_added_after_startup`'s pattern.
    fs::write(
        &script_path,
        r#"
        register_command("greet", "Greet", "on_greet");
        bind_key("F9", "greet");
        fn on_greet() { set_buffer_text("v2"); }
        "#,
    )
    .unwrap();
    app.run_action("script.reload");
    app.on_key(KeyEvent::new(KeyCode::F(9), KeyModifiers::NONE));
    assert_eq!(app.editor.active_tab().unwrap().text(), "v2");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn two_scripts_binding_the_same_token_both_reject_via_real_discovery() {
    let dir = unique_dir("script-bind-key-conflict");
    fs::create_dir_all(dir.join(".vix/scripts")).unwrap();
    // F9: claimed by no built-in binding and, unlike a bare Ctrl+letter,
    // never falls through to the editor's own literal-character insert
    // when unclaimed -- a clean no-op to assert against.
    fs::write(
        dir.join(".vix/scripts/alpha.rhai"),
        r#"
        register_command("go", "Go A", "on_go");
        bind_key("F9", "go");
        fn on_go() { set_buffer_text("from alpha"); }
        "#,
    )
    .unwrap();
    fs::write(
        dir.join(".vix/scripts/beta.rhai"),
        r#"
        register_command("go", "Go B", "on_go");
        bind_key("F9", "go");
        fn on_go() { set_buffer_text("from beta"); }
        "#,
    )
    .unwrap();
    let mut app = app_at(&dir);
    // Real discovery (not a hand-built Vec<Override>): two genuinely
    // loaded scripts both requesting F9 through resolve_key_overrides,
    // the actual assembly path -- not apply_key_overrides called directly.
    load_scripts_trusted(&mut app);
    app.resolve_key_overrides();
    assert!(
        app.messages
            .items
            .iter()
            .any(|m| matches!(m.level, vix::messages::Level::Error)
                && (m.text.contains("alpha") || m.text.contains("beta"))),
        "two scripts claiming the same token is reported, naming a source: {:?}",
        app.messages
            .items
            .iter()
            .map(|m| &m.text)
            .collect::<Vec<_>>()
    );
    app.on_key(KeyEvent::new(KeyCode::F(9), KeyModifiers::NONE));
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "",
        "neither script's handler ran -- the conflict rejected both"
    );
    fs::remove_dir_all(&dir).ok();
}
