//! The ten read-only tools: their `tools/list` descriptions and their
//! implementations over the workspace and the [`Host`].
//!
//! The four `db_*` connection tools (T783) only shape JSON: the [`Host`]
//! decides which saved connections are shared, checks statements, logs every
//! call and opens connections read-only.
//!
//! Every tool is read-only and advertises `readOnlyHint: true`; every file
//! access goes through [`crate::workspace`], so confinement to the root and
//! the secret-file refusal apply uniformly. [`list`] and [`call`] are kept
//! in step by hand: a name added to one must be added to the other (the
//! server's `tools/call` rejects names absent from [`list`] before [`call`]
//! ever sees them). Owning spec: `crates/vix-mcp-server/spec/index.md` and
//! the repo-level `spec/mcp-server`.

use crate::workspace::{self, WorkspaceError, read_text, relative};
use crate::{DbDescription, DbTables, Host, HostError, Rows, SharedConnection};
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
/// Tool name: the saved DB connections the user shared (T783).
pub const DB_CONNECTIONS: &str = "db_connections";
/// Tool name: the tables of a shared connection's schema.
pub const DB_TABLES: &str = "db_tables";
/// Tool name: one table of a shared connection.
pub const DB_DESCRIBE: &str = "db_describe";
/// Tool name: one read-only statement on a shared connection.
pub const DB_QUERY: &str = "db_query";

/// The most rows `db_query` returns (the host enforces it; reported in the
/// result as `row_limit`).
pub const DB_QUERY_ROW_LIMIT: usize = 100;

/// The `tools/list` result: every tool with its input schema and read-only
/// annotations.
#[must_use]
pub fn list() -> Value {
    // Builders for the repeated JSON Schema shapes. `additionalProperties:
    // false` makes clients reject misspelled arguments instead of silently
    // ignoring them.
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
    // The DB connection tools: a required `connection` plus the given
    // string arguments (`required` lists which of them are required too).
    let db_args = |extra: &[(&str, &str)], required: &[&str]| {
        let mut properties = serde_json::Map::new();
        properties.insert(
            "connection".into(),
            json!({"type": "string", "description": "A connection name from db_connections."}),
        );
        for (name, description) in extra {
            properties.insert(
                (*name).to_string(),
                json!({"type": "string", "description": description}),
            );
        }
        let mut all = vec!["connection"];
        all.extend_from_slice(required);
        json!({
            "type": "object",
            "properties": properties,
            "required": all,
            "additionalProperties": false
        })
    };
    let schema_arg = (
        "schema",
        "Schema name; omitted, the default (main for SQLite, public for PostgreSQL, the current database for MySQL).",
    );
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
             "Tables, views and columns of the SQLite databases inside the workspace, opened read-only.",
             no_args.clone()),
        tool(DB_CONNECTIONS, "Shared database connections",
             "The saved Vix database connections the user chose to share with MCP clients: name, kind, environment and database (never host, user or password). Every DB tool call is logged.",
             no_args),
        tool(DB_TABLES, "Database tables",
             "Tables, views and routines in one schema of a shared connection, and the names of its other schemas. Read-only.",
             db_args(&[schema_arg], &[])),
        tool(DB_DESCRIBE, "Describe table",
             "Columns, primary key, indexes, foreign keys and constraints of one table or view on a shared connection, as the engine's catalog reports them. Read-only.",
             db_args(&[schema_arg, ("table", "Table or view name.")], &["table"])),
        tool(DB_QUERY, "Query database",
             "Run one read-only statement (SELECT, WITH, EXPLAIN without ANALYZE, SHOW or VALUES) on a shared connection, inside a read-only transaction with a 30-second timeout that is rolled back. Returns at most 100 rows as text, with the column names, and says when rows were cut off.",
             db_args(&[("sql", "One SQL statement.")], &["sql"])),
    ]})
}

