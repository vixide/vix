//! `vix --mcp`'s App-shell host (T731, `spec/mcp-server`): open buffers come
//! from the saved session for the canonical root, and `SQLite` schemas are
//! read through `vix-db` without touching the file.

use std::path::PathBuf;
use vix::mcp::ShellHost;
use vix_mcp_server::Host as _;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-mcp-host-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn open_buffers_come_from_the_session_for_this_root_only() {
    let root = scratch("session");
    let key = root.to_string_lossy().into_owned();
    let mut session = vix::session::Session::default();
    session.set_workspace(vix::session::WorkspaceSession {
        root: "/some/other/root".into(),
        files: vec!["/some/other/root/x.rs".into()],
        ..Default::default()
    });
    session.set_workspace(vix::session::WorkspaceSession {
        root: key,
        // The session stores paths the way the platform writes them.
        files: vec![root.join("a.rs").to_string_lossy().into_owned()],
        ..Default::default()
    });
    let path = root.join("session.toml");
    session.save_to(&path).unwrap();
    let host = ShellHost {
        session_path: Some(path),
    };
    assert_eq!(host.open_buffers(&root), vec![root.join("a.rs")]);
    let missing = ShellHost {
        session_path: Some(root.join("missing.toml")),
    };
    assert_eq!(missing.open_buffers(&root), Vec::<PathBuf>::new());
    assert!(!root.join("missing.toml").exists());
}

#[cfg(feature = "sql")]
#[test]
fn sqlite_schema_groups_columns_into_tables() {
    let root = scratch("sqlite");
    let file = root.join("app.db");
    let url = vix::db::catalog::sqlite_url(&file.to_string_lossy(), "rwc");
    let mut seed = vix::db::session::Session::connect(&url, &[]).unwrap();
    seed.run("CREATE TABLE a (x INTEGER, y TEXT);").unwrap();
    seed.run("CREATE TABLE b (z REAL);").unwrap();
    drop(seed);
    let tables = ShellHost::default()
        .sqlite_schema(&file)
        .expect("the sql capability is compiled in")
        .unwrap();
    let shape: Vec<(String, usize)> = tables
        .iter()
        .map(|t| (t.name.clone(), t.columns.len()))
        .collect();
    assert_eq!(shape, vec![("a".to_string(), 2), ("b".to_string(), 1)]);
}

/// T783: a scratch workspace with two saved `SQLite` connections, `app`
/// (shared, 150 rows) and `private` (not shared), and an MCP server over
/// them whose settings and DB tool log are scratch files.
#[cfg(feature = "sql")]
fn shared_db_server(
    name: &str,
) -> (
    vix_mcp_server::Server<vix::mcp::McpHost>,
    PathBuf,
    PathBuf,
    PathBuf,
) {
    let root = scratch(name);
    let app = root.join("app.db");
    let url = vix::db::catalog::sqlite_url(&app.to_string_lossy(), "rwc");
    let mut seed = vix::db::session::Session::connect(&url, &[]).unwrap();
    seed.run("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT);")
        .unwrap();
    seed.run(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 150) \
         INSERT INTO items (id, name) SELECT i, 'item ' || i FROM n;",
    )
    .unwrap();
    drop(seed);
    std::fs::copy(&app, root.join("private.db")).unwrap();
    let settings = root.join("config.toml");
    std::fs::write(
        &settings,
        format!(
            "[[db_connections]]\nname = \"app\"\nkind = \"sqlite\"\nfile = {app:?}\n\n\
             [[db_connections]]\nname = \"private\"\nkind = \"sqlite\"\nfile = {private:?}\n\n\
             [db_environments]\napp = \"staging\"\n\n\
             [db_mcp_shared]\napp = true\nprivate = false\n",
            app = app.display().to_string(),
            private = root.join("private.db").display().to_string(),
        ),
    )
    .unwrap();
    let log = root.join("mcp-db.log");
    let host = vix::mcp::McpHost {
        session_path: Some(root.join("session.toml")),
        settings_path: Some(settings.clone()),
        db_log_path: Some(log.clone()),
    };
    let mut server = vix_mcp_server::Server::new(&root, host, "0.0.0").unwrap();
    server
        .handle_line(
            r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        )
        .unwrap();
    (server, app, settings, log)
}

/// One `tools/call` through the protocol, returning its `result`.
#[cfg(feature = "sql")]
fn call_tool(
    server: &mut vix_mcp_server::Server<vix::mcp::McpHost>,
    tool: &str,
    arguments: &serde_json::Value,
) -> serde_json::Value {
    let request = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": tool, "arguments": arguments}
    });
    let reply = server.handle_line(&request.to_string()).unwrap();
    let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
    reply["result"].clone()
}

