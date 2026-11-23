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

/// A workbench connected, through the real background connect, to a `SQLite`
/// file in a fresh scratch directory that holds `ddl`. Returns the browser
/// and the directory (the caller removes it).
pub(crate) fn file_workbench(
    name: &str,
    writable: bool,
    ddl: &[&str],
) -> (Browser, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("vix-wb-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("db.sqlite").display().to_string();
    let mut seed =
        session::Session::connect(&catalog::sqlite_url(&file, "rwc"), &[]).expect("seed db");
    for sql in ddl {
        seed.run(sql).expect("fixture statement");
    }
    drop(seed);
    let mut b = Browser::new(vec![connect::Connection {
        name: "file".into(),
        kind: connect::Kind::Sqlite,
        file,
        writable,
        ..connect::Connection::default()
    }]);
    b.view = View::Connections;
    press(&mut b, KeyCode::Enter);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while b.connect_running() {
        b.poll_connect();
        assert!(std::time::Instant::now() < deadline, "connect stalled");
        std::thread::yield_now();
    }
    assert_eq!(b.view, View::Workbench, "{:?}", b.message);
    (b, dir)
}

/// Serializes the tests that install a process-global custom theme.
static THEME: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Run `f` with a custom theme named `themed` active (colors from `json`'s
/// `syntax`), restoring "no theme" afterwards, even if `f` panics.
pub(crate) fn with_theme<R>(json: &str, f: impl FnOnce() -> R) -> R {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            vix_theme::set_custom(None);
        }
    }
    let _guard = THEME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _reset = Reset;
    vix_theme::set_custom(Some(
        serde_json::from_str::<vix_theme::CustomTheme>(json).expect("a valid theme"),
    ));
    f()
}

/// Run `f` with no custom theme active, serialized against [`with_theme`].
pub(crate) fn without_theme<R>(f: impl FnOnce() -> R) -> R {
    let _guard = THEME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    vix_theme::set_custom(None);
    f()
}
