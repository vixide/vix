//! T125's new AI features: "Edit selection with instruction" and "Generate
//! doc comment" (the Git panel's AI commit-message generator has its own
//! throwaway-repo test in `git.rs`, alongside its siblings). These drive the
//! CLI spawn path (`ai_command`, made deterministic by reading back a file)
//! end to end -- the first real test coverage `spawn_ai_cmd`/`AiReplace`/
//! `poll_ai_replace` have had at all (Annotate/Improve/Summarize/Explain/
//! Define had none before this).

use crate::common::*;

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
    let settings = Settings {
        ai_command: cmd,
        misc: MiscSettings {
            ai_diff_review,
            ..MiscSettings::default()
        },
        ..Settings::default()
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
    let settings = Settings {
        ai_command: "echo {prompt}".to_string(),
        misc: MiscSettings {
            ai_diff_review: false,
            ..MiscSettings::default()
        },
        ..Settings::default()
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
