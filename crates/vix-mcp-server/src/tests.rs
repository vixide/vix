//! Request/response tests over a fake [`Host`].

use super::*;
use std::fs;

/// A host with a fixed buffer list and an optional fixed schema.
struct FakeHost {
    /// Paths returned by `open_buffers`.
    buffers: Vec<PathBuf>,
    /// `None` = no `sql` capability.
    schema: Option<Vec<Table>>,
}

impl Host for FakeHost {
    fn open_buffers(&self, _root: &Path) -> Vec<PathBuf> {
        self.buffers.clone()
    }

    fn sqlite_schema(&self, _path: &Path) -> Option<Result<Vec<Table>, String>> {
        self.schema.clone().map(Ok)
    }
}

/// A scratch workspace with a Rust file, a Markdown file, an Org file, a
/// conflicted file, a secret, and a SQLite-headed database.
fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-mcp-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/lib.rs"),
        "pub struct Point;\n\npub fn area() -> u32 {\n    1\n}\n",
    )
    .unwrap();
    fs::write(
        dir.join("README.md"),
        "# Title\n\nText.\n\n## Usage\n\nMore.\n",
    )
    .unwrap();
    fs::write(dir.join("notes.org"), "* Top\n** Child\ntext\n* Second\n").unwrap();
    fs::write(
        dir.join("merge.txt"),
        "a\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> branch\nb\n",
    )
    .unwrap();
    fs::write(dir.join(".env"), "TOKEN=secret\n").unwrap();
    let mut db = b"SQLite format 3\0".to_vec();
    db.extend_from_slice(&[0; 84]);
    fs::write(dir.join("app.db"), db).unwrap();
    dir.canonicalize().unwrap()
}

fn server(name: &str, schema: Option<Vec<Table>>) -> Server<FakeHost> {
    let root = workspace(name);
    let buffers = vec![
        root.join("src/lib.rs"),
        root.join("README.md"),
        root.join("src/lib.rs"),
        root.join(".env"),
        PathBuf::from("/definitely/not/inside"),
    ];
    let mut server = Server::new(&root, FakeHost { buffers, schema }, "9.9.9").unwrap();
    let init = server.handle(&json!({
        "jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}
    }));
    assert!(init.is_some());
    server
}

fn request(server: &mut Server<FakeHost>, method: &str, params: &Value) -> Value {
    server
        .handle(&json!({"jsonrpc": "2.0", "id": 7, "method": method, "params": params}))
        .expect("a request gets a reply")
}

fn call(server: &mut Server<FakeHost>, tool: &str, args: &Value) -> Value {
    request(
        server,
        "tools/call",
        &json!({"name": tool, "arguments": args}),
    )["result"]
        .clone()
}

