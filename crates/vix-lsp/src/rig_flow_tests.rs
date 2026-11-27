//! Server lifecycle and the capability-gated features, against a scripted
//! server (see [`crate::rig_tests`]): `initialize` negotiation, pull
//! diagnostics, semantic tokens, lenses and document extras, file
//! operations and watchers, server-initiated requests, crashes and restarts.

use std::path::PathBuf;
use std::time::Instant;

use serde_json::{Value, json};
use vix_lsp_core::file_ops::{FileChangeType, FileOp, FileOpItem};
use vix_lsp_core::{Encoding, Severity};

use crate::rig_tests::{LANG, Rig, edit_a, file, rng};
use crate::{LensSupport, LspEvent, ServerError, ServerState, path_to_uri};

fn uri() -> String {
    path_to_uri(&file())
}

fn diag(message: &str) -> Value {
    json!({"range": rng(0, 0, 0, 2), "severity": 1, "message": message})
}

/// A rig that opened `a.rs` while starting and finished `initialize` with `capabilities`.
fn started(capabilities: &Value) -> Rig {
    let mut rig = Rig::new(false);
    rig.lsp.did_open(&file(), "fn main() {}\n");
    rig.initialize(json!({ "capabilities": capabilities }));
    rig
}

// ----- initialize -----------------------------------------------------------

fn rich_capabilities() -> Value {
    json!({
        "positionEncoding": "utf-8",
        "completionProvider": {"triggerCharacters": [".", "::"]},
        "documentOnTypeFormattingProvider": {"firstTriggerCharacter": "}"},
        "codeLensProvider": {"resolveProvider": true},
        "executeCommandProvider": {"commands": ["do.thing"]},
        "colorProvider": true,
        "documentLinkProvider": {},
        "inlineCompletionProvider": true,
        "semanticTokensProvider": {
            "legend": {"tokenTypes": ["keyword", "variable"], "tokenModifiers": []},
            "full": {"delta": true}, "range": true},
        "workspace": {"workspaceFolders": {"changeNotifications": true}},
    })
}

#[test]
fn initialize_records_what_the_server_offers() {
    let mut rig = started(&rich_capabilities());
    assert_eq!(rig.methods(), ["initialized", "textDocument/didOpen"]);
    assert_eq!(rig.lsp.encoding_for(&file()), Encoding::Utf8);
    assert!(rig.lsp.is_completion_trigger(&file(), '.'));
    assert!(
        !rig.lsp.is_completion_trigger(&file(), ':'),
        "only one-char triggers match"
    );
    assert!(rig.lsp.is_on_type_trigger(&file(), '}'));
    assert!(rig.lsp.executes_command(&file(), "do.thing"));
    assert!(!rig.lsp.executes_command(&file(), "other"));
    assert!(rig.lsp.resolves_code_lens(&file()));
    assert!(rig.lsp.supports_inline_completion(&file()));
    assert_eq!(rig.lsp.semantic_tokens_ranged(&file()), Some(true));
    let status = &rig.lsp.statuses()[0];
    assert_eq!(status.state, ServerState::Running);
    assert_eq!(status.open_documents, 1);
    for cap in [
        "color",
        "documentLink",
        "inlineCompletion",
        "semanticTokens",
    ] {
        assert!(status.capabilities.iter().any(|c| c == cap), "{cap}");
    }
}

#[test]
fn an_initialize_error_leaves_defaults_but_still_readies_the_server() {
    let mut rig = Rig::new(false);
    rig.lsp.did_open(&file(), "x");
    rig.deliver(json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -1, "message": "no"}}));
    assert_eq!(rig.methods(), ["initialized", "textDocument/didOpen"]);
    assert_eq!(rig.lsp.encoding_for(&file()), Encoding::Utf16);
    assert_eq!(rig.lsp.semantic_tokens_ranged(&file()), None);
    // With no `initialize` for a vanished server, the reply is simply dropped.
    let mut events = Vec::new();
    let mut gone = rig.lsp.servers.remove(LANG).expect("server");
    let _ = gone.child.kill();
    let _ = gone.child.wait();
    rig.lsp.finish_initialize(LANG, &json!({"result": {}}));
    rig.lsp.handle(
        LANG,
        &json!({"method": "$/progress", "params": {}}),
        &mut events,
    );
    assert!(events.is_empty());
}

#[test]
fn workspace_folders_are_announced_only_to_servers_that_asked() {
    let mut rig = started(&rich_capabilities());
    rig.sent();
    let extra = PathBuf::from("/other/project");
    rig.lsp.add_workspace_folder(&extra);
    let sent = rig.sent();
    assert_eq!(sent[0]["method"], "workspace/didChangeWorkspaceFolders");
    assert_eq!(sent[0]["params"]["event"]["added"][0]["name"], "project");
    rig.lsp.add_workspace_folder(&extra);
    rig.quiet("already a folder");

    let mut plain = started(&json!({}));
    plain.sent();
    plain.lsp.add_workspace_folder(&extra);
    plain.quiet("this server never asked");
}

// ----- pull diagnostics (T619) ---------------------------------------------------

#[test]
fn opening_pulls_diagnostics_and_merges_the_report() {
    let mut rig = started(&json!({"diagnosticProvider": {}}));
    assert_eq!(
        rig.methods(),
        [
            "initialized",
            "textDocument/didOpen",
            "textDocument/diagnostic"
        ]
    );
    let id = *rig
        .server()
        .pending
        .keys()
        .next()
        .expect("a pull in flight");
    let report = json!({"kind": "full", "resultId": "r1", "items": [diag("bad")],
        "relatedDocuments": {"file:///proj/b.rs": {"kind": "full", "items": [diag("worse")]}}});
    let events = rig.answer(id, report);
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| matches!(e, LspEvent::Diagnostics(_))));
    assert_eq!(rig.lsp.diagnostics_for(&file()).len(), 1);
    assert_eq!(
        rig.lsp.diagnostics_for(&file())[0].severity,
        Severity::Error
    );
    assert_eq!(rig.lsp.diagnostic_count(), 2);
    assert_eq!(rig.lsp.all_diagnostics().count(), 2);
    assert_eq!(rig.server().diagnostic_result_ids[&uri()], "r1");
}

