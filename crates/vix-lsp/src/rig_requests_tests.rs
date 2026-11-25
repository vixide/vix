//! Document sync and the request/response pairs, against a scripted server
//! (see [`crate::rig_tests`]): what each `request_*` writes, and which
//! [`LspEvent`] each reply turns into.

use std::path::PathBuf;

use serde_json::{Value, json};

use crate::rig_tests::{LANG, Rig, edit_a, file, loc, rng};
use crate::{
    EditReply, EditReplyKind, HierarchyDirection, HierarchyItem, LspEvent, RenamePrepared,
    ServerError, path_to_uri,
};

fn uri() -> String {
    path_to_uri(&file())
}

fn position(line: u32, character: u32) -> Value {
    json!({"textDocument": {"uri": uri()}, "position": {"line": line, "character": character}})
}

// ----- document sync --------------------------------------------------------

#[test]
fn opening_a_document_twice_sends_one_did_open_with_version_one() {
    let mut rig = Rig::new(true);
    rig.lsp.did_open(&file(), "a");
    rig.lsp.did_open(&file(), "different");
    let sent = rig.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["method"], "textDocument/didOpen");
    let doc = &sent[0]["params"]["textDocument"];
    assert_eq!(doc["languageId"], LANG);
    assert_eq!(doc["version"], 1);
    assert_eq!(doc["text"], "a");
    assert_eq!(rig.server().docs[&uri()], 1);
}

#[test]
fn a_server_that_is_still_starting_queues_messages_until_initialized() {
    let mut rig = Rig::new(false);
    rig.lsp.did_open(&file(), "x");
    rig.quiet("nothing is written before initialize");
    assert!(rig.lsp.busy(), "a starting server keeps the loop ticking");
    rig.initialize(json!({"capabilities": {}}));
    assert_eq!(rig.methods(), ["initialized", "textDocument/didOpen"]);
    assert!(!rig.lsp.busy());
}

#[test]
fn changes_bump_the_version_and_skip_identical_text() {
    let mut rig = Rig::open();
    rig.lsp.did_change(&file(), "fn main() {}\n");
    rig.quiet("unchanged text sends nothing");
    rig.lsp.did_change(&file(), "fn main() { 1 }\n");
    let sent = rig.sent();
    assert_eq!(sent[0]["method"], "textDocument/didChange");
    assert_eq!(sent[0]["params"]["textDocument"]["version"], 2);
    assert_eq!(
        sent[0]["params"]["contentChanges"],
        json!([{"text": "fn main() { 1 }\n"}])
    );
    assert_eq!(rig.server().docs[&uri()], 2);
}

#[test]
fn an_incremental_server_gets_a_ranged_change() {
    let mut rig = Rig::new(false);
    rig.lsp.did_open(&file(), "hello world\n");
    rig.initialize(json!({"capabilities": {"textDocumentSync": 2}}));
    rig.sent();
    rig.lsp.did_change(&file(), "hello there\n");
    let sent = rig.sent();
    let change = &sent[0]["params"]["contentChanges"][0];
    assert!(change.get("range").is_some(), "ranged: {change}");
    assert_eq!(change["text"], "there");
}

#[test]
fn closing_forgets_the_document_and_tells_the_server_once() {
    let mut rig = Rig::open();
    rig.lsp.did_close(&file());
    let sent = rig.sent();
    assert_eq!(sent[0]["method"], "textDocument/didClose");
    assert_eq!(sent[0]["params"]["textDocument"]["uri"], uri());
    assert!(rig.server().docs.is_empty());
    rig.lsp.did_close(&file());
    rig.quiet("closing an unopened document is a no-op");
    rig.lsp.did_change(&file(), "later");
    rig.quiet("no changes for a closed document");
}

