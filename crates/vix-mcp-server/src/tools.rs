//! The six read-only tools: their `tools/list` descriptions and their
//! implementations over the workspace and the [`Host`].

use crate::Host;
use crate::workspace::{self, read_text, relative};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Tool name: the files Vix last had open in this workspace.
pub const LIST_OPEN_BUFFERS: &str = "list_open_buffers";
/// Tool name: one file's text.
pub const READ_BUFFER: &str = "read_buffer";
/// Tool name: a file's declaration-style symbols.
pub const DOCUMENT_SYMBOLS: &str = "document_symbols";
/// Tool name: a file's heading or symbol outline.
pub const OUTLINE: &str = "outline";
/// Tool name: offline diagnostics.
pub const DIAGNOSTICS: &str = "diagnostics";
/// Tool name: SQLite schemas in the workspace.
pub const DB_SCHEMA: &str = "db_schema";

/// The `tools/list` result: every tool with its input schema and read-only
/// annotations.
#[must_use]
pub fn list() -> Value {
    let path_arg = |required: bool| {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path, relative to the workspace root (or absolute inside it)."
                }
            },
            "additionalProperties": false
        });
        if required {
            schema["required"] = json!(["path"]);
        }
        schema
    };
    let no_args = json!({"type": "object", "properties": {}, "additionalProperties": false});
    let tool = |name: &str, title: &str, description: &str, schema: Value| {
        json!({
            "name": name,
            "title": title,
            "description": description,
            "inputSchema": schema,
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        })
    };
    json!({"tools": [
        tool(LIST_OPEN_BUFFERS, "List open buffers",
             "The files Vix last had open in this workspace (from its saved session), as workspace-relative paths.",
             no_args.clone()),
        tool(READ_BUFFER, "Read buffer",
             "The saved text of one file in the workspace. Secret files (.env, keys, .git/) are refused.",
             path_arg(true)),
        tool(DOCUMENT_SYMBOLS, "Document symbols",
             "Declaration-style symbols (kind, name, 1-based line) from Vix's offline symbol scan; no language server.",
             path_arg(true)),
        tool(OUTLINE, "Outline",
             "The file's outline: Markdown headings, Org headlines, or (for code) its symbols, each with a level and 1-based line.",
             path_arg(true)),
        tool(DIAGNOSTICS, "Diagnostics",
             "Offline diagnostics (unresolved merge-conflict blocks) for one file, or every open buffer when path is omitted. Language-server diagnostics are not available in this headless mode.",
             path_arg(false)),
        tool(DB_SCHEMA, "Database schema",
             "Tables, views and columns of the SQLite databases inside the workspace, opened read-only. Server databases needing passwords are never opened.",
             no_args),
    ]})
}

/// Run tool `name` with `args`, returning its structured result or a
/// tool-level failure message.
///
/// # Errors
///
/// Returns a message for an unknown tool, a missing `path`, or anything the
/// workspace boundary refuses.
pub fn call<H: Host>(root: &Path, host: &H, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        LIST_OPEN_BUFFERS => Ok(json!({
            "root": root.to_string_lossy(),
            "buffers": open_buffers(root, host)
                .iter()
                .map(|p| relative(root, p))
                .collect::<Vec<_>>(),
        })),
        READ_BUFFER => {
            let (path, text) = read_text(root, required_path(args)?)?;
            Ok(json!({"path": path, "text": text}))
        }
        DOCUMENT_SYMBOLS => {
            let (path, text) = read_text(root, required_path(args)?)?;
            let symbols: Vec<Value> = vix_palette::symbols(&text)
                .into_iter()
                .map(|s| json!({"kind": s.kind, "name": s.name, "line": s.line}))
                .collect();
            Ok(json!({"path": path, "symbols": symbols}))
        }
        OUTLINE => {
            let (path, text) = read_text(root, required_path(args)?)?;
            let (format, entries) = outline(&path, &text);
            Ok(json!({"path": path, "format": format, "entries": entries}))
        }
        DIAGNOSTICS => diagnostics(root, host, args),
        DB_SCHEMA => Ok(db_schema(root, host)),
        other => Err(format!("unknown tool: {other}")),
    }
}