#[test]
fn a_changed_document_is_repulled_after_a_debounce_with_the_previous_result_id() {
    let mut rig = started(&json!({"diagnosticProvider": {}}));
    rig.sent();
    let id = *rig.server().pending.keys().next().expect("pull");
    rig.answer(id, json!({"kind": "full", "resultId": "r1", "items": []}));
    rig.lsp.did_change(&file(), "fn main() { 2 }\n");
    assert_eq!(rig.methods(), ["textDocument/didChange"]);
    assert!(rig.lsp.busy(), "a pull is scheduled");
    assert!(rig.lsp.poll().is_empty());
    rig.quiet("not due yet");

    // Make the debounce elapse deterministically instead of sleeping.
    for due in rig.lsp.pull_due.values_mut() {
        *due = Instant::now();
    }
    rig.lsp.poll();
    let (id, msg) = rig.one_request("textDocument/diagnostic");
    assert_eq!(msg["params"]["previousResultId"], "r1");
    assert!(rig.lsp.pull_due.is_empty());
    rig.answer(id, json!({"kind": "unchanged", "resultId": "r2"}));
    assert_eq!(rig.server().diagnostic_result_ids[&uri()], "r2");
}

#[test]
fn a_pull_reply_for_a_moved_on_document_is_dropped() {
    let mut rig = started(&json!({"diagnosticProvider": {}}));
    rig.sent();
    let id = *rig.server().pending.keys().next().expect("pull");
    rig.lsp.did_change(&file(), "newer text");
    let events = rig.answer(id, json!({"kind": "full", "items": [diag("stale")]}));
    assert!(events.is_empty());
    assert!(rig.lsp.diagnostics_for(&file()).is_empty());
}

#[test]
fn a_full_report_without_a_result_id_forgets_the_old_one_and_junk_is_ignored() {
    let mut rig = started(&json!({"diagnosticProvider": {}}));
    let id = *rig.server().pending.keys().next().expect("pull");
    rig.answer(id, json!({"kind": "full", "resultId": "r1", "items": []}));
    assert!(rig.server().diagnostic_result_ids.contains_key(&uri()));
    rig.lsp.pull_diagnostics(LANG, &uri());
    let id = *rig.server().pending.keys().next().expect("pull");
    rig.answer(id, json!({"kind": "full", "items": []}));
    assert!(!rig.server().diagnostic_result_ids.contains_key(&uri()));
    rig.lsp.pull_diagnostics(LANG, &uri());
    let id = *rig.server().pending.keys().next().expect("pull");
    assert!(rig.answer(id, json!({"kind": "mystery"})).is_empty());
    // Pulling for a document that is not open sends nothing.
    rig.sent();
    rig.lsp.pull_diagnostics(LANG, "file:///proj/never.rs");
    rig.lsp.pull_diagnostics("nobody", &uri());
    rig.quiet("");
}

#[test]
fn the_servers_refresh_request_is_acknowledged_and_repulls_every_open_document() {
    let mut rig = started(&json!({"diagnosticProvider": {}}));
    rig.sent();
    rig.lsp.did_open(&PathBuf::from("/proj/b.rs"), "b");
    rig.sent();
    rig.deliver(json!({"jsonrpc": "2.0", "id": 77, "method": "workspace/diagnostic/refresh"}));
    let sent = rig.sent();
    assert_eq!(sent[0], json!({"jsonrpc": "2.0", "id": 77, "result": null}));
    let pulls: Vec<&Value> = sent
        .iter()
        .filter(|m| m["method"] == "textDocument/diagnostic")
        .collect();
    assert_eq!(pulls.len(), 2);
    assert!(
        pulls[0]["params"]["textDocument"]["uri"].as_str()
            < pulls[1]["params"]["textDocument"]["uri"].as_str()
    );
}

#[test]
fn closing_cancels_a_scheduled_pull() {
    let mut rig = started(&json!({"diagnosticProvider": {}}));
    rig.lsp.did_change(&file(), "changed");
    assert!(!rig.lsp.pull_due.is_empty());
    rig.lsp.did_close(&file());
    assert!(rig.lsp.pull_due.is_empty());
}

#[test]
fn workspace_diagnostics_are_merged_per_file() {
    let mut rig = Rig::open();
    rig.lsp.request_workspace_diagnostics();
    let (id, _) = rig.one_request("workspace/diagnostic");
    let result = json!({"items": [
        {"uri": "file:///proj/x.rs", "kind": "full", "items": [diag("one"), diag("two")]},
        {"uri": "file:///proj/y.rs", "kind": "unchanged", "items": []},
    ]});
    let events = rig.answer(id, result);
    assert!(
        matches!(events.as_slice(), [LspEvent::Diagnostics(p)] if p == &PathBuf::from("/proj/x.rs"))
    );
    assert_eq!(
        rig.lsp.diagnostics_for(&PathBuf::from("/proj/x.rs")).len(),
        2
    );
    assert_eq!(rig.lsp.diagnostic_count(), 2);
}

// ----- published diagnostics, progress, server-initiated requests ------------------

#[test]
fn published_diagnostics_replace_and_clear_per_server() {
    let mut rig = Rig::open();
    let publish = |ds: Vec<Value>| {
        json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
               "params": {"uri": uri(), "diagnostics": ds}})
    };
    let events = rig.deliver(publish(vec![diag("a"), diag("b")]));
    assert!(matches!(events.as_slice(), [LspEvent::Diagnostics(p)] if p == &file()));
    assert_eq!(rig.lsp.diagnostics_for(&file()).len(), 2);
    rig.deliver(publish(vec![diag("only")]));
    assert_eq!(rig.lsp.diagnostics_for(&file()).len(), 1);
    rig.deliver(publish(vec![]));
    assert!(rig.lsp.diagnostics_for(&file()).is_empty());
    assert_eq!(rig.lsp.diagnostic_count(), 0);
}

#[test]
fn progress_notifications_become_status_text() {
    let mut rig = Rig::open();
    let progress = |value: Value| json!({"jsonrpc": "2.0", "method": "$/progress", "params": {"token": 5, "value": value}});
    let events = rig.deliver(progress(
        json!({"kind": "begin", "title": "Indexing", "percentage": 10}),
    ));
    assert!(matches!(events.as_slice(), [LspEvent::Progress(t)] if t == "Indexing (10%)"));
    assert_eq!(
        rig.lsp.progress_line().as_deref(),
        Some("rs: Indexing (10%)")
    );
    let events = rig.deliver(progress(
        json!({"kind": "report", "message": "half", "percentage": 50}),
    ));
    assert!(matches!(events.as_slice(), [LspEvent::Progress(t)] if t == "half (50%)"));
    assert_eq!(
        rig.lsp.progress_line().as_deref(),
        Some("rs: Indexing: half (50%)")
    );
    assert!(rig.deliver(progress(json!({"kind": "end"}))).is_empty());
    assert_eq!(rig.lsp.progress_line(), None);
    // A report for a token never begun starts its own task; a percentage-only
    // task is shown by its percentage alone.
    rig.deliver(progress(json!({"kind": "report", "percentage": 7})));
    assert_eq!(rig.lsp.progress_line().as_deref(), Some("rs: 7%"));
}

