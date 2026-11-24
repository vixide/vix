//! Round 4 coverage: the editor-command surface when there is *no* tab to act
//! on (every command must be a harmless no-op, not a panic) and when the active
//! tab shows an image rather than text (editing commands must refuse it).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use ratatui::layout::Rect;
use vix_editor::editor::{Editor, Flags};

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

fn tabless() -> Editor {
    let mut e = Editor::new(Flags::empty(), "    ".into());
    e.tabs.clear();
    e.active = 0;
    e
}

#[test]
fn every_command_is_a_no_op_without_a_tab() {
    let mut e = tabless();
    e.cursor_line_home();
    e.cursor_document_start(AREA);
    e.cursor_document_end(AREA);
    e.cursor_line_end();
    e.cursor_line_start();
    e.cursor_paragraph_start(AREA);
    e.cursor_paragraph_end(AREA);
    e.cursor_section_start(AREA);
    e.cursor_section_end(AREA);
    e.cursor_line_down(AREA);
    e.cursor_line_up(AREA);
    e.cursor_paragraph_next(AREA);
    e.cursor_paragraph_prev(AREA);
    e.goto_paragraph(3, AREA);
    e.cursor_section_next(AREA);
    e.cursor_section_prev(AREA);
    e.goto_section(2, AREA);
    e.goto_offset(5, AREA);
    e.cursor_sentence_start(AREA);
    e.cursor_sentence_end(AREA);
    e.cursor_sentence_next(AREA);
    e.cursor_sentence_prev(AREA);
    e.goto_sentence(2, AREA);
    e.cursor_word_start(AREA);
    e.cursor_word_end(AREA);
    e.cursor_word_next(AREA);
    e.cursor_word_prev(AREA);
    e.goto_word(2, AREA);
    e.goto_percent(50, AREA);
    e.goto_byte(4, AREA);
    e.select_line(AREA);
    e.select_paragraph(AREA);
    e.select_section(AREA);
    e.select_all();
    e.duplicate_line();
    e.join_lines();
    e.sort_lines();
    e.trim_trailing_whitespace();
    e.remove_duplicate_lines();
    e.reverse_lines();
    e.sort_unique();
    e.shuffle_lines();
    e.move_line(true, AREA);
    e.move_line(false, AREA);
    e.select_word(true, AREA);
    e.select_word(false, AREA);
    e.jump_matching_bracket(AREA);
    e.delete_forward();
    e.page_up(3);
    e.page_down(3);
    e.set_word_marks(vec![(0, 1)]);
    assert!(!e.insert_str("x", AREA), "nothing to insert into");
    assert!(!e.paste_str("x", AREA), "nothing to paste into");
    assert_eq!(e.top_visible_line(), 0);
    assert!(e.tabs.is_empty(), "no command may conjure a tab");
    assert!(e.active_tab().is_none());
}

#[test]
fn refresh_applies_the_display_flags_to_every_buffer() {
    let mut e = Editor::new(Flags::LINE_NUMBERS | Flags::SOFT_WRAP, "    ".into());
    e.new_tab();
    assert_eq!(e.tabs.len(), 2);
    e.refresh_line_numbers();
    e.refresh_soft_wrap();
    e.refresh_theme();
    let shown: Vec<usize> = e.tabs.iter().map(|t| t.editor.gutter_width()).collect();
    for tab in &e.tabs {
        assert!(tab.editor.soft_wrap_enabled());
    }

    e.flags.remove(Flags::LINE_NUMBERS | Flags::SOFT_WRAP);
    e.refresh_line_numbers();
    e.refresh_soft_wrap();
    for tab in &e.tabs {
        assert!(!tab.editor.soft_wrap_enabled());
    }
    for (tab, was) in e.tabs.iter().zip(&shown) {
        assert!(
            tab.editor.gutter_width() < *was,
            "hiding line numbers narrows the gutter"
        );
    }

    // Whitespace: the toggle reports the *new* state, so it shows what the
    // refresh left behind (on -> toggling gives off, and vice versa).
    e.flags.insert(Flags::SHOW_WHITESPACE);
    e.refresh_whitespace();
    for tab in &mut e.tabs {
        assert!(!tab.editor.toggle_whitespace(), "refresh had turned it on");
    }
    e.flags.remove(Flags::SHOW_WHITESPACE);
    e.refresh_whitespace();
    for tab in &mut e.tabs {
        assert!(tab.editor.toggle_whitespace(), "refresh had turned it off");
    }

    // Relative line numbers go through the same refresh.
    e.flags
        .insert(Flags::LINE_NUMBERS | Flags::RELATIVE_LINE_NUMBERS);
    e.refresh_line_numbers();
    for (tab, was) in e.tabs.iter().zip(&shown) {
        assert_eq!(
            tab.editor.gutter_width(),
            *was,
            "shown again at the same width"
        );
    }
}