#[test]
fn saving_sends_the_text_and_a_disabled_client_sends_nothing() {
    let mut rig = Rig::open();
    rig.lsp.did_save(&file(), "saved text");
    let sent = rig.sent();
    assert_eq!(sent[0]["method"], "textDocument/didSave");
    assert_eq!(sent[0]["params"]["text"], "saved text");
    rig.lsp
        .did_save(&PathBuf::from("/proj/other.rs"), "unopened");
    rig.quiet("");

    rig.lsp.enabled = false;
    rig.lsp.did_open(&PathBuf::from("/proj/b.rs"), "b");
    rig.lsp.did_change(&file(), "zzz");
    rig.lsp.did_close(&file());
    rig.lsp.did_save(&file(), "zzz");
    rig.quiet("a disabled client is silent");
}

// ----- simple position requests and their replies ----------------------------

/// One request/reply pair: how to ask, what the server is expected to be
/// sent, and the reply it answers with.
struct Case {
    method: &'static str,
    ask: fn(&mut crate::Lsp),
    params: Value,
    result: Value,
    check: fn(&[LspEvent]),
}

fn cases() -> Vec<Case> {
    let mut all = navigation_cases();
    all.extend(document_cases());
    all.extend(edit_cases());
    all
}

/// Position requests: where is it, what is it, who uses it.
fn navigation_cases() -> Vec<Case> {
    let target = loc("b.rs", 3, 4);
    vec![
        Case {
            method: "textDocument/hover",
            ask: |l| l.request_hover(&file(), 1, 2),
            params: position(1, 2),
            result: json!({"contents": {"kind": "markdown", "value": "docs"}}),
            check: |e| assert!(matches!(e, [LspEvent::Hover(t)] if t == "docs")),
        },
        Case {
            method: "textDocument/definition",
            ask: |l| l.request_definition(&file(), 1, 2),
            params: position(1, 2),
            result: target.clone(),
            check: |e| {
                assert!(matches!(
                    e,
                    [LspEvent::Definition { path, line: 3, character: 4 }]
                        if path == &PathBuf::from("/proj/b.rs")
                ));
            },
        },
        Case {
            method: "textDocument/implementation",
            ask: |l| l.request_implementation(&file(), 1, 2),
            params: position(1, 2),
            result: json!([target.clone()]),
            check: |e| assert!(matches!(e, [LspEvent::Definition { line: 3, .. }])),
        },
        Case {
            method: "textDocument/typeDefinition",
            ask: |l| l.request_type_definition(&file(), 1, 2),
            params: position(1, 2),
            result: json!([target.clone()]),
            check: |e| assert!(matches!(e, [LspEvent::Definition { character: 4, .. }])),
        },
        Case {
            method: "textDocument/declaration",
            ask: |l| l.request_declaration(&file(), 1, 2),
            params: position(1, 2),
            result: target,
            check: |e| assert!(matches!(e, [LspEvent::Definition { .. }])),
        },
        Case {
            method: "textDocument/completion",
            ask: |l| l.request_completion(&file(), 0, 1),
            params: json!({"textDocument": {"uri": uri()}, "position": {"line": 0, "character": 1},
                           "context": {"triggerKind": 1}}),
            result: json!({"isIncomplete": true, "items": [{"label": "alpha"}]}),
            check: |e| {
                let [LspEvent::Completion(list)] = e else {
                    panic!("expected a completion list");
                };
                assert!(list.is_incomplete);
                assert_eq!(list.items.len(), 1);
            },
        },
        Case {
            method: "textDocument/references",
            ask: |l| l.request_references(&file(), 5, 6),
            params: json!({"textDocument": {"uri": uri()}, "position": {"line": 5, "character": 6},
                           "context": {"includeDeclaration": true}}),
            result: json!([loc("b.rs", 1, 1), loc("c.rs", 2, 2)]),
            check: |e| {
                let [LspEvent::References(refs)] = e else {
                    panic!("expected references");
                };
                assert_eq!(
                    refs,
                    &[
                        (PathBuf::from("/proj/b.rs"), 1, 1),
                        (PathBuf::from("/proj/c.rs"), 2, 2)
                    ]
                );
            },
        },
    ]
}

