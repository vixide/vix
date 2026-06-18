//! JSON-RPC envelope + LSP parameter builders, and parsers for the responses and
//! notifications Vix consumes (diagnostics, hover, definition, completion).
//!
//! Builders return [`serde_json::Value`] envelopes the host frames with
//! [`crate::lsp_core::frame::encode`]; parsers take the already-decoded `result`/`params`
//! value and extract a small, host-friendly shape.

#![warn(clippy::pedantic)]

use serde_json::{json, Value};

use crate::lsp_core::{CompletionItem, Diagnostic, Location, Position, Range, Severity};

// ----- envelopes ----------------------------------------------------------

/// A JSON-RPC request envelope (expects a response with the same `id`).
#[must_use]
pub fn request(id: i64, method: &str, params: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

/// A JSON-RPC notification envelope (no response).
#[must_use]
pub fn notification(method: &str, params: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

// ----- parameter builders -------------------------------------------------

/// `initialize` params, advertising the capabilities Vix supports.
#[must_use]
pub fn initialize_params(process_id: Option<u32>, root_uri: Option<&str>) -> Value {
    json!({
        "processId": process_id,
        "rootUri": root_uri,
        "clientInfo": { "name": "vix" },
        "capabilities": {
            "general": { "positionEncodings": ["utf-16", "utf-8"] },
            "textDocument": {
                "synchronization": { "dynamicRegistration": false, "didSave": false },
                "hover": { "contentFormat": ["markdown", "plaintext"] },
                "definition": { "linkSupport": true },
                "completion": {
                    "completionItem": {
                        "snippetSupport": false,
                        "documentationFormat": ["plaintext"]
                    }
                },
                "publishDiagnostics": { "relatedInformation": false }
            }
        }
    })
}

/// `textDocument/didOpen` params.
#[must_use]
pub fn did_open_params(uri: &str, language_id: &str, version: i64, text: &str) -> Value {
    json!({
        "textDocument": {
            "uri": uri,
            "languageId": language_id,
            "version": version,
            "text": text
        }
    })
}

/// `textDocument/didChange` params using full-document sync (one change covering
/// the whole text).
#[must_use]
pub fn did_change_full_params(uri: &str, version: i64, text: &str) -> Value {
    json!({
        "textDocument": { "uri": uri, "version": version },
        "contentChanges": [ { "text": text } ]
    })
}

/// `textDocument/didClose` params.
#[must_use]
pub fn did_close_params(uri: &str) -> Value {
    json!({ "textDocument": { "uri": uri } })
}

/// A `TextDocumentPositionParams` body (shared by hover/definition/completion/
/// implementation/typeDefinition/signatureHelp/documentHighlight).
#[must_use]
pub fn position_params(uri: &str, line: u32, character: u32) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character }
    })
}

/// A `ReferenceParams` body: a position plus whether to include the declaration.
#[must_use]
pub fn reference_params(uri: &str, line: u32, character: u32, include_declaration: bool) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character },
        "context": { "includeDeclaration": include_declaration }
    })
}

/// A `RenameParams` body: a position plus the desired new name.
#[must_use]
pub fn rename_params(uri: &str, line: u32, character: u32, new_name: &str) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character },
        "newName": new_name
    })
}

/// A `DocumentFormattingParams` body (spaces, `tab_size`-wide indentation).
#[must_use]
pub fn formatting_params(uri: &str, tab_size: u32) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "options": { "tabSize": tab_size, "insertSpaces": true }
    })
}

/// A `DocumentRangeFormattingParams` body for the given range.
#[must_use]
pub fn range_formatting_params(
    uri: &str,
    start: (u32, u32),
    end: (u32, u32),
    tab_size: u32,
) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "range": {
            "start": { "line": start.0, "character": start.1 },
            "end": { "line": end.0, "character": end.1 }
        },
        "options": { "tabSize": tab_size, "insertSpaces": true }
    })
}

/// A `DocumentSymbolParams` / text-document-only params body (also used for
/// `foldingRange`).
#[must_use]
pub fn text_document_params(uri: &str) -> Value {
    json!({ "textDocument": { "uri": uri } })
}

/// A `WorkspaceSymbolParams` body with the query string.
#[must_use]
pub fn workspace_symbol_params(query: &str) -> Value {
    json!({ "query": query })
}

/// A `DidSaveTextDocumentParams` body including the full saved text.
#[must_use]
pub fn did_save_params(uri: &str, text: &str) -> Value {
    json!({ "textDocument": { "uri": uri }, "text": text })
}

// ----- parsers ------------------------------------------------------------

/// The position encoding the server chose, read from an `initialize` result
/// (`capabilities.positionEncoding`). Defaults to UTF-16.
#[must_use]
pub fn parse_position_encoding(result: &Value) -> crate::lsp_core::Encoding {
    result
        .get("capabilities")
        .and_then(|c| c.get("positionEncoding"))
        .and_then(Value::as_str)
        .map_or(crate::lsp_core::Encoding::Utf16, crate::lsp_core::Encoding::from_lsp)
}

