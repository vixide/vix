//! T628: terminal protocol polish — focus events (reload on focus gained,
//! `auto_save = "focus_lost"`), the color-depth downsample of every frame,
//! and OSC 8 hyperlinks in the markdown preview and the messages dock.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;
use ratatui::style::Color;
use vix::settings::AutoSave;
use vix::term_caps::TerminalCaps;
use vix::theme::ColorDepth;

/// Open `path` and replace its buffer with `text`, marking it dirty.
fn open_dirty(app: &mut App, path: &Path, text: &str) {
    app.editor.open(path, false).unwrap();
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content(text);
    tab.dirty = true;
}

/// Draw one frame into a test terminal and return its buffer.
fn render(app: &mut App) -> ratatui::buffer::Buffer {
    use ratatui::{Terminal, backend::TestBackend};
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
    term.backend().buffer().clone()
}

#[test]
fn focus_gained_reloads_a_file_changed_on_disk_at_once() {
    let dir = unique_dir("t628-focus-reload");
    let path = dir.join("a.txt");
    fs::write(&path, "old\n").unwrap();
    let mut app = app_at(&dir);
    app.editor.open(&path, false).unwrap();
    // The first check only records each file's modification time.
    app.on_focus_gained();
    fs::write(&path, "new\n").unwrap();
    let later = std::time::SystemTime::now() + Duration::from_secs(10);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(later)
        .unwrap();
    // Well inside the one-second poll throttle: only focus can trigger this.
    app.on_focus_gained();
    assert_eq!(app.editor.active_tab().unwrap().text(), "new\n");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn focus_lost_saves_every_dirty_buffer_in_focus_lost_mode() {
    let dir = unique_dir("t628-focus-lost");
    fs::write(dir.join("a.txt"), "a\n").unwrap();
    fs::write(dir.join("b.txt"), "b\n").unwrap();
    let mut settings = Settings::default();
    settings.editor_behavior.auto_save = AutoSave::FocusLost;
    let mut app = app_at_with(&dir, settings);
    open_dirty(&mut app, &dir.join("a.txt"), "a2\n");
    open_dirty(&mut app, &dir.join("b.txt"), "b2\n");
    app.editor.active = 0;

    app.on_focus_lost();

    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "a2\n");
    assert_eq!(fs::read_to_string(dir.join("b.txt")).unwrap(), "b2\n");
    assert!(app.editor.tabs.iter().all(|t| !t.dirty));
    assert_eq!(app.editor.active, 0, "the active tab is restored");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn focus_lost_saves_nothing_in_the_other_modes() {
    for mode in [AutoSave::Off, AutoSave::Interval] {
        let dir = unique_dir(&format!("t628-focus-lost-{mode:?}"));
        fs::write(dir.join("a.txt"), "a\n").unwrap();
        let mut settings = Settings::default();
        settings.editor_behavior.auto_save = mode;
        let mut app = app_at_with(&dir, settings);
        open_dirty(&mut app, &dir.join("a.txt"), "a2\n");
        app.on_focus_lost();
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "a\n");
        fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn the_interval_poll_ignores_focus_lost_mode() {
    let dir = unique_dir("t628-interval-poll");
    fs::write(dir.join("a.txt"), "a\n").unwrap();
    let mut settings = Settings::default();
    settings.editor_behavior.auto_save = AutoSave::FocusLost;
    let mut app = app_at_with(&dir, settings);
    open_dirty(&mut app, &dir.join("a.txt"), "a2\n");
    app.poll_auto_save();
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "a\n");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_auto_save_toggle_turns_any_mode_off_and_off_into_interval() {
    let dir = unique_dir("t628-toggle");
    let mut settings = Settings::default();
    settings.editor_behavior.auto_save = AutoSave::FocusLost;
    let mut app = app_at_with(&dir, settings);
    app.run_action("view.auto_save");
    assert_eq!(app.settings.editor_behavior.auto_save, AutoSave::Off);
    app.run_action("view.auto_save");
    assert_eq!(app.settings.editor_behavior.auto_save, AutoSave::Interval);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_16_color_terminal_gets_no_rgb_colors() {
    let dir = unique_dir("t628-depth");
    let mut app = app_at(&dir);
    let rgb = |buf: &ratatui::buffer::Buffer| {
        buf.content
            .iter()
            .any(|c| [c.fg, c.bg].iter().any(|k| matches!(k, Color::Rgb(..))))
    };
    // The bundled theme paints in RGB...
    assert!(rgb(&render(&mut app)));
    // ...and a 16-color terminal sees none of it.
    app.terminal_caps = TerminalCaps {
        color_depth: ColorDepth::Ansi16,
        hyperlinks: false,
    };
    let buf = render(&mut app);
    assert!(!rgb(&buf));
    assert!(
        buf.content
            .iter()
            .all(|c| !matches!(c.fg, Color::Indexed(..)))
    );
    fs::remove_dir_all(&dir).ok();
}

/// Whether any cell of `buf` carries an OSC 8 link to `url`.
fn linked(buf: &ratatui::buffer::Buffer, url: &str) -> bool {
    let open = format!("\x1b]8;;{url}\x1b\\");
    buf.content.iter().any(|c| c.symbol().starts_with(&open))
}

#[test]
fn markdown_preview_links_its_urls_when_the_terminal_supports_it() {
    let dir = unique_dir("t628-md-links");
    let path = dir.join("doc.md");
    fs::write(&path, "See [the site](https://example.com/docs).\n").unwrap();
    let mut app = app_at(&dir);
    app.editor.open(&path, false).unwrap();
    app.run_action("tools.markdown_preview");
    assert!(!linked(&render(&mut app), "https://example.com/docs"));
    app.terminal_caps.hyperlinks = true;
    assert!(linked(&render(&mut app), "https://example.com/docs"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn notifications_link_their_urls() {
    let dir = unique_dir("t628-msg-links");
    let mut settings = Settings::default();
    settings.panels.show_messages = true;
    let mut app = app_at_with(&dir, settings);
    app.terminal_caps.hyperlinks = true;
    app.messages.info("Docs: https://vix.example/x".to_string());
    assert!(linked(&render(&mut app), "https://vix.example/x"));
    fs::remove_dir_all(&dir).ok();
}
