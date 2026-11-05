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
            |_| 1,
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

use super::git::{t634_git, t634_repo, t665_cached, t665_diff};
use std::fs;

/// Put the review cursor on the buffer row whose text is exactly `text`.
fn cursor_to(app: &mut App, text: &str) {
    let row = app
        .editor
        .active_tab()
        .unwrap()
        .text()
        .lines()
        .position(|l| l == text)
        .unwrap_or_else(|| panic!("no row {text:?}"));
    app.editor.goto(row + 1, None, app.layout.editor);
}

/// A repo whose `a.txt` is committed as `a b c d` (one per line) and then
/// edited in the working tree so one hunk holds `-b +b2 ... +e`.
fn line_repo(name: &str) -> (std::path::PathBuf, App) {
    let dir = t634_repo(name);
    fs::write(dir.join("a.txt"), "a\nb\nc\nd\n").unwrap();
    t634_git(&dir, &["add", "a.txt"]);
    t634_git(&dir, &["commit", "-q", "-m", "four"]);
    let mut app = app_at(&dir);
    app.refresh_git();
    fs::write(dir.join("a.txt"), "a\nb2\nc\nd\ne\n").unwrap();
    app.open_review_from_diff("work", &t665_diff(&dir));
    (dir, app)
}

/// Stage Review Lines stages only the line under the cursor, or only the
/// lines of a visual selection, through a reduced patch.
#[test]
fn stage_lines_stages_only_the_selected_lines() {
    let (dir, mut app) = line_repo("t665lines-stage");
    // The cursor on `+e`: just that line is staged.
    cursor_to(&mut app, "+e");
    app.run_action("git.review_stage_lines");
    let cached = t665_cached(&dir);
    assert!(cached.contains("+e") && !cached.contains("b2"), "{cached}");
    // ("work" is not a range git can diff again, so the buffer is unchanged.)

    // A visual selection of `-b` and `+b2` (two rows) stages the replacement.
    cursor_to(&mut app, "-b");
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    app.run_action("git.review_stage_lines");
    let cached = t665_cached(&dir);
    assert!(cached.contains("-b") && cached.contains("+b2"), "{cached}");
    assert!(t665_diff(&dir).is_empty(), "everything is staged now");
    fs::remove_dir_all(dir.parent().unwrap()).ok();
}

/// Nothing to stage: a context row, or no review, only says so.
#[test]
fn stage_lines_refuses_context_and_needs_a_review() {
    let (dir, mut app) = line_repo("t665lines-refuse");
    cursor_to(&mut app, " a");
    app.run_action("git.review_stage_lines");
    assert!(t665_cached(&dir).is_empty());
    // Outside a review the actions only say so.
    let mut plain = app_at(&dir);
    plain.run_action("git.review_stage_lines");
    plain.run_action("git.review_discard_lines");
    assert!(plain.confirm.is_none() && plain.review.is_none());
    fs::remove_dir_all(dir.parent().unwrap()).ok();
}

/// Discard Review Lines asks first, undoes only the selected lines in the
/// working tree, and shows git's reason (changing nothing) when the lines no
/// longer match.
#[test]
fn discard_lines_confirms_then_reverts_only_those_lines() {
    let (dir, mut app) = line_repo("t665lines-discard");
    let file = dir.join("a.txt");
    cursor_to(&mut app, "+e");
    app.run_action("git.review_discard_lines");
    assert_eq!(
        app.confirm.as_ref().map(|c| c.kind),
        Some(vix::app::ConfirmKind::GitReviewDiscardLines)
    );
    app.on_key(key('n'));
    assert!(app.confirm.is_none());
    assert_eq!(fs::read_to_string(&file).unwrap(), "a\nb2\nc\nd\ne\n");
    app.run_action("git.review_discard_lines");
    app.on_key(key('y'));
    assert_eq!(fs::read_to_string(&file).unwrap(), "a\nb2\nc\nd\n");

    // The tree changed under the review: git refuses, the file is untouched
    // and the reason is a message, not silence.
    cursor_to(&mut app, "+b2");
    fs::write(&file, "a\nzzz\nc\nd\n").unwrap();
    let before = app.messages.items.len();
    app.run_action("git.review_discard_lines");
    app.on_key(key('y'));
    assert_eq!(fs::read_to_string(&file).unwrap(), "a\nzzz\nc\nd\n");
    assert!(app.messages.items.len() > before, "git's reason is shown");
    fs::remove_dir_all(dir.parent().unwrap()).ok();
}

