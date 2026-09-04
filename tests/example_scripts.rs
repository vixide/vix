//! The sample scripts under `examples/scripts/` (improvement plan T105,
//! `docs/scripting/index.md`) are documentation, not application code —
//! nothing else in the workspace loads or runs them. This is what keeps
//! them from silently rotting: real discovery through `vix_script::
//! load_all`, real `Runtime::invoke` calls, asserting on the actual
//! `HostState` each handler produces, the same pattern `vix-script`'s own
//! tests use.

use std::path::{Path, PathBuf};

use vix_script::{HostState, InvokeOutcome, Runtime};

fn scripts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/scripts")
}

fn ran(outcome: InvokeOutcome) -> HostState {
    match outcome {
        InvokeOutcome::Ran(state) => state,
        InvokeOutcome::Error { message, .. } => {
            panic!("handler raised a runtime error: {message}")
        }
    }
}

#[test]
fn every_sample_script_loads_without_error() {
    let runtime = Runtime::new();
    let (loaded, errors) = vix_script::load_all(&runtime, None, Some(&scripts_dir()));
    assert!(
        errors.is_empty(),
        "sample scripts failed to load: {errors:?}"
    );
    let mut stems: Vec<&str> = loaded.iter().map(|s| s.stem.as_str()).collect();
    stems.sort_unstable();
    assert_eq!(
        stems,
        vec![
            "dedupe-selection",
            "insert-file-header",
            "open-scratch-with-template",
            "timestamp-signature",
            "title-case-line",
            "wrap-selection-in-markdown-link",
        ]
    );
}

fn load_one(runtime: &Runtime, stem: &str) -> vix_script::LoadedScript {
    let (loaded, errors) = vix_script::load_all(runtime, None, Some(&scripts_dir()));
    assert!(errors.is_empty(), "{stem}: {errors:?}");
    loaded
        .into_iter()
        .find(|s| s.stem == stem)
        .unwrap_or_else(|| panic!("{stem}.rhai did not load"))
}

#[test]
fn wrap_selection_in_markdown_link_wraps_the_selection() {
    let runtime = Runtime::new();
    let script = load_one(&runtime, "wrap-selection-in-markdown-link");
    assert_eq!(script.bindings.len(), 1);
    assert_eq!(script.bindings[0].key_token, "C-S-l");

    // Empty selection: refuses, via error(), not a crash.
    let state = ran(runtime.invoke(&script, "on_wrap_link", vec![], HostState::default()));
    assert!(!state.selection_text_written);
    assert_eq!(state.messages.len(), 1);

    // A real selection: prompts, then the answer handler wraps it.
    let state = HostState {
        selection_text: "Vix".to_string(),
        ..Default::default()
    };
    let state = ran(runtime.invoke(&script, "on_wrap_link", vec![], state));
    assert!(state.prompt.is_some());

    let answer_state = HostState {
        selection_text: "Vix".to_string(),
        ..Default::default()
    };
    let state = ran(runtime.invoke(
        &script,
        "on_wrap_link_url",
        vec!["https://example.com".into()],
        answer_state,
    ));
    assert_eq!(state.selection_text, "[Vix](https://example.com)");
    assert!(state.selection_text_written);
}

#[test]
fn insert_file_header_prepends_a_comment_and_moves_the_cursor() {
    let runtime = Runtime::new();
    let script = load_one(&runtime, "insert-file-header");

    let state = HostState {
        buffer_text: "fn main() {}\n".to_string(),
        ..Default::default()
    };
    let state = ran(runtime.invoke(
        &script,
        "on_insert_header_desc",
        vec!["Entry point".into()],
        state,
    ));
    assert_eq!(state.buffer_text, "// Entry point\nfn main() {}\n");
    assert!(state.buffer_text_written);
    assert_eq!(state.cursor_offset, "// Entry point\n".chars().count());
}

#[test]
fn title_case_line_only_changes_the_line_under_the_cursor() {
    let runtime = Runtime::new();
    let script = load_one(&runtime, "title-case-line");
    assert_eq!(script.bindings[0].key_token, "C-S-t");

    let buffer_text = "first line\nthe quick fox\nlast line\n".to_string();
    // Cursor somewhere inside "the quick fox", the middle line.
    let cursor_offset = buffer_text.find("quick").unwrap();
    let state = HostState {
        buffer_text,
        cursor_offset,
        ..Default::default()
    };
    let state = ran(runtime.invoke(&script, "on_title_case_line", vec![], state));
    assert_eq!(state.buffer_text, "first line\nThe Quick Fox\nlast line\n");
}

#[test]
fn dedupe_selection_keeps_first_occurrence_and_order() {
    let runtime = Runtime::new();
    let script = load_one(&runtime, "dedupe-selection");

    let state = HostState {
        selection_text: "b\na\nb\nc\na".to_string(),
        ..Default::default()
    };
    let state = ran(runtime.invoke(&script, "on_dedupe_selection", vec![], state));
    assert_eq!(state.selection_text, "b\na\nc");
}

#[test]
fn timestamp_signature_inserts_todays_date() {
    let runtime = Runtime::new();
    let script = load_one(&runtime, "timestamp-signature");
    assert_eq!(script.bindings[0].key_token, "C-S-d");

    let state = ran(runtime.invoke(
        &script,
        "on_timestamp_signature",
        vec![],
        HostState::default(),
    ));
    let today = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();
    assert_eq!(state.selection_text, format!("-- {today}"));
}

#[test]
fn open_scratch_with_template_refuses_a_non_empty_buffer() {
    let runtime = Runtime::new();
    let script = load_one(&runtime, "open-scratch-with-template");

    let dirty = HostState {
        buffer_text: "already something here".to_string(),
        ..Default::default()
    };
    let state = ran(runtime.invoke(&script, "on_scratch_template", vec![], dirty));
    assert!(!state.buffer_text_written);
    assert_eq!(state.messages.len(), 1);

    let state = ran(runtime.invoke(&script, "on_scratch_template", vec![], HostState::default()));
    let today = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();
    assert_eq!(state.buffer_text, format!("# {today}\n\n"));
    assert!(state.buffer_text_written);
}
