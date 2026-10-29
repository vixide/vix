//! T125's new AI features: "Edit selection with instruction" and "Generate
//! doc comment" (the Git panel's AI commit-message generator has its own
//! throwaway-repo test in `git.rs`, alongside its siblings). These drive the
//! CLI spawn path (`ai_command`, made deterministic by reading back a file)
//! end to end -- the first real test coverage `spawn_ai_cmd`/`AiReplace`/
//! `poll_ai_replace` have had at all (Annotate/Improve/Summarize/Explain/
//! Define had none before this). The end of the file covers T730's
//! `ai = "off"` switch.

use crate::integration::common::*;

/// Build an app whose `ai_command` ignores its input and always prints
/// `output` -- deterministic, no real assistant needed. Writes `output` to a
/// file and has the command print that file back (`type` on Windows, `cat`
/// on Unix) rather than trying to build one shell-escaped literal that
/// reproduces `output` exactly on both platforms' own builtins (T547: an
/// earlier version used `printf '%s' '...'`, portable across `sh`
/// implementations but not present on `cmd.exe`'s own `PATH` the way it is
/// once `sh` itself is already running -- confirmed against real Windows
/// CI, not assumed).
fn app_with_canned_ai_reply(dir: &Path, output: &str, ai_diff_review: bool) -> App {
    let canned = dir.join("canned-ai-reply.txt");
    fs::write(&canned, output).unwrap();
    let cmd = if cfg!(windows) {
        format!("type {}", canned.display())
    } else {
        format!("cat {}", canned.display())
    };
    let settings = {
        let mut s = Settings::default();
        s.ai_command = cmd;
        s.misc.ai_diff_review = ai_diff_review;
        s
    };
    app_at_with(dir, settings)
}

