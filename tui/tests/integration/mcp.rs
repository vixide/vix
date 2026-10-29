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
        root: key.clone(),
        files: vec![format!("{key}/a.rs")],
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
    let url = format!("sqlite:{}?mode=rwc", file.display());
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
