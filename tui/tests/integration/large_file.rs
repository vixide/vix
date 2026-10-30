//! The Large File Viewer (T733): opened from the size-limit confirmation or
//! File → Open in Large File Viewer…, driven by keys through the real `App`,
//! and drawn without loading the file into a buffer.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use std::io::Write as _;

use crate::integration::common::*;

/// `n` numbered lines of about 60 bytes each.
fn write_log(path: &Path, n: usize) {
    let mut out = std::io::BufWriter::new(fs::File::create(path).unwrap());
    for i in 1..=n {
        writeln!(out, "entry {i:07} lorem ipsum dolor sit amet consectetur").unwrap();
    }
}

/// Draw one frame, as the event loop would, so the viewer learns its size.
fn draw(app: &mut App) -> String {
    use ratatui::{Terminal, backend::TestBackend};
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
    let buf = term.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
                + "\n"
        })
        .collect()
}

#[test]
fn the_size_limit_confirmation_offers_the_viewer() {
    use vix::app::ConfirmKind;
    let dir = unique_dir("t733-confirm");
    let path = dir.join("big.log");
    write_log(&path, 1_000);
    let mut app = app_at(&dir);
    app.editor.max_text_bytes = 100;

    app.open_initial(&path);
    assert_eq!(
        app.confirm.as_ref().map(|c| c.kind),
        Some(ConfirmKind::OpenLarge)
    );
    app.on_key(key('v'));
    assert!(app.confirm.is_none());
    let v = app.large_file.as_ref().expect("the viewer opened");
    assert_eq!(v.path(), path.as_path());
    assert!(
        app.editor.tabs.iter().all(|t| t.path.is_none()),
        "the file was not loaded into a buffer"
    );
    let screen = draw(&mut app);
    assert!(screen.contains("entry 0000001"), "{screen}");

    // `q` closes it.
    app.on_key(key('q'));
    assert!(app.large_file.as_ref().is_none());
}

#[test]
fn open_in_large_file_viewer_navigates_a_multi_megabyte_file() {
    let dir = unique_dir("t733-action");
    let path = dir.join("app.log");
    // ~8 MB, 150,000 lines.
    write_log(&path, 150_000);
    let mut app = app_at(&dir);

    app.run_action("file.open_large");
    assert!(matches!(
        app.prompt.as_ref().map(|p| p.kind),
        Some(PromptKind::OpenLargeFile)
    ));
    for c in "app.log".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.large_file.as_ref().is_some(), "{}", app.status);
    draw(&mut app);

    // End shows the last line without waiting for the index.
    app.on_key(keycode(KeyCode::End));
    let screen = draw(&mut app);
    assert!(screen.contains("entry 0150000"), "{screen}");

    // Go to a line through the viewer's own input line, once indexed.
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.large_file.as_ref().unwrap().busy() && Instant::now() < deadline {
        app.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
    app.on_key(key(':'));
    for c in "123456".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    let screen = draw(&mut app);
    assert!(screen.contains("entry 0123456"), "{screen}");

    // Search backward for an earlier line.
    app.on_key(key('?'));
    for c in "entry 0000042".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.large_file.as_ref().unwrap().search().is_some() && Instant::now() < deadline {
        app.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
    let screen = draw(&mut app);
    assert!(screen.contains("entry 0000042"), "{screen}");

    app.on_key(keycode(KeyCode::Esc));
    assert!(app.large_file.as_ref().is_none());
}

#[test]
fn a_missing_file_reports_an_error_instead_of_opening() {
    let dir = unique_dir("t733-missing");
    let mut app = app_at(&dir);
    app.open_large_file_viewer(&dir.join("nope.log"));
    assert!(app.large_file.as_ref().is_none());
}
