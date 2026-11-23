//! Shared fixtures for the workbench tests: a [`Browser`] connected to an
//! in-memory `SQLite` database, driven by real key events.

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

use crate::{Browser, Pages, Pane, View, catalog, connect, session};

/// A plain key press.
pub(crate) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// A Ctrl-modified key press.
pub(crate) fn ctrl(b: &mut Browser, c: char) {
    b.handle_key(
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL),
        Pages::default(),
    );
}

/// Press one key with default pane sizes.
pub(crate) fn press(b: &mut Browser, code: KeyCode) {
    b.handle_key(key(code), Pages::default());
}

/// Press each character of `text` in turn.
pub(crate) fn typed(b: &mut Browser, text: &str) {
    for c in text.chars() {
        press(b, KeyCode::Char(c));
    }
}

/// A workbench connected to a fresh in-memory `SQLite` database that has had
/// `ddl` run on it. `writable` mirrors the connection's write flag.
pub(crate) fn workbench(writable: bool, ddl: &[&str]) -> Browser {
    let mut b = Browser::new(Vec::new());
    let mut session = session::Session::connect("sqlite::memory:", &[]).expect("memory db");
    for sql in ddl {
        session.run(sql).expect("fixture statement");
    }
    b.session = Some(session);
    b.conn = Some(connect::Connection {
        name: "mem".into(),
        kind: connect::Kind::Sqlite,
        writable,
        ..connect::Connection::default()
    });
    b.write_enabled = writable;
    b.view = View::Workbench;
    b.focus = Pane::Tree;
    b.refresh_catalog();
    b
}

/// The usual two-table schema: `parent` and a `child` that references it.
pub(crate) fn family(writable: bool) -> Browser {
    workbench(
        writable,
        &[
            "CREATE TABLE parent (id INTEGER PRIMARY KEY, name TEXT)",
            "CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER \
             REFERENCES parent(id), status TEXT CHECK (status IN ('open','done')))",
            "INSERT INTO parent VALUES (1,'alpha'),(2,'beta')",
            "INSERT INTO child VALUES (10,1,'open'),(11,2,'done')",
        ],
    )
}

/// Move the tree selection onto the table or view called `name`.
pub(crate) fn select_table(b: &mut Browser, name: &str) {
    for schema in &mut b.tree.schemas {
        schema.expanded = true;
        for folder in [catalog::Folder::Tables, catalog::Folder::Views] {
            *schema.folder_flag_mut(folder) = true;
        }
    }
    let at = (0..b.tree.rows().len())
        .find(|&i| {
            b.tree.sel = i;
            b.tree
                .selected_object()
                .is_some_and(|(_, n, folder)| n == name && folder != catalog::Folder::Functions)
        })
        .expect("table in the tree");
    b.tree.sel = at;
}

/// Preview `name` (tree `p`), leaving focus on the results grid.
pub(crate) fn preview(b: &mut Browser, name: &str) {
    b.focus = Pane::Tree;
    select_table(b, name);
    press(b, KeyCode::Char('p'));
    assert_eq!(b.focus, Pane::Results, "{:?}", b.message);
}
