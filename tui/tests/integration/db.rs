#![allow(
    clippy::cast_possible_truncation,
    clippy::format_collect,
    reason = "tests count and index small fixtures, and build expected text with format! over iterators"
)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

#[test]
fn edit_sql_lists_formats_and_saves_statements() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "select 1;\ninsert into t values (1)");
    app.run_action("tools.edit_sql");
    assert!(
        app.edit.edit_sql().is_some(),
        "Edit → Mode → SQL opens the SQL editor"
    );

    // Format all (Shift+F) uppercases keywords; Ctrl+S writes back to the buffer.
    app.on_key(KeyEvent::new(KeyCode::Char('F'), KeyModifiers::SHIFT));
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    let text = app.editor.active_tab().unwrap().text();
    assert!(
        text.contains("SELECT 1;"),
        "keywords uppercased and saved: {text:?}"
    );
    assert!(text.contains("INSERT INTO t VALUES (1);"));

    // q closes the editor.
    app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(app.edit.edit_sql().is_none());
}

#[test]
fn interactive_query_replace_y_n_y() {
    let dir = unique_dir("qr");
    let file = dir.join("q.txt");
    fs::write(&file, "foo foo foo\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("edit.query_replace");
    for c in "foo".chars() {
        app.on_key(key(c)); // query field; cursor must NOT move in interactive mode
    }
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)); // to replace field
    for c in "bar".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)); // begin step-through
    assert!(app.query_replace.is_some(), "session should be active");

    app.on_key(key('y')); // replace first
    app.on_key(key('n')); // skip second
    app.on_key(key('y')); // replace third -> no more matches, session ends

    assert!(app.query_replace.is_none(), "session ends after last match");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "bar foo bar");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn interactive_query_replace_bang_replaces_rest() {
    let dir = unique_dir("qrbang");
    let file = dir.join("q.txt");
    fs::write(&file, "x x x x\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("edit.query_replace");
    app.on_key(key('x'));
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    app.on_key(key('Z'));
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.on_key(key('!')); // replace this and all the rest

    assert!(app.query_replace.is_none());
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "Z Z Z Z");

    fs::remove_dir_all(&dir).ok();
}

