//! JSON-RPC envelope + LSP parameter builders, and parsers for the responses and
//! notifications Vix consumes (diagnostics, hover, definition, completion).
//!
//! Builders return [`serde_json::Value`] envelopes the host frames with
//! [`crate::lsp_core::frame::encode`]; parsers take the already-decoded `result`/`params`
//! value and extract a small, host-friendly shape.

#![warn(clippy::pedantic)]

use serde_json::{json, Value};

use crate::lsp_core::{CompletionItem, Diagnostic, Location, Position, Range, Severity};

/// One file's text edits within a workspace edit: `(uri, [(range, new_text)])`.
pub type UriEdits = (String, Vec<(Range, String)>);
/// One offered code action: `(title, [per-file edits])`.
pub type CodeActionEdit = (String, Vec<UriEdits>);

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

/// An `InlayHintParams` body covering `[start, end)` of the document.
#[must_use]
pub fn inlay_hint_params(uri: &str, start: (u32, u32), end: (u32, u32)) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "range": {
            "start": { "line": start.0, "character": start.1 },
            "end": { "line": end.0, "character": end.1 }
        }
    })
}

// ----- parsers ------------------------------------------------------------

/// Parse a `textDocument/inlayHint` result (`InlayHint[]`) into
/// `(line, character, label)`. The label may be a string or label parts;
/// `paddingLeft`/`paddingRight` become surrounding spaces.
#[must_use]
pub fn parse_inlay_hints(result: &Value) -> Vec<(u32, u32, String)> {
    let Value::Array(arr) = result else { return Vec::new() };
    arr.iter()
        .filter_map(|h| {
            let pos = h.get("position")?;
            let line = u32::try_from(pos.get("line")?.as_u64()?).ok()?;
            let character = u32::try_from(pos.get("character")?.as_u64()?).ok()?;
            let mut label = match h.get("label")? {
                Value::String(s) => s.clone(),
                Value::Array(parts) => {
                    parts.iter().filter_map(|p| p.get("value").and_then(Value::as_str)).collect()
                }
                _ => return None,
            };
            if h.get("paddingLeft").and_then(Value::as_bool) == Some(true) {
                label.insert(0, ' ');
            }
            if h.get("paddingRight").and_then(Value::as_bool) == Some(true) {
                label.push(' ');
            }
            (!label.is_empty()).then_some((line, character, label))
        })
        .collect()
}

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

/// A `completionItem/resolve` params body: the item to resolve (`label`, plus
/// the opaque `data` the server round-trips).
#[must_use]
pub fn completion_resolve_params(label: &str, data: Option<&Value>) -> Value {
    let mut item = json!({ "label": label });
    if let Some(d) = data {
        item["data"] = d.clone();
    }
    item
}

/// Parse a `completionItem/resolve` result into a one-block detail string
/// (`detail`, then `documentation`), or `None` when it adds nothing.
#[must_use]
pub fn parse_resolved_detail(result: &Value) -> Option<String> {
    let detail = result.get("detail").and_then(Value::as_str);
    let documentation = result.get("documentation").and_then(|d| {
        d.as_str().or_else(|| d.get("value").and_then(Value::as_str))
    });
    match (detail, documentation) {
        (Some(d), Some(doc)) => Some(format!("{d}\n{doc}")),
        (Some(d), None) => Some(d.to_string()),
        (None, Some(doc)) => Some(doc.to_string()),
        (None, None) => None,
    }
}

/// Parse a `textDocument/foldingRange` result (`FoldingRange[]`) into
/// `(start_line, end_line)` pairs (0-based), keeping only multi-line ranges.
#[must_use]
pub fn parse_folding_ranges(result: &Value) -> Vec<(u32, u32)> {
    let Value::Array(arr) = result else { return Vec::new() };
    arr.iter()
        .filter_map(|r| {
            let start = u32::try_from(r.get("startLine")?.as_u64()?).ok()?;
            let end = u32::try_from(r.get("endLine")?.as_u64()?).ok()?;
            (end > start).then_some((start, end))
        })
        .collect()
}