/// Whole-document requests and edits.
fn document_cases() -> Vec<Case> {
    vec![
        Case {
            method: "textDocument/documentSymbol",
            ask: |l| l.request_document_symbols(&file()),
            params: json!({"textDocument": {"uri": uri()}}),
            result: json!([{"name": "main", "kind": 12, "range": rng(0, 0, 3, 0),
                            "selectionRange": rng(0, 3, 0, 7)}]),
            check: |e| {
                assert!(matches!(e, [LspEvent::DocumentSymbols(s)]
                    if s == &[(0, 3, "main".to_string())]));
            },
        },
        Case {
            method: "workspace/symbol",
            ask: |l| l.request_workspace_symbols(&file(), "ma"),
            params: json!({"query": "ma"}),
            result: json!([{"name": "main", "kind": 12, "location": loc("b.rs", 4, 5)}]),
            check: |e| {
                assert!(matches!(e, [LspEvent::WorkspaceSymbols(s)]
                    if s == &[(PathBuf::from("/proj/b.rs"), 4, 5, "main".to_string())]));
            },
        },
        Case {
            method: "textDocument/signatureHelp",
            ask: |l| l.request_signature_help(&file(), 1, 1),
            params: position(1, 1),
            result: json!({"signatures": [{"label": "f(a, b)"}]}),
            check: |e| assert!(matches!(e, [LspEvent::SignatureHelp(t)] if t.contains("f(a, b)"))),
        },
        Case {
            method: "textDocument/selectionRange",
            ask: |l| l.request_selection_range(&file(), 0, 1),
            params: json!({"textDocument": {"uri": uri()}, "positions": [{"line": 0, "character": 1}]}),
            result: json!([{"range": rng(0, 1, 0, 2), "parent": {"range": rng(0, 0, 0, 9)}}]),
            check: |e| assert!(matches!(e, [LspEvent::SelectionRanges(r)] if r.len() == 2)),
        },
        Case {
            method: "textDocument/documentHighlight",
            ask: |l| l.request_document_highlight(&file(), 0, 1),
            params: position(0, 1),
            result: json!([{"range": rng(0, 1, 0, 2)}, {"range": rng(2, 1, 2, 2)}]),
            check: |e| assert!(matches!(e, [LspEvent::Highlights(r)] if r.len() == 2)),
        },
        Case {
            method: "textDocument/foldingRange",
            ask: |l| l.request_folding_range(&file()),
            params: json!({"textDocument": {"uri": uri()}}),
            result: json!([{"startLine": 1, "endLine": 4}, {"startLine": 2, "endLine": 2}]),
            check: |e| assert!(matches!(e, [LspEvent::FoldingRanges(r)] if r == &[(1, 4)])),
        },
        Case {
            method: "textDocument/inlayHint",
            ask: |l| l.request_inlay_hint(&file(), (0, 0), (9, 0)),
            params: json!({"textDocument": {"uri": uri()}, "range": rng(0, 0, 9, 0)}),
            result: json!([{"position": {"line": 1, "character": 2}, "label": ": u8"}]),
            check: |e| {
                assert!(matches!(e, [LspEvent::InlayHints(h)]
                    if h == &[(1, 2, ": u8".to_string())]));
            },
        },
        Case {
            method: "textDocument/codeLens",
            ask: |l| l.request_code_lens(&file()),
            params: json!({"textDocument": {"uri": uri()}}),
            result: json!([{"range": rng(2, 0, 2, 4), "command": {"title": "Run", "command": "run.it"}}]),
            check: |e| {
                let [LspEvent::CodeLenses(lenses)] = e else {
                    panic!("expected code lenses");
                };
                assert_eq!(lenses[0].0, 2);
                assert_eq!(lenses[0].1, "Run");
                assert_eq!(lenses[0].2, "run.it");
            },
        },
    ]
}