#[test]
fn edit_with_instruction_does_nothing_without_a_selection() {
    let dir = unique_dir("ai-edit-instr-none");
    let mut app = app_with_canned_ai_reply(&dir, "unused", true);
    buffer_with(&mut app, "hello world\n", 0);
    app.run_action("ai.edit_with_instruction");
    assert!(app.prompt.is_none(), "no prompt without a selection");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn edit_with_instruction_runs_the_typed_text_and_always_opens_a_diff() {
    let dir = unique_dir("ai-edit-instr");
    // ai_diff_review is deliberately off here: unlike Annotate/Improve,
    // "Edit selection with instruction" must force a reviewable diff
    // regardless of the setting.
    let mut app = app_with_canned_ai_reply(&dir, "REPLACED", false);
    buffer_with(&mut app, "hello world\n", 0);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, 5); // "hello"

    app.run_action("ai.edit_with_instruction");
    let prompt = app.prompt.as_ref().expect("the instruction prompt opened");
    assert!(matches!(prompt.kind, vix::app::PromptKind::AiInstruction));
    for ch in "make it louder".chars() {
        app.on_key(key(ch));
    }
    app.on_key(keycode(KeyCode::Enter));

    wait_for_ai_replace(&mut app, |app| app.ai_diff_review().is_some());
    assert_eq!(app.ai_diff_review().unwrap().result(), "REPLACED");
    // The buffer itself is untouched until the diff is accepted.
    assert_eq!(
        app.editor.active_tab().unwrap().editor.get_content(),
        "hello world\n"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn edit_with_instruction_prompt_cannot_inject_shell_commands() {
    // T547: the same protection `ai_command_line`'s {prompt} placeholder
    // gets from `sh_single_quote`/`cmd_double_quote`, exercised end to end
    // through a real subprocess spawn (not just the string-level unit tests
    // in vix-settings). `echo` (a builtin on both `sh` and `cmd.exe`, unlike
    // `printf` -- which turned out not to be on `cmd.exe`'s own PATH the way
    // it is once `sh` itself is running, an earlier version of this test
    // found the hard way against real Windows CI) always succeeds, so
    // instead of comparing its exact output (`cmd.exe`'s builtin `echo`
    // doesn't strip the surrounding quotes the way a real argv-parsing
    // program would, so a byte-for-byte comparison isn't portable either),
    // this proves the instruction was inert by side effect: an embedded
    // command that -- if it ever escaped its quoting and got interpreted as
    // a *separate* statement -- would create a marker file that must never
    // appear.
    let dir = unique_dir("ai-edit-instr-injection");
    let marker = dir.join("injected.txt");
    let settings = {
        let mut s = Settings::default();
        s.ai_command = "echo {prompt}".to_string();
        s.misc.ai_diff_review = false;
        s
    };
    let mut app = app_at_with(&dir, settings);
    buffer_with(&mut app, "hello world\n", 0);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, 5); // "hello"

    app.run_action("ai.edit_with_instruction");
    let touch = if cfg!(windows) {
        format!("type nul > {}", marker.display())
    } else {
        format!("touch {}", marker.display())
    };
    let evil = format!("\"; {touch}; echo \" & {touch} & echo %PATH% & echo `id`");
    for ch in evil.chars() {
        app.on_key(key(ch));
    }
    app.on_key(keycode(KeyCode::Enter));

    wait_for_ai_replace(&mut app, |app| app.ai_diff_review().is_some());
    assert!(
        !marker.exists(),
        "the embedded command must never actually run: {}",
        app.ai_diff_review().unwrap().result()
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn generate_doc_comment_inserts_just_above_the_cursor_line() {
    let dir = unique_dir("ai-doc-comment");
    // ai_diff_review off: a direct, deterministic buffer assertion.
    let mut app = app_with_canned_ai_reply(&dir, "/// A comment.\n", false);
    buffer_with(&mut app, "fn foo() {}\n", 0);

    app.run_action("ai.generate_doc_comment");
    wait_for_ai_replace(&mut app, |app| {
        app.editor.active_tab().unwrap().editor.get_content() != "fn foo() {}\n"
    });

    assert_eq!(
        app.editor.active_tab().unwrap().editor.get_content(),
        "/// A comment.\nfn foo() {}\n",
        "the comment is a pure insertion before the original line, nothing else changes"
    );
    fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// T730: the AI optional switch (`ai = "off"`).
// ---------------------------------------------------------------------------

/// An app with `ai = "off"` whose `ai_command`, were it ever run, would leave
/// `marker` behind -- so a missing marker proves no AI request was made.
fn app_with_ai_off(dir: &Path, marker: &Path) -> App {
    let cmd = if cfg!(windows) {
        format!("type nul > {}", marker.display())
    } else {
        format!("touch {}", marker.display())
    };
    let settings = {
        let mut s = Settings::default();
        s.ai = "off".to_string();
        s.ai_command = cmd;
        s
    };
    app_at_with(dir, settings)
}

/// Every action id in `items`, submenus included.
fn leaf_actions(items: &[vix::menu::Item], out: &mut Vec<&'static str>) {
    for it in items {
        if let Some(sub) = it.submenu {
            leaf_actions(sub, out);
        } else {
            out.push(it.action);
        }
    }
}

#[test]
fn ai_off_hides_the_ai_menu() {
    let dir = unique_dir("ai-off-menu");
    let on = app_at_with(&dir, Settings::default());
    assert!(
        on.menu.defs().iter().any(|m| m.name == "menu.ai"),
        "AI menu is shown by default"
    );

    let mut off = app_with_ai_off(&dir, &dir.join("marker"));
    let defs = off.menu.defs();
    assert!(
        defs.iter().all(|m| m.name != "menu.ai"),
        "AI menu is hidden"
    );
    let mut actions = Vec::new();
    for m in defs {
        leaf_actions(m.items, &mut actions);
    }
    assert!(
        actions.iter().all(|a| !a.starts_with("ai.")),
        "no AI item anywhere in the menu bar: {actions:?}"
    );
    // Alt+A, the AI menu's mnemonic, opens nothing.
    off.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT));
    assert!(
        off.menu
            .open
            .is_none_or(|i| off.menu.defs()[i].name != "menu.ai"),
        "Alt+A must not open an AI menu"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn ai_off_lists_no_ai_command_in_the_palette() {
    let dir = unique_dir("ai-off-palette");
    let mut app = app_with_ai_off(&dir, &dir.join("marker"));
    for query in [
        ">",
        ">ai",
        ">summarize",
        ">explain",
        ">chat",
        ">doc comment",
    ] {
        app.on_key(ctrl('p'));
        for c in query.chars() {
            app.on_key(key(c));
        }
        let p = app.palette.as_ref().expect("palette opens");
        for e in &p.entries {
            if let vix::palette::Action::RunCommand(a) = &e.action {
                assert!(!a.starts_with("ai."), "{query}: AI command {a} listed");
            }
        }
        app.on_key(esc());
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn ai_off_refuses_every_ai_action_without_running_anything() {
    let dir = unique_dir("ai-off-run");
    let marker = dir.join("ai-ran");
    let file = dir.join("notes.txt");
    fs::write(&file, "Some text for the assistant.\n").unwrap();
    let mut app = app_with_ai_off(&dir, &marker);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.open_initial(&file);
    let tabs = app.editor.tabs.len();
    let content = app.editor.active_tab().map(|t| t.editor.get_content());

    // Every AI action the full menu bar knows, run the way a keymap binding
    // or a script would (the menu itself no longer offers them).
    let mut actions = Vec::new();
    for m in vix::menu::menus() {
        leaf_actions(m.items, &mut actions);
    }
    actions.retain(|a| a.starts_with("ai."));
    assert!(!actions.is_empty(), "the full menu bar has AI actions");
    for action in actions {
        app.run_action(action);
        assert!(!app.ai_replace_running(), "{action} started an AI request");
        assert!(app.prompt.is_none(), "{action} opened a prompt");
    }
    std::thread::sleep(Duration::from_millis(200));
    app.poll_ai_replace();
    assert!(!marker.exists(), "the AI command never ran");
    assert_eq!(app.editor.tabs.len(), tabs, "no reply tab opened");
    assert_eq!(
        app.editor.active_tab().map(|t| t.editor.get_content()),
        content,
        "the buffer is untouched"
    );
    fs::remove_dir_all(&dir).ok();
}