/// The `path` argument, required.
fn required_path(args: &Value) -> Result<&str, String> {
    args.get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing string argument: path".to_string())
}

/// The host's open buffers, canonicalized, deduplicated, and filtered to
/// readable non-secret files inside `root`, in the host's order.
pub(crate) fn open_buffers<H: Host>(root: &Path, host: &H) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for path in host.open_buffers(root) {
        let Some(request) = path.to_str() else {
            continue;
        };
        if let Ok(canonical) = workspace::resolve(root, request)
            && !out.contains(&canonical)
        {
            out.push(canonical);
        }
    }
    out
}

/// The outline of the file at workspace-relative `path`: its format name
/// and `{level, title, line}` entries.
fn outline(path: &str, text: &str) -> (&'static str, Vec<Value>) {
    let extension = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match extension.as_str() {
        "md" | "markdown" => {
            let rendered = vix_markdown_preview::render_full(text);
            let entries = rendered
                .toc
                .iter()
                .map(|e| {
                    // `line` is the heading's row in the rendered preview;
                    // map it back to its source line.
                    let line = rendered
                        .source_lines
                        .get(e.line.saturating_sub(1))
                        .copied()
                        .unwrap_or(e.line);
                    json!({"level": e.kind.len(), "title": e.name, "line": line})
                })
                .collect();
            ("markdown", entries)
        }
        "org" => {
            let entries = vix_org::headlines(text)
                .into_iter()
                .map(|(line, level, title)| json!({"level": level, "title": title, "line": line + 1}))
                .collect();
            ("org", entries)
        }
        _ => {
            let entries = vix_palette::symbols(text)
                .into_iter()
                .map(|s| {
                    let title = if s.kind.is_empty() {
                        s.name
                    } else {
                        format!("{} {}", s.kind, s.name)
                    };
                    json!({"level": 1, "title": title, "line": s.line})
                })
                .collect();
            ("symbols", entries)
        }
    }
}

/// The `diagnostics` tool: merge-conflict blocks in one file, or in every
/// open buffer when `path` is absent.
fn diagnostics<H: Host>(root: &Path, host: &H, args: &Value) -> Result<Value, String> {
    let requests: Vec<String> = match args.get("path") {
        Some(Value::String(p)) => vec![p.clone()],
        Some(Value::Null) | None => open_buffers(root, host)
            .iter()
            .map(|p| relative(root, p))
            .collect(),
        Some(_) => return Err("argument path must be a string".into()),
    };
    let explicit = matches!(args.get("path"), Some(Value::String(_)));
    let mut found = Vec::new();
    for request in &requests {
        // An explicitly named file that cannot be read is the caller's
        // error; an open buffer that became unreadable is just skipped.
        let (path, text) = match read_text(root, request) {
            Ok(read) => read,
            Err(e) if explicit => return Err(e),
            Err(_) => continue,
        };
        for conflict in vix_conflict_tool::find_all(&text) {
            found.push(json!({
                "path": path,
                "line": conflict.start + 1,
                "end_line": conflict.end,
                "severity": "error",
                "source": "vix-conflict",
                "message": "unresolved merge conflict",
            }));
        }
    }
    Ok(json!({"language_server": "not_running", "diagnostics": found}))
}

/// The `db_schema` tool: each SQLite database's tables, or why it could not
/// be read.
fn db_schema<H: Host>(root: &Path, host: &H) -> Value {
    let mut databases = Vec::new();
    for path in workspace::find_sqlite(root) {
        let rel = relative(root, &path);
        match host.sqlite_schema(&path) {
            None => {
                return json!({
                    "databases": [],
                    "error": "this vix build has no `sql` capability",
                });
            }
            Some(Ok(tables)) => {
                let tables: Vec<Value> = tables
                    .into_iter()
                    .map(|t| {
                        json!({
                            "name": t.name,
                            "kind": t.kind,
                            "columns": t.columns.into_iter()
                                .map(|c| json!({"name": c.name, "type": c.data_type}))
                                .collect::<Vec<_>>(),
                        })
                    })
                    .collect();
                databases.push(json!({"path": rel, "tables": tables}));
            }
            Some(Err(error)) => databases.push(json!({"path": rel, "error": error})),
        }
    }
    json!({"databases": databases})
}
