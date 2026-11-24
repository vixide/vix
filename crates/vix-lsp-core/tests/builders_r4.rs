//! Round-4 coverage: the pure JSON builders and the tolerant parsers of
//! `vix_lsp_core::message` and `file_ops`, asserted on exact output.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use vix_lsp_core::file_ops::{
    FileChangeType, FileOp, parse_file_operation, parse_registrations, parse_unregistrations,
};
use vix_lsp_core::message::*;

#[test]
fn document_lifecycle_params_carry_uri_version_and_text() {
    assert_eq!(
        did_open_params("file:///a.rs", "rust", 3, "fn x() {}"),
        json!({"textDocument": {
            "uri": "file:///a.rs", "languageId": "rust", "version": 3, "text": "fn x() {}"
        }})
    );
    assert_eq!(
        did_change_full_params("file:///a.rs", 4, "new"),
        json!({"textDocument": {"uri": "file:///a.rs", "version": 4},
               "contentChanges": [{"text": "new"}]})
    );
    assert_eq!(
        did_close_params("file:///a.rs"),
        json!({"textDocument": {"uri": "file:///a.rs"}})
    );
    assert_eq!(
        text_document_params("file:///a.rs"),
        json!({"textDocument": {"uri": "file:///a.rs"}})
    );
}

#[test]
fn range_based_params_nest_line_and_character() {
    let range = json!({"start": {"line": 1, "character": 2}, "end": {"line": 3, "character": 4}});
    assert_eq!(
        range_formatting_params("file:///a.rs", (1, 2), (3, 4), 8),
        json!({"textDocument": {"uri": "file:///a.rs"}, "range": range,
               "options": {"tabSize": 8, "insertSpaces": true}})
    );
    assert_eq!(
        inlay_hint_params("file:///a.rs", (1, 2), (3, 4)),
        json!({"textDocument": {"uri": "file:///a.rs"}, "range": range})
    );
    assert_eq!(
        selection_range_params("file:///a.rs", 5, 6),
        json!({"textDocument": {"uri": "file:///a.rs"},
               "positions": [{"line": 5, "character": 6}]})
    );
}

#[test]
fn command_and_hierarchy_params_wrap_their_payload() {
    assert_eq!(
        execute_command_params("do.it", &json!([1, "two"])),
        json!({"command": "do.it", "arguments": [1, "two"]})
    );
    assert_eq!(
        workspace_diagnostic_params(),
        json!({"previousResultIds": []})
    );
    assert_eq!(
        hierarchy_item_params(&json!({"name": "f"})),
        json!({"item": {"name": "f"}})
    );
}

#[test]
fn inlay_hints_pad_label_parts_and_drop_unusable_ones() {
    let hints = parse_inlay_hints(&json!([
        {"position": {"line": 0, "character": 4}, "label": "x", "paddingLeft": true, "paddingRight": true},
        {"position": {"line": 1, "character": 0},
         "label": [{"value": "a"}, {"value": "b"}, {"nope": 1}]},
        {"position": {"line": 2, "character": 0}, "label": 5},
        {"position": {"line": 3, "character": 0}, "label": ""},
        {"label": "no-position"}
    ]));
    assert_eq!(
        hints,
        vec![(0, 4, " x ".to_string()), (1, 0, "ab".to_string())]
    );
    assert_eq!(parse_inlay_hints(&json!(null)).len(), 0);
}

#[test]
fn resolved_detail_joins_detail_and_documentation_in_every_combination() {
    assert_eq!(
        parse_resolved_detail(&json!({"detail": "d", "documentation": "doc"})),
        Some("d\ndoc".to_string())
    );
    assert_eq!(
        parse_resolved_detail(&json!({"detail": "d"})),
        Some("d".to_string())
    );
    assert_eq!(
        parse_resolved_detail(&json!({"documentation": {"value": "md"}})),
        Some("md".to_string())
    );
    assert_eq!(parse_resolved_detail(&json!({})), None);
}