#[test]
fn server_requests_are_answered_so_the_server_never_stalls() {
    let mut rig = Rig::open();
    rig.deliver(json!({"jsonrpc": "2.0", "id": "a", "method": "workspace/codeLens/refresh"}));
    // `CodeLensRefresh` is an event too.
    let events =
        rig.deliver(json!({"jsonrpc": "2.0", "id": 3, "method": "workspace/codeLens/refresh"}));
    assert!(matches!(events.as_slice(), [LspEvent::CodeLensRefresh]));
    rig.deliver(json!({"jsonrpc": "2.0", "id": 4, "method": "window/workDoneProgress/create"}));
    rig.deliver(
        json!({"jsonrpc": "2.0", "id": 5, "method": "workspace/configuration",
                       "params": {"items": [{"section": "a"}, {"section": "b"}]}}),
    );
    rig.deliver(json!({"jsonrpc": "2.0", "id": 6, "method": "workspace/configuration"}));
    let sent = rig.sent();
    let result_of = |id: Value| {
        sent.iter()
            .find(|m| m["id"] == id)
            .map(|m| m["result"].clone())
            .expect("an answer")
    };
    assert_eq!(result_of(json!("a")), Value::Null);
    assert_eq!(result_of(json!(3)), Value::Null);
    assert_eq!(result_of(json!(4)), Value::Null);
    assert_eq!(result_of(json!(5)), json!([null, null]));
    assert_eq!(result_of(json!(6)), json!([]));
}

#[test]
fn apply_edit_is_surfaced_and_acknowledged_as_applied() {
    let mut rig = Rig::open();
    let events = rig.deliver(
        json!({"jsonrpc": "2.0", "id": 8, "method": "workspace/applyEdit",
                                    "params": {"edit": edit_a("zz")}}),
    );
    assert!(matches!(events.as_slice(), [LspEvent::WorkspaceEdit(f)] if f[0].0 == file()));
    assert_eq!(
        rig.sent()[0],
        json!({"jsonrpc": "2.0", "id": 8, "result": {"applied": true}})
    );
    // An empty edit is acknowledged without an event.
    let events = rig.deliver(
        json!({"jsonrpc": "2.0", "id": 9, "method": "workspace/applyEdit",
                                    "params": {"edit": {}}}),
    );
    assert!(events.is_empty());
    assert_eq!(rig.sent()[0]["result"], json!({"applied": true}));
    rig.deliver(json!({"jsonrpc": "2.0", "id": 10, "method": "workspace/applyEdit"}));
    assert_eq!(rig.sent()[0]["id"], 10);
}

// ----- semantic tokens (T123a, T615) ------------------------------------------------

fn token_caps() -> Value {
    json!({"semanticTokensProvider": {
        "legend": {"tokenTypes": ["keyword", "variable"], "tokenModifiers": []},
        "full": {"delta": true}, "range": true}})
}

#[test]
fn semantic_tokens_go_full_then_delta_then_ranged() {
    let mut rig = started(&token_caps());
    rig.sent();
    rig.lsp.request_semantic_tokens(&file(), None);
    let (id, msg) = rig.one_request("textDocument/semanticTokens/full");
    assert_eq!(msg["params"], json!({"textDocument": {"uri": uri()}}));
    let events = rig.answer(
        id,
        json!({"resultId": "t1", "data": [0, 0, 3, 0, 0, 0, 4, 2, 1, 0]}),
    );
    let [LspEvent::SemanticTokens(tokens)] = events.as_slice() else {
        panic!("expected tokens");
    };
    assert_eq!(tokens.len(), 2);

    rig.lsp.request_semantic_tokens(&file(), None);
    let (id, msg) = rig.one_request("textDocument/semanticTokens/full/delta");
    assert_eq!(msg["params"]["previousResultId"], "t1");
    let edits = json!({"resultId": "t2", "edits": [{"start": 5, "deleteCount": 5, "data": [1, 0, 3, 0, 0]}]});
    let events = rig.answer(id, edits);
    assert!(matches!(events.as_slice(), [LspEvent::SemanticTokens(t)] if t.len() == 2));
    assert_eq!(rig.server().semantic_results[&uri()].0, "t2");

    let visible = vix_lsp_core::Range::default();
    rig.lsp.request_semantic_tokens(&file(), Some(visible));
    let (id, _) = rig.one_request("textDocument/semanticTokens/range");
    let events = rig.answer(id, json!({"data": [0, 0, 3, 1, 0]}));
    assert!(matches!(events.as_slice(), [LspEvent::SemanticTokens(t)] if t.len() == 1));
    assert_eq!(
        rig.server().semantic_results[&uri()].0,
        "t2",
        "a range is never a delta base"
    );
}

#[test]
fn an_unappliable_delta_forgets_its_base_and_a_stale_reply_is_dropped() {
    let mut rig = started(&token_caps());
    rig.sent();
    rig.lsp.request_semantic_tokens(&file(), None);
    let (id, _) = rig.one_request("textDocument/semanticTokens/full");
    rig.answer(id, json!({"resultId": "t1", "data": [0, 0, 3, 0, 0]}));
    rig.lsp.request_semantic_tokens(&file(), None);
    let (id, _) = rig.one_request("textDocument/semanticTokens/full/delta");
    let beyond = json!({"resultId": "t2", "edits": [{"start": 50, "deleteCount": 1, "data": []}]});
    assert!(rig.answer(id, beyond).is_empty());
    assert!(
        rig.server().semantic_results.is_empty(),
        "the base is dropped, so the next ask is full"
    );

    rig.lsp.request_semantic_tokens(&file(), None);
    let (id, msg) = rig.one_request("textDocument/semanticTokens/full");
    assert!(msg["params"].get("previousResultId").is_none());
    rig.lsp.did_change(&file(), "moved on");
    rig.sent();
    assert!(
        rig.answer(id, json!({"resultId": "t3", "data": [0, 0, 3, 0, 0]}))
            .is_empty()
    );
}

