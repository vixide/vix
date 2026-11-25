//! Round-4 behavior tests for the table editor, through its public API only.

use vix_edit_table::{Grid, Outcome};
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

fn press(g: &mut Grid, code: KeyCode) -> Outcome {
    g.handle_key(KeyEvent::new(code, KeyModifiers::NONE), 2)
}

fn ch(g: &mut Grid, c: char) -> Outcome {
    press(g, KeyCode::Char(c))
}

fn alt(g: &mut Grid, code: KeyCode) {
    g.handle_key(KeyEvent::new(code, KeyModifiers::ALT), 2);
}

fn text(g: &mut Grid, s: &str) {
    for c in s.chars() {
        ch(g, c);
    }
}

fn grid() -> Grid {
    Grid::from_text("name,qty\nb,10\na,9\nc,100\n", false)
}

#[test]
fn page_keys_move_by_the_page_size_and_clamp() {
    let mut g = Grid::from_text("h\n1\n2\n3\n4\n5\n", false);
    press(&mut g, KeyCode::PageDown);
    assert_eq!(g.row(), 2);
    press(&mut g, KeyCode::PageDown);
    press(&mut g, KeyCode::PageDown);
    assert_eq!(g.row(), 5, "clamped to the last row");
    press(&mut g, KeyCode::PageUp);
    assert_eq!(g.row(), 3);
    ch(&mut g, 'g');
    assert_eq!(g.row(), 0);
    // A zero page still moves one row.
    let out = g.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), 0);
    assert_eq!((out, g.row()), (Outcome::Consumed, 1));
}

#[test]
fn horizontal_navigation_and_home() {
    let mut g = Grid::from_text("a,b,c\n", false);
    press(&mut g, KeyCode::Right);
    ch(&mut g, 'l');
    ch(&mut g, 'l');
    assert_eq!(g.col(), 2, "stops at the last column");
    press(&mut g, KeyCode::Left);
    ch(&mut g, 'h');
    ch(&mut g, 'h');
    assert_eq!(g.col(), 0);
    press(&mut g, KeyCode::End);
    assert_eq!(g.col(), 2);
    press(&mut g, KeyCode::Home);
    assert_eq!(g.col(), 0);
    ch(&mut g, 'k');
    assert_eq!(g.row(), 0);
    ch(&mut g, 'j');
    assert_eq!(g.row(), 0, "single-row grid cannot move down");
}

#[test]
fn escape_and_q_close_and_ctrl_s_saves_from_any_mode() {
    let mut g = grid();
    assert_eq!(press(&mut g, KeyCode::Esc), Outcome::Close);
    assert_eq!(ch(&mut g, 'q'), Outcome::Close);
    assert_eq!(
        ch(&mut g, 'z'),
        Outcome::Consumed,
        "unbound key is swallowed"
    );
    // Ctrl+S in the middle of an edit commits it first.
    press(&mut g, KeyCode::Down);
    press(&mut g, KeyCode::F(2));
    assert!(g.is_editing());
    text(&mut g, "7");
    let out = g.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL), 1);
    assert_eq!(out, Outcome::Save);
    assert!(!g.is_editing());
    assert_eq!(g.cell(1, 0), "b7");
    assert!(g.is_dirty());
    g.mark_saved();
    assert!(!g.is_dirty());
    assert_eq!(g.to_text(), "name,qty\nb7,10\na,9\nc,100\n");
    // Ctrl+S from normal mode saves without changing anything.
    assert_eq!(
        g.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL), 1),
        Outcome::Save
    );
    assert!(!g.is_dirty());
}

#[test]
fn edit_mode_tab_escape_backspace_and_unrelated_keys() {
    let mut g = grid();
    press(&mut g, KeyCode::Down);
    press(&mut g, KeyCode::Enter);
    text(&mut g, "xy");
    assert_eq!(g.edit_buffer(), "bxy");
    press(&mut g, KeyCode::Backspace);
    press(&mut g, KeyCode::Up); // ignored while editing
    assert_eq!(g.edit_buffer(), "bx");
    press(&mut g, KeyCode::Tab);
    assert_eq!((g.row(), g.col()), (1, 1), "tab commits and moves right");
    assert_eq!(g.cell(1, 0), "bx");
    // Escape abandons the edit.
    press(&mut g, KeyCode::Enter);
    text(&mut g, "999");
    press(&mut g, KeyCode::Esc);
    assert!(!g.is_editing());
    assert_eq!(g.cell(1, 1), "10");
    // Committing an unchanged cell is not an edit and not an undo step.
    g.mark_saved();
    press(&mut g, KeyCode::Enter);
    press(&mut g, KeyCode::Enter);
    assert!(!g.is_dirty());
}

#[test]
fn delete_clears_a_cell_and_undo_redo_roundtrip() {
    let mut g = grid();
    press(&mut g, KeyCode::Down);
    press(&mut g, KeyCode::Delete);
    assert_eq!(g.cell(1, 0), "");
    assert!(g.is_dirty());
    // Clearing an already-empty cell does nothing (no new undo step).
    press(&mut g, KeyCode::Delete);
    ch(&mut g, 'u');
    assert_eq!(g.cell(1, 0), "b");
    ch(&mut g, 'u'); // nothing left to undo
    assert_eq!(g.cell(1, 0), "b");
    g.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL), 1);
    assert_eq!(g.cell(1, 0), "");
    g.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL), 1);
    assert_eq!(g.cell(1, 0), "", "redo with nothing to redo is harmless");
}