/// Requests whose reply is an edit the user can apply.
fn edit_cases() -> Vec<Case> {
    vec![
        Case {
            method: "textDocument/rename",
            ask: |l| l.request_rename(&file(), 0, 3, "renamed"),
            params: json!({"textDocument": {"uri": uri()}, "position": {"line": 0, "character": 3},
                           "newName": "renamed"}),
            result: edit_a("renamed"),
            check: |e| {
                let [LspEvent::WorkspaceEdit(files)] = e else {
                    panic!("expected a workspace edit");
                };
                assert_eq!(files[0].0, file());
                assert_eq!(files[0].1[0].1, "renamed");
            },
        },
        Case {
            method: "textDocument/codeAction",
            ask: |l| l.request_code_action(&file(), (0, 0), (0, 4), &json!([])),
            params: json!({"textDocument": {"uri": uri()}, "range": rng(0, 0, 0, 4),
                           "context": {"diagnostics": []}}),
            result: json!([{"title": "Fix it", "edit": edit_a("fixed")}]),
            check: |e| {
                let [LspEvent::CodeActions(actions)] = e else {
                    panic!("expected code actions");
                };
                assert_eq!(actions[0].0, "Fix it");
                assert_eq!(actions[0].1[0].1[0].1, "fixed");
            },
        },
        Case {
            method: "textDocument/codeAction",
            ask: |l| l.request_code_action_only(&file(), (0, 0), (0, 4), &json!([]), &["quickfix"]),
            params: json!({"textDocument": {"uri": uri()}, "range": rng(0, 0, 0, 4),
                           "context": {"diagnostics": [], "only": ["quickfix"]}}),
            result: json!([]),
            check: |e| assert!(e.is_empty(), "no actions, no event"),
        },
    ]
}

#[test]
fn each_request_is_written_as_specified_and_its_reply_becomes_an_event() {
    for case in cases() {
        let mut rig = Rig::open();
        (case.ask)(&mut rig.lsp);
        let (id, msg) = rig.one_request(case.method);
        assert_eq!(msg["params"], case.params, "{}", case.method);
        assert!(rig.lsp.busy(), "{}: a request is in flight", case.method);
        let events = rig.answer(id, case.result);
        (case.check)(&events);
        assert!(!rig.lsp.busy(), "{}: the reply settles it", case.method);
        assert_eq!(rig.lsp.take_edit_replies(), Vec::new());
    }
}

#[test]
fn a_null_reply_produces_no_event_and_an_unknown_id_is_dropped() {
    for case in cases() {
        let mut rig = Rig::open();
        (case.ask)(&mut rig.lsp);
        let (id, _) = rig.one_request(case.method);
        assert!(rig.answer(id, Value::Null).is_empty(), "{}", case.method);
    }
    let mut rig = Rig::open();
    assert!(rig.answer(999, json!("anything")).is_empty());
    // A reply to a server that is no longer running is dropped, too.
    let mut gone = rig.lsp.servers.remove(LANG).expect("server");
    let _ = gone.child.kill();
    let _ = gone.child.wait();
    let mut events = Vec::new();
    rig.lsp
        .handle_response(LANG, 7, &json!({"result": 1}), &mut events);
    rig.lsp
        .handle_response(LANG, 1, &json!({"result": {}}), &mut events);
    assert!(events.is_empty());
}

#[test]
fn empty_non_null_results_produce_no_event() {
    for case in cases() {
        let mut rig = Rig::open();
        (case.ask)(&mut rig.lsp);
        let (id, _) = rig.one_request(case.method);
        let events = rig.answer(id, json!([]));
        // Completion and folding always report their (possibly empty) list.
        assert!(
            events
                .iter()
                .all(|e| matches!(e, LspEvent::Completion(_) | LspEvent::FoldingRanges(_))),
            "{}: an empty result is nothing to show",
            case.method
        );
    }
}

#[test]
fn requests_for_unopened_or_unconfigured_files_are_not_sent() {
    let mut rig = Rig::open();
    let elsewhere = PathBuf::from("/proj/not_open.rs");
    let unknown = PathBuf::from("/proj/readme.txt");
    for path in [&elsewhere, &unknown] {
        rig.lsp.request_hover(path, 0, 0);
        rig.lsp.request_references(path, 0, 0);
        rig.lsp.request_document_symbols(path);
        rig.lsp.request_rename(path, 0, 0, "x");
        rig.lsp.request_formatting(path, 4);
        rig.lsp.request_linked_editing(path, 0, 0);
    }
    rig.lsp.request_completion_resolve(&unknown, "x", None);
    rig.lsp.execute_command(&unknown, "c", &json!([]));
    rig.quiet("");
}