#[test]
fn a_full_reply_without_a_result_id_cannot_seed_a_delta() {
    let mut rig = started(&token_caps());
    rig.sent();
    rig.lsp.request_semantic_tokens(&file(), None);
    let (id, _) = rig.one_request("textDocument/semanticTokens/full");
    rig.answer(id, json!({"data": [0, 0, 3, 0, 0]}));
    assert!(rig.server().semantic_results.is_empty());
    // A reply that is neither data nor edits is nothing.
    rig.lsp.request_semantic_tokens(&file(), None);
    let (id, _) = rig.one_request("textDocument/semanticTokens/full");
    assert!(rig.answer(id, json!({"unexpected": true})).is_empty());
}

#[test]
fn servers_without_a_legend_or_a_document_are_not_asked_for_tokens() {
    let mut rig = started(&json!({}));
    rig.sent();
    rig.lsp.request_semantic_tokens(&file(), None);
    rig.quiet("no legend");
    let mut rig = started(&token_caps());
    rig.sent();
    rig.lsp
        .request_semantic_tokens(&PathBuf::from("/proj/closed.rs"), None);
    rig.quiet("document not open");
    assert_eq!(
        rig.lsp
            .semantic_tokens_ranged(&PathBuf::from("/proj/closed.rs")),
        None
    );
}

// ----- code lenses and document extras (T621, T618, T726) ------------------------------

#[test]
fn background_lens_refresh_updates_resolved_and_unresolved_lenses() {
    let mut rig = started(&json!({"codeLensProvider": {"resolveProvider": true}}));
    rig.sent();
    assert!(rig.lsp.refresh_code_lens(&file()));
    let (id, _) = rig.one_request("textDocument/codeLens");
    let lenses = json!([
        {"range": rng(1, 0, 1, 3), "command": {"title": "Run", "command": "run"}},
        {"range": rng(4, 0, 4, 3)},
    ]);
    let events = rig.answer(id, lenses);
    let [
        LspEvent::CodeLensUpdate {
            path,
            lenses,
            unresolved,
        },
    ] = events.as_slice()
    else {
        panic!("expected a lens update");
    };
    assert_eq!(path, &file());
    assert_eq!(lenses.len(), 1);
    assert_eq!(unresolved[0].0, 4);

    let pending = json!({"range": rng(4, 0, 4, 3), "data": 1});
    assert!(rig.lsp.resolve_code_lens(&file(), &pending));
    let (id, msg) = rig.one_request("codeLens/resolve");
    assert_eq!(msg["params"], pending);
    let resolved =
        json!({"range": rng(4, 0, 4, 3), "command": {"title": "Test", "command": "test"}});
    let events = rig.answer(id, resolved);
    assert!(matches!(events.as_slice(),
        [LspEvent::CodeLensResolved { lens, .. }] if lens.1 == "Test"));
}

#[test]
fn lens_refresh_is_skipped_for_servers_without_lenses_but_asked_while_starting() {
    let mut rig = Rig::open();
    assert!(
        !rig.lsp.refresh_code_lens(&file()),
        "ready, but no lens support"
    );
    assert!(!rig.lsp.resolve_code_lens(&file(), &json!({})));
    assert!(!rig.lsp.refresh_code_lens(&PathBuf::from("/proj/closed.rs")));
    let mut rig = Rig::new(false);
    rig.lsp.did_open(&file(), "x");
    assert!(
        rig.lsp.refresh_code_lens(&file()),
        "capabilities unknown yet: ask anyway"
    );
    assert_eq!(rig.server().code_lens, LensSupport::None);
}

#[test]
fn colors_and_links_are_requested_only_for_servers_that_offer_them() {
    let mut rig = started(&json!({"colorProvider": true, "documentLinkProvider": {}}));
    rig.sent();
    assert!(rig.lsp.refresh_document_extras(&file()));
    let sent = rig.sent();
    let methods: Vec<&str> = sent.iter().filter_map(|m| m["method"].as_str()).collect();
    assert_eq!(
        methods,
        ["textDocument/documentColor", "textDocument/documentLink"]
    );
    let color = json!([{"range": rng(0, 0, 0, 7),
        "color": {"red": 1.0, "green": 0.0, "blue": 0.0, "alpha": 1.0}}]);
    let events = rig.answer(sent[0]["id"].as_i64().unwrap(), color);
    let [LspEvent::DocumentColors { colors, .. }] = events.as_slice() else {
        panic!("expected colors");
    };
    assert_eq!(colors[0].rgb, (255, 0, 0));
    let links = json!([{"range": rng(0, 0, 0, 4), "target": "https://example.org"}]);
    let events = rig.answer(sent[1]["id"].as_i64().unwrap(), links);
    assert!(matches!(events.as_slice(),
        [LspEvent::DocumentLinks { links, .. }] if links[0].target == "https://example.org"));

    assert!(rig.lsp.request_color_presentation(&file(), &colors[0]));
    let (id, msg) = rig.one_request("textDocument/colorPresentation");
    assert_eq!(msg["params"]["color"]["red"], 1.0);
    let events = rig.answer(id, json!([{"label": "#ff0000"}]));
    assert!(matches!(events.as_slice(),
        [LspEvent::ColorPresentations { presentations, .. }] if presentations[0].label == "#ff0000"));

    let mut bare = started(&json!({}));
    bare.sent();
    assert!(!bare.lsp.refresh_document_extras(&file()));
    assert!(!bare.lsp.request_color_presentation(&file(), &colors[0]));
    let mut cold = Rig::new(false);
    cold.lsp.did_open(&file(), "x");
    assert!(
        !cold.lsp.refresh_document_extras(&file()),
        "unknown before initialize"
    );
}

#[test]
fn inline_completion_reaches_the_document_and_stale_replies_do_not() {
    let mut rig = started(&json!({"inlineCompletionProvider": true}));
    rig.sent();
    rig.lsp.request_inline_completion(&file(), 0, 3, true);
    let (id, msg) = rig.one_request("textDocument/inlineCompletion");
    assert_eq!(
        msg["params"]["position"],
        json!({"line": 0, "character": 3})
    );
    let events = rig.answer(id, json!({"items": [{"insertText": "suggested"}]}));
    assert!(matches!(events.as_slice(),
        [LspEvent::InlineCompletion { items, .. }] if items == &["suggested".to_string()]));

    rig.lsp.request_inline_completion(&file(), 0, 3, false);
    let (id, _) = rig.one_request("textDocument/inlineCompletion");
    rig.lsp.did_change(&file(), "changed");
    rig.sent();
    assert!(
        rig.answer(id, json!({"items": [{"insertText": "late"}]}))
            .is_empty()
    );

    rig.lsp.request_inline_completion(&file(), 0, 3, false);
    let (id, _) = rig.one_request("textDocument/inlineCompletion");
    assert!(
        rig.answer(id, json!({"items": []})).is_empty(),
        "nothing to suggest"
    );

    let mut bare = Rig::open();
    assert!(!bare.lsp.supports_inline_completion(&file()));
    bare.lsp.request_inline_completion(&file(), 0, 0, false);
    bare.quiet("");
}

