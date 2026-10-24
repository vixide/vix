#![allow(
    clippy::cast_possible_truncation,
    clippy::format_collect,
    reason = "tests count and index small fixtures, and build expected text with format! over iterators"
)]

// Test setup casts small counts to `u16` cell coordinates and builds fixture
// strings by collecting `format!`; both are fine in tests.

pub(crate) use std::fs;
pub(crate) use std::path::{Path, PathBuf};
// Used by `wait_until` in every build, not only with `ai` (T605's helper).
pub(crate) use std::time::{Duration, Instant};

pub(crate) use ratatui::layout::Rect;
pub(crate) use vix_input::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

pub(crate) use vix::app::{App, AppFlags, Focus, PromptKind};
pub(crate) use vix::calendar;
pub(crate) use vix::clock;
pub(crate) use vix::fileops;
pub(crate) use vix::palette::{fuzzy_match, parse_path_target};
pub(crate) use vix::search::{Flags as SearchFlags, SearchBar};
#[cfg(feature = "ai")]
pub(crate) use vix::settings::MiscSettings;
pub(crate) use vix::settings::{Settings, SubsystemSettings};
pub(crate) use vix::workspace_search::Flags as WorkspaceFlags;

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

/// Initialize a git repo at `dir` with a deterministic identity, for tests
/// that need a throwaway repo. Extracted (Run H, T523) after this exact
/// `git init` + two `git config` calls were found copy-pasted at 13 call
/// sites across `git.rs`/`editing.rs`. `name` is the commit author name —
/// almost every caller wants the literal `"Test"`; one test (proving `git
/// blame` surfaces the real author, not a hardcoded placeholder) passes a
/// distinctive one instead. Callers still declare their own `run` closure
/// for whatever git commands they need after this (`add`, `commit`, …) —
/// only the init/identity sequence itself was ever actually duplicated.
pub(crate) fn init_git_repo(dir: &Path, name: &str) {
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(dir)
            .env("GIT_TERMINAL_PROMPT", "0")
            .args(args)
            .output()
            .unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", name]);
}

/// Build an app with custom settings and a realistic editor viewport.
pub(crate) fn app_with(settings: Settings) -> App {
    let mut app =
        App::new(Path::new(".").to_path_buf(), settings).with_session_path(isolated_session_path());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

/// Build an app rooted at `root` with custom `settings` and a realistic
/// editor viewport -- the one combination neither `app_at` (default
/// settings) nor `app_with` (always rooted at `.`) covers on its own.
/// Extracted (Run H, T530) after two call sites each hand-rolled this exact
/// three-line builder because they needed both a real root and custom
/// settings (an `ai_command` override) together.
#[cfg(any(feature = "ai", feature = "git", feature = "tasks"))]
pub(crate) fn app_at_with(root: &Path, settings: Settings) -> App {
    let mut app = App::new(root.to_path_buf(), settings).with_session_path(isolated_session_path());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

/// Poll until `pred` holds, or fail after `timeout`. `poll` is the app's own
/// drain call (`poll_command`, `poll_dashboard`, ...), run before every check
/// exactly as the event loop would. One shared bounded loop (T605) instead of a
/// hand-rolled iteration count at each call site; give it plenty of headroom
/// (10 s) -- slow macOS and Windows runners lag far behind a local machine, and
/// the loop returns the moment `pred` holds, so a generous bound costs nothing.
pub(crate) fn wait_until(
    app: &mut App,
    mut poll: impl FnMut(&mut App),
    mut pred: impl FnMut(&App) -> bool,
    timeout: Duration,
) {
    let deadline = Instant::now() + timeout;
    loop {
        poll(app);
        if pred(app) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out after {timeout:?} waiting for the condition"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Poll `App::poll_ai_replace` until `pred` holds, or fail. Extracted (Run H,
/// T530) after this exact loop was found duplicated (once as this named helper
/// in `ai.rs`, once inlined in `git.rs`); now a thin wrapper over [`wait_until`].
#[cfg(feature = "ai")]
pub(crate) fn wait_for_ai_replace(app: &mut App, pred: impl FnMut(&App) -> bool) {
    wait_until(app, App::poll_ai_replace, pred, Duration::from_secs(10));
}

/// If a script-trust prompt (T132) is pending, answer "yes" -- the same
/// flow a real user granting workspace trust goes through, via the real
/// `on_key` dispatch. Safe to call even when nothing is pending (a fresh
/// `.vix/scripts/` with no files yet queues no prompt at all).
#[cfg(feature = "script")]
pub(crate) fn trust_scripts_if_prompted(app: &mut App) {
    if app.script_trust.is_some() {
        app.on_key(key('y'));
    }
}

/// How many script-registered commands are currently loaded, via the real
/// `script.run` chooser (`App::scripts` itself is private) -- closes the
/// chooser it opens to check, so it doesn't leak into whatever the test
/// does next.
#[cfg(feature = "script")]
pub(crate) fn loaded_script_command_count(app: &mut App) -> usize {
    app.run_action("script.run");
    let n = app.script_chooser.as_ref().map_or(0, |c| c.commands.len());
    app.script_chooser = None;
    n
}

/// `load_scripts()` plus an immediate "yes" to this workspace's trust
/// prompt (T132), for tests whose point is a script's own behavior, not
/// the trust gate itself.
#[cfg(feature = "script")]
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