/// T786: DB → Run Buffer in Workbench loads the active buffer into the
/// workbench editor and runs it statement by statement; without a
/// connection it opens the connections list instead.
#[test]
fn run_buffer_in_workbench_loads_and_runs_the_buffer() {
    let dir = std::env::temp_dir().join(format!("vix-run-buffer-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("run.db");
    let _ = std::fs::remove_file(&file);
    let mut app = app_at(&dir);
    type_str(&mut app, "select 41 + 1;\nselect 'two'");

    app.run_action("db.run_buffer");
    let b = app.db.as_ref().expect("the overlay opens");
    assert_eq!(b.view, vix::db::View::Connections, "no connection yet");

    let mut browser = vix::db::Browser::new(vec![vix::db::connect::Connection {
        name: "run".into(),
        kind: vix::db::connect::Kind::Sqlite,
        file: file.display().to_string(),
        ..vix::db::connect::Connection::default()
    }]);
    browser.handle_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        vix::db::Pages::default(),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while browser.connect_running() {
        browser.poll_connect();
        assert!(std::time::Instant::now() < deadline, "connect stalled");
        std::thread::yield_now();
    }
    assert_eq!(
        browser.view,
        vix::db::View::Workbench,
        "{:?}",
        browser.message
    );
    app.db = Some(browser);

    app.run_action("db.run_buffer");
    let b = app.db.as_ref().unwrap();
    assert_eq!(b.view, vix::db::View::Workbench, "{:?}", b.message);
    assert_eq!(b.query.text(), "select 41 + 1;\nselect 'two'");
    assert_eq!(
        b.grid.rows,
        vec![vec!["two".to_string()]],
        "the last statement's rows"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// T783: the connection form's **Share with MCP** row is off for a new
/// connection; turning it on saves a `[db_mcp_shared]` entry by name, and
/// `vix --mcp`'s host then lists the connection.
#[test]
fn share_with_mcp_row_saves_a_db_mcp_shared_entry() {
    let dir = unique_dir("db-mcp-share");
    let config = dir.join("config.toml");
    let _ = fs::remove_file(&config);
    let mut app = app_at(&dir).with_settings_path(&config);
    app.run_action("db.connections");
    app.on_key(key('a'));
    for c in "notes".chars() {
        app.on_key(key(c));
    }
    assert!(
        !app.db.as_ref().unwrap().form_mcp_sharing.is_shared(),
        "off by default"
    );
    for _ in 0..vix::db::FORM_MCP_SHARED {
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    app.on_key(key(' '));
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.settings.db_mcp_shared.get("notes"), Some(&true));
    let saved = fs::read_to_string(&config).expect("settings were written");
    assert!(saved.contains("[db_mcp_shared]"), "{saved}");
    assert!(saved.contains("notes = true"), "{saved}");

    #[cfg(feature = "sql")]
    {
        use vix_mcp_server::Host as _;
        let host = vix::mcp::McpHost {
            settings_path: Some(config),
            db_log_path: Some(dir.join("mcp-db.log")),
            ..Default::default()
        };
        let shared = host.shared_connections().unwrap().unwrap();
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0].name, "notes");
        assert_eq!(shared[0].kind, "sqlite");
    }
    let _ = fs::remove_dir_all(&dir);
}

/// T781: a connection string pasted into the DB connection form fills it, the
/// environment row cycles to production, and the saved settings file holds the
/// connection and its environment but never the pasted password.
#[test]
fn pasted_connection_string_fills_the_db_form_and_saves_without_its_password() {
    let dir = unique_dir("db-paste");
    let config = dir.join("config.toml");
    let _ = fs::remove_file(&config);
    let mut app = app_at(&dir).with_settings_path(&config);
    app.run_action("db.connections");
    app.on_key(key('a'));
    app.on_paste("postgres://ops:hunter2@db.example.com:5433/shop?sslmode=require");
    let db = app.db.as_ref().expect("the DB overlay is open");
    assert_eq!(
        db.form.fields[0], "shop",
        "the name defaults to the database"
    );
    assert_eq!(db.form.fields[3], "db.example.com");
    assert!(db.form_password_held(), "the password is held, not typed");
    for _ in 0..vix::db::FORM_ENVIRONMENT {
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    for _ in 0..4 {
        app.on_key(key(' '));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let conn = &app.settings.db_connections[0];
    assert_eq!(conn.name, "shop");
    assert_eq!(conn.kind, vix::db::connect::Kind::Postgres);
    assert_eq!(conn.port, "5433");
    assert_eq!(conn.user, "ops");
    assert_eq!(conn.sslmode, "require");
    assert_eq!(
        app.settings.db_environments.get("shop"),
        Some(&vix::db::connect::Environment::Production)
    );
    let saved = fs::read_to_string(&config).expect("settings were written");
    assert!(!saved.contains("hunter2"), "a password is never saved");
    assert!(saved.contains("[db_environments]"), "{saved}");
    assert!(saved.contains("shop = \"production\""), "{saved}");

    // Ordinary text pasted into a field is typed as before.
    app.on_key(key('a'));
    app.on_paste("plain");
    assert_eq!(app.db.as_ref().unwrap().form.fields[0], "plain");
    let _ = fs::remove_dir_all(&dir);
}

/// Every DB workbench view draws, with real data behind it: a seeded
/// `SQLite` file, connected and queried, then each `View` drawn wide and
/// narrow (the smoke tests drive `Browser` directly and never draw).
#[test]
fn every_db_workbench_view_draws_with_data() {
    use crate::db::{Browser, Pages, View, connect, session::Session};

    fn settle(b: &mut Browser) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while b.query_running() || b.connect_running() || b.reconnect_running() {
            b.poll_query();
            b.poll_connect();
            b.poll_reconnect();
            assert!(Instant::now() < deadline, "DB work did not finish");
            std::thread::yield_now();
        }
    }
    fn press(b: &mut Browser, code: KeyCode) {
        b.handle_key(KeyEvent::new(code, KeyModifiers::NONE), Pages::default());
        settle(b);
    }

    let dir = unique_dir("db-draw");
    let file = dir.join("shop.db");
    {
        let mut s = Session::connect(&format!("sqlite:{}?mode=rwc", file.display()), &[]).unwrap();
        for sql in [
            "CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT)",
            "CREATE TABLE orders (id INTEGER PRIMARY KEY, customer_id INTEGER REFERENCES customers(id), total REAL)",
            "INSERT INTO customers VALUES (1, 'Ada'), (2, '李雷')",
            "INSERT INTO orders VALUES (1, 1, 9.5), (2, 2, 12.25)",
        ] {
            s.run(sql).unwrap();
        }
    }
    let conn = connect::Connection {
        name: "shop".into(),
        kind: connect::Kind::Sqlite,
        file: file.display().to_string(),
        writable: true,
        ..connect::Connection::default()
    };
    let mut app = app_at(&dir);
    let mut b = Browser::new(vec![conn]);
    app.db = Some(Browser::new(Vec::new()));
    // The connection list, before connecting.
    std::mem::swap(app.db.as_mut().unwrap(), &mut b);
    draw_frame(&mut app, 120, 40);
    std::mem::swap(app.db.as_mut().unwrap(), &mut b);

    press(&mut b, KeyCode::Enter);
    assert_eq!(b.view, View::Workbench, "{:?}", b.message);
    for c in "SELECT * FROM customers".chars() {
        press(&mut b, KeyCode::Char(c));
    }
    press(&mut b, KeyCode::Esc);
    press(&mut b, KeyCode::F(5));
    assert_eq!(b.grid.rows.len(), 2, "{:?}", b.message);
    app.db = Some(b);

    let screen = draw_frame(&mut app, 120, 40);
    assert!(screen.contains("Ada"), "{screen}");
    for view in [
        View::Connections,
        View::Form,
        View::Password,
        View::Workbench,
        View::History,
        View::Saved,
        View::SaveName,
        View::Confirm,
        View::Cell,
        View::Export,
        View::Log,
        View::Erd,
        View::Ask,
        View::Params,
        View::Import,
        View::CellEdit,
    ] {
        app.db.as_mut().unwrap().view = view;
        draw_frame(&mut app, 120, 40);
        draw_frame(&mut app, 50, 16);
    }
    app.db = None;
    fs::remove_dir_all(&dir).ok();
}

/// Settle the browser's background work (queries, connects, compares).
fn settle_db(b: &mut crate::db::Browser) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while b.query_running() || b.connect_running() || b.compare_running() {
        b.poll_query();
        b.poll_connect();
        b.poll_compare();
        assert!(Instant::now() < deadline, "DB work did not finish");
        std::thread::yield_now();
    }
}

/// A key to the browser, then its work settled.
fn press_db(b: &mut crate::db::Browser, code: KeyCode, mods: KeyModifiers) {
    b.handle_key(KeyEvent::new(code, mods), crate::db::Pages::default());
    settle_db(b);
}

/// Two `SQLite` files in `dir` (a source with `users`, `orders`, an index
/// and a view; an older, read-only target), connected to the source, with
/// the schema compare opened on its target picker (Ctrl+D).
fn compare_browser(dir: &Path) -> crate::db::Browser {
    use crate::db::{Browser, connect, session::Session};
    let seed = |path: &Path, sql: &[&str]| {
        let mut s = Session::connect(&format!("sqlite:{}?mode=rwc", path.display()), &[]).unwrap();
        for q in sql {
            s.run(q).unwrap();
        }
    };
    let conn = |name: &str, file: &Path, writable: bool| connect::Connection {
        name: name.into(),
        kind: connect::Kind::Sqlite,
        file: file.display().to_string(),
        writable,
        ..connect::Connection::default()
    };
    let (source, target) = (dir.join("source.db"), dir.join("target.db"));
    seed(
        &source,
        &[
            "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL)",
            "CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users(id), total REAL DEFAULT 0)",
            "CREATE INDEX orders_user_idx ON orders(user_id)",
            "CREATE VIEW big_orders AS SELECT id, total FROM orders WHERE total > 100",
        ],
    );
    seed(
        &target,
        &[
            "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL, legacy TEXT)",
            "CREATE VIEW v_legacy AS SELECT legacy FROM users",
        ],
    );
    let mut b = Browser::new(vec![
        conn("source", &source, true),
        conn("target", &target, false),
    ]);
    press_db(&mut b, KeyCode::Enter, KeyModifiers::NONE);
    press_db(&mut b, KeyCode::Char('d'), KeyModifiers::CONTROL);
    b
}

/// The schema compare, drawn screen by screen over two real `SQLite` files:
/// picking the target, the review list (moving, toggling, a change's
/// detail), the migration script, and the run confirmation.
#[test]
fn schema_compare_screens_draw_with_real_changes() {
    use crate::db::compare::review::Screen;
    let dir = unique_dir("db-compare-draw");
    let b = compare_browser(&dir);
    let mut app = app_at(&dir);
    let screen_of = |app: &App| app.db.as_ref().and_then(|b| b.compare()).map(|c| c.screen);
    app.db = Some(b);
    assert_eq!(screen_of(&app), Some(Screen::Pick));
    draw_frame(&mut app, 120, 40);
    draw_frame(&mut app, 50, 16);

    let step = |app: &mut App, code: KeyCode| {
        let b = app.db.as_mut().unwrap();
        press_db(b, code, KeyModifiers::NONE);
        let screen = draw_frame(app, 120, 40);
        draw_frame(app, 50, 16);
        screen
    };
    let screen = step(&mut app, KeyCode::Enter);
    assert_eq!(screen_of(&app), Some(Screen::Review));
    assert!(screen.contains("orders"), "{screen}");
    for code in [
        KeyCode::Down,
        KeyCode::Char(' '),
        KeyCode::Char(' '),
        KeyCode::End,
        KeyCode::Home,
        KeyCode::PageDown,
        KeyCode::PageUp,
        KeyCode::Char('y'), // copy the script
    ] {
        step(&mut app, code);
    }
    step(&mut app, KeyCode::Enter); // the script
    assert_eq!(screen_of(&app), Some(Screen::Script));
    for code in [
        KeyCode::Down,
        KeyCode::PageDown,
        KeyCode::PageUp,
        KeyCode::Up,
        KeyCode::Home,
    ] {
        step(&mut app, code);
    }
    step(&mut app, KeyCode::Esc);
    assert_eq!(
        screen_of(&app),
        Some(Screen::Review),
        "Esc goes back to the review"
    );
    step(&mut app, KeyCode::Char('s')); // save the script: asks for a path
    assert_eq!(screen_of(&app), Some(Screen::SavePath));
    step(&mut app, KeyCode::Esc);
    // Run against the read-only target: refused with a message, still here.
    step(&mut app, KeyCode::Char('r'));
    assert_eq!(
        screen_of(&app),
        Some(Screen::Review),
        "a read-only target is not run"
    );
    assert!(app.db.as_ref().unwrap().message.is_some());
    // F8 allows writing to the target; then Run asks to confirm, and Esc
    // backs out without running anything.
    step(&mut app, KeyCode::F(8));
    step(&mut app, KeyCode::Char('r'));
    if screen_of(&app) == Some(Screen::Confirm) {
        step(&mut app, KeyCode::Esc);
    }
    assert_eq!(screen_of(&app), Some(Screen::Review));
    // `o` hands the script to the workbench editor and closes the compare.
    step(&mut app, KeyCode::Char('o'));
    let b = app.db.as_ref().unwrap();
    assert!(b.compare().is_none());
    assert!(
        b.query.text().contains("CREATE TABLE"),
        "{}",
        b.query.text()
    );
    app.db = None;
    fs::remove_dir_all(&dir).ok();
}

/// The row-search form and the file-path prompt draw, wide and narrow, with
/// their live previews and cursors.
#[test]
fn row_filter_and_path_prompt_screens_draw() {
    use crate::db::{Browser, View};
    let mut app = app_at(Path::new("."));
    let mut b = Browser::new(Vec::new());
    b.row_filter.open = true;
    b.row_filter.value = "abc".into();
    for field in 0..3 {
        b.row_filter.field = field;
        app.db = Some(std::mem::replace(&mut b, Browser::new(Vec::new())));
        draw_frame(&mut app, 120, 40);
        draw_frame(&mut app, 50, 16);
        b = app.db.take().unwrap();
    }
    b.row_filter.open = false;
    b.view = View::Import;
    b.import_path = "data.csv".into();
    app.db = Some(b);
    let screen = draw_frame(&mut app, 120, 40);
    assert!(screen.contains("data.csv"), "{screen}");
    draw_frame(&mut app, 50, 16);
}