// ----- file operations and watchers (T623) -----------------------------------------------

fn file_op_caps() -> Value {
    let filter = json!({"filters": [{"pattern": {"glob": "**/*.rs"}}]});
    json!({"workspace": {"fileOperations": {
        "willRename": filter, "didRename": filter, "didCreate": filter, "didDelete": filter}}})
}

#[test]
fn a_rename_is_announced_before_and_after_to_servers_that_asked() {
    let mut rig = started(&file_op_caps());
    rig.sent();
    let (old, new) = (PathBuf::from("/proj/a.rs"), PathBuf::from("/proj/z.rs"));
    assert!(rig.lsp.will_rename_files(&old, &new));
    let (id, msg) = rig.one_request("workspace/willRenameFiles");
    assert_eq!(msg["params"]["files"][0]["oldUri"], path_to_uri(&old));
    assert_eq!(msg["params"]["files"][0]["newUri"], path_to_uri(&new));
    let events = rig.answer(id, edit_a("fixed-import"));
    assert!(matches!(events.as_slice(),
        [LspEvent::WillFileOperations { edits, last: true }] if edits.len() == 1));

    rig.lsp.did_rename_files(&old, &new);
    assert_eq!(rig.methods(), ["workspace/didRenameFiles"]);

    // A server that answers with an error must not block the rename.
    assert!(rig.lsp.will_rename_files(&old, &new));
    let (id, _) = rig.one_request("workspace/willRenameFiles");
    let events = rig.fail(id, Some("no"));
    assert!(matches!(events.as_slice(),
        [LspEvent::WillFileOperations { edits, last: true }] if edits.is_empty()));
}

#[test]
fn will_operations_wait_for_every_server_and_skip_unwanted_paths() {
    let mut rig = started(&file_op_caps());
    rig.sent();
    let unwanted = FileOpItem::renamed(
        &PathBuf::from("/proj/a.txt"),
        &PathBuf::from("/proj/b.txt"),
        false,
    );
    assert!(
        !rig.lsp
            .will_file_operations(&[(FileOp::Rename, vec![unwanted])])
    );
    rig.quiet("");
    let mut cold = Rig::new(false);
    assert!(
        !cold
            .lsp
            .will_rename_files(&PathBuf::from("/a.rs"), &PathBuf::from("/b.rs"))
    );
    // An answer nobody is waiting for does not underflow the counter.
    let events = rig.deliver(json!({"jsonrpc": "2.0", "id": 40, "result": null}));
    assert!(events.is_empty());
    let mut events = Vec::new();
    rig.lsp.finish_will_rename(Vec::new(), &mut events);
    assert!(matches!(
        events.as_slice(),
        [LspEvent::WillFileOperations { last: true, .. }]
    ));
}

#[test]
fn did_file_operations_report_the_changes_for_watchers_too() {
    let mut rig = started(&file_op_caps());
    rig.sent();
    let created = FileOpItem::single(&PathBuf::from("/proj/new.rs"), false);
    let deleted = FileOpItem::single(&PathBuf::from("/proj/old.rs"), false);
    let dir = FileOpItem::single(&PathBuf::from("/proj/sub"), true);
    let renamed = FileOpItem::renamed(
        &PathBuf::from("/proj/r1.rs"),
        &PathBuf::from("/proj/r2.rs"),
        false,
    );
    let changes = rig.lsp.did_file_operations(&[
        (FileOp::Create, vec![created, dir]),
        (FileOp::Delete, vec![deleted]),
        (FileOp::Rename, vec![renamed]),
    ]);
    assert_eq!(
        changes,
        vec![
            (PathBuf::from("/proj/new.rs"), FileChangeType::Created),
            (PathBuf::from("/proj/old.rs"), FileChangeType::Deleted),
            (PathBuf::from("/proj/r1.rs"), FileChangeType::Deleted),
            (PathBuf::from("/proj/r2.rs"), FileChangeType::Created),
        ]
    );
    assert_eq!(
        rig.methods(),
        [
            "workspace/didCreateFiles",
            "workspace/didDeleteFiles",
            "workspace/didRenameFiles"
        ]
    );
}

fn register(id: &str, glob: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": 21, "method": "client/registerCapability",
           "params": {"registrations": [
               {"id": id, "method": "workspace/didChangeWatchedFiles",
                "registerOptions": {"watchers": [{"globPattern": glob}]}},
               {"id": "other", "method": "textDocument/hover"}]}})
}

#[test]
fn dynamic_watchers_filter_which_changes_the_server_hears_about() {
    let mut rig = Rig::open();
    assert!(!rig.lsp.watches_files());
    rig.deliver(register("w1", "**/*.toml"));
    assert_eq!(rig.sent()[0]["id"], 21, "registration is acknowledged");
    assert!(rig.lsp.watches_files());
    assert!(rig.lsp.watch_covers(&PathBuf::from("/proj/Cargo.toml")));
    assert!(!rig.lsp.watch_covers(&PathBuf::from("/proj/a.rs")));
    assert_eq!(rig.lsp.watch_roots(), vec![PathBuf::from("/proj")]);

    rig.lsp.did_change_watched_files(&[
        (PathBuf::from("/proj/Cargo.toml"), FileChangeType::Changed),
        (PathBuf::from("/proj/a.rs"), FileChangeType::Changed),
    ]);
    let sent = rig.sent();
    let changes = sent[0]["params"]["changes"].as_array().expect("changes");
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0]["uri"],
        path_to_uri(&PathBuf::from("/proj/Cargo.toml"))
    );
    assert_eq!(changes[0]["type"], 2);
    rig.lsp
        .did_change_watched_files(&[(PathBuf::from("/proj/a.rs"), FileChangeType::Changed)]);
    rig.quiet("nothing matched");

    rig.deliver(json!({"jsonrpc": "2.0", "id": 22, "method": "client/unregisterCapability",
                       "params": {"unregisterations": [{"id": "w1", "method": "workspace/didChangeWatchedFiles"}]}}));
    rig.sent();
    assert!(!rig.lsp.watches_files());
    rig.lsp
        .did_change_watched_files(&[(PathBuf::from("/proj/Cargo.toml"), FileChangeType::Changed)]);
    rig.quiet("");
}

