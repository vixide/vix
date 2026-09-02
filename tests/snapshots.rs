//! TUI snapshot tests (`spec/test/index.md`'s "Snapshot" layer).
//!
//! Each test boots a real [`vix::app::App`], drives it with scripted key
//! events exactly like `tests/integration.rs` does, renders one frame to a
//! ratatui [`TestBackend`] at a fixed size, flattens that frame to plain
//! text, and compares it against a golden file under `tests/snapshots/`
//! with `insta::assert_snapshot!`.
//!
//! Screens are rendered at 100×30 unless a scenario needs a different size.
//! The locale is pinned to `en` in every test (`rust_i18n::locale()` is
//! process-global) so a screen's snapshot never depends on run order or on
//! another test file's locale changes elsewhere in the suite.
//!
//! Each app opens a small synthetic fixture tree, not the real repo root —
//! the repo checkout carries local-only, untracked top-level entries (build
//! output, downloaded dictionaries, …) that differ between a workstation and
//! a fresh CI checkout, which would make the explorer pane's contents (and
//! so the golden screen) depend on where the test runs.
//!
//! See "Reviewing/updating snapshots" in `agents/conventions.md` for the
//! workflow when a golden file needs to change.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use vix::app::App;
use vix::settings::Settings;

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn func(n: u8) -> KeyEvent {
    KeyEvent::new(KeyCode::F(n), KeyModifiers::NONE)
}

fn alt(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::ALT)
}

fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        if c == '\n' {
            app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        } else {
            app.on_key(key(c));
        }
    }
}

/// A small, fixed project tree under a fresh temp directory keyed by `tag`
/// and the process id, so parallel tests never collide and nothing here
/// depends on the machine running the test.
///
/// Rooted under `/tmp` rather than `std::env::temp_dir()` deliberately: a
/// scenario that opens a real file shows its full canonicalized path in the
/// status bar (`Tab::path` is always `path.canonicalize()`d — see
/// `Editor::open` in `crates/vix-editor/src/editor.rs`), and `render_screen`
/// redacts that whole path back out to `<root>` — but only if it survives
/// intact. `std::env::temp_dir()` resolves (via `TMPDIR`) to a long,
/// per-session path on macOS (`/var/folders/<hash>/T/`, itself a symlink
/// `canonicalize()` resolves to `/private/var/folders/...`) that the status
/// bar's limited width truncates before the redaction ever sees the whole
/// string, leaving a machine- and run-specific fragment in the golden file.
/// `/tmp` canonicalizes to something short on both Linux (`/tmp`) and macOS
/// (`/private/tmp`) — short enough that the full path fits and gets redacted
/// cleanly. (Unix-only, matching this suite's `ubuntu-latest`/`macos-latest`
/// CI matrix — no `windows-latest` job to keep portable for.)
fn fixture_root(tag: &str) -> PathBuf {
    let dir = PathBuf::from("/tmp").join(format!("vix-snapshot-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
    fs::write(dir.join("README.md"), "# Demo\n").unwrap();
    fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
    dir
}

/// A fixture tree (see [`fixture_root`]) turned into a git repo with one
/// commit, then given an unstaged change to `README.md` so `git.changes`
/// has something to show. The branch name is forced explicitly
/// (`git init -b`) rather than left to `init.defaultBranch`, which differs
/// by machine (`master` vs `main` vs a custom default) and would otherwise
/// leak into the golden screen's title bar.
fn git_fixture_root(tag: &str) -> PathBuf {
    let dir = fixture_root(tag);
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .current_dir(&dir)
            .args(args)
            .status()
            .expect("git must be on PATH to build the fixture repo");
        assert!(status.success(), "git {args:?} failed");
    };
    git(&["init", "-b", "vix-test"]);
    git(&["config", "user.name", "vix-test"]);
    git(&["config", "user.email", "vix-test@example.invalid"]);
    // Belt-and-braces against a global `core.autocrlf`/`.gitattributes`
    // normalizing the freshly-written files into a spurious "modified".
    git(&["config", "core.autocrlf", "false"]);
    // Never inherit the real developer's `commit.gpgsign=true` (global
    // config) for this throwaway fixture repo — it would invoke their real
    // signing key just to build a test fixture, and could hang on a
    // passphrase prompt.
    git(&["config", "commit.gpgsign", "false"]);
    git(&["add", "-A"]);
    git(&["commit", "-m", "initial"]);
    fs::write(dir.join("README.md"), "# Demo\n\nUpdated.\n").unwrap();
    dir
}

/// Build an app with a realistic viewport, rooted at `root`.
fn app_at_root(root: PathBuf) -> App {
    rust_i18n::set_locale("en");
    let mut app = App::new(root, Settings::default());
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 100, 30);
    app
}

/// Build an app rooted at a fresh fixture tree unique to `tag`.
fn app_at(tag: &str) -> App {
    app_at_root(fixture_root(tag))
}