#[test]
fn completion_requests_carry_their_trigger_context() {
    let mut rig = Rig::open();
    rig.lsp
        .request_completion_with(&file(), 0, 1, Some('.'), false);
    let (id, msg) = rig.one_request("textDocument/completion");
    assert_eq!(msg["params"]["context"]["triggerKind"], 2);
    assert_eq!(msg["params"]["context"]["triggerCharacter"], ".");
    rig.answer(id, json!([]));
    rig.lsp.request_completion_with(&file(), 0, 1, None, true);
    let (_, msg) = rig.one_request("textDocument/completion");
    assert_eq!(msg["params"]["context"]["triggerKind"], 3);
}

#[test]
fn completion_resolve_goes_to_the_files_server_and_reports_detail() {
    let mut rig = Rig::open();
    rig.lsp
        .request_completion_resolve(&file(), "alpha", Some(&json!({"k": 1})));
    let (id, msg) = rig.one_request("completionItem/resolve");
    assert_eq!(msg["params"], json!({"label": "alpha", "data": {"k": 1}}));
    let events = rig.answer(
        id,
        json!({"detail": "fn alpha()", "documentation": "Does alpha."}),
    );
    assert!(
        matches!(events.as_slice(), [LspEvent::CompletionDetail(t)] if t == "fn alpha()\nDoes alpha.")
    );
}

#[test]
fn execute_command_sends_the_request_without_tracking_a_reply() {
    let mut rig = Rig::open();
    rig.lsp.execute_command(&file(), "do.thing", &json!([1, 2]));
    let (_, msg) = rig.one_request("workspace/executeCommand");
    assert_eq!(
        msg["params"],
        json!({"command": "do.thing", "arguments": [1, 2]})
    );
    assert!(!rig.lsp.busy(), "fire and forget");
}

// ----- errors ---------------------------------------------------------------

#[test]
fn a_failed_request_surfaces_the_servers_message_and_is_remembered() {
    let mut rig = Rig::open();
    rig.lsp.request_hover(&file(), 0, 0);
    let (id, _) = rig.one_request("textDocument/hover");
    let events = rig.fail(id, Some("boom"));
    assert!(matches!(events.as_slice(), [LspEvent::RequestFailed(t)] if t == "boom"));
    assert_eq!(
        rig.lsp.statuses()[0].last_error,
        Some(ServerError::Request("boom".into()))
    );

    rig.lsp.request_hover(&file(), 0, 0);
    let (id, _) = rig.one_request("textDocument/hover");
    let events = rig.fail(id, None);
    assert!(matches!(events.as_slice(),
        [LspEvent::RequestFailed(t)] if t == "the server returned an error"));
}

#[test]
fn a_reply_with_neither_result_nor_error_is_ignored() {
    let mut rig = Rig::open();
    rig.lsp.request_hover(&file(), 0, 0);
    let (id, _) = rig.one_request("textDocument/hover");
    assert!(rig.deliver(json!({"jsonrpc": "2.0", "id": id})).is_empty());
}

#[test]
fn ambient_requests_fail_silently() {
    let mut rig = Rig::open();
    rig.server().code_lens = crate::LensSupport::Resolve;
    assert!(rig.lsp.refresh_code_lens(&file()));
    let (id, _) = rig.one_request("textDocument/codeLens");
    assert!(rig.fail(id, Some("not today")).is_empty());
    assert_eq!(rig.lsp.statuses()[0].last_error, None);
}

// ----- rename ---------------------------------------------------------------

#[test]
fn prepare_rename_distinguishes_placeholder_default_and_refusal() {
    let replies = [
        (
            json!({"range": rng(0, 0, 0, 3), "placeholder": "old"}),
            "placeholder",
        ),
        (rng(0, 0, 0, 3), "default"),
        (Value::Null, "refused"),
    ];
    for (reply, what) in replies {
        let mut rig = Rig::open();
        rig.lsp.request_prepare_rename(&file(), 0, 1);
        let (id, _) = rig.one_request("textDocument/prepareRename");
        let events = rig.answer(id, reply);
        match (what, events.as_slice()) {
            ("placeholder", [LspEvent::RenamePrepared(RenamePrepared::Placeholder(t))]) => {
                assert_eq!(t, "old");
            }
            ("default", [LspEvent::RenamePrepared(RenamePrepared::Default)])
            | ("refused", [LspEvent::RenamePrepared(RenamePrepared::NotRenameable)]) => {}
            _ => panic!("{what}: unexpected events"),
        }
    }
}