// ----- crashes and restarts (T134, T576, T732) ---------------------------------------------

#[test]
fn a_crashed_server_is_respawned_and_its_documents_are_replayed() {
    let mut rig = Rig::open();
    rig.crash();
    let events = rig.lsp.poll();
    let [LspEvent::ServerRestarted(paths)] = events.as_slice() else {
        panic!("expected a restart");
    };
    assert_eq!(paths, &vec![file()]);
    assert_eq!(rig.lsp.statuses()[0].last_error, Some(ServerError::Crashed));
    assert_eq!(
        rig.lsp.statuses()[0].open_documents,
        0,
        "the host replays didOpen"
    );
    assert_eq!(rig.lsp.restart_attempts[LANG], 1);
}

#[test]
fn a_crash_loop_gives_up_after_the_budget() {
    let mut rig = Rig::open();
    let mut last = Vec::new();
    for attempt in 1..=4 {
        // The live server (the original, then each respawn) dies again.
        rig.crash();
        last = rig.lsp.poll();
        if attempt <= 3 {
            assert!(
                rig.lsp.servers.contains_key(LANG),
                "respawned on attempt {attempt}"
            );
        }
    }
    assert!(matches!(last.as_slice(), [LspEvent::ServerCrashed(l)] if l == LANG));
    assert!(!rig.lsp.servers.contains_key(LANG));
    assert_eq!(rig.lsp.statuses()[0].state, ServerState::Failed);
}

#[test]
fn a_long_lived_server_earns_a_fresh_budget_when_it_crashes() {
    let mut rig = Rig::open();
    rig.lsp.restart_attempts.insert(LANG.into(), 3);
    rig.server().ready_since = Instant::now().checked_sub(std::time::Duration::from_secs(60));
    rig.crash();
    let events = rig.lsp.poll();
    assert!(matches!(events.as_slice(), [LspEvent::ServerRestarted(_)]));
    assert_eq!(
        rig.lsp.restart_attempts[LANG], 1,
        "the old count was forgiven"
    );
}

#[test]
fn a_missing_rustup_component_is_reported_with_its_install_command() {
    let mut rig = Rig::open();
    let log = rig.lsp.log_for(LANG);
    crate::log_push(
        &log,
        "error: Unknown binary 'sleep' in official toolchain".into(),
    );
    rig.crash();
    let events = rig.lsp.poll();
    assert!(matches!(events.as_slice(),
        [LspEvent::ServerNotInstalled { language_id, install }]
            if language_id == LANG && install == "rustup component add sleep"));
    assert_eq!(
        rig.lsp.statuses()[0].last_error,
        Some(ServerError::NotInstalled(
            "rustup component add sleep".into()
        ))
    );
}

#[test]
fn a_server_sending_an_oversized_frame_is_dropped_not_respawned() {
    // T848: the reader thread reports the framing error (see
    // `tests::the_reader_stops_a_server_declaring_a_terabyte_message`).
    let mut rig = Rig::open();
    let error = vix_lsp_core::frame::FrameError::TooLarge {
        length: 1_000_000_000_000,
        limit: vix_lsp_core::frame::MAX_FRAME_BYTES,
    };
    let (tx, rx) = std::sync::mpsc::channel();
    tx.send(crate::Incoming::Framing(error.clone())).unwrap();
    rig.server().rx = rx;
    let events = rig.lsp.poll();
    assert!(matches!(events.as_slice(),
        [LspEvent::ServerDropped { language_id, error: e }] if language_id == LANG && *e == error));
    assert!(
        !rig.lsp.servers.contains_key(LANG),
        "the connection is closed"
    );
    let status = &rig.lsp.statuses()[0];
    assert_eq!(status.state, ServerState::Failed);
    assert_eq!(status.last_error, Some(ServerError::Framing(error)));
    assert!(
        !rig.lsp.restart_attempts.contains_key(LANG),
        "not counted as a crash"
    );
}

#[test]
fn a_crashed_server_that_cannot_be_started_again_is_reported_crashed() {
    let mut rig = Rig::open();
    rig.lsp.configs[0].command = vec!["vix-no-such-language-server-cov".into()];
    rig.crash();
    let events = rig.lsp.poll();
    assert!(matches!(events.as_slice(), [LspEvent::ServerCrashed(l)] if l == LANG));
    assert_eq!(
        rig.lsp.statuses()[0].last_error,
        Some(ServerError::SpawnFailed)
    );
    // A language with no configuration cannot be respawned at all.
    let mut orphan = Rig::open();
    orphan.lsp.configs.clear();
    orphan.crash();
    assert!(orphan.lsp.poll().is_empty());
}

#[test]
fn restarting_reports_the_open_files_and_clears_the_servers_diagnostics() {
    let mut rig = Rig::open();
    rig.deliver(
        json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
                       "params": {"uri": uri(), "diagnostics": [diag("x")]}}),
    );
    rig.lsp.restart_attempts.insert(LANG.into(), 2);
    let reopen = rig.lsp.restart(&file());
    assert_eq!(reopen, vec![file()]);
    assert!(rig.lsp.diagnostics_for(&file()).is_empty());
    assert!(!rig.lsp.restart_attempts.contains_key(LANG));
    assert!(rig.lsp.server_log(&file()).contains("--- restarted ---"));
    assert_eq!(rig.lsp.restart_language("nope"), Vec::<PathBuf>::new());
    // The old rig channels are gone with the old server; nothing to read.
    assert!(rig.lsp.servers.contains_key(LANG));
}

#[test]
fn shutdown_asks_politely_then_stops_every_server() {
    let mut rig = Rig::open();
    rig.lsp.shutdown();
    assert_eq!(rig.methods(), ["shutdown", "exit"]);
    assert!(rig.lsp.servers.is_empty());
    assert_eq!(rig.lsp.statuses()[0].state, ServerState::NotStarted);
}

#[test]
fn configs_can_be_added_once_per_language() {
    let mut rig = Rig::new(true);
    let more = |lang: &str| vix_settings::LspServer {
        language_id: lang.into(),
        extensions: vec!["py".into()],
        command: vec!["pylsp".into()],
    };
    rig.lsp
        .add_configs(vec![more("python"), more("python"), more(LANG)]);
    assert_eq!(
        rig.lsp.configs.len(),
        2,
        "python once; the existing rs id is kept"
    );
    assert!(rig.lsp.handles(&PathBuf::from("/x/a.py")));
    assert!(rig.lsp.is_active());
}

// ----- guards: servers that are missing, closed documents, odd messages -------------