/// Render one frame and flatten it to plain text: one line per row, each
/// trimmed of trailing spaces so an unrelated cell-width fix elsewhere in
/// the screen doesn't ripple through every row's diff.
///
/// Any row containing the fixture root's absolute path (raw or
/// canonicalized — e.g. macOS resolves `/var` to `/private/var`) is
/// replaced wholesale with a fixed placeholder line, not just had that
/// substring swapped out: the status bar shows the full path for a file
/// outside the workspace-relative case (and, for a freshly opened file,
/// shows it a *second* time inside the status message, `t!("status.opened",
/// path = ...)`), and that path bakes in both the temp directory's
/// OS-specific location and this process's id (the fixture is keyed by
/// it). A substring swap alone isn't enough — the status bar right-aligns
/// trailing fields (language, line ending, cursor position) against the
/// *real*, pre-redaction path length, so even a same-length replacement
/// token leaves a residue: a different amount of padding survives
/// depending on how long the real path happened to be, which still differs
/// by PID digit count alone (confirmed: this broke CI even after fixing
/// truncation — same OS as the machine that generated the golden file,
/// just a different-length PID). Blanking the whole row sidesteps that
/// layout dependency entirely instead of trying to out-think it.
fn render_screen(app: &mut App, width: u16, height: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
    let buf = term.backend().buffer();
    let raw = app.root.display().to_string();
    let canonical = fs::canonicalize(&app.root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| raw.clone());
    (0..height)
        .map(|y| {
            let line = (0..width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string();
            if line.contains(&canonical) || line.contains(&raw) {
                "<status bar redacted: shows the opened file's absolute path>".to_string()
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn default_screen_with_no_file_open() {
    let mut app = app_at("default-screen");
    let screen = render_screen(&mut app, 100, 30);
    insta::assert_snapshot!(screen);
}

#[test]
fn editor_with_typed_rust_source() {
    let mut app = app_at("typed-rust-source");
    type_str(&mut app, "fn main() {\n    println!(\"hi\");\n}\n");
    let screen = render_screen(&mut app, 100, 30);
    insta::assert_snapshot!(screen);
}

#[test]
fn command_palette_open_with_a_query() {
    // `>` switches the palette to Commands mode, which ranks with
    // `palette::fuzzy_score` and ties-break on a stable catalog index — unlike
    // the default Files mode, which pushes matches in raw `ignore::WalkBuilder`
    // order (filesystem-traversal order, not sorted; see `build_file_index` in
    // `src/app.rs`) and so is not safe to pin in a golden screen.
    let mut app = app_at("command-palette");
    app.run_action("tools.palette");
    type_str(&mut app, ">goto");
    let screen = render_screen(&mut app, 100, 30);
    insta::assert_snapshot!(screen);
}

#[test]
fn welcome_screen() {
    let mut app = app_at("welcome");
    app.run_action("help.welcome");
    let screen = render_screen(&mut app, 100, 30);
    insta::assert_snapshot!(screen);
}

#[test]
fn editor_with_an_opened_rust_file() {
    let root = fixture_root("real-rust-file");
    fs::write(
        root.join("src/main.rs"),
        "struct Point {\n    x: i32,\n    y: i32,\n}\n\nfn main() {\n    let p = Point { x: 1, y: 2 };\n    println!(\"{p:?}\");\n}\n",
    )
    .unwrap();
    let mut app = app_at_root(root.clone());
    app.open_initial(&root.join("src/main.rs"));
    // Wide viewport: see the `render_screen` doc comment on why a scenario
    // that shows an opened file's path needs the whole thing to fit
    // unclipped.
    let screen = render_screen(&mut app, 220, 30);
    insta::assert_snapshot!(screen);
}

#[test]
fn file_menu_open() {
    // Alt+F is the File menu's mnemonic (`menu_index_for_alt` in `src/app.rs`).
    let mut app = app_at("file-menu");
    app.on_key(alt(KeyCode::Char('f')));
    let screen = render_screen(&mut app, 100, 30);
    insta::assert_snapshot!(screen);
}

#[test]
fn find_bar_with_matches() {
    let root = fixture_root("find-bar");
    fs::write(
        root.join("src/main.rs"),
        "fn main() {\n    let main_result = compute_main();\n    println!(\"{main_result}\");\n}\n",
    )
    .unwrap();
    let mut app = app_at_root(root.clone());
    app.open_initial(&root.join("src/main.rs"));
    app.run_action("edit.find");
    type_str(&mut app, "main");
    // Wide viewport — see the `render_screen` doc comment.
    let screen = render_screen(&mut app, 220, 30);
    insta::assert_snapshot!(screen);
}

#[test]
fn git_panel_with_changes() {
    let root = git_fixture_root("git-panel");
    let mut app = app_at_root(root);
    app.run_action("git.changes");
    let screen = render_screen(&mut app, 100, 30);
    insta::assert_snapshot!(screen);
}

#[test]
fn table_edit_surface() {
    let mut app = app_at("table-edit");
    type_str(&mut app, "name,role\nAda,Engineer\nGrace,Admiral\n");
    app.run_action("tools.edit_table");
    let screen = render_screen(&mut app, 100, 30);
    insta::assert_snapshot!(screen);
}

#[test]
fn f1_help_overlay() {
    let mut app = app_at("help-overlay");
    app.on_key(func(1));
    let screen = render_screen(&mut app, 100, 30);
    insta::assert_snapshot!(screen);
}

#[test]
fn zen_mode_hides_the_docks() {
    let mut app = app_at("zen-mode");
    type_str(&mut app, "fn main() {}\n");
    app.run_action("view.zen");
    let screen = render_screen(&mut app, 100, 30);
    insta::assert_snapshot!(screen);
}

#[test]
fn theme_other_than_default() {
    // A bundled theme name (from `themes/nord.json`), not one loaded from a
    // user's on-disk custom-themes directory — that keeps this deterministic
    // regardless of what the machine running the test happens to have there.
    let mut app = app_at("theme-nord");
    app.run_action("view.theme:Nord");
    let screen = render_screen(&mut app, 100, 30);
    insta::assert_snapshot!(screen);
}
