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