#[test]
fn structural_edits_insert_and_delete_rows_and_columns() {
    let mut g = grid();
    press(&mut g, KeyCode::Down); // row 1
    alt(&mut g, KeyCode::Up); // insert above
    assert_eq!((g.row_count(), g.row()), (5, 1));
    assert_eq!(g.cell(1, 0), "");
    assert_eq!(g.cell(2, 0), "b");
    alt(&mut g, KeyCode::Down); // insert below
    assert_eq!((g.row_count(), g.row()), (6, 2));
    alt(&mut g, KeyCode::Delete);
    alt(&mut g, KeyCode::Delete);
    assert_eq!(g.row_count(), 4);
    alt(&mut g, KeyCode::Left);
    assert_eq!((g.col_count(), g.col()), (3, 0));
    alt(&mut g, KeyCode::Right);
    assert_eq!((g.col_count(), g.col()), (4, 1));
    alt(&mut g, KeyCode::Backspace);
    alt(&mut g, KeyCode::Backspace);
    alt(&mut g, KeyCode::Backspace);
    assert_eq!(g.col_count(), 1);
    alt(&mut g, KeyCode::Backspace);
    assert_eq!(g.col_count(), 1, "the last column cannot be deleted");
    // Unbound alt keys are swallowed.
    alt(&mut g, KeyCode::Char('x'));
    for _ in 0..10 {
        alt(&mut g, KeyCode::Delete);
    }
    assert_eq!(g.row_count(), 1, "the last row cannot be deleted");
}

#[test]
fn sort_orders_numerically_ascending_and_descending_and_keeps_the_header() {
    let mut g = grid();
    ch(&mut g, 'l');
    ch(&mut g, 's');
    assert_eq!(g.to_text(), "name,qty\na,9\nb,10\nc,100\n");
    ch(&mut g, 'S');
    assert_eq!(g.to_text(), "name,qty\nc,100\nb,10\na,9\n");
    ch(&mut g, 'u');
    assert_eq!(g.to_text(), "name,qty\na,9\nb,10\nc,100\n");
    // Text columns sort lexically; a tiny grid is left alone.
    ch(&mut g, 'h');
    ch(&mut g, 'S');
    assert_eq!(g.to_text(), "name,qty\nc,100\nb,10\na,9\n");
    let mut tiny = Grid::from_text("h\nx\n", false);
    ch(&mut tiny, 's');
    assert!(!tiny.is_dirty());
}

#[test]
fn find_wraps_is_case_insensitive_and_repeats_with_n() {
    let mut g = Grid::from_text("k,v\nApple,1\nbanana,2\napricot,3\n", false);
    ch(&mut g, '/');
    assert!(g.is_finding());
    text(&mut g, "ap");
    press(&mut g, KeyCode::Backspace);
    press(&mut g, KeyCode::Up); // ignored while finding
    text(&mut g, "p");
    assert_eq!(g.find_buffer(), "ap");
    press(&mut g, KeyCode::Enter);
    assert!(!g.is_finding());
    assert_eq!((g.row(), g.col()), (1, 0));
    ch(&mut g, 'n');
    assert_eq!((g.row(), g.col()), (3, 0));
    ch(&mut g, 'n');
    assert_eq!((g.row(), g.col()), (1, 0), "wraps around");
    // Escape abandons a search; a query with no match leaves the cursor.
    ch(&mut g, '/');
    text(&mut g, "zzz");
    press(&mut g, KeyCode::Esc);
    assert_eq!(g.find_buffer(), "");
    ch(&mut g, '/');
    text(&mut g, "zzz");
    press(&mut g, KeyCode::Enter);
    assert_eq!((g.row(), g.col()), (1, 0));
    // `n` with no query at all does nothing.
    let mut fresh = grid();
    ch(&mut fresh, 'n');
    assert_eq!((fresh.row(), fresh.col()), (0, 0));
}

#[test]
fn scroll_windows_follow_the_cursor() {
    let mut g = Grid::from_text("h\n1\n2\n3\n4\n5\n6\n", false);
    g.ensure_row_visible(3);
    assert_eq!(g.row_scroll(), 1, "body starts below the header");
    ch(&mut g, 'G');
    g.ensure_row_visible(3);
    assert_eq!(g.row_scroll(), 4);
    ch(&mut g, 'g');
    ch(&mut g, 'j');
    g.ensure_row_visible(3);
    assert_eq!(g.row_scroll(), 1, "scrolls back up to the selected row");
    // A height of zero behaves as one row.
    g.ensure_row_visible(0);
    assert_eq!(g.row_scroll(), 1);
    let mut one = Grid::from_text("only\n", false);
    one.ensure_row_visible(5);
    assert_eq!(one.row_scroll(), 1);

    let mut wide = Grid::from_text("a,b,c,d\n", false);
    press(&mut wide, KeyCode::End);
    wide.set_col_scroll(9);
    assert_eq!(wide.col_scroll(), 3, "never past the selected column");
    wide.set_col_scroll(1);
    assert_eq!(wide.col_scroll(), 1);
}

#[test]
fn tsv_roundtrips_with_tabs() {
    let g = Grid::from_text("a\tb\n1\t2\n", true);
    assert_eq!(g.col_count(), 2);
    assert_eq!(g.to_text(), "a\tb\n1\t2\n");
    assert_eq!(g.cell(9, 9), "", "out-of-range cells read as empty");
}
