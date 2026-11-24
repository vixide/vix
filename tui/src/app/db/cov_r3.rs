//! Behavior tests for the DB workbench glue that need no database: action
//! dispatch, the "not connected" paths, paste, closing, and the pollers'
//! idle answers. Keys that would record history are never sent, so nothing
//! here writes the user's DB history files.

use super::*;
use crate::app::cov4::{app_at as base_app_at, buffer_with, code, key, temp};
use vix_input::KeyCode;

fn app_at(d: &std::path::Path) -> App {
    let mut app = base_app_at(d);
    app.settings_path = Some(d.join("vix-test-config.toml"));
    app
}

#[test]
fn db_actions_open_the_workbench_and_unknown_ones_are_refused() {
    let d = temp("db-actions");
    let mut app = app_at(&d);
    assert!(!app.db_action("db.nothing"));
    assert!(app.db.is_none());
    assert!(app.db_action("db.connections"));
    assert!(matches!(
        app.db.as_ref().unwrap().view,
        crate::db::View::Connections
    ));
    // Without a live connection the query view does not open.
    assert!(app.db_action("db.query"));
    assert!(matches!(
        app.db.as_ref().unwrap().view,
        crate::db::View::Connections
    ));
    // Every other action runs against the browser without a connection and
    // leaves it open.
    for action in [
        "db.execute",
        "db.execute_all",
        "db.explain",
        "db.explain_analyze",
        "db.format",
        "db.begin",
        "db.commit",
        "db.rollback",
        "db.refresh",
        "db.disconnect",
        "db.history",
        "db.saved",
        "db.save_query",
        "db.export",
        "db.schema_compare",
    ] {
        assert!(app.db_action(action), "{action}");
        assert!(app.db.is_some(), "{action}");
    }
}

#[test]
fn running_the_buffer_needs_a_buffer_and_a_connection() {
    let d = temp("db-run-buffer");
    let mut app = app_at(&d);
    app.editor.tabs.clear();
    let before = app.messages.items.len();
    app.db_action("db.run_buffer");
    assert_eq!(app.messages.items.len(), before + 1);
    assert_eq!(
        app.messages.items.last().map(|m| m.text.clone()),
        Some(t!("msg.edit_sql_no_buffer").to_string())
    );
    assert!(app.db.is_none(), "nothing opens without a buffer");

    app.editor.new_tab();
    buffer_with(&mut app, "select 1;", 0);
    app.db_action("db.run_buffer");
    let b = app.db.as_ref().unwrap();
    assert!(matches!(b.view, crate::db::View::Connections));
    assert_eq!(
        b.message.as_deref(),
        Some(t!("msg.db_not_connected").as_ref())
    );
}

#[test]
fn the_overlay_takes_keys_only_while_open_and_esc_closes_it() {
    let d = temp("db-keys");
    let mut app = app_at(&d);
    // No browser: keys and pastes are ignored.
    app.db_key(key('x'));
    assert!(!app.db_paste("postgres://u@h/db"));
    app.db_action("db.connections");
    // Outside the connection form a paste is not a connection string.
    assert!(!app.db_paste("postgres://u@h/db"));
    app.db_key(code(KeyCode::Esc));
    assert!(app.db.is_none(), "Esc closes the connections list");
}

#[test]
fn the_pollers_are_idle_without_tasks() {
    let d = temp("db-poll");
    let mut app = app_at(&d);
    assert!(!app.db_query_running());
    assert!(!app.db_connect_running());
    assert!(!app.db_reconnect_running());
    app.poll_db_query();
    app.poll_db_connect();
    app.poll_db_reconnect();
    app.open_db();
    app.poll_db_query();
    app.poll_db_connect();
    app.poll_db_reconnect();
    assert!(!app.db_query_running());
    assert!(!app.db_connect_running());
    assert!(!app.db_reconnect_running());
}

#[test]
fn an_ai_request_from_the_workbench_fails_cleanly_when_ai_is_off() {
    let d = temp("db-ai");
    let mut app = app_at(&d);
    app.settings.ai = "off".to_string();
    app.open_db();
    // No request pending: nothing happens.
    app.db_run_ai_request();
    assert!(app.ai_replace.is_none());
}

#[test]
fn deleting_a_saved_connection_updates_and_persists_the_settings() {
    let d = temp("db-delete");
    let mut settings = crate::settings::Settings::default();
    settings.db_connections = ["alpha-db", "beta-db"]
        .iter()
        .map(|n| vix_settings::Connection {
            name: (*n).to_string(),
            kind: vix_settings::Kind::Sqlite,
            file: format!("/tmp/{n}.db"),
            ..vix_settings::Connection::default()
        })
        .collect();
    let mut app = crate::app::cov4::app_with(&d, settings);
    let config = d.join("vix-test-config.toml");
    app.settings_path = Some(config.clone());
    app.db_action("db.connections");
    assert_eq!(app.db.as_ref().unwrap().connections.len(), 2);
    // `d` removes the highlighted connection; the App's settings follow and
    // are written (to this test's own file).
    app.db_key(key('d'));
    assert_eq!(app.settings.db_connections.len(), 1);
    assert_eq!(app.settings.db_connections[0].name, "beta-db");
    let saved = std::fs::read_to_string(&config).unwrap();
    assert!(
        saved.contains("beta-db") && !saved.contains("alpha-db"),
        "{saved}"
    );
    // An unwritable settings location is a warning, not a failure.
    app.settings_path = Some(d.join("missing-dir").join("..").join("x").join("y.toml"));
    std::fs::write(d.join("x"), "a file where a directory is needed").unwrap();
    let before = app.messages.items.len();
    app.db_key(key('d'));
    assert_eq!(app.settings.db_connections, []);
    assert!(app.messages.items.len() > before);
}