#[test]
fn a_prepare_rename_error_falls_back_to_the_default_seed() {
    let mut rig = Rig::open();
    rig.lsp.request_prepare_rename(&file(), 0, 1);
    let (id, _) = rig.one_request("textDocument/prepareRename");
    let events = rig.fail(id, Some("method not found"));
    assert!(matches!(
        events.as_slice(),
        [LspEvent::RenamePrepared(RenamePrepared::Default)]
    ));
}

// ----- edit-returning requests (T652) --------------------------------------------

#[test]
fn formatting_edits_apply_to_the_version_they_were_asked_about() {
    let mut rig = Rig::open();
    rig.lsp.request_formatting(&file(), 4);
    let (id, msg) = rig.one_request("textDocument/formatting");
    assert_eq!(msg["params"]["options"]["tabSize"], 4);
    let edits = json!([{"range": rng(0, 0, 0, 2), "newText": "FN"}]);
    assert!(
        rig.answer(id, edits).is_empty(),
        "edit replies are not events"
    );
    let replies = rig.lsp.take_edit_replies();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].path, file());
    assert!(matches!(&replies[0].kind, EditReplyKind::Edits(e) if e[0].1 == "FN"));
    assert!(rig.lsp.take_edit_replies().is_empty(), "taken once");
}

#[test]
fn a_reply_for_a_document_that_moved_on_is_reported_stale() {
    let mut rig = Rig::open();
    rig.lsp.request_range_formatting(&file(), (0, 0), (0, 5), 2);
    let (id, msg) = rig.one_request("textDocument/rangeFormatting");
    assert_eq!(msg["params"]["range"], rng(0, 0, 0, 5));
    rig.lsp.did_change(&file(), "changed meanwhile");
    rig.sent();
    rig.answer(id, json!([{"range": rng(0, 0, 0, 2), "newText": "x"}]));
    assert_eq!(
        rig.lsp.take_edit_replies(),
        vec![EditReply {
            path: file(),
            kind: EditReplyKind::Stale
        }]
    );
}

#[test]
fn empty_formatting_and_single_linked_ranges_are_dropped() {
    let mut rig = Rig::open();
    rig.lsp.request_formatting(&file(), 2);
    let (id, _) = rig.one_request("textDocument/formatting");
    rig.answer(id, json!([]));
    rig.lsp.request_linked_editing(&file(), 0, 1);
    let (id, _) = rig.one_request("textDocument/linkedEditingRange");
    rig.answer(id, json!({"ranges": [rng(0, 0, 0, 1)]}));
    assert_eq!(rig.lsp.take_edit_replies(), Vec::new());
}

#[test]
fn linked_editing_reports_two_or_more_ranges() {
    let mut rig = Rig::open();
    rig.lsp.request_linked_editing(&file(), 0, 1);
    let (id, msg) = rig.one_request("textDocument/linkedEditingRange");
    assert_eq!(msg["params"], position(0, 1));
    rig.answer(id, json!({"ranges": [rng(0, 0, 0, 1), rng(2, 0, 2, 1)]}));
    let replies = rig.lsp.take_edit_replies();
    assert!(matches!(&replies[0].kind, EditReplyKind::LinkedRanges(r) if r.len() == 2));
}