/// Parse a `textDocument/linkedEditingRange` result into the ranges that should
/// be edited together (from the `ranges` array).
#[must_use]
pub fn parse_linked_editing_ranges(result: &Value) -> Vec<Range> {
    let Some(arr) = result.get("ranges").and_then(Value::as_array) else { return Vec::new() };
    arr.iter().filter_map(parse_range).collect()
}

/// Parse a `textDocument/documentHighlight` result (`DocumentHighlight[]`) into
/// the ranges to highlight.
#[must_use]
pub fn parse_document_highlights(result: &Value) -> Vec<Range> {
    let Value::Array(arr) = result else { return Vec::new() };
    arr.iter().filter_map(|h| h.get("range").and_then(parse_range)).collect()
}

/// A `SelectionRangeParams` body querying the single position `(line, character)`.
#[must_use]
pub fn selection_range_params(uri: &str, line: u32, character: u32) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "positions": [{ "line": line, "character": character }]
    })
}

/// Parse a `textDocument/selectionRange` result into the chain of ranges for the
/// first requested position, innermost first (following `parent` links).
#[must_use]
pub fn parse_selection_ranges(result: &Value) -> Vec<Range> {
    let mut ranges = Vec::new();
    let mut node = result.as_array().and_then(|a| a.first());
    while let Some(n) = node {
        if let Some(r) = n.get("range").and_then(parse_range) {
            ranges.push(r);
        }
        node = n.get("parent");
    }
    ranges
}

/// A `CodeActionParams` body for `[start, end)` with the overlapping
/// `diagnostics` (raw LSP objects) in the request context.
#[must_use]
pub fn code_action_params(uri: &str, start: (u32, u32), end: (u32, u32), diagnostics: &Value) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "range": {
            "start": { "line": start.0, "character": start.1 },
            "end": { "line": end.0, "character": end.1 }
        },
        "context": { "diagnostics": diagnostics }
    })
}

/// Parse a `textDocument/codeAction` result (`(Command | CodeAction)[]`) into
/// `(title, workspace_edit)` pairs. Actions that carry only a command (no inline
/// edit) yield an empty edit list.
#[must_use]
pub fn parse_code_actions(result: &Value) -> Vec<CodeActionEdit> {
    let Value::Array(arr) = result else { return Vec::new() };
    arr.iter()
        .filter_map(|item| {
            let title = item.get("title")?.as_str()?.to_string();
            let edit = item.get("edit").map(parse_workspace_edit).unwrap_or_default();
            Some((title, edit))
        })
        .collect()
}

/// Parse a `textDocument/rename` `WorkspaceEdit` result into per-file edits:
/// `(uri, [(range, new_text)])`. Handles both the `changes` map and the
/// `documentChanges` array shapes.
#[must_use]
pub fn parse_workspace_edit(result: &Value) -> Vec<UriEdits> {
    let mut out = Vec::new();
    if let Some(Value::Object(changes)) = result.get("changes") {
        for (uri, edits) in changes {
            out.push((uri.clone(), parse_text_edits(edits)));
        }
    }
    if let Some(Value::Array(doc_changes)) = result.get("documentChanges") {
        for dc in doc_changes {
            if let Some(uri) = dc.get("textDocument").and_then(|td| td.get("uri")).and_then(Value::as_str)
                && let Some(edits) = dc.get("edits")
            {
                out.push((uri.to_string(), parse_text_edits(edits)));
            }
        }
    }
    out.retain(|(_, edits)| !edits.is_empty());
    out
}

/// Parse a `textDocument/documentSymbol` result into `(line, character, name)`
/// in document order. Handles both the hierarchical `DocumentSymbol[]` (with
/// nested `children`) and the flat `SymbolInformation[]` shapes.
#[must_use]
pub fn parse_document_symbols(result: &Value) -> Vec<(u32, u32, String)> {
    let mut out = Vec::new();
    if let Value::Array(arr) = result {
        for item in arr {
            collect_symbol(item, &mut out);
        }
    }
    out
}

fn collect_symbol(item: &Value, out: &mut Vec<(u32, u32, String)>) {
    let Some(name) = item.get("name").and_then(Value::as_str) else { return };
    // DocumentSymbol uses `selectionRange`/`range`; SymbolInformation nests under
    // `location.range`.
    let range = item
        .get("selectionRange")
        .or_else(|| item.get("range"))
        .or_else(|| item.get("location").and_then(|l| l.get("range")))
        .and_then(parse_range);
    if let Some(range) = range {
        out.push((range.start.line, range.start.character, name.to_string()));
    }
    if let Some(Value::Array(children)) = item.get("children") {
        for child in children {
            collect_symbol(child, out);
        }
    }
}