/// Parse a `textDocument/publishDiagnostics` notification into `(uri, diagnostics)`.
#[must_use]
pub fn parse_diagnostics(params: &Value) -> Option<(String, Vec<Diagnostic>)> {
    let uri = params.get("uri")?.as_str()?.to_string();
    let diags = params
        .get("diagnostics")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(parse_one_diagnostic).collect())
        .unwrap_or_default();
    Some((uri, diags))
}

fn parse_one_diagnostic(v: &Value) -> Option<Diagnostic> {
    Some(Diagnostic {
        range: parse_range(v.get("range")?)?,
        severity: v
            .get("severity")
            .and_then(Value::as_i64)
            .map_or(Severity::Error, Severity::from_lsp),
        message: v.get("message").and_then(Value::as_str).unwrap_or("").to_string(),
        source: v.get("source").and_then(Value::as_str).map(str::to_string),
    })
}

/// Extract the plain text of a `textDocument/hover` result, or `None` when empty.
#[must_use]
pub fn parse_hover(result: &Value) -> Option<String> {
    let contents = result.get("contents")?;
    let text = hover_contents_text(contents);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn hover_contents_text(contents: &Value) -> String {
    match contents {
        // A bare string, or a MarkupContent { kind, value }.
        Value::String(s) => s.clone(),
        Value::Object(o) => o
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        // MarkedString[] / mixed array: join each element's text.
        Value::Array(arr) => arr
            .iter()
            .map(hover_contents_text)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Parse a `textDocument/definition` result (`Location`, `Location[]`, or
/// `LocationLink[]`) into the first target location.
#[must_use]
pub fn parse_definition(result: &Value) -> Option<Location> {
    match result {
        Value::Object(_) => parse_location(result),
        Value::Array(arr) => arr.iter().find_map(parse_location),
        _ => None,
    }
}

/// Parse a `textDocument/formatting`/`rangeFormatting` result (`TextEdit[]`)
/// into `(range, new_text)` pairs, in document order.
#[must_use]
pub fn parse_text_edits(result: &Value) -> Vec<(crate::lsp_core::Range, String)> {
    let Value::Array(arr) = result else { return Vec::new() };
    arr.iter()
        .filter_map(|e| {
            let range = parse_range(e.get("range")?)?;
            let new_text = e.get("newText")?.as_str()?.to_string();
            Some((range, new_text))
        })
        .collect()
}

/// Parse a `textDocument/references`/`implementation`/`typeDefinition` result
/// (`Location`, `Location[]`, or `LocationLink[]`) into all target locations.
#[must_use]
pub fn parse_locations(result: &Value) -> Vec<Location> {
    match result {
        Value::Object(_) => parse_location(result).into_iter().collect(),
        Value::Array(arr) => arr.iter().filter_map(parse_location).collect(),
        _ => Vec::new(),
    }
}

fn parse_location(v: &Value) -> Option<Location> {
    // A LocationLink uses targetUri / targetSelectionRange; a Location uses
    // uri / range.
    if let Some(uri) = v.get("targetUri").and_then(Value::as_str) {
        let range = v
            .get("targetSelectionRange")
            .or_else(|| v.get("targetRange"))
            .and_then(parse_range)
            .unwrap_or_default();
        return Some(Location { uri: uri.to_string(), range });
    }
    let uri = v.get("uri")?.as_str()?.to_string();
    let range = v.get("range").and_then(parse_range).unwrap_or_default();
    Some(Location { uri, range })
}

/// Parse a `textDocument/completion` result (`CompletionItem[]` or a
/// `CompletionList` `{ items: [...] }`) into completion candidates.
#[must_use]
pub fn parse_completion(result: &Value) -> Vec<CompletionItem> {
    let items = match result {
        Value::Array(arr) => arr.as_slice(),
        Value::Object(o) => o.get("items").and_then(Value::as_array).map_or(&[][..], |a| a.as_slice()),
        _ => &[][..],
    };
    items.iter().filter_map(parse_completion_item).collect()
}

fn parse_completion_item(v: &Value) -> Option<CompletionItem> {
    let label = v.get("label")?.as_str()?.to_string();
    // Prefer an explicit textEdit/insertText; otherwise insert the label.
    let insert_text = v
        .get("textEdit")
        .and_then(|e| e.get("newText"))
        .and_then(Value::as_str)
        .or_else(|| v.get("insertText").and_then(Value::as_str))
        .unwrap_or(&label)
        .to_string();
    let detail = v.get("detail").and_then(Value::as_str).map(str::to_string);
    Some(CompletionItem { label, insert_text, detail })
}

fn parse_position(v: &Value) -> Option<Position> {
    Some(Position {
        line: u32::try_from(v.get("line")?.as_i64()?).ok()?,
        character: u32::try_from(v.get("character")?.as_i64()?).ok()?,
    })
}

fn parse_range(v: &Value) -> Option<Range> {
    Some(Range {
        start: parse_position(v.get("start")?)?,
        end: parse_position(v.get("end")?)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_and_notification_envelopes() {
        let req = request(7, "textDocument/hover", &json!({}));
        assert_eq!(req["id"], 7);
        assert_eq!(req["method"], "textDocument/hover");
        let note = notification("initialized", &json!({}));
        assert!(note.get("id").is_none());
    }

    #[test]
    fn diagnostics_round_trip() {
        let params = json!({
            "uri": "file:///x.rs",
            "diagnostics": [
                { "range": {"start": {"line": 1, "character": 2}, "end": {"line": 1, "character": 5}},
                  "severity": 1, "message": "boom", "source": "rustc" },
                { "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}},
                  "severity": 2, "message": "careful" }
            ]
        });
        let (uri, diags) = parse_diagnostics(&params).unwrap();
        assert_eq!(uri, "file:///x.rs");
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].message, "boom");
        assert_eq!(diags[0].range.start.line, 1);
        assert_eq!(diags[1].severity, Severity::Warning);
    }

    #[test]
    fn hover_handles_string_markup_and_array() {
        assert_eq!(parse_hover(&json!({"contents": "hi"})).unwrap(), "hi");
        assert_eq!(
            parse_hover(&json!({"contents": {"kind": "markdown", "value": "**x**"}})).unwrap(),
            "**x**"
        );
        assert_eq!(
            parse_hover(&json!({"contents": ["a", {"value": "b"}]})).unwrap(),
            "a\nb"
        );
        assert!(parse_hover(&json!({"contents": "   "})).is_none());
    }

    #[test]
    fn definition_handles_location_array_and_link() {
        let loc = parse_definition(&json!({
            "uri": "file:///a.rs",
            "range": {"start": {"line": 3, "character": 0}, "end": {"line": 3, "character": 4}}
        }))
        .unwrap();
        assert_eq!(loc.uri, "file:///a.rs");
        assert_eq!(loc.range.start.line, 3);

        let from_array = parse_definition(&json!([
            {"uri": "file:///b.rs", "range": {"start": {"line": 9, "character": 1}, "end": {"line": 9, "character": 2}}}
        ]))
        .unwrap();
        assert_eq!(from_array.uri, "file:///b.rs");

        let link = parse_definition(&json!([
            {"targetUri": "file:///c.rs",
             "targetSelectionRange": {"start": {"line": 2, "character": 2}, "end": {"line": 2, "character": 6}}}
        ]))
        .unwrap();
        assert_eq!(link.uri, "file:///c.rs");
        assert_eq!(link.range.start.character, 2);
    }

    #[test]
    fn completion_handles_list_and_array_and_insert_text() {
        let items = parse_completion(&json!({
            "items": [
                { "label": "push", "detail": "fn(self, T)" },
                { "label": "pop", "insertText": "pop()" },
                { "label": "len", "textEdit": { "newText": "len()", "range": {} } }
            ]
        }));
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].insert_text, "push", "falls back to the label");
        assert_eq!(items[1].insert_text, "pop()");
        assert_eq!(items[2].insert_text, "len()", "textEdit wins");
        assert_eq!(items[0].detail.as_deref(), Some("fn(self, T)"));
    }

    #[test]
    fn position_encoding_defaults_to_utf16() {
        assert_eq!(parse_position_encoding(&json!({})), crate::lsp_core::Encoding::Utf16);
        assert_eq!(
            parse_position_encoding(&json!({"capabilities": {"positionEncoding": "utf-8"}})),
            crate::lsp_core::Encoding::Utf8
        );
    }

    #[test]
    fn locations_parse_single_array_and_links() {
        let one = parse_locations(&json!({
            "uri": "file:///a.rs", "range": {"start": {"line": 1, "character": 2}, "end": {"line": 1, "character": 5}}
        }));
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].range.start.line, 1);
        let many = parse_locations(&json!([
            {"uri": "file:///a.rs", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}},
            {"targetUri": "file:///b.rs", "targetSelectionRange": {"start": {"line": 3, "character": 0}, "end": {"line": 3, "character": 4}}}
        ]));
        assert_eq!(many.len(), 2);
        assert_eq!(many[1].uri, "file:///b.rs");
        assert!(parse_locations(&Value::Null).is_empty());
    }

    #[test]
    fn text_edits_parse() {
        let edits = parse_text_edits(&json!([
            {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 3}}, "newText": "let"},
            {"range": {"start": {"line": 2, "character": 1}, "end": {"line": 2, "character": 1}}, "newText": "  "}
        ]));
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].1, "let");
        assert_eq!(edits[1].0.start.line, 2);
        assert!(parse_text_edits(&Value::Null).is_empty());
    }

    #[test]
    fn param_builders_shape() {
        assert_eq!(reference_params("u", 1, 2, true)["context"]["includeDeclaration"], json!(true));
        assert_eq!(rename_params("u", 1, 2, "x")["newName"], json!("x"));
        assert_eq!(workspace_symbol_params("foo")["query"], json!("foo"));
        assert_eq!(did_save_params("u", "hi")["text"], json!("hi"));
        assert_eq!(formatting_params("u", 4)["options"]["tabSize"], json!(4));
    }
}
