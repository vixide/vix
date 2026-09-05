#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]

// Test setup casts small counts to `u16` cell coordinates and builds fixture
// strings by collecting `format!`; both are fine in tests.

pub(crate) use std::fs;
pub(crate) use std::path::{Path, PathBuf};

pub(crate) use crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
pub(crate) use ratatui::layout::Rect;

pub(crate) use vix::app::{App, Focus, PromptKind};
pub(crate) use vix::calendar;
pub(crate) use vix::clock;
pub(crate) use vix::fileops;
pub(crate) use vix::palette::{fuzzy_match, parse_path_target};
pub(crate) use vix::search::SearchBar;
pub(crate) use vix::settings::Settings;

pub(crate) fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

pub(crate) fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

pub(crate) fn keycode(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub(crate) fn esc() -> KeyEvent {
    keycode(KeyCode::Esc)
}

pub(crate) fn func(n: u8) -> KeyEvent {
    keycode(KeyCode::F(n))
}

pub(crate) fn mouse(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

pub(crate) fn click(col: u16, row: u16) -> MouseEvent {
    mouse(MouseEventKind::Down(MouseButton::Left), col, row)
}

/// Build an app with a realistic editor viewport so the code editor's
/// scroll-into-view logic has a sane area to work with.
/// A private `session.toml` path, unique per call — so no test (run
/// sequentially or, as `cargo test` does by default, in parallel with
/// others) ever reads or writes the real developer's session file, and no
/// two `App`s in the same test binary ever share one either (T132 was the
/// first feature to make a normal `App` write to its session at all;
/// before it, no existing test touched this).
pub(crate) fn isolated_session_path() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("vix-test-session-{}-{n}.toml", std::process::id()))
}

pub(crate) fn app_at(root: &Path) -> App {
    let mut app = App::new(root.to_path_buf(), Settings::default())
        .with_session_path(isolated_session_path());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

pub(crate) fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-{tag}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Build an app with custom settings and a realistic editor viewport.
pub(crate) fn app_with(settings: Settings) -> App {
    let mut app =
        App::new(Path::new(".").to_path_buf(), settings).with_session_path(isolated_session_path());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

/// If a script-trust prompt (T132) is pending, answer "yes" -- the same
/// flow a real user granting workspace trust goes through, via the real
/// `on_key` dispatch. Safe to call even when nothing is pending (a fresh
/// `.vix/scripts/` with no files yet queues no prompt at all).
pub(crate) fn trust_scripts_if_prompted(app: &mut App) {
    if app.script_trust.is_some() {
        app.on_key(key('y'));
    }
}

/// How many script-registered commands are currently loaded, via the real
/// `script.run` chooser (`App::scripts` itself is private) -- closes the
/// chooser it opens to check, so it doesn't leak into whatever the test
/// does next.
pub(crate) fn loaded_script_command_count(app: &mut App) -> usize {
    app.run_action("script.run");
    let n = app.script_chooser.as_ref().map_or(0, |c| c.commands.len());
    app.script_chooser = None;
    n
}

/// `load_scripts()` plus an immediate "yes" to this workspace's trust
/// prompt (T132), for tests whose point is a script's own behavior, not
/// the trust gate itself.
pub(crate) fn load_scripts_trusted(app: &mut App) {
    app.load_scripts();
    app.maybe_prompt_script_trust();
    trust_scripts_if_prompted(app);
}

pub(crate) fn alt(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::ALT)
}

pub(crate) fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        if c == '\n' {
            app.on_key(keycode(KeyCode::Enter));
        } else {
            app.on_key(key(c));
        }
    }
}

pub(crate) fn node_index(app: &App, name: &str) -> usize {
    app.explorer
        .nodes
        .iter()
        .position(|n| n.name == name)
        .unwrap_or_else(|| panic!("no explorer node named {name}"))
}

/// Column inside the `idx`-th top-level menu's title (mirrors the bar layout),
/// computed from the actual titles so it is locale-independent.
pub(crate) fn top_menu_col(app: &App, idx: usize) -> u16 {
    let mut x = app.layout.menu.x + 1;
    for m in &vix::menu::menus()[..idx] {
        x += m.title().chars().count() as u16 + 2;
    }
    x + 1
}

/// Fill the active buffer with `text` and put the cursor at char `cursor`.
pub(crate) fn buffer_with(app: &mut App, text: &str, cursor: usize) {
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content(text);
    tab.editor.set_cursor(cursor);
}