#[test]
fn on_type_formatting_is_sent_only_for_a_trigger_the_server_declared() {
    let mut rig = Rig::new(false);
    rig.lsp.did_open(&file(), "x");
    rig.initialize(json!({"capabilities": {"documentOnTypeFormattingProvider":
        {"firstTriggerCharacter": "}", "moreTriggerCharacter": [";"]}}}));
    rig.sent();
    assert!(rig.lsp.is_on_type_trigger(&file(), '}'));
    assert!(rig.lsp.is_on_type_trigger(&file(), ';'));
    assert!(!rig.lsp.is_on_type_trigger(&file(), 'x'));
    rig.lsp.request_on_type_formatting(&file(), 0, 1, 'x', 4);
    rig.quiet("'x' is no trigger");
    rig.lsp.request_on_type_formatting(&file(), 3, 4, '}', 2);
    let (id, msg) = rig.one_request("textDocument/onTypeFormatting");
    assert_eq!(msg["params"]["ch"], "}");
    rig.answer(id, json!([{"range": rng(3, 0, 3, 1), "newText": "  "}]));
    assert_eq!(rig.lsp.take_edit_replies().len(), 1);
    rig.lsp.enabled = false;
    assert!(!rig.lsp.is_on_type_trigger(&file(), '}'));
    assert!(!rig.lsp.is_completion_trigger(&file(), '.'));
}

// ----- hierarchy (T620) ---------------------------------------------------------

#[test]
fn call_hierarchy_prepares_an_item_then_walks_both_directions() {
    let mut rig = Rig::open();
    rig.lsp.request_prepare_call_hierarchy(&file(), 0, 3);
    let (id, _) = rig.one_request("textDocument/prepareCallHierarchy");
    let item = json!({"name": "main", "uri": uri(), "range": rng(0, 0, 0, 4), "selectionRange": rng(0, 0, 0, 4)});
    let events = rig.answer(id, json!([item]));
    let [LspEvent::CallHierarchyPrepared(prepared)] = events.as_slice() else {
        panic!("expected a prepared call item");
    };
    assert_eq!(prepared.server, LANG);
    assert_eq!(prepared.item["name"], "main");

    rig.lsp.request_incoming_calls(prepared);
    let (id, msg) = rig.one_request("callHierarchy/incomingCalls");
    assert_eq!(msg["params"]["item"]["name"], "main");
    let incoming = json!([{"from": {"name": "caller", "uri": uri(), "range": rng(7, 0, 7, 3),
                                    "selectionRange": rng(7, 0, 7, 3)}, "fromRanges": []}]);
    let events = rig.answer(id, incoming);
    assert!(matches!(events.as_slice(),
        [LspEvent::Hierarchy { direction: HierarchyDirection::Incoming, locations }]
            if locations == &[(file(), 7, 0)]));

    rig.lsp.request_outgoing_calls(prepared);
    let (id, _) = rig.one_request("callHierarchy/outgoingCalls");
    let events = rig.answer(id, json!([]));
    assert!(matches!(events.as_slice(),
        [LspEvent::Hierarchy { direction: HierarchyDirection::Outgoing, locations }]
            if locations.is_empty()));
}

#[test]
fn type_hierarchy_walks_supertypes_and_subtypes() {
    let mut rig = Rig::open();
    rig.lsp.request_prepare_type_hierarchy(&file(), 0, 3);
    let (id, _) = rig.one_request("textDocument/prepareTypeHierarchy");
    let item = json!({"name": "T", "uri": uri(), "range": rng(1, 0, 1, 1), "selectionRange": rng(1, 0, 1, 1)});
    let events = rig.answer(id, json!([item]));
    let [LspEvent::TypeHierarchyPrepared(prepared)] = events.as_slice() else {
        panic!("expected a prepared type item");
    };
    for (direction, method) in [
        (HierarchyDirection::Supertypes, "typeHierarchy/supertypes"),
        (HierarchyDirection::Subtypes, "typeHierarchy/subtypes"),
    ] {
        rig.lsp.request_hierarchy(prepared, direction);
        let (id, _) = rig.one_request(method);
        let events = rig.answer(id, json!([item]));
        assert!(matches!(events.as_slice(),
            [LspEvent::Hierarchy { direction: d, locations }]
                if *d == direction && locations == &[(file(), 1, 0)]));
    }
    rig.lsp.request_supertypes(prepared);
    rig.lsp.request_subtypes(prepared);
    assert_eq!(
        rig.methods(),
        ["typeHierarchy/supertypes", "typeHierarchy/subtypes"]
    );
    let gone = HierarchyItem::new("not-running", json!({}));
    rig.lsp.request_supertypes(&gone);
    rig.quiet("");
}