#[test]
fn hover_and_definition_tolerate_odd_shapes() {
    assert_eq!(parse_hover(&json!({"contents": 5})).unwrap_or_default(), "");
    let loc = parse_definition(&json!([
        {"targetUri": "file:///t.rs", "targetRange": {
            "start": {"line": 7, "character": 1}, "end": {"line": 7, "character": 3}}}
    ]))
    .expect("a LocationLink with only targetRange");
    assert_eq!(loc.uri, "file:///t.rs");
    assert_eq!((loc.range.start.line, loc.range.start.character), (7, 1));
    assert!(parse_definition(&json!("nonsense")).is_none());
    assert!(parse_definition(&json!(null)).is_none());
}

#[test]
fn document_symbols_accept_every_shape_and_visit_children_of_rangeless_items() {
    let result = json!([
        {"name": "outer", "children": [
            {"name": "inner", "range": {
                "start": {"line": 4, "character": 2}, "end": {"line": 5, "character": 0}}}
        ]},
        {"name": "flat", "location": {"uri": "file:///a", "range": {
            "start": {"line": 9, "character": 0}, "end": {"line": 9, "character": 4}}}},
        {"nameless": true}
    ]);
    assert_eq!(
        parse_document_symbols(&result),
        vec![(4, 2, "inner".to_string()), (9, 0, "flat".to_string())]
    );
    assert_eq!(parse_document_symbols(&json!({})).len(), 0);
}

#[test]
fn incoming_calls_fall_back_from_from_ranges_to_the_callers_own_range() {
    let r =
        |l: u64| json!({"start": {"line": l, "character": 1}, "end": {"line": l, "character": 2}});
    let calls = parse_incoming_calls(&json!([
        {"from": {"uri": "file:///a", "selectionRange": r(1)}, "fromRanges": [r(10)]},
        {"from": {"uri": "file:///b", "selectionRange": r(2)}, "fromRanges": []},
        {"from": {"uri": "file:///c", "range": r(3)}},
        {"from": {"uri": "file:///d"}},
        {"from": {"name": "no-uri"}},
        {"nofrom": true}
    ]));
    let got: Vec<_> = calls
        .iter()
        .map(|l| (l.uri.as_str(), l.range.start.line))
        .collect();
    assert_eq!(
        got,
        vec![
            ("file:///a", 10),
            ("file:///b", 2),
            ("file:///c", 3),
            ("file:///d", 0)
        ]
    );
    assert!(parse_incoming_calls(&json!({})).is_empty());
}

#[test]
fn completion_results_may_be_arrays_lists_or_garbage() {
    let none = parse_completion(&json!("garbage"));
    assert!(!none.is_incomplete);
    let empty_list = parse_completion(&json!({"isIncomplete": true}));
    assert!(empty_list.is_incomplete);
}

#[test]
fn file_operation_filters_honour_scheme_kind_case_and_bad_globs() {
    let init = json!({"capabilities": {"workspace": {"fileOperations": {
        "didRename": {"filters": [
            {"scheme": "ftp", "pattern": {"glob": "**/*.rs"}},
            {"pattern": {"matches": "file", "glob": "**/*.RS", "options": {"ignoreCase": true}}},
            {"pattern": {"matches": "folder", "glob": "src"}},
            {"pattern": {"glob": "[unclosed"}},
            {"scheme": "file"},
            {"pattern": {}}
        ]},
        "willRename": {"filters": [{"scheme": "ftp", "pattern": {"glob": "**"}}]},
        "didCreate": {"filters": []},
        "willCreate": null
    }}}});
    let roots = vec![PathBuf::from("/w")];
    let f = parse_file_operation(&init, "didRename").unwrap();
    assert!(f.matches(Path::new("/w/a/b.rs"), false, &roots));
    assert!(
        !f.matches(Path::new("/w/a/b.rs"), true, &roots),
        "files only"
    );
    assert!(
        f.matches(Path::new("/w/src"), true, &roots),
        "root-relative folder glob"
    );
    assert!(!f.matches(Path::new("/w/src"), false, &roots));
    // Only non-file schemes: the operation is offered but matches nothing.
    let none = parse_file_operation(&init, "willRename").unwrap();
    assert!(!none.matches(Path::new("/w/x"), false, &roots));
    // Named without filters: matches everything.
    let all = parse_file_operation(&init, "didCreate").unwrap();
    assert!(all.matches(Path::new("/anything"), false, &roots));
    assert!(parse_file_operation(&init, "willCreate").is_none());
    assert!(parse_file_operation(&init, "absent").is_none());
    assert!(parse_file_operation(&json!({}), "didRename").is_none());
}