/// A final line without a newline: selecting only one of its two copies is
/// refused with a status, and nothing is staged.
#[test]
fn stage_lines_refuses_to_split_a_missing_final_newline() {
    let dir = t634_repo("t665lines-nonl");
    fs::write(dir.join("a.txt"), "one\ntwo").unwrap();
    t634_git(&dir, &["add", "a.txt"]);
    t634_git(&dir, &["commit", "-q", "-m", "nonl"]);
    let mut app = app_at(&dir);
    app.refresh_git();
    fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    app.open_review_from_diff("work", &t665_diff(&dir));
    cursor_to(&mut app, "+two");
    app.run_action("git.review_stage_lines");
    assert!(t665_cached(&dir).is_empty(), "refused, nothing staged");
    // Both copies together do stage, with the marker.
    cursor_to(&mut app, "-two");
    for _ in 0..4 {
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    }
    app.run_action("git.review_stage_lines");
    assert!(
        t665_cached(&dir).contains("+three"),
        "{}",
        t665_cached(&dir)
    );
    fs::remove_dir_all(dir.parent().unwrap()).ok();
}

/// A drawn frame as a buffer, to look at cell styles.
fn frame_buffer(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    use ratatui::{Terminal, backend::TestBackend};
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
    term.backend().buffer().clone()
}

const WORDS: &str = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n keep\n-the quick fox\n+the slow fox\n x\n";

/// In a paired line the words that changed are drawn reversed on both sides
/// and the words that did not are not.
#[test]
fn two_pane_highlights_the_changed_words_of_paired_lines() {
    use ratatui::style::Modifier;
    let dir = unique_dir("review-words");
    let mut app = app_at(&dir);
    app.settings.review_view = "side_by_side".to_string();
    app.open_review_from_diff("a..b", WORDS);
    let buf = frame_buffer(&mut app, 200, 30);
    let row = (0..buf.area.height)
        .find(|&y| {
            let text: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            text.contains("quick") && text.contains("slow")
        })
        .unwrap_or_else(|| {
            panic!(
                "the paired row is drawn: {:#?}",
                frame_rows(&mut app, 200, 30)
            )
        });
    let text: String = (0..buf.area.width)
        .map(|x| buf[(x, row)].symbol())
        .collect();
    let at = |word: &str| u16::try_from(text[..text.find(word).unwrap()].chars().count()).unwrap();
    let reversed = |x: u16| buf[(x, row)].modifier.contains(Modifier::REVERSED);
    assert!(reversed(at("quick")), "the old word is marked");
    assert!(reversed(at("slow")), "the new word is marked");
    assert!(!reversed(at("the ")), "unchanged words are not");
    assert!(!reversed(at("fox")), "unchanged words are not");
    std::fs::remove_dir_all(&dir).ok();
}

/// With soft wrap on, a long line wraps within its own pane: the short side
/// is padded so the next pair still starts on the same screen row for both
/// panes, a click on a continuation line lands on the line's row, and with
/// wrap off the line is cut.
#[test]
fn two_pane_wraps_long_lines_and_keeps_rows_aligned() {
    let dir = unique_dir("review-wrap");
    let mut app = app_at(&dir);
    app.settings.review_view = "side_by_side".to_string();
    let long = format!("{}END", "w".repeat(150));
    let diff = format!(
        "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n keep\n-{long}\n+short\n tail\n"
    );
    app.open_review_from_diff("a..b", &diff);

    // Off: the line is cut, the end is not drawn anywhere.
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_soft_wrap(false);
    assert!(
        !frame_rows(&mut app, 200, 30)
            .iter()
            .any(|r| r.contains("END"))
    );

    // On: the end shows on a later screen row, and `tail` still follows.
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_soft_wrap(true);
    let rows = frame_rows(&mut app, 200, 30);
    let first = rows.iter().position(|r| r.contains("short")).unwrap();
    let last = rows.iter().position(|r| r.contains("END")).unwrap();
    let tail = rows.iter().position(|r| r.contains("tail")).unwrap();
    assert!(last > first, "the wrapped end is on a continuation row");
    assert!(tail > last, "the next pair starts below the wrapped row");
    // Both `tail` cells are on that one row (aligned).
    assert_eq!(rows[tail].matches("tail").count(), 2);
    // The short side's continuation rows are blank on the right.
    assert!(!rows[last].contains("short"));

    // Clicking the continuation line, left half, is the removed line's row.
    let area = app.layout.editor;
    app.on_mouse(click(area.x + 8, u16::try_from(last).unwrap()));
    assert_eq!(cursor_text(&app), format!("-{long}"));
    // ... and the right half there is the added line's.
    app.on_mouse(click(area.x + area.width - 8, u16::try_from(last).unwrap()));
    assert_eq!(cursor_text(&app), "+short");
    std::fs::remove_dir_all(&dir).ok();
}
