//! JSON-RPC envelope + LSP parameter builders, and parsers for the responses and
//! notifications Vix consumes (diagnostics, hover, definition, completion).
//!
//! Builders return [`serde_json::Value`] envelopes the host frames with
//! [`crate::frame::encode`]; parsers take the already-decoded `result`/`params`
//! value and extract a small, host-friendly shape.

#![warn(clippy::pedantic)]

use serde_json::{Value, json};

use crate::{CompletionItem, Diagnostic, Location, Position, Range, SemanticToken, Severity};

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
///
/// `folders` is every workspace folder open at spawn time, as `(uri, name)`
/// pairs (T123f) -- `rootUri` (kept for servers that predate `workspaceFolders`,
/// LSP 3.6) is the first folder's URI, or `null` when there are none;
/// `workspaceFolders` carries the full list, or `null` when empty (per spec, not
/// an empty array).
#[must_use]
pub fn initialize_params(process_id: Option<u32>, folders: &[(String, String)]) -> Value {
    let root_uri = folders.first().map(|(uri, _)| uri.as_str());
    let workspace_folders = if folders.is_empty() {
        Value::Null
    } else {
        Value::Array(
            folders
                .iter()
                .map(|(uri, name)| json!({ "uri": uri, "name": name }))
                .collect(),
        )
    };
    json!({
        "processId": process_id,
        "rootUri": root_uri,
        "workspaceFolders": workspace_folders,
        "clientInfo": { "name": "vix" },
        "capabilities": {
            "general": { "positionEncodings": ["utf-16", "utf-8"] },
            "textDocument": {
                // `didSave` really is sent (`Lsp::did_save`) -- this used to
                // (wrongly) claim otherwise (T123 audit).
                "synchronization": { "dynamicRegistration": false, "didSave": true },
                "hover": { "contentFormat": ["markdown", "plaintext"] },
                "definition": { "linkSupport": true },
                "declaration": { "linkSupport": true },
                "typeDefinition": { "linkSupport": true },
                "implementation": { "linkSupport": true },
                "references": {},
                "documentHighlight": {},
                // The document-symbol parser already keeps a response's
                // nested `children`, so this is accurate, not aspirational.
                "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
                "completion": {
                    "completionItem": {
                        "snippetSupport": false,
                        "documentationFormat": ["plaintext"]
                    }
                },
                "signatureHelp": {},
                "codeAction": {},
                "codeLens": {},
                "formatting": {},
                "rangeFormatting": {},
                "rename": { "prepareSupport": true },
                "foldingRange": {},
                "selectionRange": {},
                "linkedEditingRange": {},
                "callHierarchy": {},
                "inlayHint": {},
                "publishDiagnostics": { "relatedInformation": true },
                // T123d: opts into pull-based `textDocument/diagnostic` /
                // `workspace/diagnostic`, alongside push
                // (`publishDiagnostics`) — some servers only enable pull
                // support for clients that declare this.
                "diagnostic": {},
                // T123a: only `full` (whole-document) requests -- no
                // `range` or `full.delta` in v1. `tokenTypes`/
                // `tokenModifiers` list the LSP 3.17 standard set; per
                // spec a server may still report types/modifiers outside
                // it (its own `semanticTokensProvider.legend` is
                // authoritative regardless), so this is advisory, not a
                // hard filter — `parse_semantic_tokens` decodes against
                // whatever legend the server actually sent.
                "semanticTokens": {
                    "requests": { "full": true },
                    "tokenTypes": [
                        "namespace", "type", "class", "enum", "interface",
                        "struct", "typeParameter", "parameter", "variable",
                        "property", "enumMember", "event", "function",
                        "method", "macro", "keyword", "modifier", "comment",
                        "string", "number", "regexp", "operator", "decorator"
                    ],
                    "tokenModifiers": [
                        "declaration", "definition", "readonly", "static",
                        "deprecated", "abstract", "async", "modification",
                        "documentation", "defaultLibrary"
                    ],
                    "formats": ["relative"]
                }
            },
            "workspace": {
                "applyEdit": true,
                "symbol": {},
                "executeCommand": {},
                // T123d: no `previousResultIds` tracking in v1, so the
                // server never needs to ask the client to discard cached
                // reports.
                "diagnostics": { "refreshSupport": false },
                // T123f: Vix understands `workspaceFolders` and will send
                // `workspace/didChangeWorkspaceFolders` when the server's own
                // response asks for it (`changeNotifications`).
                "workspaceFolders": true
            },
            // T123d: `$/progress` (e.g. an initial index build) is only
            // sent to clients that declare support for it.
            "window": { "workDoneProgress": true }
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
/// `foldingRange` and `semanticTokens/full`, T123a).
#[must_use]
pub fn text_document_params(uri: &str) -> Value {
    json!({ "textDocument": { "uri": uri } })
}

/// A `workspace/executeCommand` params body.
#[must_use]
pub fn execute_command_params(command: &str, arguments: &Value) -> Value {
    json!({ "command": command, "arguments": arguments })
}

/// A `WorkspaceSymbolParams` body with the query string.
#[must_use]
pub fn workspace_symbol_params(query: &str) -> Value {
    json!({ "query": query })
}

/// A `WorkspaceDiagnosticParams` body (T123d): always requests a fresh, full
/// report — v1 tracks no `previousResultIds`, so there is nothing to ask the
/// server to skip re-sending.
#[must_use]
pub fn workspace_diagnostic_params() -> Value {
    json!({ "previousResultIds": [] })
}

/// A `DidChangeWorkspaceFoldersParams` body (T123f): the folders newly added
/// and/or removed, as `(uri, name)` pairs -- sent only to a server whose own
/// `initialize` response asked for it (`parse_workspace_folders_change_support`).
#[must_use]
pub fn did_change_workspace_folders_params(
    added: &[(String, String)],
    removed: &[(String, String)],
) -> Value {
    let to_json = |folders: &[(String, String)]| -> Value {
        Value::Array(
            folders
                .iter()
                .map(|(uri, name)| json!({ "uri": uri, "name": name }))
                .collect(),
        )
    };
    json!({ "event": { "added": to_json(added), "removed": to_json(removed) } })
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
    let Value::Array(arr) = result else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|h| {
            let pos = h.get("position")?;
            let line = u32::try_from(pos.get("line")?.as_u64()?).ok()?;
            let character = u32::try_from(pos.get("character")?.as_u64()?).ok()?;
            let mut label = match h.get("label")? {
                Value::String(s) => s.clone(),
                Value::Array(parts) => parts
                    .iter()
                    .filter_map(|p| p.get("value").and_then(Value::as_str))
                    .collect(),
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
pub fn parse_position_encoding(result: &Value) -> crate::Encoding {
    result
        .get("capabilities")
        .and_then(|c| c.get("positionEncoding"))
        .and_then(Value::as_str)
        .map_or(crate::Encoding::Utf16, crate::Encoding::from_lsp)
}

/// The server's semantic-token type legend, read from an `initialize`
/// result (`capabilities.semanticTokensProvider.legend.tokenTypes`, T123a) —
/// the index each token in a later `semanticTokens/full` response names by
/// number resolves against this list. Empty when the server doesn't
/// advertise semantic tokens support at all.
#[must_use]
pub fn parse_semantic_tokens_legend(result: &Value) -> Vec<String> {
    result
        .get("capabilities")
        .and_then(|c| c.get("semanticTokensProvider"))
        .and_then(|p| p.get("legend"))
        .and_then(|l| l.get("tokenTypes"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether the server's `initialize` response asked to be told about
/// workspace-folder changes (T123f): `capabilities.workspace.
/// workspaceFolders.changeNotifications`, truthy as either `true` or a
/// (dynamic-registration id) string -- both mean "yes, send me
/// `workspace/didChangeWorkspaceFolders`". A server that omits the whole
/// `workspaceFolders` object, or sets `changeNotifications` to `false` or
/// leaves it unset, gets none.
#[must_use]
pub fn parse_workspace_folders_change_support(result: &Value) -> bool {
    let Some(notifications) = result
        .get("capabilities")
        .and_then(|c| c.get("workspace"))
        .and_then(|w| w.get("workspaceFolders"))
        .and_then(|wf| wf.get("changeNotifications"))
    else {
        return false;
    };
    notifications.as_bool() == Some(true) || notifications.is_string()
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
        message: v
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        source: v.get("source").and_then(Value::as_str).map(str::to_string),
        related: parse_related_information(v),
    })
}

/// A diagnostic's `relatedInformation`: secondary `(location, message)`
/// pairs (e.g. "previous definition here" for a duplicate-symbol error).
/// Empty when the field is absent, not an array, or every entry fails to
/// parse.
fn parse_related_information(diagnostic: &Value) -> Vec<(Location, String)> {
    diagnostic
        .get("relatedInformation")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let location = parse_location(entry.get("location")?)?;
                    let message = entry.get("message").and_then(Value::as_str)?.to_string();
                    Some((location, message))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse a `workspace/diagnostic` result's `items` into `(uri, diagnostics)`
/// pairs (T123d) — merges the same way a `(uri, diagnostics)` pair from
/// push-based `textDocument/publishDiagnostics` does. A
/// `WorkspaceUnchangedDocumentDiagnosticReport` (`kind: "unchanged"`) is
/// skipped: v1 tracks no `previousResultIds`, so the server never actually
/// sends one, but skipping it explicitly is still correct if one ever
/// arrives — there is nothing to update.
#[must_use]
pub fn parse_workspace_diagnostics(result: &Value) -> Vec<(String, Vec<Diagnostic>)> {
    result
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let uri = item.get("uri").and_then(Value::as_str)?.to_string();
                    if item.get("kind").and_then(Value::as_str) != Some("full") {
                        return None;
                    }
                    let diags = item
                        .get("items")
                        .and_then(Value::as_array)
                        .map(|arr| arr.iter().filter_map(parse_one_diagnostic).collect())
                        .unwrap_or_default();
                    Some((uri, diags))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Decode a `textDocument/semanticTokens/full` result's delta-encoded
/// `data` array (T123a) into absolute [`SemanticToken`]s, resolved against
/// `legend` (the server's own `tokenTypes` list from `initialize`, via
/// [`parse_semantic_tokens_legend`]).
///
/// LSP encodes each token as 5 integers relative to the *previous* token:
/// `[deltaLine, deltaStartChar, length, tokenType, tokenModifiers]` —
/// `deltaStartChar` is relative to the previous token's start only when
/// `deltaLine` is 0 (same line); otherwise it's absolute from the new
/// line's start. Modifiers are deliberately not decoded in v1 (see
/// [`SemanticToken`]). A token whose type index is out of range for
/// `legend`, or a trailing group of fewer than 5 integers, is skipped
/// rather than treated as fatal to the rest.
#[must_use]
pub fn parse_semantic_tokens(result: &Value, legend: &[String]) -> Vec<SemanticToken> {
    let Some(data) = result.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    let nums: Vec<u32> = data
        .iter()
        .filter_map(|v| v.as_u64().and_then(|n| u32::try_from(n).ok()))
        .collect();
    let mut tokens = Vec::new();
    let mut line = 0_u32;
    let mut character = 0_u32;
    for &[delta_line, delta_start, length, token_type_idx, _modifiers] in nums.as_chunks::<5>().0 {
        if delta_line == 0 {
            character += delta_start;
        } else {
            line += delta_line;
            character = delta_start;
        }
        if let Some(token_type) = legend.get(token_type_idx as usize) {
            tokens.push(SemanticToken {
                line,
                character,
                length,
                token_type: token_type.clone(),
            });
        }
    }
    tokens
}

/// Parse a `$/progress` notification's `params` into a one-line status
/// message (T123d), or `None` for an `"end"` report (nothing more to show)
/// or a payload with neither a title nor a message.
#[must_use]
pub fn parse_progress(params: &Value) -> Option<String> {
    let value = params.get("value")?;
    if value.get("kind").and_then(Value::as_str) == Some("end") {
        return None;
    }
    let title = value.get("title").and_then(Value::as_str);
    let message = value.get("message").and_then(Value::as_str);
    let percentage = value.get("percentage").and_then(Value::as_u64);
    let mut out = match (title, message) {
        (Some(t), Some(m)) => format!("{t}: {m}"),
        (Some(t), None) => t.to_string(),
        (None, Some(m)) => m.to_string(),
        (None, None) => return None,
    };
    if let Some(p) = percentage {
        out = format!("{out} ({p}%)");
    }
    Some(out)
}

/// Parse a `textDocument/prepareRename` result into a placeholder decision.
///
/// - `None` — the server says this position cannot be renamed (a bare
///   `null` response, or `{"defaultBehavior": false}`).
/// - `Some(None)` — renameable, but the server left the placeholder text to
///   the client's own default (a bare `Range`, or
///   `{"defaultBehavior": true}`).
/// - `Some(Some(text))` — renameable, with the server's own placeholder
///   text (a `{"range": ..., "placeholder": "..."}` response).
#[must_use]
pub fn parse_prepare_rename(result: &Value) -> Option<Option<String>> {
    if result.is_null() {
        return None;
    }
    if let Some(default_behavior) = result.get("defaultBehavior").and_then(Value::as_bool) {
        return default_behavior.then_some(None);
    }
    if let Some(placeholder) = result.get("placeholder").and_then(Value::as_str) {
        return Some(Some(placeholder.to_string()));
    }
    // A bare `Range` (just `start`/`end`, no placeholder) -- still
    // renameable, no explicit placeholder text.
    Some(None)
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
        d.as_str()
            .or_else(|| d.get("value").and_then(Value::as_str))
    });
    match (detail, documentation) {
        (Some(d), Some(doc)) => Some(format!("{d}\n{doc}")),
        (Some(d), None) => Some(d.to_string()),
        (None, Some(doc)) => Some(doc.to_string()),
        (None, None) => None,
    }
}

/// One resolved code lens: `(line, title, command, arguments)`.
pub type CodeLens = (u32, String, String, Value);

/// Parse a `textDocument/codeLens` result (`CodeLens[]`) into the lenses that
/// carry an invokable command: `(line, title, command, arguments)`.
#[must_use]
pub fn parse_code_lenses(result: &Value) -> Vec<CodeLens> {
    let Value::Array(arr) = result else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|lens| {
            let line =
                u32::try_from(lens.get("range")?.get("start")?.get("line")?.as_u64()?).ok()?;
            let cmd = lens.get("command")?;
            let title = cmd.get("title")?.as_str()?.to_string();
            let command = cmd.get("command")?.as_str()?.to_string();
            let arguments = cmd
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Array(Vec::new()));
            Some((line, title, command, arguments))
        })
        .collect()
}

/// Parse the `WorkspaceEdit` from a server-initiated `workspace/applyEdit`
/// request's params (`{ edit: WorkspaceEdit, label? }`).
#[must_use]
pub fn parse_apply_edit(params: &Value) -> Vec<UriEdits> {
    params
        .get("edit")
        .map(parse_workspace_edit)
        .unwrap_or_default()
}

/// Parse a `textDocument/foldingRange` result (`FoldingRange[]`) into
/// `(start_line, end_line)` pairs (0-based), keeping only multi-line ranges.
#[must_use]
pub fn parse_folding_ranges(result: &Value) -> Vec<(u32, u32)> {
    let Value::Array(arr) = result else {
        return Vec::new();
    };
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
    let Some(arr) = result.get("ranges").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_range).collect()
}

/// Parse a `textDocument/documentHighlight` result (`DocumentHighlight[]`) into
/// the ranges to highlight.
#[must_use]
pub fn parse_document_highlights(result: &Value) -> Vec<Range> {
    let Value::Array(arr) = result else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|h| h.get("range").and_then(parse_range))
        .collect()
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
pub fn code_action_params(
    uri: &str,
    start: (u32, u32),
    end: (u32, u32),
    diagnostics: &Value,
) -> Value {
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
    let Value::Array(arr) = result else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let title = item.get("title")?.as_str()?.to_string();
            let edit = item
                .get("edit")
                .map(parse_workspace_edit)
                .unwrap_or_default();
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
            if let Some(uri) = dc
                .get("textDocument")
                .and_then(|td| td.get("uri"))
                .and_then(Value::as_str)
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
    let Some(name) = item.get("name").and_then(Value::as_str) else {
        return;
    };
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
    let Value::Array(arr) = result else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.to_string();
            let location = item.get("location")?;
            let uri = location.get("uri")?.as_str()?.to_string();
            let range = location
                .get("range")
                .and_then(parse_range)
                .unwrap_or_default();
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
        && let Some(param) = sig
            .get("parameters")
            .and_then(Value::as_array)
            .and_then(|ps| ps.get(p))
        && let Some(plabel) = param.get("label").and_then(Value::as_str)
    {
        return Some(format!("{label}\n→ {plabel}"));
    }
    Some(label)
}

/// Parse a `textDocument/formatting`/`rangeFormatting` result (`TextEdit[]`)
/// into `(range, new_text)` pairs, in document order.
#[must_use]
pub fn parse_text_edits(result: &Value) -> Vec<(crate::Range, String)> {
    let Value::Array(arr) = result else {
        return Vec::new();
    };
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
        return Some(Location {
            uri: uri.to_string(),
            range,
        });
    }
    let uri = v.get("uri")?.as_str()?.to_string();
    let range = v.get("range").and_then(parse_range).unwrap_or_default();
    Some(Location { uri, range })
}

/// The first item of a `textDocument/prepareCallHierarchy` result (the symbol to
/// query calls for), cloned for round-tripping to `callHierarchy/incomingCalls`.
#[must_use]
pub fn first_call_hierarchy_item(result: &Value) -> Option<Value> {
    result.as_array()?.first().cloned()
}

/// Parse a `callHierarchy/incomingCalls` result into the call-site locations:
/// each caller's `fromRanges` (falling back to its `selectionRange`/`range`).
#[must_use]
pub fn parse_incoming_calls(result: &Value) -> Vec<Location> {
    let Some(arr) = result.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for call in arr {
        let Some(from) = call.get("from") else {
            continue;
        };
        let Some(uri) = from.get("uri").and_then(Value::as_str) else {
            continue;
        };
        let range = call
            .get("fromRanges")
            .and_then(Value::as_array)
            .and_then(|r| r.first())
            .and_then(parse_range)
            .or_else(|| from.get("selectionRange").and_then(parse_range))
            .or_else(|| from.get("range").and_then(parse_range))
            .unwrap_or_default();
        out.push(Location {
            uri: uri.to_string(),
            range,
        });
    }
    out
}

/// Parse a `textDocument/completion` result (`CompletionItem[]` or a
/// `CompletionList` `{ items: [...] }`) into completion candidates.
#[must_use]
pub fn parse_completion(result: &Value) -> Vec<CompletionItem> {
    let items = match result {
        Value::Array(arr) => arr.as_slice(),
        Value::Object(o) => o
            .get("items")
            .and_then(Value::as_array)
            .map_or(&[][..], |a| a.as_slice()),
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
    Some(CompletionItem {
        label,
        insert_text,
        detail,
        data,
    })
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
    fn incoming_calls_parse_to_call_sites() {
        let prep = json!([{ "name": "f", "uri": "file:///a.rs", "kind": 12,
            "range": {"start":{"line":1,"character":0},"end":{"line":1,"character":1}},
            "selectionRange": {"start":{"line":1,"character":3},"end":{"line":1,"character":4}} }]);
        assert!(first_call_hierarchy_item(&prep).is_some());
        assert!(first_call_hierarchy_item(&Value::Null).is_none());

        let incoming = json!([{
            "from": { "name": "caller", "uri": "file:///b.rs", "kind": 12,
                "range": {"start":{"line":9,"character":0},"end":{"line":9,"character":1}},
                "selectionRange": {"start":{"line":9,"character":0},"end":{"line":9,"character":6}} },
            "fromRanges": [ {"start":{"line":5,"character":4},"end":{"line":5,"character":9}} ]
        }]);
        let calls = parse_incoming_calls(&incoming);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].uri, "file:///b.rs");
        assert_eq!(calls[0].range.start.line, 5, "uses the call-site fromRange");
        assert_eq!(calls[0].range.start.character, 4);
    }

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
        assert!(diags[0].related.is_empty());
        assert!(diags[1].related.is_empty());
    }

    #[test]
    fn diagnostics_carry_related_information() {
        let params = json!({
            "uri": "file:///x.rs",
            "diagnostics": [{
                "range": {"start": {"line": 1, "character": 2}, "end": {"line": 1, "character": 5}},
                "severity": 1, "message": "duplicate definition",
                "relatedInformation": [
                    { "location": { "uri": "file:///y.rs",
                        "range": {"start": {"line": 4, "character": 0}, "end": {"line": 4, "character": 3}} },
                      "message": "previous definition here" },
                    // A malformed entry (no `message`) is skipped, not fatal to the rest.
                    { "location": { "uri": "file:///z.rs",
                        "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}} } }
                ]
            }]
        });
        let (_, diags) = parse_diagnostics(&params).unwrap();
        assert_eq!(diags[0].related.len(), 1);
        assert_eq!(diags[0].related[0].0.uri, "file:///y.rs");
        assert_eq!(diags[0].related[0].0.range.start.line, 4);
        assert_eq!(diags[0].related[0].1, "previous definition here");
    }

    #[test]
    fn prepare_rename_distinguishes_the_three_outcomes() {
        assert_eq!(
            parse_prepare_rename(&Value::Null),
            None,
            "null: not renameable"
        );
        assert_eq!(
            parse_prepare_rename(&json!({"defaultBehavior": false})),
            None
        );
        assert_eq!(
            parse_prepare_rename(&json!({"defaultBehavior": true})),
            Some(None),
            "renameable, client picks its own placeholder"
        );
        assert_eq!(
            parse_prepare_rename(
                &json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 3}})
            ),
            Some(None),
            "a bare Range: still renameable, no explicit placeholder"
        );
        assert_eq!(
            parse_prepare_rename(&json!({
                "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 3}},
                "placeholder": "foo"
            })),
            Some(Some("foo".to_string()))
        );
    }

    #[test]
    fn capabilities_declare_prepare_rename_and_related_information() {
        let params = initialize_params(None, &[]);
        let text_document = &params["capabilities"]["textDocument"];
        assert_eq!(text_document["rename"]["prepareSupport"], true);
        assert_eq!(
            text_document["publishDiagnostics"]["relatedInformation"],
            true
        );
    }

    #[test]
    fn capabilities_declare_pull_diagnostics_and_progress() {
        let params = initialize_params(None, &[]);
        assert_eq!(
            params["capabilities"]["textDocument"]["diagnostic"],
            json!({})
        );
        assert_eq!(params["capabilities"]["window"]["workDoneProgress"], true);
    }

    #[test]
    fn initialize_params_with_no_folders_sends_null_root_and_folders() {
        let params = initialize_params(None, &[]);
        assert_eq!(params["rootUri"], Value::Null);
        assert_eq!(params["workspaceFolders"], Value::Null);
        assert_eq!(
            params["capabilities"]["workspace"]["workspaceFolders"],
            true
        );
    }

    #[test]
    fn initialize_params_sends_every_folder_and_the_first_as_root() {
        let folders = vec![
            ("file:///a".to_string(), "a".to_string()),
            ("file:///b".to_string(), "b".to_string()),
        ];
        let params = initialize_params(None, &folders);
        assert_eq!(params["rootUri"], "file:///a");
        assert_eq!(
            params["workspaceFolders"],
            json!([
                {"uri": "file:///a", "name": "a"},
                {"uri": "file:///b", "name": "b"}
            ])
        );
    }

    #[test]
    fn workspace_folders_change_support_reads_change_notifications() {
        assert!(!parse_workspace_folders_change_support(&json!({})));
        assert!(!parse_workspace_folders_change_support(&json!({
            "capabilities": {}
        })));
        assert!(!parse_workspace_folders_change_support(&json!({
            "capabilities": {"workspace": {"workspaceFolders": {"supported": true}}}
        })));
        assert!(!parse_workspace_folders_change_support(&json!({
            "capabilities": {"workspace": {"workspaceFolders": {
                "supported": true, "changeNotifications": false
            }}}
        })));
        assert!(parse_workspace_folders_change_support(&json!({
            "capabilities": {"workspace": {"workspaceFolders": {
                "supported": true, "changeNotifications": true
            }}}
        })));
        assert!(
            parse_workspace_folders_change_support(&json!({
                "capabilities": {"workspace": {"workspaceFolders": {
                    "supported": true, "changeNotifications": "some-registration-id"
                }}}
            })),
            "a registration-id string also means yes, per spec"
        );
    }

    #[test]
    fn did_change_workspace_folders_params_names_added_and_removed() {
        let params = did_change_workspace_folders_params(
            &[("file:///new".to_string(), "new".to_string())],
            &[("file:///old".to_string(), "old".to_string())],
        );
        assert_eq!(
            params,
            json!({
                "event": {
                    "added": [{"uri": "file:///new", "name": "new"}],
                    "removed": [{"uri": "file:///old", "name": "old"}]
                }
            })
        );
    }

    #[test]
    fn workspace_diagnostics_parse_full_reports_and_skip_unchanged() {
        let result = json!({
            "items": [
                { "uri": "file:///a.rs", "kind": "full", "items": [
                    { "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}},
                      "severity": 1, "message": "boom" }
                ]},
                { "uri": "file:///b.rs", "kind": "unchanged", "resultId": "abc" },
                { "uri": "file:///c.rs", "kind": "full", "items": [] }
            ]
        });
        let pairs = parse_workspace_diagnostics(&result);
        assert_eq!(pairs.len(), 2, "the unchanged report is skipped");
        assert_eq!(pairs[0].0, "file:///a.rs");
        assert_eq!(pairs[0].1.len(), 1);
        assert_eq!(pairs[0].1[0].message, "boom");
        assert_eq!(pairs[1].0, "file:///c.rs");
        assert!(pairs[1].1.is_empty());
    }

    #[test]
    fn progress_formats_title_message_and_percentage() {
        assert_eq!(
            parse_progress(&json!({"value": {"kind": "begin", "title": "Indexing"}})),
            Some("Indexing".to_string())
        );
        assert_eq!(
            parse_progress(&json!({"value": {"kind": "report", "message": "3/10 crates"}})),
            Some("3/10 crates".to_string())
        );
        assert_eq!(
            parse_progress(&json!({"value": {
                "kind": "report", "title": "Indexing", "message": "3/10 crates", "percentage": 30
            }})),
            Some("Indexing: 3/10 crates (30%)".to_string())
        );
        assert_eq!(
            parse_progress(&json!({"value": {"kind": "end"}})),
            None,
            "an end report has nothing left to show"
        );
        assert_eq!(parse_progress(&json!({"value": {"kind": "begin"}})), None);
    }

    #[test]
    fn semantic_tokens_legend_reads_token_types() {
        let result = json!({"capabilities": {"semanticTokensProvider": {
            "legend": {"tokenTypes": ["variable", "function"], "tokenModifiers": []},
            "full": true
        }}});
        assert_eq!(
            parse_semantic_tokens_legend(&result),
            vec!["variable".to_string(), "function".to_string()]
        );
        assert!(parse_semantic_tokens_legend(&json!({})).is_empty());
    }

    #[test]
    fn semantic_tokens_decode_relative_deltas() {
        let legend = vec!["variable".to_string(), "function".to_string()];
        // Token 1: line 2, char 5, length 3, type 1 ("function"), no modifiers.
        // Token 2: same line (deltaLine 0), char 5+4=9, length 6, type 0 ("variable").
        // Token 3: deltaLine 1 -> line 3, char reset to the given deltaStartChar 0.
        let result = json!({"data": [
            2, 5, 3, 1, 0,
            0, 4, 6, 0, 0,
            1, 0, 4, 0, 0
        ]});
        let tokens = parse_semantic_tokens(&result, &legend);
        assert_eq!(tokens.len(), 3);
        assert_eq!(
            tokens[0],
            SemanticToken {
                line: 2,
                character: 5,
                length: 3,
                token_type: "function".into()
            }
        );
        assert_eq!(
            tokens[1],
            SemanticToken {
                line: 2,
                character: 9,
                length: 6,
                token_type: "variable".into()
            }
        );
        assert_eq!(
            tokens[2],
            SemanticToken {
                line: 3,
                character: 0,
                length: 4,
                token_type: "variable".into()
            }
        );
    }

    #[test]
    fn semantic_tokens_skip_out_of_range_types_and_malformed_data() {
        let legend = vec!["variable".to_string()];
        // Type index 5 doesn't exist in a 1-element legend -- skipped.
        let result = json!({"data": [0, 0, 3, 5, 0]});
        assert!(parse_semantic_tokens(&result, &legend).is_empty());
        assert!(parse_semantic_tokens(&json!({}), &legend).is_empty());
    }

    #[test]
    fn capabilities_declare_semantic_tokens_full_requests() {
        let params = initialize_params(None, &[]);
        let semantic = &params["capabilities"]["textDocument"]["semanticTokens"];
        assert_eq!(semantic["requests"]["full"], true);
        assert!(
            semantic["tokenTypes"]
                .as_array()
                .unwrap()
                .contains(&json!("function"))
        );
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
        assert_eq!(parse_position_encoding(&json!({})), crate::Encoding::Utf16);
        assert_eq!(
            parse_position_encoding(&json!({"capabilities": {"positionEncoding": "utf-8"}})),
            crate::Encoding::Utf8
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
            parse_resolved_detail(
                &json!({"detail": "fn push(&mut self, T)", "documentation": "Appends."})
            ),
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
        assert_eq!(
            hints[0],
            (0, 5, " : i32".to_string()),
            "paddingLeft adds a space"
        );
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
    fn code_lenses_parse_commands() {
        let lenses = parse_code_lenses(&json!([
            {"range": {"start": {"line": 3, "character": 0}, "end": {"line": 3, "character": 1}},
             "command": {"title": "2 references", "command": "rust-analyzer.showReferences", "arguments": [1, 2]}},
            {"range": {"start": {"line": 9, "character": 0}, "end": {"line": 9, "character": 1}}}
        ]));
        assert_eq!(lenses.len(), 1, "only the lens with a command");
        assert_eq!(lenses[0].0, 3);
        assert_eq!(lenses[0].1, "2 references");
        assert_eq!(lenses[0].2, "rust-analyzer.showReferences");
        // workspace/applyEdit extraction.
        let edits = parse_apply_edit(&json!({
            "edit": {"changes": {"file:///x.rs": [
                {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}, "newText": "// "}
            ]}}
        }));
        assert_eq!(edits.len(), 1);
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
        assert_eq!(
            reference_params("u", 1, 2, true)["context"]["includeDeclaration"],
            json!(true)
        );
        assert_eq!(rename_params("u", 1, 2, "x")["newName"], json!("x"));
        assert_eq!(workspace_symbol_params("foo")["query"], json!("foo"));
        assert_eq!(did_save_params("u", "hi")["text"], json!("hi"));
        assert_eq!(formatting_params("u", 4)["options"]["tabSize"], json!(4));
    }
}