/// Every position, document and edit request, asked about `path`.
fn poke_everything(lsp: &mut crate::Lsp, path: &std::path::Path) {
    lsp.request_hover(path, 0, 0);
    lsp.request_definition(path, 0, 0);
    lsp.request_completion(path, 0, 0);
    lsp.request_references(path, 0, 0);
    lsp.request_document_symbols(path);
    lsp.request_workspace_symbols(path, "q");
    lsp.request_signature_help(path, 0, 0);
    lsp.request_selection_range(path, 0, 0);
    lsp.request_document_highlight(path, 0, 0);
    lsp.request_code_lens(path);
    lsp.request_linked_editing(path, 0, 0);
    lsp.request_inlay_hint(path, (0, 0), (1, 0));
    lsp.request_folding_range(path);
    lsp.request_rename(path, 0, 0, "n");
    lsp.request_prepare_rename(path, 0, 0);
    lsp.request_code_action(path, (0, 0), (0, 1), &json!([]));
    lsp.request_formatting(path, 4);
    lsp.request_range_formatting(path, (0, 0), (0, 1), 4);
    lsp.request_on_type_formatting(path, 0, 0, '}', 4);
    lsp.request_prepare_call_hierarchy(path, 0, 0);
    lsp.request_prepare_type_hierarchy(path, 0, 0);
    lsp.request_semantic_tokens(path, None);
    lsp.request_inline_completion(path, 0, 0, false);
    lsp.did_save(path, "t");
    lsp.did_change(path, "t");
    lsp.did_close(path);
    let _ = lsp.refresh_code_lens(path);
    let _ = lsp.refresh_document_extras(path);
}

/// Capabilities that make a server accept every request `poke_everything` makes.
fn everything_caps() -> Value {
    let mut caps = rich_capabilities();
    caps["codeLensProvider"] = json!({"resolveProvider": true});
    caps["documentOnTypeFormattingProvider"] = json!({"firstTriggerCharacter": "}"});
    caps
}

#[test]
fn nothing_is_sent_for_documents_that_are_not_open() {
    let mut rig = started(&everything_caps());
    rig.sent();
    poke_everything(&mut rig.lsp, &PathBuf::from("/proj/closed.rs"));
    rig.quiet("");
}

#[test]
fn a_second_configured_server_that_is_not_running_adds_nothing() {
    let mut single = started(&everything_caps());
    single.sent();
    poke_everything(&mut single.lsp, &file());
    let expected = single.sent().len();
    assert!(
        expected > 20,
        "the open document is asked about ({expected})"
    );

    // A second server for the same extension that cannot start: every fan-out
    // skips it, and the first server sees exactly the same traffic.
    let mut rig = started(&everything_caps());
    rig.lsp.configs.push(vix_settings::LspServer {
        language_id: "ghost".into(),
        extensions: vec!["rs".into()],
        command: vec!["vix-no-such-language-server-cov".into()],
    });
    rig.sent();
    poke_everything(&mut rig.lsp, &file());
    assert_eq!(rig.sent().len(), expected);
    assert_eq!(rig.lsp.statuses().len(), 2);
}

#[test]
fn requests_that_target_the_first_config_do_nothing_when_its_server_is_absent() {
    let mut rig = started(&json!({}));
    rig.lsp.configs.insert(
        0,
        vix_settings::LspServer {
            language_id: "ghost".into(),
            extensions: vec!["rs".into()],
            command: vec!["vix-no-such-language-server-cov".into()],
        },
    );
    rig.sent();
    rig.lsp.execute_command(&file(), "c", &json!([]));
    rig.lsp.request_completion_resolve(&file(), "x", None);
    rig.quiet("");
    assert_eq!(rig.lsp.encoding_for(&file()), Encoding::Utf16);
}

#[test]
fn messages_for_a_server_that_is_gone_are_ignored() {
    let mut rig = Rig::open();
    let mut events = Vec::new();
    for method in [
        "workspace/codeLens/refresh",
        "workspace/diagnostic/refresh",
        "workspace/applyEdit",
        "client/registerCapability",
    ] {
        rig.lsp.handle(
            "nobody",
            &json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": {}}),
            &mut events,
        );
    }
    let end = vix_lsp_core::message::parse_progress_update(
        &json!({"token": "t", "value": {"kind": "end"}}),
    )
    .expect("an end update");
    rig.lsp.track_progress("nobody", end);
    // Only the code-lens refresh is surfaced; there is nobody to answer.
    assert!(matches!(events.as_slice(), [LspEvent::CodeLensRefresh]));
    // A reply with a non-numeric id is not one of ours.
    let mut quiet = Vec::new();
    rig.lsp
        .handle(LANG, &json!({"id": "text", "result": 1}), &mut quiet);
    assert!(quiet.is_empty());
    rig.quiet("");
}

#[test]
fn a_server_with_no_command_cannot_be_started() {
    let mut rig = Rig::open();
    rig.lsp.configs[0].command = Vec::new();
    rig.crash();
    let events = rig.lsp.poll();
    assert!(matches!(events.as_slice(), [LspEvent::ServerCrashed(l)] if l == LANG));
    assert_eq!(
        rig.lsp.statuses()[0].last_error,
        Some(ServerError::SpawnFailed)
    );
}

#[test]
fn code_lens_support_without_resolve_is_lenses_only() {
    let mut rig = started(&json!({"codeLensProvider": {}}));
    assert_eq!(rig.server().code_lens, LensSupport::Lenses);
    assert!(!rig.lsp.resolves_code_lens(&file()));
    assert!(!rig.lsp.resolve_code_lens(&file(), &json!({})));
    let mut rig = started(&json!({"codeLensProvider": false}));
    assert!(rig.lsp.enabled());
    assert_eq!(rig.server().code_lens, LensSupport::None);
}

#[test]
fn an_empty_semantic_token_reply_is_not_an_event() {
    let mut rig = started(&token_caps());
    rig.sent();
    rig.lsp.request_semantic_tokens(&file(), None);
    let (id, _) = rig.one_request("textDocument/semanticTokens/full");
    assert!(
        rig.answer(id, json!({"resultId": "e", "data": []}))
            .is_empty()
    );
}

