//! Shared fixtures for the coverage tests of `scripts`, `ai`, `keymap`,
//! `picker_panels`, `session`, `oc`, `helix` and friends (test-only).

use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::settings::Settings;

/// A fresh, empty temp directory unique to one call.
pub(crate) fn temp(tag: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("vix-cov4-{tag}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Canonical, so paths compare equal to what the app reports (macOS
    // temp dirs sit behind the `/var` -> `/private/var` symlink).
    dir.canonicalize().unwrap()
}

/// An [`App`] rooted at `root` with default settings and a private session
/// file, never touching the user's real config.
pub(crate) fn app_at(root: &Path) -> App {
    app_with(root, Settings::default())
}

/// [`app_at`] with explicit settings.
pub(crate) fn app_with(root: &Path, settings: Settings) -> App {
    let session = root.join("session.toml");
    let mut app = App::new(root.to_path_buf(), settings).with_session_path(session);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

/// A plain-character key press.
pub(crate) fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// A non-character key press.
pub(crate) fn code(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Alt + a character.
pub(crate) fn alt(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)
}

/// Ctrl + a character.
pub(crate) fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

/// Replace the active buffer with `text` and put the cursor at `cursor`.
pub(crate) fn buffer_with(app: &mut App, text: &str, cursor: usize) {
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content(text);
    tab.editor.set_cursor(cursor);
}

/// The active buffer's full text.
pub(crate) fn content(app: &mut App) -> String {
    app.editor.active_tab_mut().unwrap().editor.get_content()
}

/// The text of the newest message in the drawer, if any.
pub(crate) fn last_message(app: &App) -> Option<String> {
    app.messages.items.last().map(|m| m.text.clone())
}

/// Serializes this module's tests that read or write the process-global
/// clipboard (the editor-core lock is crate-private, so the tui tests keep
/// their own).
pub(crate) static CLIP: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A left-button press at (`col`, `row`).
pub(crate) fn click(col: u16, row: u16) -> vix_input::MouseEvent {
    vix_input::MouseEvent {
        kind: vix_input::MouseEventKind::Down(vix_input::MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}