/// Parse a `workspace/symbol` result (`SymbolInformation[]` / `WorkspaceSymbol[]`)
/// into `(uri, line, character, name)`.
#[must_use]
pub fn parse_workspace_symbols(result: &Value) -> Vec<(String, u32, u32, String)> {
    let Value::Array(arr) = result else { return Vec::new() };
    arr.iter()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.to_string();
            let location = item.get("location")?;
            let uri = location.get("uri")?.as_str()?.to_string();
            let range = location.get("range").and_then(parse_range).unwrap_or_default();
            Some((uri, range.start.line, range.start.character, name))
        })
        .collect()
}

/// Parse a `textDocument/signatureHelp` result into a one-line summary of the
/// active signature (with the active parameter, when reported).
#[must_use]
pub fn parse_signature_help(result: &Value) -> Option<String> {
    let sigs = result.get("signatures")?.as_array()?;
    if sigs.is_empty() {
        return None;
    }
    let active = result
        .get("activeSignature")
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0);
    let sig = sigs.get(active).or_else(|| sigs.first())?;
    let label = sig.get("label")?.as_str()?.to_string();
    let active_param = sig
        .get("activeParameter")
        .or_else(|| result.get("activeParameter"))
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok());
    if let Some(p) = active_param
        && let Some(param) = sig.get("parameters").and_then(Value::as_array).and_then(|ps| ps.get(p))
        && let Some(plabel) = param.get("label").and_then(Value::as_str)
    {
        return Some(format!("{label}\n→ {plabel}"));
    }
    Some(label)
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
    let data = v.get("data").cloned();
    Some(CompletionItem { label, insert_text, detail, data })
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
    fn document_symbols_flatten_children() {
        let syms = parse_document_symbols(&json!([
            {"name": "Foo", "kind": 5,
             "range": {"start": {"line": 0, "character": 0}, "end": {"line": 9, "character": 0}},
             "selectionRange": {"start": {"line": 0, "character": 6}, "end": {"line": 0, "character": 9}},
             "children": [
                {"name": "bar", "kind": 6,
                 "selectionRange": {"start": {"line": 1, "character": 4}, "end": {"line": 1, "character": 7}}}
             ]}
        ]));
        assert_eq!(syms.len(), 2);
        assert_eq!(syms[0], (0, 6, "Foo".to_string()));
        assert_eq!(syms[1], (1, 4, "bar".to_string()));
    }

    #[test]
    fn workspace_symbols_and_signature_help() {
        let ws = parse_workspace_symbols(&json!([
            {"name": "main", "kind": 12,
             "location": {"uri": "file:///m.rs", "range": {"start": {"line": 3, "character": 3}, "end": {"line": 3, "character": 7}}}}
        ]));
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].0, "file:///m.rs");
        assert_eq!(ws[0].3, "main");
        let help = parse_signature_help(&json!({
            "signatures": [{"label": "fn f(a: i32, b: i32)", "parameters": [{"label": "a: i32"}, {"label": "b: i32"}]}],
            "activeSignature": 0, "activeParameter": 1
        }));
        let help = help.unwrap();
        assert!(help.contains("fn f(a: i32, b: i32)"));
        assert!(help.contains("b: i32"));
        assert!(parse_signature_help(&json!({"signatures": []})).is_none());
    }

    #[test]
    fn completion_resolve_roundtrip() {
        let params = completion_resolve_params("push", Some(&json!({"id": 7})));
        assert_eq!(params["label"], json!("push"));
        assert_eq!(params["data"], json!({"id": 7}));
        assert!(completion_resolve_params("x", None).get("data").is_none());

        assert_eq!(
            parse_resolved_detail(&json!({"detail": "fn push(&mut self, T)", "documentation": "Appends."})),
            Some("fn push(&mut self, T)\nAppends.".to_string())
        );
        assert_eq!(
            parse_resolved_detail(&json!({"documentation": {"kind": "markdown", "value": "docs"}})),
            Some("docs".to_string())
        );
        assert!(parse_resolved_detail(&json!({})).is_none());
    }

    #[test]
    fn inlay_hints_parse_string_and_parts() {
        let hints = parse_inlay_hints(&json!([
            {"position": {"line": 0, "character": 5}, "label": ": i32", "paddingLeft": true},
            {"position": {"line": 2, "character": 1}, "label": [{"value": "name"}, {"value": ":"}]}
        ]));
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0], (0, 5, " : i32".to_string()), "paddingLeft adds a space");
        assert_eq!(hints[1], (2, 1, "name:".to_string()), "label parts joined");
        assert!(parse_inlay_hints(&Value::Null).is_empty());
    }

    #[test]
    fn folding_ranges_parse_multiline_only() {
        let fr = parse_folding_ranges(&json!([
            {"startLine": 0, "endLine": 4, "kind": "region"},
            {"startLine": 7, "endLine": 7},
            {"startLine": 9, "endLine": 12}
        ]));
        assert_eq!(fr, vec![(0, 4), (9, 12)], "single-line range dropped");
        assert!(parse_folding_ranges(&Value::Null).is_empty());
    }

    #[test]
    fn linked_editing_ranges_parse() {
        let ranges = parse_linked_editing_ranges(&json!({
            "ranges": [
                {"start": {"line": 1, "character": 1}, "end": {"line": 1, "character": 4}},
                {"start": {"line": 5, "character": 2}, "end": {"line": 5, "character": 5}}
            ],
            "wordPattern": "[a-z]+"
        }));
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[1].start.line, 5);
        assert!(parse_linked_editing_ranges(&json!({})).is_empty());
    }

    #[test]
    fn document_highlights_parse_ranges() {
        let hs = parse_document_highlights(&json!([
            {"range": {"start": {"line": 0, "character": 4}, "end": {"line": 0, "character": 7}}, "kind": 1},
            {"range": {"start": {"line": 3, "character": 0}, "end": {"line": 3, "character": 3}}}
        ]));
        assert_eq!(hs.len(), 2);
        assert_eq!(hs[1].start.line, 3);
        assert!(parse_document_highlights(&Value::Null).is_empty());
    }

    #[test]
    fn selection_ranges_follow_parent_chain() {
        let ranges = parse_selection_ranges(&json!([
            {"range": {"start": {"line": 1, "character": 4}, "end": {"line": 1, "character": 7}},
             "parent": {"range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 12}},
                        "parent": {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 5, "character": 0}}}}}
        ]));
        assert_eq!(ranges.len(), 3, "innermost, middle, outermost");
        assert_eq!(ranges[0].start.character, 4);
        assert_eq!(ranges[2].end.line, 5);
        assert!(parse_selection_ranges(&Value::Null).is_empty());
    }

    #[test]
    fn code_actions_parse_titles_and_edits() {
        let actions = parse_code_actions(&json!([
            {"title": "Import Foo", "kind": "quickfix",
             "edit": {"changes": {"file:///a.rs": [
                 {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}, "newText": "use foo;\n"}
             ]}}},
            {"title": "Run command only", "command": {"command": "x", "arguments": []}}
        ]));
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].0, "Import Foo");
        assert_eq!(actions[0].1.len(), 1, "first action has an edit");
        assert!(actions[1].1.is_empty(), "command-only action has no edit");
        assert!(parse_code_actions(&Value::Null).is_empty());
    }

    #[test]
    fn workspace_edit_parses_both_shapes() {
        let by_changes = parse_workspace_edit(&json!({
            "changes": {
                "file:///a.rs": [{"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 3}}, "newText": "baz"}]
            }
        }));
        assert_eq!(by_changes.len(), 1);
        assert_eq!(by_changes[0].0, "file:///a.rs");
        assert_eq!(by_changes[0].1[0].1, "baz");
        let by_doc = parse_workspace_edit(&json!({
            "documentChanges": [
                {"textDocument": {"uri": "file:///b.rs", "version": 2},
                 "edits": [{"range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 2}}, "newText": "x"}]}
            ]
        }));
        assert_eq!(by_doc.len(), 1);
        assert_eq!(by_doc[0].0, "file:///b.rs");
        assert!(parse_workspace_edit(&json!({})).is_empty());
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