#[test]
fn progress_tasks_are_replaced_by_a_new_begin_and_shown_by_message_alone() {
    let mut rig = Rig::open();
    let progress = |value: Value| json!({"jsonrpc": "2.0", "method": "$/progress", "params": {"token": "t", "value": value}});
    rig.deliver(progress(json!({"kind": "begin", "title": "First"})));
    rig.deliver(progress(
        json!({"kind": "begin", "message": "only a message"}),
    ));
    assert_eq!(
        rig.lsp.progress_line().as_deref(),
        Some("rs: only a message")
    );
    assert_eq!(
        rig.lsp.statuses()[0].progress.len(),
        1,
        "same token: replaced"
    );
    rig.deliver(progress(
        json!({"kind": "report", "title": "Renamed", "message": "m"}),
    ));
    assert_eq!(rig.lsp.progress_line().as_deref(), Some("rs: Renamed: m"));
    // A task with nothing to show is just the language.
    rig.deliver(progress(json!({"kind": "end"})));
    rig.deliver(progress(json!({"kind": "begin"})));
    assert_eq!(rig.lsp.progress_line().as_deref(), Some("rs"));
}

#[test]
fn a_file_without_an_extension_has_no_server() {
    let rig = Rig::new(true);
    assert!(!rig.lsp.handles(&PathBuf::from("/proj/Makefile")));
    assert_eq!(
        rig.lsp.configs_for(&PathBuf::from("/proj/Makefile")).len(),
        0
    );
}

#[test]
fn rename_operations_without_a_target_and_cold_servers_report_only_changes() {
    let mut rig = Rig::new(false);
    let item = FileOpItem::single(&PathBuf::from("/proj/odd.rs"), false);
    let changes = rig.lsp.did_file_operations(&[(FileOp::Rename, vec![item])]);
    assert_eq!(
        changes,
        vec![(PathBuf::from("/proj/odd.rs"), FileChangeType::Deleted)]
    );
    rig.quiet("a server still starting is not told");
    rig.lsp
        .did_change_watched_files(&[(PathBuf::from("/proj/odd.rs"), FileChangeType::Deleted)]);
    rig.quiet("");
    let mut ready = started(&file_op_caps());
    ready.sent();
    let unwanted = FileOpItem::single(&PathBuf::from("/proj/notes.txt"), false);
    ready
        .lsp
        .did_file_operations(&[(FileOp::Create, vec![unwanted])]);
    ready.quiet("the filter wants only .rs files");
}

#[test]
fn a_real_process_server_is_started_read_and_initialized() {
    // A shell stands in for a language server: it answers `initialize` with a
    // framed reply written to its stdout and then idles.
    let body = r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"completionProvider":{"triggerCharacters":["."]}}}}"#;
    let script = format!(
        "printf 'Content-Length: {}\\r\\n\\r\\n%s' '{body}'; echo oops >&2; sleep 30",
        body.len()
    );
    let config = vix_settings::LspServer {
        language_id: "sh".into(),
        extensions: vec!["sh".into()],
        command: vec!["sh".into(), "-c".into(), script],
    };
    let mut lsp = crate::Lsp::new(true, vec![config], &[PathBuf::from("/proj")]);
    let path = PathBuf::from("/proj/run.sh");
    lsp.did_open(&path, "echo");
    assert_eq!(lsp.statuses()[0].state, ServerState::Starting);
    let end = Instant::now() + std::time::Duration::from_secs(20);
    while lsp.statuses()[0].state != ServerState::Running {
        lsp.poll();
        assert!(Instant::now() < end, "the server never finished initialize");
        std::thread::yield_now();
    }
    assert!(lsp.is_completion_trigger(&path, '.'));
    // Its stderr is captured into the log (the reader runs on its own thread).
    while !lsp.server_log(&path).contains("oops") {
        assert!(Instant::now() < end, "stderr never arrived");
        std::thread::yield_now();
    }
    lsp.shutdown();
}

#[test]
fn color_presentation_and_lens_resolve_need_an_open_document_and_a_running_server() {
    let color = vix_lsp_core::document_extras::parse_colors(&json!([{"range": rng(0, 0, 0, 7),
        "color": {"red": 0.0, "green": 0.5, "blue": 1.0, "alpha": 1.0}}]))
    .remove(0);
    let mut rig =
        started(&json!({"colorProvider": true, "codeLensProvider": {"resolveProvider": true}}));
    rig.sent();
    let closed = PathBuf::from("/proj/closed.rs");
    assert!(!rig.lsp.request_color_presentation(&closed, &color));
    assert!(!rig.lsp.resolve_code_lens(&closed, &json!({"data": 1})));
    // A second server for the file that is not running is skipped over.
    rig.lsp.configs.push(vix_settings::LspServer {
        language_id: "ghost".into(),
        extensions: vec!["rs".into()],
        command: vec!["vix-no-such-language-server-cov".into()],
    });
    assert!(rig.lsp.request_color_presentation(&file(), &color));
    assert!(rig.lsp.resolve_code_lens(&file(), &json!({"data": 1})));
    assert_eq!(
        rig.methods(),
        ["textDocument/colorPresentation", "codeLens/resolve"]
    );
}

#[test]
fn empty_details_and_unknown_progress_ends_change_nothing() {
    let mut rig = Rig::open();
    rig.lsp.request_completion_resolve(&file(), "x", None);
    let (id, _) = rig.one_request("completionItem/resolve");
    assert!(rig.answer(id, json!({})).is_empty());
    rig.deliver(json!({"jsonrpc": "2.0", "method": "$/progress",
                       "params": {"token": "never-begun", "value": {"kind": "end"}}}));
    assert_eq!(rig.lsp.progress_line(), None);
}

#[test]
fn a_crashed_server_with_nothing_open_is_respawned_quietly() {
    let mut rig = Rig::new(true);
    rig.crash();
    assert!(rig.lsp.poll().is_empty(), "no documents to replay");
    assert!(
        rig.lsp.servers.contains_key(LANG),
        "but a fresh server is running"
    );
    assert_eq!(rig.lsp.statuses()[0].state, ServerState::Starting);
}

#[test]
fn a_refresh_request_to_a_server_without_pull_support_is_only_acknowledged() {
    let mut rig = Rig::open();
    rig.deliver(json!({"jsonrpc": "2.0", "id": 5, "method": "workspace/diagnostic/refresh"}));
    let sent = rig.sent();
    assert_eq!(
        sent,
        vec![json!({"jsonrpc": "2.0", "id": 5, "result": null})]
    );
    // A pull report for a server that has since gone is dropped.
    let mut events = Vec::new();
    rig.lsp.apply_pull_reply(
        "nobody",
        &uri(),
        &json!({"kind": "full", "resultId": "r", "items": [diag("x")]}),
        &mut events,
    );
    assert!(
        matches!(events.as_slice(), [LspEvent::Diagnostics(_)]),
        "still merged"
    );
    assert!(rig.server().diagnostic_result_ids.is_empty());
}
