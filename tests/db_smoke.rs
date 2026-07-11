//! End-to-end smoke tests for the DB workbench (the `vix-db` crate spec).
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
use vix::db::{Browser, Pages, Pane, View, connect, session::Session};

fn key(browser: &mut Browser, code: KeyCode) {
    browser.handle_key(KeyEvent::new(code, KeyModifiers::NONE), Pages::default());
    // User statements (F5/EXPLAIN) now run asynchronously; drain the reply so
    // tests observe the result synchronously, as a real event loop would.
    let mut spins = 0;
    while browser.query_running() && spins < 1_000_000 {
        browser.poll_query();
        std::thread::yield_now();
        spins += 1;
    }
}

fn type_str(browser: &mut Browser, text: &str) {
    for c in text.chars() {
        key(browser, KeyCode::Char(c));
    }
}

/// Seed a fresh `SQLite` file at `path` with `statements`.
fn seed(path: &std::path::Path, statements: &[&str]) {
    let _ = std::fs::remove_file(path);
    let mut session = Session::connect(&format!("sqlite:{}?mode=rwc", path.display()), &[])
        .expect("seed connect");
    for sql in statements {
        session.run(sql).expect("seed statement");
    }
}

fn sqlite_connection(name: &str, file: &std::path::Path) -> connect::Connection {
    connect::Connection {
        name: name.into(),
        kind: connect::Kind::Sqlite,
        file: file.display().to_string(),
        // These flows write (DELETE, rollback); connections open read-only by
        // default, so opt this fixture into write mode.
        writable: true,
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
    assert_eq!(
        b.view,
        View::Workbench,
        "connect lands in the workbench: {:?}",
        b.message
    );

    // The catalog listed the table and the view.
    let names = b.tree.table_names();
    assert!(
        names.contains(&"users".to_string()),
        "tree has the table: {names:?}"
    );
    assert!(
        names.contains(&"v_names".to_string()),
        "tree has the view: {names:?}"
    );

    // Typing a table prefix pops up schema-fed completions; Tab accepts.
    assert_eq!(b.focus, Pane::Editor);
    type_str(&mut b, "SELECT id, name FROM us");
    let popup = b
        .popup
        .as_ref()
        .expect("autocomplete offers the table name");
    assert!(
        popup.items.contains(&"users".to_string()),
        "{:?}",
        popup.items
    );
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
    assert_eq!(
        b.history.entries.first().map(String::as_str),
        Some("SELECT id, name FROM users")
    );
    assert!(
        b.take_dirty_history().is_some(),
        "host is told to persist the history"
    );

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
    assert_eq!(
        csv.lines().nth(1).unwrap(),
        "3,radia",
        "export follows the sort"
    );

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
    assert_eq!(
        b.grid.rows,
        vec![vec!["2".to_string()]],
        "the delete really ran"
    );

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
    assert!(
        b.grid.headers.iter().any(|h| h == "name"),
        "{:?}",
        b.grid.headers
    );
    let cols: Vec<&str> = b
        .grid
        .rows
        .iter()
        .filter_map(|r| r.get(1).map(String::as_str))
        .collect();
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
fn query_log_records_metrics_and_erd_maps_foreign_keys() {
    let dir = std::env::temp_dir().join(format!("vix-db-log-erd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("shop.db");
    seed(
        &file,
        &[
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            "CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users(id))",
            "INSERT INTO users VALUES (1,'ada')",
        ],
    );
    let mut b = Browser::new(vec![sqlite_connection("shop", &file)]);
    key(&mut b, KeyCode::Enter);
    assert_eq!(b.view, View::Workbench, "{:?}", b.message);

    // Run a user query, then open the log (Ctrl+L).
    type_str(&mut b, "SELECT * FROM users");
    key(&mut b, KeyCode::F(5));
    b.handle_key(
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL),
        Pages::default(),
    );
    assert_eq!(b.view, View::Log);
    // Newest first: the user SELECT is at the front; catalog queries (App) sit
    // behind it from the connect-time schema load.
    let head = &b.log.entries[0];
    assert!(
        head.sql.contains("SELECT * FROM users"),
        "logged sql: {}",
        head.sql
    );
    assert_eq!(head.rows, 1, "one row logged");
    assert!(head.ok, "success recorded");
    assert!(
        b.log
            .entries
            .iter()
            .any(|e| e.origin == vix::db::store::Origin::App),
        "catalog queries are logged as App origin",
    );
    // Enter reloads the statement into the editor.
    key(&mut b, KeyCode::Enter);
    assert_eq!(b.view, View::Workbench);
    assert!(b.query.text().contains("SELECT * FROM users"));

    // Generate the ER diagram (Ctrl+E) and check the FK edge is mapped.
    b.handle_key(
        KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
        Pages::default(),
    );
    assert_eq!(b.view, View::Erd, "{:?}", b.message);
    assert!(b.cell_text.starts_with("erDiagram"), "{}", b.cell_text);
    assert!(
        b.cell_text.contains("users {"),
        "users entity: {}",
        b.cell_text
    );
    assert!(
        b.cell_text.contains("orders }o--|| users : \"user_id\""),
        "FK edge mapped: {}",
        b.cell_text,
    );
    key(&mut b, KeyCode::Esc);
    assert_eq!(b.view, View::Workbench);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ask_ai_builds_a_schema_only_request_and_applies_the_reply() {
    let dir = std::env::temp_dir().join(format!("vix-db-ai-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("ai.db");
    seed(
        &file,
        &[
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            "CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users(id))",
            // A row whose data must NEVER reach the assistant brief.
            "INSERT INTO users VALUES (1,'top-secret-agent')",
        ],
    );
    // A read-only connection (the default), so the AI brief must say READ-ONLY.
    let conn = connect::Connection {
        name: "ai".into(),
        kind: connect::Kind::Sqlite,
        file: file.display().to_string(),
        ..connect::Connection::default()
    };
    let mut b = Browser::new(vec![conn]);
    key(&mut b, KeyCode::Enter);
    assert_eq!(b.view, View::Workbench, "{:?}", b.message);

    // Ctrl+A opens the Ask prompt; type a question and submit.
    b.handle_key(
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
        Pages::default(),
    );
    assert_eq!(b.view, View::Ask);
    type_str(&mut b, "how many orders does each user have?");
    key(&mut b, KeyCode::Enter);
    assert!(b.ai_busy(), "a request is queued");

    // The host would drain the request; check it is schema-only and read-only.
    let req = b
        .take_ai_request()
        .expect("a request is queued for the host");
    assert!(
        req.prompt.contains("READ-ONLY"),
        "read-only connection ⇒ read-only instruction"
    );
    assert!(
        req.context.contains("users(id INTEGER, name TEXT)"),
        "schema in brief: {}",
        req.context
    );
    assert!(
        req.context.contains("orders.user_id -> users.id"),
        "FK in brief: {}",
        req.context
    );
    assert!(req.context.contains("how many orders"), "question in brief");
    assert!(
        !req.context.contains("top-secret-agent"),
        "NO row data leaks into the brief"
    );
    assert!(
        b.take_ai_request().is_none(),
        "the request is taken only once"
    );
    assert!(b.ai_busy(), "still busy while the reply is awaited");

    // The assistant reply (fenced) lands in the editor, validated by EXPLAIN.
    b.apply_ai_reply("```sql\nSELECT user_id, count(*) FROM orders GROUP BY user_id\n```");
    assert!(!b.ai_busy(), "no longer busy once applied");
    assert_eq!(
        b.query.text(),
        "SELECT user_id, count(*) FROM orders GROUP BY user_id",
        "recovered SQL is in the editor",
    );

    // Optimize round: feeds the current query plus its EXPLAIN plan back.
    b.focus = Pane::Editor;
    b.handle_key(
        KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
        Pages::default(),
    );
    let opt = b.take_ai_request().expect("optimize queues a request");
    assert!(
        opt.context.contains("EXPLAIN plan:"),
        "optimize brief carries the plan: {}",
        opt.context
    );
    assert!(
        opt.context.contains("GROUP BY user_id"),
        "optimize brief carries the query"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn params_import_fk_and_chart_flows() {
    let dir = std::env::temp_dir().join(format!("vix-db-more-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("more.db");
    seed(
        &file,
        &[
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            "CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users(id))",
            "INSERT INTO users VALUES (1,'ada'),(2,'grace')",
            "INSERT INTO orders VALUES (10,1),(11,2)",
        ],
    );
    let mut b = Browser::new(vec![sqlite_connection("more", &file)]); // writable fixture
    key(&mut b, KeyCode::Enter);
    assert_eq!(b.view, View::Workbench, "{:?}", b.message);

    // --- Bind parameters: :id prompts before running. ---
    type_str(&mut b, "SELECT name FROM users WHERE id = :id");
    key(&mut b, KeyCode::F(5));
    assert_eq!(b.view, View::Params, "a :param opens the prompt");
    type_str(&mut b, "2");
    key(&mut b, KeyCode::Enter);
    assert_eq!(b.view, View::Workbench);
    assert_eq!(
        b.grid.rows,
        vec![vec!["grace".to_string()]],
        "param substituted: {:?}",
        b.message
    );

    // --- Expanded row view (x) opens the text viewer. ---
    b.focus = Pane::Results;
    key(&mut b, KeyCode::Char('x'));
    assert_eq!(b.view, View::Cell);
    assert!(
        b.cell_text.contains("name"),
        "expanded row shows the column: {}",
        b.cell_text
    );
    key(&mut b, KeyCode::Esc);

    // --- Preview orders, then follow the user_id foreign key to users. ---
    b.focus = Pane::Tree;
    // main → Tables → (orders, users): step to orders and preview.
    key(&mut b, KeyCode::Down);
    key(&mut b, KeyCode::Down);
    key(&mut b, KeyCode::Char('p'));
    assert_eq!(
        b.grid.headers,
        vec!["id", "user_id"],
        "orders preview: {:?}",
        b.message
    );
    b.focus = Pane::Results;
    key(&mut b, KeyCode::Right); // select the user_id column
    key(&mut b, KeyCode::Char('f')); // follow the FK
    assert_eq!(
        b.grid.headers,
        vec!["id", "name"],
        "followed to users: {:?}",
        b.message
    );
    assert_eq!(b.grid.rows.len(), 1, "one parent row");

    // --- Chart a (label, number) result. ---
    b.focus = Pane::Editor;
    for _ in 0..b.query.text().len() {
        key(&mut b, KeyCode::Backspace);
    }
    type_str(&mut b, "SELECT name, id FROM users ORDER BY id");
    key(&mut b, KeyCode::F(5));
    b.focus = Pane::Results;
    key(&mut b, KeyCode::Char('c'));
    assert_eq!(b.view, View::Cell);
    assert!(
        b.cell_text.contains('█'),
        "chart drew bars: {}",
        b.cell_text
    );
    key(&mut b, KeyCode::Esc);

    // --- CSV import creates and fills a table. ---
    let csv = dir.join("pets.csv");
    std::fs::write(&csv, "id,species\n1,cat\n2,dog\n").unwrap();
    b.handle_key(
        KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        Pages::default(),
    );
    assert_eq!(b.view, View::Import);
    type_str(&mut b, csv.to_str().unwrap());
    key(&mut b, KeyCode::Enter);
    assert_eq!(b.view, View::Workbench);
    assert!(
        b.message.as_deref().unwrap_or("").contains("pets"),
        "import message: {:?}",
        b.message
    );
    b.focus = Pane::Editor;
    for _ in 0..b.query.text().len() {
        key(&mut b, KeyCode::Backspace);
    }
    type_str(&mut b, "SELECT count(*) FROM pets");
    key(&mut b, KeyCode::F(5));
    assert_eq!(
        b.grid.rows,
        vec![vec!["2".to_string()]],
        "imported rows are queryable"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn transactions_span_statements_in_the_workbench() {
    let dir = std::env::temp_dir().join(format!("vix-db-tx-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("tx.db");
    seed(
        &file,
        &["CREATE TABLE t (a INTEGER)", "INSERT INTO t VALUES (1)"],
    );
    let mut b = Browser::new(vec![sqlite_connection("tx", &file)]);
    key(&mut b, KeyCode::Enter);
    assert_eq!(b.view, View::Workbench, "{:?}", b.message);

    // BEGIN, a DELETE, then ROLLBACK — each its own execution. Inside the open
    // transaction the write skips confirmation (it is provisional); the
    // rollback then really undoes the delete.
    for stmt in [
        "BEGIN",
        "DELETE FROM t",
        "ROLLBACK",
        "SELECT count(*) FROM t",
    ] {
        b.focus = Pane::Editor;
        for _ in 0..b.query.text().len() {
            key(&mut b, KeyCode::Backspace);
        }
        type_str(&mut b, stmt);
        key(&mut b, KeyCode::F(5));
        assert_eq!(b.view, View::Workbench, "{stmt}: {:?}", b.message);
    }
    assert_eq!(
        b.grid.rows,
        vec![vec!["1".to_string()]],
        "rollback restored the row"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn large_result_streams_in_batches() {
    let dir = std::env::temp_dir().join(format!("vix-db-stream-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("stream.db");
    // 1500 rows > one batch (512), so the result arrives in several chunks.
    seed(
        &file,
        &[
            "CREATE TABLE t (n INTEGER)",
            "INSERT INTO t WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM c WHERE n < 1500) SELECT n FROM c",
        ],
    );
    let mut b = Browser::new(vec![sqlite_connection("stream", &file)]);
    key(&mut b, KeyCode::Enter);
    type_str(&mut b, "SELECT n FROM t ORDER BY n");
    key(&mut b, KeyCode::F(5)); // key() pumps poll_query until the stream is done
    assert_eq!(b.grid.headers, vec!["n"], "{:?}", b.message);
    assert_eq!(
        b.grid.rows.len(),
        1500,
        "every streamed batch accumulated: {:?}",
        b.message
    );
    assert_eq!(b.grid.rows.first(), Some(&vec!["1".to_string()]));
    assert_eq!(b.grid.rows.last(), Some(&vec!["1500".to_string()]));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn async_query_runs_off_the_event_loop_and_cancels() {
    let dir = std::env::temp_dir().join(format!("vix-db-async-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("async.db");
    seed(
        &file,
        &[
            "CREATE TABLE t (a INTEGER)",
            "INSERT INTO t VALUES (1),(2),(3)",
        ],
    );
    let mut b = Browser::new(vec![sqlite_connection("async", &file)]);
    key(&mut b, KeyCode::Enter);

    // F5 sends the query without blocking; the result arrives via poll_query.
    type_str(&mut b, "SELECT count(*) FROM t");
    b.handle_key(
        KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE),
        Pages::default(),
    );
    assert!(
        b.query_running(),
        "the query is in flight, not applied synchronously"
    );
    let mut spins = 0;
    while b.query_running() && spins < 1_000_000 {
        b.poll_query();
        std::thread::yield_now();
        spins += 1;
    }
    assert_eq!(
        b.grid.rows,
        vec![vec!["3".to_string()]],
        "async result applied"
    );

    // Cancel returns control immediately and leaves a usable session.
    b.handle_key(
        KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE),
        Pages::default(),
    );
    b.cancel_query();
    assert!(!b.query_running(), "cancel clears the in-flight query");
    b.focus = Pane::Editor;
    for _ in 0..b.query.text().len() {
        key(&mut b, KeyCode::Backspace);
    }
    type_str(&mut b, "SELECT 42");
    key(&mut b, KeyCode::F(5));
    assert_eq!(
        b.grid.rows,
        vec![vec!["42".to_string()]],
        "session works after cancel: {:?}",
        b.message
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn staged_cell_edits_commit_in_a_transaction() {
    let dir = std::env::temp_dir().join(format!("vix-db-edit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("edit.db");
    seed(
        &file,
        &[
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            "INSERT INTO users VALUES (1,'ada'),(2,'grace')",
        ],
    );
    let mut b = Browser::new(vec![sqlite_connection("edit", &file)]); // writable
    key(&mut b, KeyCode::Enter);

    // An arbitrary query result is not editable.
    type_str(&mut b, "SELECT name FROM users");
    key(&mut b, KeyCode::F(5));
    b.focus = Pane::Results;
    key(&mut b, KeyCode::Char('i'));
    assert_eq!(b.view, View::Workbench, "arbitrary result rejects editing");

    // Preview the table → editable (has a primary key).
    b.focus = Pane::Tree;
    key(&mut b, KeyCode::Down); // schema → Tables folder
    key(&mut b, KeyCode::Down); // → users
    key(&mut b, KeyCode::Char('p'));
    assert!(b.editable(), "a previewed table with a PK is editable");
    b.focus = Pane::Results;

    // Edit the name of the first row (id=1): select the name column, edit it.
    key(&mut b, KeyCode::Right); // id → name
    key(&mut b, KeyCode::Char('i'));
    assert_eq!(b.view, View::CellEdit);
    for _ in 0.."ada".len() {
        key(&mut b, KeyCode::Backspace);
    }
    type_str(&mut b, "Ada Lovelace");
    key(&mut b, KeyCode::Enter);
    assert_eq!(b.view, View::Workbench);
    assert_eq!(b.edits.len(), 1, "one staged edit");

    // Commit with W → persisted via UPDATE in a transaction.
    key(&mut b, KeyCode::Char('W'));
    assert!(
        b.edits.is_empty(),
        "edits cleared after commit: {:?}",
        b.message
    );
    b.focus = Pane::Editor;
    for _ in 0..b.query.text().len() {
        key(&mut b, KeyCode::Backspace);
    }
    type_str(&mut b, "SELECT name FROM users WHERE id = 1");
    key(&mut b, KeyCode::F(5));
    assert_eq!(
        b.grid.rows,
        vec![vec!["Ada Lovelace".to_string()]],
        "edit persisted: {:?}",
        b.message
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn transaction_state_badge_and_relaxed_confirm() {
    use vix::db::TxState;
    let dir = std::env::temp_dir().join(format!("vix-db-tx2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("tx2.db");
    seed(
        &file,
        &["CREATE TABLE t (a INTEGER)", "INSERT INTO t VALUES (1),(2)"],
    );
    let mut b = Browser::new(vec![sqlite_connection("tx2", &file)]);
    key(&mut b, KeyCode::Enter);
    assert_eq!(b.tx_state(), TxState::None, "autocommit at connect");

    // Outside a transaction a write still asks for confirmation.
    type_str(&mut b, "DELETE FROM t WHERE a = 1");
    key(&mut b, KeyCode::F(5));
    assert_eq!(b.view, View::Confirm, "write is gated outside a tx");
    key(&mut b, KeyCode::Esc);

    // Begin a transaction via the menu action path.
    b.begin_tx();
    assert_eq!(b.tx_state(), TxState::Open, "badge shows an open tx");

    // Now the same write runs without a confirmation prompt.
    b.focus = Pane::Editor;
    for _ in 0..b.query.text().len() {
        key(&mut b, KeyCode::Backspace);
    }
    type_str(&mut b, "DELETE FROM t WHERE a = 1");
    key(&mut b, KeyCode::F(5));
    assert_eq!(
        b.view,
        View::Workbench,
        "no confirm inside a tx: {:?}",
        b.message
    );

    // Roll back and confirm both the state and the data are restored.
    b.rollback_tx();
    assert_eq!(b.tx_state(), TxState::None);
    b.focus = Pane::Editor;
    for _ in 0..b.query.text().len() {
        key(&mut b, KeyCode::Backspace);
    }
    type_str(&mut b, "SELECT count(*) FROM t");
    key(&mut b, KeyCode::F(5));
    assert_eq!(
        b.grid.rows,
        vec![vec!["2".to_string()]],
        "rollback restored the row"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
