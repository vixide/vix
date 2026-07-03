//! End-to-end smoke tests for the DB workbench (`spec/db`).
//!
//! Since the workbench moved from CLI clients to embedded sqlx drivers,
//! these tests are fully self-contained: fixtures are seeded through a real
//! [`vix::db::session::Session`] on a `SQLite` file, then the
//! [`vix::db::Browser`] state machine is driven through the whole flow —
//! connect from the saved-connections list, browse the loaded schema tree,
//! type a query (watching autocomplete fire), execute at the cursor with F5,
//! sort, filter, export, EXPLAIN with the plan-doctor insight, and the
//! write-confirmation gate. No external database or CLI is required.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vix::db::{connect, session::Session, Browser, Pages, Pane, View};

fn key(browser: &mut Browser, code: KeyCode) {
    browser.handle_key(KeyEvent::new(code, KeyModifiers::NONE), Pages::default());
}

fn type_str(browser: &mut Browser, text: &str) {
    for c in text.chars() {
        key(browser, KeyCode::Char(c));
    }
}

/// Seed a fresh `SQLite` file at `path` with `statements`.
fn seed(path: &std::path::Path, statements: &[&str]) {
    let _ = std::fs::remove_file(path);
    let mut session =
        Session::connect(&format!("sqlite:{}?mode=rwc", path.display())).expect("seed connect");
    for sql in statements {
        session.run(sql).expect("seed statement");
    }
}

fn sqlite_connection(name: &str, file: &std::path::Path) -> connect::Connection {
    connect::Connection {
        name: name.into(),
        kind: connect::Kind::Sqlite,
        file: file.display().to_string(),
        ..connect::Connection::default()
    }
}

