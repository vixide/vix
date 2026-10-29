//! End-to-end test of `vix --mcp` (T731, `spec/mcp-server`): spawn the built
//! binary on a scratch workspace and run `initialize`, `tools/list` and a
//! `tools/call` round trip over its stdin/stdout, as an MCP client would.
//! No network, no terminal.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use serde_json::{Value, json};
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// A fresh scratch directory for this test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-mcp-stdio-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn initialize_list_and_call_over_stdio() {
    let workspace = scratch("ws");
    std::fs::create_dir_all(workspace.join("src")).unwrap();
    std::fs::write(
        workspace.join("src/main.rs"),
        "fn main() {}\n\nstruct Config;\n",
    )
    .unwrap();
    // An empty home, so the server's session read finds nothing of the
    // user's, and the test can check the server wrote nothing there.
    let home = scratch("home");

    let mut child = Command::new(env!("CARGO_BIN_EXE_vix"))
        .arg("--mcp")
        .arg(&workspace)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("APPDATA", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn vix --mcp");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let mut exchange = |message: &Value| -> Option<Value> {
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
        message.get("id")?;
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        Some(serde_json::from_str(&line).expect("one JSON message per line"))
    };

    let init = exchange(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "vix-test", "version": "0"}
        }
    }))
    .unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "vix");
    assert_eq!(
        init["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );

    assert!(exchange(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());

    let list = exchange(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"document_symbols"), "{names:?}");
    assert!(names.contains(&"db_schema"), "{names:?}");

    let call = exchange(&json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": {"name": "document_symbols", "arguments": {"path": "src/main.rs"}}
    }))
    .unwrap();
    assert_eq!(call["id"], 3);
    assert_eq!(call["result"]["isError"], false);
    assert_eq!(
        call["result"]["structuredContent"]["symbols"],
        json!([
            {"kind": "fn", "name": "main", "line": 1},
            {"kind": "struct", "name": "Config", "line": 3},
        ])
    );

    let escape = exchange(&json!({
        "jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": {"name": "read_buffer", "arguments": {"path": "../../etc/passwd"}}
    }))
    .unwrap();
    assert_eq!(escape["result"]["isError"], true);

    drop(exchange);
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(
        status.success(),
        "vix --mcp exits cleanly when stdin closes"
    );

    // Read-only: nothing was created in the (empty) home directory.
    let created: Vec<_> = walk(&home);
    assert!(created.is_empty(), "vix --mcp wrote files: {created:?}");
}

/// Every regular file under `dir`.
fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
fn a_missing_root_fails_on_stderr_with_a_non_zero_exit() {
    let output = Command::new(env!("CARGO_BIN_EXE_vix"))
        .arg("--mcp")
        .arg("/definitely/not/a/vix/workspace")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "stdout carries protocol only");
    assert!(
        output.stderr.iter().any(|b| !b.is_ascii_whitespace()),
        "the failure is reported on stderr"
    );
}
