//! Rhai-based user scripting core (improvement plan T102) — see
//! `spec/index.md` for the full design this implements.
//!
//! This crate is host-agnostic: it never touches a real `App`, a real
//! editor buffer, or the filesystem paths a config lives under. It exposes
//! [`discover`] (find `.rhai` files under caller-supplied directories) and
//! [`Runtime`] (compile a script, run its handlers against a caller-supplied
//! [`HostState`] snapshot). Loading scripts at startup from the real
//! `Settings::scripts_dir()`/`<App::root>/.vix/scripts`, surfacing commands
//! in the palette and the Tools → Scripts menu, and applying a handler's
//! effects back to the real editor are host wiring (`src/app.rs`, built on
//! top of this crate, not inside it) — done as of tasks.md T103. Wiring a
//! script's `bind_key` requests into the real keymap (`crates/
//! vix-keybindings`, tasks.md T104a–j) is done too, as of T104j.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

mod discovery;
mod engine;

pub use discovery::{DiscoveredScript, discover};
pub use engine::{
    Command, HostMessage, HostState, InvokeOutcome, KeyBinding, LoadError, LoadedScript,
    PromptRequest, Runtime,
};
/// A dynamically-typed Rhai value — re-exported so a host passing arguments
/// to [`Runtime::invoke`] (e.g. a `prompt` answer) never needs its own direct
/// dependency on `rhai` just for this one type.
pub use rhai::Dynamic;

/// Discover `.rhai` scripts under `global_dir` and `project_dir` (§ Script
/// discovery — both optional, both non-recursive, project shadows global by
/// stem) and load each through `runtime`. A script that fails to read from
/// disk or fails to load (§ Error handling, "at load") is skipped — its
/// [`LoadError`] is returned alongside every script that *did* load, rather
/// than aborting the whole batch.
#[must_use]
pub fn load_all(
    runtime: &Runtime,
    global_dir: Option<&std::path::Path>,
    project_dir: Option<&std::path::Path>,
) -> (Vec<LoadedScript>, Vec<LoadError>) {
    let mut loaded = Vec::new();
    let mut errors = Vec::new();
    for found in discover(global_dir, project_dir) {
        match std::fs::read_to_string(&found.path) {
            Ok(source) => match runtime.load(&found.stem, &source) {
                Ok(script) => loaded.push(script),
                Err(e) => errors.push(e),
            },
            Err(e) => errors.push(LoadError {
                stem: found.stem,
                message: e.to_string(),
            }),
        }
    }
    (loaded, errors)
}

#[cfg(test)]
mod tests {
    use rhai::Dynamic;

    use super::*;

    /// `register_command` + a handler that reads and rewrites the buffer,
    /// exercising the whole snapshot-in/effects-out round trip (§ API v1,
    /// "Buffer & selection").
    #[test]
    fn handler_reads_and_rewrites_the_buffer() {
        let runtime = Runtime::new();
        let script = runtime
            .load(
                "uppercase",
                r#"
                register_command("uppercase_selection", "Uppercase Selection", "on_uppercase");
                fn on_uppercase() {
                    set_selection_text(selection_text().to_upper());
                }
                "#,
            )
            .unwrap();
        assert_eq!(script.commands.len(), 1);
        assert_eq!(script.commands[0].id, "uppercase_selection");
        assert_eq!(script.commands[0].label, "Uppercase Selection");

        let state = HostState {
            selection_text: "hello".to_string(),
            ..Default::default()
        };
        let outcome = runtime.invoke(&script, "on_uppercase", vec![], state);
        let InvokeOutcome::Ran(state) = outcome else {
            panic!("expected Ran")
        };
        assert!(state.selection_text_written);
        assert_eq!(state.selection_text, "HELLO");
        // A handler that never touches the buffer shouldn't claim it did.
        assert!(!state.buffer_text_written);
    }

    /// `set_cursor_offset` clamps to the buffer's character length, per
    /// § API v1's "clamped in range".
    #[test]
    fn set_cursor_offset_clamps_to_buffer_length() {
        let runtime = Runtime::new();
        let script = runtime
            .load("clamp", r"fn run(n) { set_cursor_offset(n); }")
            .unwrap();

        let state = HostState {
            buffer_text: "hello".to_string(),
            ..Default::default()
        };
        let outcome = runtime.invoke(&script, "run", vec![Dynamic::from(999_i64)], state.clone());
        let InvokeOutcome::Ran(state) = outcome else {
            panic!("expected Ran")
        };
        assert_eq!(state.cursor_offset, 5); // "hello".chars().count()

        let outcome = runtime.invoke(&script, "run", vec![Dynamic::from(-5_i64)], state);
        let InvokeOutcome::Ran(state) = outcome else {
            panic!("expected Ran")
        };
        assert_eq!(state.cursor_offset, 0);
    }

    /// `prompt` records a request rather than blocking; answering it is a
    /// fresh, separate `invoke` of the named `on_submit` function
    /// (§ "Prompting for input").
    #[test]
    fn prompt_then_answer_is_two_separate_invokes() {
        let runtime = Runtime::new();
        let script = runtime
            .load(
                "rename",
                r#"
                register_command("rename_word", "Rename Word Under Cursor", "on_rename");
                fn on_rename() { prompt("New name:", "on_rename_answer"); }
                fn on_rename_answer(answer) { set_selection_text(answer); }
                "#,
            )
            .unwrap();

        let outcome = runtime.invoke(&script, "on_rename", vec![], HostState::default());
        let InvokeOutcome::Ran(state) = outcome else {
            panic!("expected Ran")
        };
        let prompt = state
            .prompt
            .expect("on_rename should have requested a prompt");
        assert_eq!(prompt.message, "New name:");
        assert_eq!(prompt.on_submit, "on_rename_answer");
        assert!(!state.selection_text_written); // on_rename itself changed nothing

        let outcome = runtime.invoke(
            &script,
            &prompt.on_submit,
            vec![Dynamic::from("new_name".to_string())],
            HostState::default(),
        );
        let InvokeOutcome::Ran(state) = outcome else {
            panic!("expected Ran")
        };
        assert_eq!(state.selection_text, "new_name");
    }