/// Why a tool call failed (T610).
///
/// `Display` text matches the earlier `String` errors; it becomes the MCP
/// result's error text.
#[derive(Debug)]
#[non_exhaustive]
pub enum ToolError {
    /// No tool has this name.
    UnknownTool(String),
    /// A required `path` argument is absent or not a string.
    MissingPath,
    /// An optional `path` argument is present but not a string.
    PathNotString,
    /// The workspace boundary refused the path, or the file could not be read.
    Workspace(WorkspaceError),
    /// A required string argument (named) is absent or not a string.
    MissingArgument(&'static str),
    /// An optional argument (named) is present but not a string.
    NotAString(&'static str),
    /// This build has no database support (the `sql` capability).
    NoSqlCapability,
    /// The host failed (a refused statement, an unshared connection, a
    /// database error); its `Display` text is the message.
    Host(HostError),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownTool(name) => write!(f, "unknown tool: {name}"),
            Self::MissingPath => f.write_str("missing string argument: path"),
            Self::PathNotString => f.write_str("argument path must be a string"),
            Self::Workspace(e) => e.fmt(f),
            Self::MissingArgument(name) => write!(f, "missing string argument: {name}"),
            Self::NotAString(name) => write!(f, "argument {name} must be a string"),
            Self::NoSqlCapability => f.write_str("this vix build has no `sql` capability"),
            Self::Host(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ToolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Workspace(e) => Some(e),
            Self::Host(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl From<WorkspaceError> for ToolError {
    fn from(e: WorkspaceError) -> Self {
        Self::Workspace(e)
    }
}

/// Run tool `name` with `args`, returning its structured result or a
/// tool-level failure.
///
/// # Errors
///
/// Returns a [`ToolError`] for an unknown tool, a missing `path`, or anything
/// the workspace boundary refuses.
pub fn call<H: Host>(root: &Path, host: &H, name: &str, args: &Value) -> Result<Value, ToolError> {
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
        DB_CONNECTIONS => db_connections(host),
        DB_TABLES => {
            let connection = required(args, "connection")?;
            let schema = optional(args, "schema")?;
            let tables = host_result(host.db_tables(connection, schema))?;
            Ok(tables_json(connection, &tables))
        }
        DB_DESCRIBE => {
            let connection = required(args, "connection")?;
            let table = required(args, "table")?;
            let schema = optional(args, "schema")?;
            let description = host_result(host.db_describe(connection, schema, table))?;
            Ok(describe_json(connection, &description))
        }
        DB_QUERY => {
            let connection = required(args, "connection")?;
            let sql = required(args, "sql")?;
            let rows = host_result(host.db_query(connection, sql))?;
            Ok(query_json(connection, &rows))
        }
        other => Err(ToolError::UnknownTool(other.to_string())),
    }
}

/// The string argument `name`, required.
fn required<'a>(args: &'a Value, name: &'static str) -> Result<&'a str, ToolError> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or(ToolError::MissingArgument(name))
}

/// The string argument `name`, optional (`null` counts as absent).
fn optional<'a>(args: &'a Value, name: &'static str) -> Result<Option<&'a str>, ToolError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(ToolError::NotAString(name)),
    }
}

/// A host answer: `None` is a build without database support.
fn host_result<T>(answer: Option<Result<T, HostError>>) -> Result<T, ToolError> {
    match answer {
        None => Err(ToolError::NoSqlCapability),
        Some(result) => result.map_err(ToolError::Host),
    }
}

/// The `db_connections` tool. Without database support the list is empty
/// and says why, like `db_schema`.
fn db_connections<H: Host>(host: &H) -> Result<Value, ToolError> {
    match host.shared_connections() {
        None => Ok(json!({
            "connections": [],
            "error": ToolError::NoSqlCapability.to_string(),
        })),
        Some(result) => Ok(connections_json(&result.map_err(ToolError::Host)?)),
    }
}

/// `db_connections` JSON: `{connections: [{name, kind, environment, database}]}`.
#[must_use]
pub fn connections_json(connections: &[SharedConnection]) -> Value {
    json!({"connections": connections.iter().map(|c| json!({
        "name": c.name,
        "kind": c.kind,
        "environment": c.environment,
        "database": c.database,
    })).collect::<Vec<_>>()})
}

/// One result set as `{columns, rows, truncated}`.
#[must_use]
pub fn rows_json(rows: &Rows) -> Value {
    json!({"columns": rows.columns, "rows": rows.rows, "truncated": rows.truncated})
}

/// `db_tables` JSON: `{connection, schema, schemas, objects: [{name, kind}],
/// truncated}`.
#[must_use]
pub fn tables_json(connection: &str, tables: &DbTables) -> Value {
    json!({
        "connection": connection,
        "schema": tables.schema,
        "schemas": tables.schemas,
        "objects": tables.objects.iter()
            .map(|(name, kind)| json!({"name": name, "kind": kind}))
            .collect::<Vec<_>>(),
        "truncated": tables.truncated,
    })
}

/// `db_describe` JSON: `{connection, schema, table, columns, primary_key,
/// indexes, foreign_keys, constraints}`, each report as [`rows_json`].
#[must_use]
pub fn describe_json(connection: &str, d: &DbDescription) -> Value {
    json!({
        "connection": connection,
        "schema": d.schema,
        "table": d.table,
        "columns": rows_json(&d.columns),
        "primary_key": rows_json(&d.primary_key),
        "indexes": rows_json(&d.indexes),
        "foreign_keys": rows_json(&d.foreign_keys),
        "constraints": rows_json(&d.constraints),
    })
}

/// `db_query` JSON: `{connection, columns, rows, row_count, row_limit,
/// truncated}`.
#[must_use]
pub fn query_json(connection: &str, rows: &Rows) -> Value {
    json!({
        "connection": connection,
        "columns": rows.columns,
        "rows": rows.rows,
        "row_count": rows.rows.len(),
        "row_limit": DB_QUERY_ROW_LIMIT,
        "truncated": rows.truncated,
    })
}

/// The `path` argument, required: absent or non-string is
/// [`ToolError::MissingPath`].
fn required_path(args: &Value) -> Result<&str, ToolError> {
    args.get("path")
        .and_then(Value::as_str)
        .ok_or(ToolError::MissingPath)
}

/// The host's open buffers, canonicalized, deduplicated, and filtered to
/// readable non-secret files inside `root`, in the host's order.
///
/// The host is untrusted for confinement purposes: it may list stale paths
/// or files outside the workspace, so each goes through
/// [`workspace::resolve`]; ones it rejects are dropped silently.
pub(crate) fn open_buffers<H: Host>(root: &Path, host: &H) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for path in host.open_buffers(root) {
        // Non-UTF-8 paths cannot be expressed as a JSON string anyway.
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
/// and `{level, title, line}` entries. The format is chosen by extension:
/// Markdown headings, Org headlines, otherwise the symbol scan. Lines in the
/// result are always 1-based source lines.
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
                    // `kind` holds the heading marker (`#`, `##`, ...), so
                    // its length is the heading level.
                    json!({"level": e.kind.len(), "title": e.name, "line": line})
                })
                .collect();
            ("markdown", entries)
        }
        "org" => {
            let entries = vix_org::headlines(text)
                .into_iter()
                // `headlines` reports 0-based lines; the tool contract is 1-based.
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
fn diagnostics<H: Host>(root: &Path, host: &H, args: &Value) -> Result<Value, ToolError> {
    let requests: Vec<String> = match args.get("path") {
        Some(Value::String(p)) => vec![p.clone()],
        Some(Value::Null) | None => open_buffers(root, host)
            .iter()
            .map(|p| relative(root, p))
            .collect(),
        Some(_) => return Err(ToolError::PathNotString),
    };
    let explicit = matches!(args.get("path"), Some(Value::String(_)));
    let mut found = Vec::new();
    for request in &requests {
        // An explicitly named file that cannot be read is the caller's
        // error; an open buffer that became unreadable is just skipped.
        let (path, text) = match read_text(root, request) {
            Ok(read) => read,
            Err(e) if explicit => return Err(e.into()),
            Err(_) => continue,
        };
        for conflict in vix_conflict_tool::find_all(&text) {
            // `start` is a 0-based line (hence `+ 1`); `end` is already the
            // 1-based line of the closing marker.
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
            // The host has no SQL support at all: report once and stop,
            // rather than one identical error per database.
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
            Some(Err(error)) => {
                databases.push(json!({"path": rel, "error": error.to_string()}));
            }
        }
    }
    json!({"databases": databases})
}