#[test]
fn multi_caret_line_moves_move_every_caret() {
    let mut e = Editor::new(Flags::empty(), "    ".into());
    let t = e.active_tab_mut().unwrap();
    t.editor.set_content("  ab\n  cd");
    t.editor.set_cursor(3);
    t.editor.add_caret_at(8);
    assert_eq!(t.editor.caret_count(), 2);

    e.cursor_line_home();
    let t = e.active_tab().unwrap();
    assert_eq!(t.editor.caret_count(), 2, "both carets survive the move");
    assert_eq!(
        t.editor.get_cursor(),
        7,
        "smart Home: first non-blank of line 2"
    );

    e.cursor_line_end();
    assert_eq!(e.active_tab().unwrap().editor.get_cursor(), 9);
    e.cursor_line_start();
    assert_eq!(e.active_tab().unwrap().editor.get_cursor(), 5);
    assert_eq!(e.active_tab().unwrap().editor.caret_count(), 2);
}

#[test]
fn focusing_panes_of_an_unsplit_editor_does_nothing() {
    let mut e = Editor::new(Flags::empty(), "    ".into());
    e.focus_leaf(1);
    e.focus_other_pane();
    assert_eq!(e.focused_leaf(), 0);
    assert_eq!(e.active, 0);
    assert!(!e.is_split());
}

#[cfg(feature = "image")]
mod image_tab {
    use std::fs;
    use std::path::PathBuf;

    use ratatui_image::picker::Picker;

    use super::{AREA, Editor, Flags};

    fn proto() -> vix_editor::editor::ImageProtocol {
        Picker::halfblocks().new_resize_protocol(image::DynamicImage::new_rgb8(2, 2))
    }

    fn temp_png() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("vix-editor-img-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pic.png");
        fs::write(&path, b"not really a png; only the extension matters").unwrap();
        (dir, path)
    }

    #[test]
    fn open_image_adds_an_image_tab_and_reuses_it_for_the_same_path() {
        let (dir, path) = temp_png();
        let mut e = Editor::new(Flags::empty(), "    ".into());
        e.open_image(&path, proto());
        assert_eq!(e.tabs.len(), 2);
        assert_eq!(e.active, 1);
        assert!(e.active_tab().unwrap().is_image());
        assert!(!e.active_tab().unwrap().dirty);

        // Switch away, then re-open: same tab, re-focused, no duplicate.
        e.active = 0;
        e.open_image(&path, proto());
        assert_eq!(e.tabs.len(), 2);
        assert_eq!(e.active, 1);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn editing_commands_refuse_an_image_tab() {
        let (dir, path) = temp_png();
        let mut e = Editor::new(Flags::empty(), "    ".into());
        e.open_image(&path, proto());
        assert!(!e.insert_str("x", AREA));
        assert!(!e.paste_str("x", AREA));
        e.duplicate_line();
        e.join_lines();
        e.sort_lines();
        e.shuffle_lines();
        e.trim_trailing_whitespace();
        e.move_line(true, AREA);
        e.select_word(true, AREA);
        let tab = e.active_tab().unwrap();
        assert!(!tab.dirty, "no command may dirty an image tab");
        assert_eq!(tab.text(), "");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn open_image_of_a_missing_file_still_opens_a_tab_under_the_given_path() {
        let mut e = Editor::new(Flags::empty(), "    ".into());
        let path = std::env::temp_dir().join("vix-editor-no-such-dir-r4/ghost.png");
        e.open_image(&path, proto());
        let tab = e.active_tab().unwrap();
        assert!(tab.is_image());
        assert_eq!(tab.path.as_deref(), Some(path.as_path()));
    }
}

#[test]
fn reopen_with_encoding_refuses_untitled_dirty_missing_and_non_file_tabs() {
    use std::io::ErrorKind;
    let enc = vix_encoding::TextEncoding::UTF8;
    let dir = std::env::temp_dir().join(format!("vix-editor-reopen-r4-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let mut e = Editor::new(Flags::CHECK_CONFLICTS, "    ".into());
    // Untitled buffer: nothing to reopen.
    assert_eq!(
        e.reopen_active_with_encoding(enc).unwrap_err().kind(),
        ErrorKind::NotFound
    );
    // No tabs at all.
    e.tabs.clear();
    assert_eq!(
        e.reopen_active_with_encoding(enc).unwrap_err().kind(),
        ErrorKind::NotFound
    );

    e.new_tab();
    let tab = e.active_tab_mut().unwrap();
    tab.path = Some(dir.clone());
    tab.dirty = true;
    assert_eq!(
        e.reopen_active_with_encoding(enc).unwrap_err().kind(),
        ErrorKind::InvalidInput,
        "unsaved edits would be discarded"
    );
    // A directory is not a regular file.
    e.active_tab_mut().unwrap().dirty = false;
    assert_eq!(
        e.reopen_active_with_encoding(enc).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    // A vanished file surfaces the metadata error.
    e.active_tab_mut().unwrap().path = Some(dir.join("gone.txt"));
    assert_eq!(
        e.reopen_active_with_encoding(enc).unwrap_err().kind(),
        ErrorKind::NotFound
    );
    std::fs::remove_dir_all(dir).ok();
}