#[test]
fn initialize_negotiates_the_protocol_version() {
    let root = workspace("init");
    let host = || FakeHost {
        buffers: vec![],
        schema: None,
    };
    let mut s = Server::new(&root, host(), "1.2.3").unwrap();
    let reply = s
        .handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}))
        .unwrap();
    assert_eq!(reply["id"], 1);
    assert_eq!(reply["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(reply["result"]["serverInfo"]["name"], "vix");
    assert_eq!(reply["result"]["serverInfo"]["version"], "1.2.3");
    assert!(reply["result"]["capabilities"]["tools"].is_object());

    let mut s = Server::new(&root, host(), "1.2.3").unwrap();
    let reply = s
        .handle(&json!({"jsonrpc":"2.0","id":"a","method":"initialize","params":{"protocolVersion":"1999-01-01"}}))
        .unwrap();
    assert_eq!(reply["id"], "a");
    assert_eq!(reply["result"]["protocolVersion"], PROTOCOL_VERSION);
}

#[test]
fn requests_before_initialize_are_refused_but_ping_works() {
    let root = workspace("preinit");
    let mut s = Server::new(
        &root,
        FakeHost {
            buffers: vec![],
            schema: None,
        },
        "1",
    )
    .unwrap();
    let reply = s
        .handle(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .unwrap();
    assert_eq!(reply["error"]["code"], NOT_INITIALIZED);
    let reply = s
        .handle(&json!({"jsonrpc":"2.0","id":2,"method":"ping"}))
        .unwrap();
    assert_eq!(reply["result"], json!({}));
}

#[test]
fn notifications_and_blank_lines_get_no_reply() {
    let mut s = server("notify", None);
    assert!(
        s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .is_none()
    );
    assert!(s.handle_line("   ").is_none());
}

#[test]
fn malformed_input_gets_json_rpc_errors() {
    let mut s = server("errors", None);
    let parse: Value = serde_json::from_str(&s.handle_line("{not json").unwrap()).unwrap();
    assert_eq!(parse["error"]["code"], PARSE_ERROR);
    assert_eq!(parse["id"], Value::Null);

    let batch: Value = serde_json::from_str(&s.handle_line("[]").unwrap()).unwrap();
    assert_eq!(batch["error"]["code"], INVALID_REQUEST);

    let reply = s.handle(&json!({"id": 3, "method": "ping"})).unwrap();
    assert_eq!(reply["error"]["code"], INVALID_REQUEST);

    let reply = request(&mut s, "no/such", &json!({}));
    assert_eq!(reply["error"]["code"], METHOD_NOT_FOUND);

    let reply = request(&mut s, "tools/call", &json!({"name": "write_file"}));
    assert_eq!(reply["error"]["code"], INVALID_PARAMS);
}

#[test]
fn every_reply_is_one_line() {
    let mut s = server("oneline", None);
    let line = s
        .handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"read_buffer","arguments":{"path":"README.md"}}}"#)
        .unwrap();
    assert!(!line.contains('\n'));
}

#[test]
fn tools_list_names_six_read_only_tools() {
    let mut s = server("list", None);
    let reply = request(&mut s, "tools/list", &json!({}));
    let tools = reply["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "list_open_buffers",
            "read_buffer",
            "document_symbols",
            "outline",
            "diagnostics",
            "db_schema"
        ]
    );
    for tool in tools {
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
}

#[test]
fn list_open_buffers_filters_to_readable_files_inside_the_root() {
    let mut s = server("buffers", None);
    let result = call(&mut s, "list_open_buffers", &json!({}));
    assert_eq!(result["isError"], false);
    assert_eq!(
        result["structuredContent"]["buffers"],
        json!(["src/lib.rs", "README.md"])
    );
    // The text content carries the same JSON.
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, result["structuredContent"]);
}

#[test]
fn read_buffer_returns_text_and_refuses_secrets_and_escapes() {
    let mut s = server("read", None);
    let ok = call(&mut s, "read_buffer", &json!({"path": "README.md"}));
    assert_eq!(
        ok["structuredContent"]["text"],
        "# Title\n\nText.\n\n## Usage\n\nMore.\n"
    );
    for path in [".env", "../outside.txt", "/etc/hosts", "nope.txt"] {
        let refused = call(&mut s, "read_buffer", &json!({"path": path}));
        assert_eq!(refused["isError"], true, "{path}");
        assert!(refused.get("structuredContent").is_none());
    }
    let missing = call(&mut s, "read_buffer", &json!({}));
    assert_eq!(missing["isError"], true);
}

#[test]
fn document_symbols_uses_the_offline_scan() {
    let mut s = server("symbols", None);
    let result = call(&mut s, "document_symbols", &json!({"path": "src/lib.rs"}));
    assert_eq!(
        result["structuredContent"]["symbols"],
        json!([
            {"kind": "struct", "name": "Point", "line": 1},
            {"kind": "fn", "name": "area", "line": 3},
        ])
    );
}