    /// `message`/`error` both land in `HostState::messages`, distinguished
    /// by variant, and neither aborts the script (§ API v1, "Messages").
    #[test]
    fn message_and_error_both_record_without_aborting() {
        let runtime = Runtime::new();
        let script = runtime
            .load(
                "notify",
                r#"fn run() { message("hello"); error("uh oh"); set_buffer_text("done"); }"#,
            )
            .unwrap();
        let outcome = runtime.invoke(&script, "run", vec![], HostState::default());
        let InvokeOutcome::Ran(state) = outcome else {
            panic!("expected Ran")
        };
        assert_eq!(
            state.messages,
            vec![
                HostMessage::Info("hello".to_string()),
                HostMessage::Error("uh oh".to_string())
            ]
        );
        // Raising an error is still just a message — the rest of the handler ran.
        assert!(state.buffer_text_written);
        assert_eq!(state.buffer_text, "done");
    }

    /// `now()` (§ API v1, "Clock", added T105) returns the local date as
    /// `YYYY-MM-DD` — checked against the same `jiff` call the host makes,
    /// not a hardcoded literal, so this doesn't rot on the next run.
    #[test]
    fn now_returns_todays_local_date() {
        let runtime = Runtime::new();
        let script = runtime
            .load("clock", r"fn run() { set_buffer_text(now()); }")
            .unwrap();
        let outcome = runtime.invoke(&script, "run", vec![], HostState::default());
        let InvokeOutcome::Ran(state) = outcome else {
            panic!("expected Ran")
        };
        assert_eq!(
            state.buffer_text,
            jiff::Zoned::now().strftime("%Y-%m-%d").to_string()
        );
    }

    /// A runtime error aborts just that call and is never a Rust panic; any
    /// effects made before the error still apply (§ Error handling, "at
    /// invocation" — explicitly not transactional).
    #[test]
    fn runtime_error_is_not_transactional() {
        let runtime = Runtime::new();
        let script = runtime
            .load(
                "half_done",
                r#"
                fn run() {
                    set_buffer_text("partial");
                    this_function_does_not_exist();
                }
                "#,
            )
            .unwrap();
        let outcome = runtime.invoke(&script, "run", vec![], HostState::default());
        let InvokeOutcome::Error { message, state } = outcome else {
            panic!("expected Error")
        };
        assert!(!message.is_empty());
        assert!(state.buffer_text_written);
        assert_eq!(state.buffer_text, "partial");
    }

    /// An operation-count cap catches a runaway script deterministically —
    /// no wall-clock timer, no second thread (§ Execution model).
    #[test]
    fn infinite_loop_is_caught_by_the_operation_cap() {
        let runtime = Runtime::new();
        let script = runtime.load("loopy", r"fn run() { loop {} }").unwrap();
        let outcome = runtime.invoke(&script, "run", vec![], HostState::default());
        assert!(matches!(outcome, InvokeOutcome::Error { .. }));
    }

    /// A malformed `bind_key` token is a load error (rejected up front,
    /// § API v1 "Key bindings" — reusing `vix-macros`' grammar), not a
    /// binding that silently never fires.
    #[test]
    fn bind_key_rejects_an_invalid_token() {
        let runtime = Runtime::new();
        let err = runtime
            .load(
                "bad_bind",
                r#"
                register_command("x", "X", "on_x");
                bind_key("not a real key token!!", "x");
                fn on_x() {}
                "#,
            )
            .unwrap_err();
        assert_eq!(err.stem, "bad_bind");
        assert!(err.message.contains("not a real key token"));
    }

    /// A valid `bind_key` is recorded verbatim.
    #[test]
    fn bind_key_records_a_valid_token() {
        let runtime = Runtime::new();
        let script = runtime
            .load(
                "good_bind",
                r#"
                register_command("x", "X", "on_x");
                bind_key("C-c", "x");
                fn on_x() {}
                "#,
            )
            .unwrap();
        assert_eq!(
            script.bindings,
            vec![KeyBinding {
                key_token: "C-c".to_string(),
                command_id: "x".to_string()
            }]
        );
    }

    /// A script that fails to parse is a load error naming its own stem —
    /// callers loading many scripts (see [`load_all`]) skip just this one.
    #[test]
    fn parse_error_names_the_failing_script() {
        let runtime = Runtime::new();
        let err = runtime
            .load("broken", "this is not valid rhai (((")
            .unwrap_err();
        assert_eq!(err.stem, "broken");
        assert!(!err.message.is_empty());
    }

    /// `load_all` isolates a broken script from the rest of the batch — one
    /// bad `.rhai` file does not take scripting down (§ Error handling).
    #[test]
    fn load_all_skips_a_broken_script_and_loads_the_rest() {
        let dir = std::env::temp_dir().join(format!("vix-script-load-all-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("good.rhai"),
            r#"register_command("ok", "OK", "on_ok"); fn on_ok() {}"#,
        )
        .unwrap();
        std::fs::write(dir.join("broken.rhai"), "this is not valid rhai (((").unwrap();

        let runtime = Runtime::new();
        let (loaded, errors) = load_all(&runtime, Some(&dir), None);
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].stem, "good");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].stem, "broken");
    }
}
