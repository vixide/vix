//! Behavior tests for the code-lens glue: toggling, the refresh cycle's
//! guards, server events, choosing and running lenses, and the command
//! parsers. Nothing here starts a process or a language server.

use super::*;
use crate::app::cov4::{app_at as base_app_at, buffer_with, temp};
use serde_json::json;

fn app_at(d: &Path) -> App {
    let mut app = base_app_at(d);
    app.settings_path = Some(d.join("vix-test-config.toml"));
    app
}

fn lsp_lens_at(line: usize, title: &str, command: &str) -> Lens {
    Lens {
        line,
        title: title.to_string(),
        action: LensAction::Lsp {
            command: command.to_string(),
            arguments: json!([]),
        },
    }
}

fn rust_file(tag: &str) -> (App, PathBuf) {
    let d = temp(tag);
    let file = d.join("lib.rs");
    std::fs::write(
        &file,
        "fn helper() {}\n\n#[test]\nfn it_works() {}\n\n#[test]\nfn also() {}\n",
    )
    .unwrap();
    let mut app = app_at(&d);
    app.open_initial(&file);
    let path = app.editor.active_tab().unwrap().path.clone().unwrap();
    (app, path)
}

#[test]
fn toggling_lenses_reports_and_clears_them_from_open_tabs() {
    let (mut app, path) = rust_file("cl-toggle");
    assert!(app.run_code_lens_action("view.code_lens"));
    let on = app.settings.language_features.code_lens;
    assert_eq!(app.status, t!("status.code_lens", on = on).to_string());
    if !on {
        app.run_code_lens_action("view.code_lens");
    }
    assert!(app.settings.language_features.code_lens);
    app.code_lenses.entry(path).or_default();
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_code_lenses(vec![(0, "stale".to_string())]);
    app.run_code_lens_action("view.code_lens");
    assert!(!app.settings.language_features.code_lens);
    assert!(!app.editor.active_tab().unwrap().editor.has_code_lenses());
    assert!(!app.run_code_lens_action("lsp.something_else"));
}

#[test]
fn polling_adds_local_test_lenses_and_stays_quiet_when_switched_off() {
    let (mut app, path) = rust_file("cl-poll");
    app.settings.language_features.code_lens = false;
    app.poll_code_lens();
    assert!(app.code_lenses.is_empty(), "off: nothing is tracked");
    app.settings.language_features.code_lens = true;
    app.poll_code_lens();
    let title = t!("ui.code_lens_run_test").to_string();
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.editor.code_lens_at(3), Some(title.as_str()));
    assert_eq!(tab.editor.code_lens_at(6), Some(title.as_str()));
    assert_eq!(tab.editor.code_lens_at(0), None);
    // No language server handles the file, so nothing was left to retry.
    assert_eq!(app.code_lenses[&path].retries, 0);
    // A file with no test syntax is not scanned for local lenses.
    let d = temp("cl-poll-txt");
    let notes = d.join("notes.txt");
    std::fs::write(&notes, "#[test]\n").unwrap();
    app.open_path(&notes, false);
    app.poll_code_lens();
    assert!(!app.editor.active_tab().unwrap().editor.has_code_lenses());
    // An untitled buffer has no path to key lenses by.
    app.editor.new_tab();
    app.poll_code_lens();
    // Saving makes the next poll refresh at once.
    app.code_lens_saved(&path);
    app.code_lens_saved(Path::new("/not/tracked.rs"));
    app.editor.tabs.clear();
    app.poll_code_lens();
}