#[test]
fn outline_covers_markdown_org_and_code() {
    let mut s = server("outline", None);
    let md = call(&mut s, "outline", &json!({"path": "README.md"}));
    assert_eq!(md["structuredContent"]["format"], "markdown");
    assert_eq!(
        md["structuredContent"]["entries"],
        json!([
            {"level": 1, "title": "Title", "line": 1},
            {"level": 2, "title": "Usage", "line": 5},
        ])
    );
    let org = call(&mut s, "outline", &json!({"path": "notes.org"}));
    assert_eq!(org["structuredContent"]["format"], "org");
    assert_eq!(
        org["structuredContent"]["entries"],
        json!([
            {"level": 1, "title": "Top", "line": 1},
            {"level": 2, "title": "Child", "line": 2},
            {"level": 1, "title": "Second", "line": 4},
        ])
    );
    let code = call(&mut s, "outline", &json!({"path": "src/lib.rs"}));
    assert_eq!(code["structuredContent"]["format"], "symbols");
    assert_eq!(code["structuredContent"]["entries"][1]["title"], "fn area");
}

#[test]
fn diagnostics_report_conflicts_and_no_language_server() {
    let mut s = server("diag", None);
    let one = call(&mut s, "diagnostics", &json!({"path": "merge.txt"}));
    let content = &one["structuredContent"];
    assert_eq!(content["language_server"], "not_running");
    assert_eq!(content["diagnostics"][0]["line"], 2);
    assert_eq!(content["diagnostics"][0]["end_line"], 6);
    assert_eq!(content["diagnostics"][0]["path"], "merge.txt");
    let all = call(&mut s, "diagnostics", &json!({}));
    assert_eq!(all["structuredContent"]["diagnostics"], json!([]));
}

#[test]
fn db_schema_reports_tables_or_a_missing_capability() {
    let schema = vec![Table {
        name: "users".into(),
        kind: "table".into(),
        columns: vec![Column {
            name: "id".into(),
            data_type: "INTEGER".into(),
        }],
    }];
    let mut s = server("db", Some(schema));
    let result = call(&mut s, "db_schema", &json!({}));
    assert_eq!(
        result["structuredContent"]["databases"],
        json!([{"path": "app.db", "tables": [
            {"name": "users", "kind": "table", "columns": [{"name": "id", "type": "INTEGER"}]}
        ]}])
    );
    let mut s = server("db-off", None);
    let result = call(&mut s, "db_schema", &json!({}));
    assert!(result["structuredContent"]["error"].is_string());
}

#[test]
fn resources_list_and_read_round_trip() {
    let mut s = server("resources", None);
    let list = request(&mut s, "resources/list", &json!({}));
    let resources = list["result"]["resources"].as_array().unwrap();
    assert_eq!(resources.len(), 2);
    let uri = resources[1]["uri"].as_str().unwrap().to_string();
    assert_eq!(resources[1]["name"], "README.md");
    let read = request(&mut s, "resources/read", &json!({"uri": uri}));
    assert!(
        read["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("# Title")
    );
    let secret = file_uri(&s.root().join(".env"));
    let refused = request(&mut s, "resources/read", &json!({"uri": secret}));
    assert_eq!(refused["error"]["code"], INVALID_PARAMS);
    let other = request(
        &mut s,
        "resources/read",
        &json!({"uri": "https://example.com/x"}),
    );
    assert_eq!(other["error"]["code"], INVALID_PARAMS);
}

#[test]
fn file_uris_round_trip_with_escapes() {
    let path = Path::new("/tmp/a b/c%d.rs");
    let uri = file_uri(path);
    assert_eq!(uri, "file:///tmp/a%20b/c%25d.rs");
    assert_eq!(uri_path(&uri).unwrap(), "/tmp/a b/c%d.rs");
    assert_eq!(uri_path("file://localhost/x").unwrap(), "/x");
    assert!(uri_path("file://host/x").is_none());
    assert!(uri_path("file:///bad%zz").is_none());
}

#[test]
fn serve_answers_each_request_line() {
    let root = workspace("serve");
    let mut s = Server::new(
        &root,
        FakeHost {
            buffers: vec![],
            schema: None,
        },
        "1",
    )
    .unwrap();
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        "\n"
    );
    let mut out = Vec::new();
    serve(&mut s, input.as_bytes(), &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    let second: Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(second["id"], 2);
}
