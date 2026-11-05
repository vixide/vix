//! Branch review follow-ups (T665): the two-pane view's mouse mapping,
//! line-level stage and discard, word highlights, wrapping and split panes.
// Shared fixtures/helpers live in `common.rs`.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;
use vix_input::MouseEventKind;

pub(crate) const DIFF: &str = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,3 +1,3 @@\n keep\n-old\n+new\n tail\n@@ -10,1 +10,2 @@\n ctx\n+more\n";

/// The text of a drawn frame, one string per terminal row.
pub(crate) fn frame_rows(app: &mut App, width: u16, height: u16) -> Vec<String> {
    use ratatui::{Terminal, backend::TestBackend};
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
    let buf = term.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect()
        })
        .collect()
}

/// The text of the inline buffer row the review cursor is on.
pub(crate) fn cursor_text(app: &App) -> String {
    let row = app.editor.cursor_line();
    app.editor
        .active_tab()
        .unwrap()
        .text()
        .lines()
        .nth(row)
        .unwrap_or_default()
        .to_string()
}

/// In the two-pane view a click puts the cursor on the clicked cell's inline
/// row (old text left, new text right of the same screen row), and the wheel
/// scrolls the rows while the cursor stays in view.
#[test]
fn clicks_and_wheel_map_onto_rows() {
    let dir = unique_dir("review-panes-mouse");
    let mut app = app_at(&dir);
    app.settings.review_view = "side_by_side".to_string();
    app.open_review_from_diff("a..b", DIFF);
    let rows = frame_rows(&mut app, 160, 30);
    let area = app.layout.editor;
    let screen_row = rows.iter().position(|r| r.contains("old")).unwrap();
    let y = u16::try_from(screen_row).unwrap();

    // Left half: the removed line; right half of the same row: the added one.
    app.on_mouse(click(area.x + 8, y));
    assert_eq!(cursor_text(&app), "-old");
    app.on_mouse(click(area.x + area.width - 8, y));
    assert_eq!(cursor_text(&app), "+new");
    // A click on the context row above lands on the context line.
    app.on_mouse(click(area.x + 8, y - 1));
    assert_eq!(cursor_text(&app), " keep");
    // A click below the last row is taken and changes nothing.
    app.on_mouse(click(area.x + 8, area.y + area.height - 1));
    assert_eq!(cursor_text(&app), " keep");

    // The wheel moves the first visible row, in a short view.
    app.layout.editor.height = 4;
    let start = |app: &App| {
        app.review.as_ref().unwrap().panes.view_start(
            app.editor.top_visible_line(),
            app.editor.cursor_line(),
            4,
        )
    };
    let before = start(&app);
    app.on_mouse(mouse(MouseEventKind::ScrollDown, area.x + 8, area.y + 1));
    let after = start(&app);
    assert_eq!(after, before + 1);
    let sel = app
        .review
        .as_ref()
        .unwrap()
        .panes
        .row_for_inline(app.editor.cursor_line());
    assert!((after..after + 4).contains(&sel), "cursor stays in view");
    app.on_mouse(mouse(MouseEventKind::ScrollUp, area.x + 8, area.y + 1));
    assert_eq!(start(&app), before);

    // The inline view is left to the ordinary editor.
    app.run_action("git.review_toggle_view");
    app.editor.goto(1, None, app.layout.editor);
    app.on_mouse(click(area.x + 20, area.y + 3));
    assert_ne!(app.editor.cursor_line(), 0);
    std::fs::remove_dir_all(&dir).ok();
}