#[test]
fn file_op_and_change_type_tables() {
    assert_eq!(FileChangeType::Created.code(), 1);
    assert_eq!(FileChangeType::Changed.code(), 2);
    assert_eq!(FileChangeType::Deleted.code(), 3);
    assert_eq!(FileChangeType::Created.watch_bit(), 1);
    assert_eq!(FileChangeType::Changed.watch_bit(), 2);
    assert_eq!(FileChangeType::Deleted.watch_bit(), 4);
    assert_eq!(FileOp::Create.will_method(), "workspace/willCreateFiles");
    assert_eq!(FileOp::Rename.will_method(), "workspace/willRenameFiles");
    assert_eq!(FileOp::Delete.will_method(), "workspace/willDeleteFiles");
    assert_eq!(FileOp::Create.did_method(), "workspace/didCreateFiles");
    assert_eq!(FileOp::Rename.did_method(), "workspace/didRenameFiles");
    assert_eq!(FileOp::Delete.did_method(), "workspace/didDeleteFiles");
}

#[test]
fn registrations_and_unregistrations_skip_malformed_entries() {
    let regs = parse_registrations(&json!({"registrations": [
        {"id": "1", "method": "workspace/didChangeWatchedFiles",
         "registerOptions": {"watchers": [{"globPattern": "**/*.rs"}]}},
        {"id": "2"},
        {"method": "x"}
    ]}));
    assert_ne!(regs.len(), 0);
    assert_eq!(
        parse_unregistrations(&json!({"unregisterations": [{"id": "1"}, {"nope": 1}]})),
        vec!["1".to_string()]
    );
    assert_eq!(parse_registrations(&Value::Null).len(), 0);
}

#[test]
fn severity_maps_the_lsp_numbers_and_defaults_to_error() {
    use vix_lsp_core::Severity;
    assert_eq!(Severity::from_lsp(2), Severity::Warning);
    assert_eq!(Severity::from_lsp(3), Severity::Information);
    assert_eq!(Severity::from_lsp(4), Severity::Hint);
    assert_eq!(Severity::from_lsp(1), Severity::Error);
    assert_eq!(Severity::from_lsp(0), Severity::Error);
    assert_eq!(Severity::from_lsp(99), Severity::Error);
}

#[test]
fn a_plain_completion_item_has_only_label_and_insert_text() {
    use vix_lsp_core::CompletionItem;
    let item = CompletionItem::new("Alice", "alice@example.org");
    assert_eq!(item.label, "Alice");
    assert_eq!(item.insert_text, "alice@example.org");
    assert_eq!(item.filter_key(), "Alice");
    assert_eq!(item.commit_chars.len(), 0);
    assert_eq!(item.insert_text_format, None);
    assert!(!item.is_snippet());
}

#[cfg(unix)]
#[test]
fn native_uris_round_trip_a_path_with_spaces() {
    use vix_lsp_core::uri::{file_uri_native, uri_path_native};
    let path = Path::new("/tmp/with space/é.rs");
    let uri = file_uri_native(path);
    assert_eq!(uri, "file:///tmp/with%20space/%C3%A9.rs");
    assert_eq!(uri_path_native(&uri), Some(PathBuf::from(path)));
    assert_eq!(uri_path_native("http://example.org/x"), None);
}
