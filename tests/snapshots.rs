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
//! See "Reviewing/updating snapshots" in `agents/conventions.md` for the
//! workflow when a golden file needs to change.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

use vix::app::App;
use vix::settings::Settings;

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
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

/// Build an app with a realistic viewport, the same way
/// `tests/integration.rs`'s `app_at` does.
fn app_at(root: &Path) -> App {
    rust_i18n::set_locale("en");
    let mut app = App::new(root.to_path_buf(), Settings::default());
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 100, 30);
    app
}

/// Render one frame and flatten it to plain text: one line per row, each
/// trimmed of trailing spaces so an unrelated cell-width fix elsewhere in
/// the screen doesn't ripple through every row's diff.
fn render_screen(app: &mut App, width: u16, height: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
    let buf = term.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn default_screen_with_no_file_open() {
    let mut app = app_at(Path::new("."));
    let screen = render_screen(&mut app, 100, 30);
    insta::assert_snapshot!(screen);
}

#[test]
fn editor_with_typed_rust_source() {
    let mut app = app_at(Path::new("."));
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
    let mut app = app_at(Path::new("."));
    app.run_action("tools.palette");
    type_str(&mut app, ">goto");
    let screen = render_screen(&mut app, 100, 30);
    insta::assert_snapshot!(screen);
}