#[test]
fn server_events_replace_resolve_and_refresh_lenses() {
    let (mut app, path) = rust_file("cl-events");
    app.settings.language_features.code_lens = true;
    let range = json!({"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 4}});
    app.on_code_lens_event(crate::lsp::LspEvent::CodeLensUpdate {
        path: path.clone(),
        lenses: vec![(
            0,
            "$(play) Run".to_string(),
            "cmd.run".to_string(),
            json!([1]),
        )],
        unresolved: vec![(1, json!({"range": range}))],
    });
    let st = &app.code_lenses[&path];
    assert_eq!(st.unresolved.len(), 1);
    assert_eq!(st.shown().len(), 1);
    assert_eq!(st.shown()[0].title, "Run", "icon markup is dropped");
    assert_eq!(
        app.editor.active_tab().unwrap().editor.code_lens_at(0),
        Some("Run")
    );
    // A resolve for a lens we asked about adds it; a stranger's is ignored.
    app.on_code_lens_event(crate::lsp::LspEvent::CodeLensResolved {
        path: path.clone(),
        lens: (
            1,
            "2 references".to_string(),
            "cmd.refs".to_string(),
            json!([]),
        ),
        range: range.clone(),
    });
    assert_eq!(app.code_lenses[&path].shown().len(), 2);
    assert_eq!(
        app.editor.active_tab().unwrap().editor.code_lens_at(1),
        Some("2 references")
    );
    app.on_code_lens_event(crate::lsp::LspEvent::CodeLensResolved {
        path: path.clone(),
        lens: (2, "stray".to_string(), "c".to_string(), json!([])),
        range: json!({"start": {"line": 9}}),
    });
    assert_eq!(app.code_lenses[&path].shown().len(), 2);
    app.on_code_lens_event(crate::lsp::LspEvent::CodeLensResolved {
        path: PathBuf::from("/unknown.rs"),
        lens: (0, "x".to_string(), "c".to_string(), json!([])),
        range,
    });
    assert!(!app.code_lenses.contains_key(Path::new("/unknown.rs")));
    // A server-side refresh makes every tracked buffer due.
    app.on_code_lens_event(crate::lsp::LspEvent::CodeLensRefresh);
    // Any other event is not this module's business.
    app.on_code_lens_event(crate::lsp::LspEvent::Hover("text".to_string()));
    // Updates apply nothing to the tab when lenses are switched off.
    app.settings.language_features.code_lens = false;
    app.on_code_lens_event(crate::lsp::LspEvent::CodeLensUpdate {
        path,
        lenses: vec![(0, "Run".to_string(), "c".to_string(), json!([]))],
        unresolved: Vec::new(),
    });
    assert!(!app.editor.active_tab().unwrap().editor.has_code_lenses());
}

#[test]
fn choosing_a_lens_lists_runs_or_explains() {
    let (mut app, path) = rust_file("cl-choose");
    app.settings.language_features.code_lens = true;
    // Nothing known and no server: say so, for both entry points.
    app.run_code_lens_action("lsp.code_lens");
    assert_eq!(app.status, t!("status.code_lens_none").to_string());
    app.run_code_lens_action("lsp.code_lens_run");
    assert_eq!(app.status, t!("status.code_lens_none_here").to_string());

    let st = app.code_lenses.entry(path).or_default();
    st.lsp = vec![
        lsp_lens_at(0, "First", "mystery.command"),
        lsp_lens_at(3, "Second", "other.command"),
        lsp_lens_at(3, "Third", "third.command"),
    ];
    // The list opens on the first lens at or after the cursor.
    app.run_code_lens_action("lsp.code_lens");
    let list = app.choice_list.take().unwrap();
    assert_eq!(list.panel.rows.len(), 3);
    assert!(list.panel.rows[0].contains("First"));
    assert_eq!(list.panel.selected, 0);
    // On a line with one lens, run it: an unknown command is refused.
    app.run_code_lens_action("lsp.code_lens_run");
    assert_eq!(
        app.status,
        t!("status.code_lens_unsupported", command = "mystery.command").to_string()
    );
    // Two lenses on the cursor's line open the list instead.
    app.editor.goto(4, None, app.layout.editor);
    app.run_code_lens_action("lsp.code_lens_run");
    assert_eq!(app.choice_list.take().unwrap().panel.rows.len(), 2);
}

#[test]
fn lenses_pushed_by_the_server_open_sorted_near_the_cursor() {
    let (mut app, _path) = rust_file("cl-push");
    app.on_code_lenses(Vec::new());
    assert_eq!(app.status, t!("status.code_lens_none").to_string());
    buffer_with(&mut app, "a\nb\nc\nd\ne\nf\n", 0);
    app.editor.goto(4, None, app.layout.editor);
    app.on_code_lenses(vec![
        (5, "later".to_string(), "c".to_string(), json!([])),
        (0, "first".to_string(), "c".to_string(), json!([])),
        (3, "mid".to_string(), "c".to_string(), json!([])),
    ]);
    let list = app.choice_list.take().unwrap();
    assert!(list.panel.rows[0].contains("first"));
    assert_eq!(
        list.panel.selected, 1,
        "the first lens at or below the cursor line"
    );
    // A cursor past every lens selects the last one.
    app.editor.goto(6, None, app.layout.editor);
    app.on_code_lenses(vec![(0, "only".to_string(), "c".to_string(), json!([]))]);
    assert_eq!(app.choice_list.take().unwrap().panel.selected, 0);
}

#[test]
fn lens_commands_open_references_and_refuse_what_nothing_provides() {
    let (mut app, _path) = rust_file("cl-run");
    let args = json!(["file:///a.rs", {"line": 0, "character": 0}, [
        {"uri": "file:///b.rs", "range": {"start": {"line": 4, "character": 2},
                                          "end": {"line": 4, "character": 5}}}
    ]]);
    app.run_lens(&Lens {
        line: 0,
        title: "1 reference".to_string(),
        action: LensAction::Lsp {
            command: "rust-analyzer.showReferences".to_string(),
            arguments: args,
        },
    });
    assert!(
        app.workspace_search.is_some(),
        "the references list opens: {}",
        app.status
    );
    // A runnable with no usable arguments falls through to "unsupported".
    app.workspace_search = None;
    app.run_lens(&Lens {
        line: 0,
        title: "Run".to_string(),
        action: LensAction::Lsp {
            command: "rust-analyzer.runSingle".to_string(),
            arguments: json!([]),
        },
    });
    assert_eq!(
        app.status,
        t!(
            "status.code_lens_unsupported",
            command = "rust-analyzer.runSingle"
        )
        .to_string()
    );
    // References with no locations are refused too.
    app.run_lens(&Lens {
        line: 0,
        title: "Refs".to_string(),
        action: LensAction::Lsp {
            command: "editor.action.showReferences".to_string(),
            arguments: json!(["u", {}, []]),
        },
    });
    assert_eq!(
        app.status,
        t!(
            "status.code_lens_unsupported",
            command = "editor.action.showReferences"
        )
        .to_string()
    );
}

#[test]
fn runnables_choose_their_program_directory_and_arguments() {
    let r = json!({
        "kind": "cargo",
        "args": {
            "cwd": "/proj",
            "overrideCargo": "my cargo",
            "cargoArgs": ["run"],
            "cargoExtraArgs": ["--release"],
            "executableArgs": []
        }
    });
    let (cmd, dir) = runnable_command(&r).unwrap();
    assert_eq!(cmd, "'my cargo' run --release");
    assert_eq!(dir, Some(PathBuf::from("/proj")));
    // A shell runnable needs its program.
    assert_eq!(
        runnable_command(&json!({"kind": "shell", "args": {}})),
        None
    );
    let (cmd, dir) =
        runnable_command(&json!({"kind": "shell", "args": {"program": "make", "args": ["-j"]}}))
            .unwrap();
    assert_eq!(cmd, "make -j");
    assert_eq!(dir, None);
    // Malformed reference entries are skipped, good ones kept.
    let args = json!(["u", {}, [
        {"uri": "file:///ok.rs", "range": {"start": {"line": 1, "character": 2}}},
        {"uri": "file:///bad.rs", "range": {"start": {"line": -1, "character": 2}}},
        {"range": {}},
        "junk"
    ]]);
    assert_eq!(
        reference_locations(&args),
        vec![(PathBuf::from("/ok.rs"), 1, 2)]
    );
    assert_eq!(
        reference_locations(&json!([])),
        Vec::<(PathBuf, u32, u32)>::new()
    );
}