#[cfg(feature = "sql")]
#[test]
fn only_shared_connections_are_visible_to_mcp_clients() {
    use serde_json::json;
    let (mut server, _, _, _) = shared_db_server("db-visible");
    let listed = call_tool(&mut server, "db_connections", &json!({}));
    assert_eq!(
        listed["structuredContent"],
        json!({"connections": [
            {"name": "app", "kind": "sqlite", "environment": "staging", "database": "app.db"}
        ]})
    );
    let text = listed["content"][0]["text"].as_str().unwrap();
    assert!(!text.contains("private"), "{text}");
    for (tool, args) in [
        ("db_tables", json!({"connection": "private"})),
        (
            "db_describe",
            json!({"connection": "private", "table": "items"}),
        ),
        (
            "db_query",
            json!({"connection": "private", "sql": "select 1"}),
        ),
        (
            "db_query",
            json!({"connection": "nobody", "sql": "select 1"}),
        ),
    ] {
        let result = call_tool(&mut server, tool, &args);
        assert_eq!(result["isError"], true, "{tool} {args}");
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .starts_with("no shared connection named"),
            "{result}"
        );
    }
    let tables = call_tool(&mut server, "db_tables", &json!({"connection": "app"}));
    assert_eq!(
        tables["structuredContent"]["objects"],
        json!([{"name": "items", "kind": "table"}])
    );
    let described = call_tool(
        &mut server,
        "db_describe",
        &json!({"connection": "app", "table": "items"}),
    );
    assert_eq!(described["isError"], false, "{described}");
    assert_eq!(
        described["structuredContent"]["primary_key"]["rows"],
        json!([["id"]])
    );
}

#[cfg(feature = "sql")]
#[test]
fn shared_queries_are_read_only_capped_and_logged() {
    use serde_json::json;
    let (mut server, app, settings, log) = shared_db_server("db-query");
    let before = std::fs::read(&app).unwrap();
    let settings_before = std::fs::read(&settings).unwrap();
    for sql in [
        "DELETE FROM items",
        "WITH doomed AS (SELECT 1) DELETE FROM items",
        "select 1; delete from items",
        "explain analyze select * from items",
    ] {
        let result = call_tool(
            &mut server,
            "db_query",
            &json!({"connection": "app", "sql": sql}),
        );
        assert_eq!(result["isError"], true, "{sql}: {result}");
    }
    let result = call_tool(
        &mut server,
        "db_query",
        &json!({"connection": "app", "sql": "SELECT id, name FROM items ORDER BY id"}),
    );
    assert_eq!(result["isError"], false, "{result}");
    let rows = &result["structuredContent"];
    assert_eq!(rows["columns"], json!(["id", "name"]));
    assert_eq!(rows["rows"].as_array().unwrap().len(), 100);
    assert_eq!(rows["row_count"], 100);
    assert_eq!(rows["truncated"], true);
    assert_eq!(rows["rows"][0], json!(["1", "item 1"]));
    let small = call_tool(
        &mut server,
        "db_query",
        &json!({"connection": "app", "sql": "select count(*) as n from items"}),
    );
    assert_eq!(small["structuredContent"]["rows"], json!([["150"]]));
    assert_eq!(small["structuredContent"]["truncated"], false);

    // Nothing was written to the database or the settings.
    assert_eq!(std::fs::read(&app).unwrap(), before);
    assert_eq!(std::fs::read(&settings).unwrap(), settings_before);

    // Every call is in the log, refused ones too, one JSON line each.
    let text = std::fs::read_to_string(&log).unwrap();
    let entries: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(entries.len(), 6, "{text}");
    for entry in &entries {
        assert_eq!(entry["tool"], "db_query");
        assert_eq!(entry["connection"], "app");
        assert!(entry["time"].as_str().unwrap().ends_with('Z'));
    }
    assert_eq!(entries[0]["sql"], "DELETE FROM items");
    assert_eq!(entries[4]["sql"], "SELECT id, name FROM items ORDER BY id");
}

#[cfg(feature = "sql")]
#[test]
fn the_db_log_rotates_instead_of_growing() {
    use serde_json::json;
    let (mut server, _, _, log) = shared_db_server("db-log");
    let oversized = vec![b'x'; usize::try_from(vix::mcp::DB_LOG_MAX_BYTES).unwrap()];
    std::fs::write(&log, oversized).unwrap();
    let result = call_tool(&mut server, "db_tables", &json!({"connection": "app"}));
    assert_eq!(result["isError"], false, "{result}");
    let mut rotated = log.clone().into_os_string();
    rotated.push(".1");
    assert_eq!(
        std::fs::metadata(&rotated).unwrap().len(),
        vix::mcp::DB_LOG_MAX_BYTES
    );
    let fresh = std::fs::read_to_string(&log).unwrap();
    assert_eq!(fresh.lines().count(), 1, "{fresh}");
    assert!(fresh.contains(r#""tool":"db_tables""#), "{fresh}");
}