#[test]
fn sqlite_connect_browse_query_and_filter() {
    let dir = std::env::temp_dir().join(format!("vix-db-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("smoke.db");
    seed(
        &file,
        &[
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            "CREATE VIEW v_names AS SELECT name FROM users",
            "INSERT INTO users VALUES (1,'ada'),(2,'grace'),(3,'radia')",
        ],
    );

    // Connect from the saved-connections list (SQLite needs no password).
    let mut b = Browser::new(vec![sqlite_connection("smoke", &file)]);
    key(&mut b, KeyCode::Enter);
    assert_eq!(b.view, View::Workbench, "connect lands in the workbench: {:?}", b.message);

    // The catalog listed the table and the view.
    let names = b.tree.table_names();
    assert!(names.contains(&"users".to_string()), "tree has the table: {names:?}");
    assert!(names.contains(&"v_names".to_string()), "tree has the view: {names:?}");

    // Typing a table prefix pops up schema-fed completions; Tab accepts.
    assert_eq!(b.focus, Pane::Editor);
    type_str(&mut b, "SELECT id, name FROM us");
    let popup = b.popup.as_ref().expect("autocomplete offers the table name");
    assert!(popup.items.contains(&"users".to_string()), "{:?}", popup.items);
    key(&mut b, KeyCode::Tab);
    assert_eq!(b.query.text(), "SELECT id, name FROM users");

    // F5 executes the statement at the cursor and fills the grid.
    key(&mut b, KeyCode::F(5));
    assert_eq!(b.grid.headers, vec!["id", "name"], "{:?}", b.message);
    assert_eq!(b.grid.rows.len(), 3);
    assert_eq!(b.focus, Pane::Results);

    // The live filter narrows the rows.
    key(&mut b, KeyCode::Char('/'));
    type_str(&mut b, "grace");
    assert_eq!(b.grid.filtered(), vec![1]);
    key(&mut b, KeyCode::Esc); // clear the filter

    // Executed statements landed in the recallable history.
    assert_eq!(b.history.entries.first().map(String::as_str), Some("SELECT id, name FROM users"));
    assert!(b.take_dirty_history().is_some(), "host is told to persist the history");

    // Sorting by name descending puts radia first.
    key(&mut b, KeyCode::Right); // select the name column
    key(&mut b, KeyCode::Char('s')); // ascending
    key(&mut b, KeyCode::Char('s')); // descending
    assert_eq!(b.grid.selected_row().unwrap()[1], "radia");

    // Export the sorted grid to a CSV file.
    key(&mut b, KeyCode::Char('e'));
    b.export_path = dir.join("out.csv").display().to_string();
    key(&mut b, KeyCode::Enter);
    let csv = std::fs::read_to_string(dir.join("out.csv")).unwrap();
    assert_eq!(csv.lines().next().unwrap(), "id,name");
    assert_eq!(csv.lines().nth(1).unwrap(), "3,radia", "export follows the sort");

    // EXPLAIN QUERY PLAN of an unindexed scan raises the plan-doctor insight.
    b.focus = Pane::Editor;
    key(&mut b, KeyCode::F(6));
    assert!(
        b.message.as_deref().is_some_and(|m| m.contains("index")),
        "seq-scan insight: {:?}",
        b.message
    );

    // A write statement is gated behind the confirmation view, then runs.
    // (Explain moved the focus to the results pane; go back to the editor.)
    b.focus = Pane::Editor;
    for _ in 0..b.query.text().len() {
        key(&mut b, KeyCode::Backspace);
    }
    type_str(&mut b, "DELETE FROM users WHERE id = 1");
    key(&mut b, KeyCode::F(5));
    assert_eq!(b.view, View::Confirm);
    key(&mut b, KeyCode::Char('y'));
    assert_eq!(b.view, View::Workbench);
    b.focus = Pane::Editor;
    for _ in 0..b.query.text().len() {
        key(&mut b, KeyCode::Backspace);
    }
    type_str(&mut b, "SELECT count(*) FROM users");
    key(&mut b, KeyCode::F(5));
    assert_eq!(b.grid.rows, vec![vec!["2".to_string()]], "the delete really ran");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn table_details_report_columns() {
    let dir = std::env::temp_dir().join(format!("vix-db-detail-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("detail.db");
    seed(&file, &["CREATE TABLE t (a INTEGER, b TEXT NOT NULL)"]);

    let mut b = Browser::new(vec![sqlite_connection("detail", &file)]);
    key(&mut b, KeyCode::Enter);
    assert_eq!(b.view, View::Workbench, "{:?}", b.message);

    // Walk the tree to the table row (schema → Tables → t) and ask for the
    // column report; PRAGMA table_info lists both columns in the grid.
    key(&mut b, KeyCode::BackTab); // Editor → Tree (via prev)
    assert_eq!(b.focus, Pane::Tree);
    key(&mut b, KeyCode::Down);
    key(&mut b, KeyCode::Down);
    key(&mut b, KeyCode::Enter);
    assert!(b.grid.headers.iter().any(|h| h == "name"), "{:?}", b.grid.headers);
    let cols: Vec<&str> = b.grid.rows.iter().filter_map(|r| r.get(1).map(String::as_str)).collect();
    assert_eq!(cols, vec!["a", "b"], "table_info reports both columns");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tree_preview_shows_table_rows() {
    let dir = std::env::temp_dir().join(format!("vix-db-preview-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("preview.db");
    seed(
        &file,
        &[
            "CREATE TABLE songs (id INTEGER PRIMARY KEY, title TEXT)",
            "INSERT INTO songs VALUES (1,'blue'),(2,'green')",
        ],
    );
    let mut b = Browser::new(vec![sqlite_connection("preview", &file)]);
    key(&mut b, KeyCode::Enter);
    assert_eq!(b.view, View::Workbench, "{:?}", b.message);
    b.focus = Pane::Tree;
    key(&mut b, KeyCode::Down);
    key(&mut b, KeyCode::Down); // schema → Tables → songs
    key(&mut b, KeyCode::Char('p'));
    assert_eq!(b.grid.headers, vec!["id", "title"], "{:?}", b.message);
    assert_eq!(b.grid.rows.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn transactions_span_statements_in_the_workbench() {
    let dir = std::env::temp_dir().join(format!("vix-db-tx-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("tx.db");
    seed(&file, &["CREATE TABLE t (a INTEGER)", "INSERT INTO t VALUES (1)"]);
    let mut b = Browser::new(vec![sqlite_connection("tx", &file)]);
    key(&mut b, KeyCode::Enter);
    assert_eq!(b.view, View::Workbench, "{:?}", b.message);

    // BEGIN, a confirmed DELETE, then ROLLBACK — each its own execution.
    // With the persistent connection, the rollback really undoes the delete.
    for (stmt, confirmed) in
        [("BEGIN", false), ("DELETE FROM t", true), ("ROLLBACK", false), ("SELECT count(*) FROM t", false)]
    {
        b.focus = Pane::Editor;
        for _ in 0..b.query.text().len() {
            key(&mut b, KeyCode::Backspace);
        }
        type_str(&mut b, stmt);
        key(&mut b, KeyCode::F(5));
        if confirmed {
            assert_eq!(b.view, View::Confirm, "{stmt} is a write");
            key(&mut b, KeyCode::Char('y'));
        }
        assert_eq!(b.view, View::Workbench, "{stmt}: {:?}", b.message);
    }
    assert_eq!(b.grid.rows, vec![vec!["1".to_string()]], "rollback restored the row");
    let _ = std::fs::remove_dir_all(&dir);
}
