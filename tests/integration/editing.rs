#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

#[test]
fn snippet_picker_filters_and_inserts_bundled() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.snippets");
    assert!(app.snippets.is_some(), "Tools → Snippets opens the picker");
    // The library includes the bundled snippets.
    assert!(app.snippet_library.iter().any(|s| s.name == "TODO comment"));

    // Filter to the TODO snippet and insert it.
    for c in "todo".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.snippets.is_none(), "Enter inserts and closes");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .starts_with("TODO: ")
    );
}

#[test]
fn media_type_picker_filters_and_inserts() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.media_types");
    assert!(
        app.media_type_panel.is_some(),
        "Media Types opens the picker"
    );

    // Type to filter down to SVG, then Enter inserts the media type.
    for c in "svg".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.editor.active_tab().unwrap().text(), "image/svg+xml");

    // Esc closes it.
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.media_type_panel.is_none());

    // The lookup table is also usable directly by extension.
    assert_eq!(
        vix::media_type::for_extension("png").unwrap().media_type,
        "image/png"
    );
}

#[test]
fn tools_draw_inserts_ditaa_ascii_art() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.draw.rectangle");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("+-------+"), "rectangle: {text:?}");
    assert!(text.contains("|       |"));

    let mut app = app_at(Path::new("."));
    app.run_action("tools.draw.rounded");
    let text = app.editor.active_tab().unwrap().text();
    assert!(
        text.contains("/-------\\") && text.contains("\\-------/"),
        "rounded: {text:?}"
    );

    let mut app = app_at(Path::new("."));
    app.run_action("tools.draw.arrow_right");
    assert_eq!(app.editor.active_tab().unwrap().text(), "------->");
}

#[test]
fn roam_capture_insert_dailies_and_views() {
    let dir = unique_dir("roam");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut app = app_at(&dir);

    // Capture a node: the prompt creates an .org file and opens it.
    app.run_action("roam.capture");
    assert!(app.prompt.is_some(), "Roam → Capture opens a prompt");
    type_str(&mut app, "My First Note");
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let node = app.editor.active_tab().unwrap().text();
    assert!(
        node.contains("#+title: My First Note"),
        "node has title: {node:?}"
    );
    assert!(node.contains(":ID:"), "node has an ID drawer");
    assert!(
        dir.join("my-first-note.org").exists(),
        "node file written to disk"
    );

    // Insert a link to a (new) node into the current buffer without leaving it.
    let mut app = app_at(&dir);
    app.run_action("roam.node_insert");
    type_str(&mut app, "Another Note");
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let buf = app.editor.active_tab().unwrap().text();
    assert!(
        buf.contains("[[id:") && buf.contains("][Another Note]]"),
        "link inserted: {buf:?}"
    );

    // Dailies → Today creates and opens today's daily note under daily/.
    app.run_action("roam.dailies_today");
    let daily = app.editor.active_tab().unwrap().text();
    assert!(
        daily.starts_with(":PROPERTIES:") && daily.contains("#+title: 20"),
        "daily note: {daily:?}"
    );

    // Graph and Sync compile cross-node buffers.
    app.run_action("roam.graph");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("flowchart LR")
    );
    app.run_action("roam.db_sync");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("Roam Nodes")
    );

    // Add a tag to the active node buffer.
    let mut app = app_at(&dir);
    type_str(&mut app, "#+title: Tagged\n");
    app.run_action("roam.tag_add");
    type_str(&mut app, "work");
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("#+filetags: :work:")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn matching_tag_jumps_between_open_and_close() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "<div><span>x</span></div>");
    // Move to the start (into the opening <div>) and jump to its </div>.
    app.on_key(keycode(KeyCode::Home));
    app.on_key(keycode(KeyCode::Right)); // inside <div>
    app.run_action("nav.matching_tag");
    let col = app.editor.cursor_1based().1;
    assert_eq!(col, 20, "jumped to the </div> at col 20 (1-based)");
}

#[test]
fn comment_banner_boxes_the_current_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "Section Title");
    app.run_action("edit.comment_banner");
    let text = app.editor.active_tab().unwrap().text();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "three banner lines: {text:?}");
    assert!(lines[0].contains('='), "top rule: {:?}", lines[0]);
    assert!(
        lines[1].contains("Section Title"),
        "title line: {:?}",
        lines[1]
    );
    assert!(lines[2].contains('='), "bottom rule: {:?}", lines[2]);
}

#[test]
fn goto_percent_and_byte_move_the_cursor() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "l1\nl2\nl3\nl4\nl5\n"); // 6 lines (incl. trailing)
    // 50% of the way through jumps roughly to the middle.
    app.run_action("nav.goto_percent");
    for c in "50".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    let mid = app.editor.cursor_1based().0;
    assert!((2..=4).contains(&mid), "50% lands mid-file, got line {mid}");

    // Go to byte 0 returns to the start.
    app.run_action("nav.goto_byte");
    app.on_key(key('0'));
    app.on_key(keycode(KeyCode::Enter));
    assert_eq!(
        app.editor.cursor_1based(),
        (1, 1),
        "byte 0 is the file start"
    );
}

#[test]
fn read_only_blocks_edits_but_allows_navigation() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "locked");
    app.run_action("view.read_only");
    // Typing is blocked.
    type_str(&mut app, "XYZ");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "locked",
        "typing blocked"
    );
    // A destructive command is blocked.
    app.run_action("edit.reverse_lines");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "locked",
        "command blocked"
    );
    // Toggling it back off restores editing.
    app.run_action("view.read_only");
    type_str(&mut app, "!");
    let txt = app.editor.active_tab().unwrap().text();
    assert!(txt.ends_with('!'), "editing restored: {txt:?}");
}

#[test]
fn text_transforms_squeeze_eol_and_rot13() {
    // Squeeze blank lines over the whole buffer (no selection).
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "a\n\n\n\nb\n");
    app.run_action("edit.squeeze_blank_lines");
    assert_eq!(app.editor.active_tab().unwrap().text(), "a\n\nb\n");

    // ROT13 over a selection.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "Hello");
    app.on_key(ctrl('a'));
    app.run_action("tools.convert.rot13");
    assert_eq!(app.editor.active_tab().unwrap().text(), "Uryyb");

    // Convert to CRLF.
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "x\ny\n");
    app.run_action("edit.eol_crlf");
    assert_eq!(app.editor.active_tab().unwrap().text(), "x\r\ny\r\n");
}

#[test]
fn which_key_lists_candidates_after_a_leader() {
    let mut app = app_at(Path::new("."));
    app.settings.keymap = "spacemacs".to_string();
    // No prefix pending → no which-key.
    assert!(app.which_key().is_none());
    // Press the Space leader, then 'f' → candidates like "ff", "fr", "fs", "fp".
    app.on_key(key(' '));
    app.on_key(key('f'));
    let (title, rows) = app.which_key().expect("which-key active after SPC f");
    assert!(
        title.contains('f'),
        "title shows the pending sequence: {title:?}"
    );
    assert!(
        rows.iter().any(|(k, a)| k == "f" && a == "file.open"),
        "SPC f f = open: {rows:?}"
    );
    assert!(
        rows.iter().any(|(_, a)| a == "file.save"),
        "includes SPC f s save"
    );
}

#[test]
fn clipboard_history_records_copies_and_pastes_from_it() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta");
    // Select "alpha" (first 5 chars) and copy it.
    app.on_key(keycode(KeyCode::Home));
    for _ in 0..5 {
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
    }
    app.run_action("edit.copy");
    assert!(
        app.clipboard_ring.iter().any(|e| e == "alpha"),
        "copy recorded: {:?}",
        app.clipboard_ring
    );
    // Collapse the selection, then move to end of buffer to paste there.
    app.on_key(keycode(KeyCode::Right));
    app.on_key(keycode(KeyCode::End));
    app.run_action("edit.paste_from_history");
    assert!(app.clipboard_chooser.is_some(), "history picker opens");
    app.on_key(keycode(KeyCode::Enter));
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("betaalpha"),
        "pasted from history"
    );
}

#[test]
fn http_send_reports_when_buffer_has_no_request() {
    // A buffer without a request line is rejected up front (no network attempt).
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "just some prose\n");
    app.run_action("tools.http_send");
    assert!(app.status.to_lowercase().contains("http") || app.status.contains("METHOD"));
    assert!(!app.http_running(), "no request was dispatched");
}

#[test]
fn jump_to_line_labels_move_the_cursor() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "line0\nline1\nline2\nline3\n");
    // Enter jump mode: line N gets label = Nth letter (a, b, c, …).
    app.run_action("nav.jump");
    assert!(app.jump.is_some(), "jump mode active");
    // 'c' is the 3rd label → 0-based line 2.
    app.on_key(key('c'));
    assert!(app.jump.is_none(), "jump mode exits on match");
    assert_eq!(
        app.editor.cursor_1based().0,
        3,
        "cursor on line 3 (0-based 2)"
    );
}

#[test]
fn scratch_buffer_opens_unsaved_with_a_header() {
    let mut app = app_at(Path::new("."));
    let before = app.editor.tabs.len();
    app.run_action("file.scratch");
    assert_eq!(app.editor.tabs.len(), before + 1);
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.path.is_none(), "scratch buffer is not file-backed");
    assert!(tab.text().contains("Scratch buffer"), "has the header");
}

#[test]
fn align_on_equals_pads_the_selection() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "a = 1\nbbb = 2\n");
    app.on_key(ctrl('a')); // select all
    app.run_action("edit.align.equals");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "a   = 1\nbbb = 2\n"
    );
}

#[test]
fn surround_wraps_and_unwraps_the_selection() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "word");
    app.on_key(ctrl('a')); // select all
    app.run_action("edit.surround.paren");
    assert_eq!(app.editor.active_tab().unwrap().text(), "(word)");
    // Repeating the same surround removes it (toggle_wrap behavior).
    app.on_key(ctrl('a'));
    app.run_action("edit.surround.paren");
    assert_eq!(app.editor.active_tab().unwrap().text(), "word");
}

#[test]
fn pomodoro_start_closes_dialog_and_runs_in_background() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.pomodoro");
    assert!(app.pomodoro_open, "dialog visible");
    assert_eq!(app.pomodoro.as_ref().unwrap().label(), "25:00");
    assert!(!app.pomodoro_running());
    app.on_key(keycode(KeyCode::Down)); // 24 minutes
    app.on_key(keycode(KeyCode::Enter)); // Start
    // Start hides the dialog but the countdown keeps running.
    assert!(!app.pomodoro_open, "dialog closed on Start");
    assert!(app.pomodoro_running(), "timer still running in background");
    assert_eq!(app.pomodoro.as_ref().unwrap().label(), "24:00");
    // Reopening reveals the still-running timer.
    app.run_action("tools.pomodoro");
    assert!(app.pomodoro_open);
    app.on_key(keycode(KeyCode::Enter)); // Stop → back to idle, dialog stays open
    assert!(!app.pomodoro_running(), "timer stopped");
    assert!(app.pomodoro_open);
    app.on_key(keycode(KeyCode::Esc)); // close
    assert!(
        !app.pomodoro_open && app.pomodoro.is_none(),
        "dialog closed and timer dropped"
    );
}

#[test]
fn calculator_runs_and_inserts_result() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.calculator");
    assert!(app.calculator.is_some(), "dialog opened");
    for ch in "6*7".chars() {
        app.on_key(key(ch));
    }
    app.on_key(keycode(KeyCode::Enter)); // Run (input focused)
    assert_eq!(app.calculator.as_ref().unwrap().result(), Some("42"));
    app.on_key(keycode(KeyCode::Tab)); // focus Run
    app.on_key(keycode(KeyCode::Tab)); // focus Insert
    app.on_key(keycode(KeyCode::Enter)); // insert
    assert!(app.calculator.is_none(), "dialog closed after insert");
    assert_eq!(app.editor.active_tab().unwrap().text(), "42");
}

#[test]
fn unit_converter_inserts_converted_value() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.convert.unit");
    assert!(app.unit_converter.is_some(), "dialog opened");
    // Default is 1 m → km; the output is "0.001 km".
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.unit_converter.is_none(), "dialog closed after insert");
    assert_eq!(app.editor.active_tab().unwrap().text(), "0.001 km");
}

#[test]
fn color_converter_syncs_fields_and_inserts() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.color_converter");
    assert!(app.color_converter.is_some(), "dialog opened");
    for ch in "#ff0000".chars() {
        app.on_key(key(ch));
    }
    {
        let conv = app.color_converter.as_ref().unwrap();
        assert_eq!(conv.fields[1], "rgb(255, 0, 0)", "RGB field synced");
        assert_eq!(conv.fields[2], "hsl(0, 100%, 50%)", "HSL field synced");
    }
    app.on_key(keycode(KeyCode::Enter)); // insert the focused (HEX) value
    assert!(app.color_converter.is_none(), "dialog closed after insert");
    assert_eq!(app.editor.active_tab().unwrap().text(), "#ff0000");
}

#[test]
fn convert_base64_round_trips_via_actions() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "hello");
    app.run_action("tools.convert.base64.encode");
    assert_eq!(app.editor.active_tab().unwrap().text(), "aGVsbG8=");
    app.run_action("tools.convert.base64.decode");
    assert_eq!(app.editor.active_tab().unwrap().text(), "hello");
}

#[test]
fn format_json_pretty_and_minify() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "{\"a\":1,\"b\":2}");
    app.run_action("tools.format.json_pretty");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("\n  \"a\": 1")
    );
    app.run_action("tools.format.json_minify");
    assert_eq!(app.editor.active_tab().unwrap().text(), "{\"a\":1,\"b\":2}");
}

#[test]
fn convert_markdown_to_html_action_transforms_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "# Title");
    app.run_action("tools.convert.markdown.html");
    assert_eq!(app.editor.active_tab().unwrap().text(), "<h1>Title</h1>\n");
}

#[test]
fn convert_number_base_actions() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "255");
    app.run_action("tools.convert.number.hex");
    assert_eq!(app.editor.active_tab().unwrap().text(), "0xff");
    app.run_action("tools.convert.number.dec");
    assert_eq!(app.editor.active_tab().unwrap().text(), "255");
}

#[test]
fn convert_jwt_decode_action() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhYmMifQ.sig");
    app.run_action("tools.convert.jwt");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("\"alg\": \"HS256\""), "got: {text}");
    assert!(text.contains("\"sub\": \"abc\""), "got: {text}");
}

#[test]
fn convert_toml_to_json_action_transforms_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "name = \"Vix\"\n");
    app.run_action("tools.convert.toml.json");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("\"name\": \"Vix\""), "got: {text}");
}

#[test]
fn convert_csv_to_json_action_transforms_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "a,b\n1,2\n");
    app.run_action("tools.convert.csv.json");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.contains("\"a\": \"1\""), "got: {text}");
}

#[test]
fn convert_failure_leaves_buffer_unchanged() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "not json");
    app.run_action("tools.convert.json.csv");
    // Invalid JSON: the buffer is left intact.
    assert_eq!(app.editor.active_tab().unwrap().text(), "not json");
}

#[test]
fn snippets_picker_inserts_selected_body() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.snippets");
    assert!(app.snippets.is_some(), "picker opened");
    app.on_key(keycode(KeyCode::Enter)); // insert the first snippet
    assert!(app.snippets.is_none(), "picker closed after insert");
    let text = app.editor.active_tab().unwrap().text();
    assert!(text.starts_with("#!/usr/bin/env bash"), "got: {text:?}");
}

#[test]
fn markdown_preview_renders_active_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "# Title\n\n- a\n- b\n");
    app.run_action("tools.markdown_preview");
    let p = app.markdown_preview.as_ref().expect("preview open");
    assert_eq!(p.lines[0], "Title");
    assert!(p.lines.iter().any(|l| l == "• a"), "{:?}", p.lines);
    app.on_key(keycode(KeyCode::Esc));
    assert!(app.markdown_preview.is_none(), "Esc closes the preview");
}

#[test]
fn text_information_reports_counts() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "Hello world.\nHow are you?");
    app.run_action("tools.text_info");
    let p = app.text_info.as_ref().expect("panel open");
    assert_eq!(p.rows[0].label, "Characters");
    assert_eq!(p.rows[0].value, "25");
    assert_eq!(p.rows[1].value, "5"); // words
    assert_eq!(p.rows[3].value, "2"); // sentences
    app.on_key(keycode(KeyCode::Enter)); // insert Characters value (25)
    assert!(app.editor.active_tab().unwrap().text().contains("25"));
}

#[test]
fn checksum_sha256_hashes_whole_buffer_when_unselected() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "abc");
    app.run_action("tools.checksum.sha256");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn checksum_on_long_buffer_keeps_caret_in_range() {
    // Regression: a transform that shrinks the buffer (120 chars → 64-char hash)
    // must move the caret back in range, or the next render panics in char_to_line.
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = app_at(Path::new("."));
    type_str(&mut app, &"x".repeat(120));
    app.run_action("tools.checksum.sha256");
    assert_eq!(app.editor.active_tab().unwrap().text().len(), 64);
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap(); // must not panic
}

#[test]
fn generate_uuid_v4_inserts_a_canonical_uuid() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.uuid.v4");
    let text = app.editor.active_tab().unwrap().text();
    assert_eq!(text.len(), 36, "v4 UUID is 36 chars: {text:?}");
    assert_eq!(text.chars().nth(14), Some('4'), "version digit is 4");
}

#[test]
fn generate_zid_sizes_insert_hex_of_the_right_length() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.zid.128");
    let text = app.editor.active_tab().unwrap().text();
    assert_eq!(text.len(), 32, "128-bit ZID is 32 hex chars: {text:?}");
    assert!(
        text.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    );

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.zid.512");
    assert_eq!(
        app.editor.active_tab().unwrap().text().len(),
        128,
        "512-bit ZID is 128 hex chars"
    );
}

#[test]
fn insert_markdown_snippets_insert_templates() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.markdown.headline1");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "# Headline 1");

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.markdown.link");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("[Example](https://www.example.com)"),
        "link snippet inserted"
    );

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.markdown.table");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("|---|---|---|"),
        "table snippet inserted"
    );
}

#[test]
fn insert_html_snippets_insert_templates() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.html.headline1");
    assert_eq!(
        app.editor.active_tab().unwrap().lines()[0],
        "<h1>Headline</h1>"
    );

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.html.link");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("<a href=\"https://www.example.com\">Example</a>"),
        "link snippet inserted"
    );

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.html.table");
    let text = app.editor.active_tab().unwrap().text();
    assert!(
        text.contains("<table>") && text.contains("<th>x</th>"),
        "table snippet inserted"
    );
}

#[test]
fn select_all_action_selects_whole_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "hello");
    app.run_action("edit.select_all"); // the menu / palette path
    app.on_key(key('z'));
    assert_eq!(app.editor.active_tab().unwrap().text(), "z");
}

#[test]
fn duplicate_line_copies_the_current_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "abc");
    // Ctrl+Shift+D duplicates the cursor line (Ctrl+D now adds a caret).
    app.on_key(KeyEvent::new(
        KeyCode::Char('D'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["abc", "abc"]);
}

#[test]
fn join_lines_merges_current_line_with_next() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "foo\nbar\nbaz");
    app.run_action("edit.go_first"); // cursor to the first line
    app.run_action("edit.join_lines");
    assert_eq!(
        app.editor.active_tab().unwrap().lines(),
        vec!["foo bar", "baz"]
    );
}

#[test]
fn sort_lines_orders_whole_buffer() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "banana\napple\ncherry");
    app.run_action("edit.sort_lines");
    assert_eq!(
        app.editor.active_tab().unwrap().lines(),
        vec!["apple", "banana", "cherry"]
    );
}

#[test]
fn line_transforms_via_actions() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "b\na\nb\na\n");
    app.run_action("edit.sort_unique");
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["a", "b"]);

    let mut app = app_at(Path::new("."));
    type_str(&mut app, "one\ntwo\nthree");
    app.run_action("edit.reverse_lines");
    assert_eq!(
        app.editor.active_tab().unwrap().lines(),
        vec!["three", "two", "one"]
    );

    let mut app = app_at(Path::new("."));
    type_str(&mut app, "x\ny\nx");
    app.run_action("edit.remove_duplicate_lines");
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["x", "y"]);

    let mut app = app_at(Path::new("."));
    type_str(&mut app, "foo   ");
    app.run_action("edit.trim_trailing_whitespace");
    assert_eq!(app.editor.active_tab().unwrap().text(), "foo");
}

#[test]
fn conflict_resolve_keeps_chosen_side() {
    let mut app = app_at(Path::new("."));
    type_str(
        &mut app,
        "a\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> b\nz\n",
    );
    app.run_action("edit.go_first"); // cursor to line 0
    app.run_action("git.conflict_ours");
    assert_eq!(app.editor.active_tab().unwrap().text(), "a\nours\nz\n");
}

#[test]
fn mode_and_suspend_actions() {
    let mut app = app_at(Path::new("."));
    app.run_action("command_mode");
    assert!(
        app.palette.is_some(),
        "command_mode opens the command palette"
    );
    let mut app = app_at(Path::new("."));
    app.run_action("shell_mode");
    assert!(
        app.prompt.is_some(),
        "shell_mode opens the run-command prompt"
    );
    let mut app = app_at(Path::new("."));
    app.run_action("suspend");
    assert!(app.suspend_requested, "suspend flags the main loop");
}

#[test]
fn inlay_hints_render_inline() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "let x = 1;\n");
    // A ": i32" hint just after `x` (char column 5).
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_inlay_hints(vec![(0, 5, ": i32".to_string())]);
    let mut term = Terminal::new(TestBackend::new(80, 6)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let screen: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(screen.contains(": i32"), "inlay hint text is rendered");
    assert!(screen.contains("let x"), "real text still present");
}

#[test]
fn folding_hides_lines_and_renders() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "fn a() {\n  x;\n  y;\n}\nfn b() {}\n");
    // Mark lines 0..=3 as a foldable range (normally supplied by the server).
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_fold_ranges(vec![(0, 3)]);
    app.run_action("edit.go_first"); // cursor to line 0
    app.run_action("editor.fold_toggle");
    let ed = &app.editor.active_tab().unwrap().editor;
    assert!(ed.has_folds(), "fold active");
    assert!(
        ed.is_line_hidden(1) && ed.is_line_hidden(3),
        "inner lines hidden"
    );
    assert!(!ed.is_line_hidden(0), "fold start stays visible");
    assert!(!ed.is_line_hidden(4), "line after fold visible");
    // Rendering with a fold active must not panic.
    let mut term = Terminal::new(TestBackend::new(80, 20)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    app.run_action("editor.unfold_all");
    assert!(!app.editor.active_tab().unwrap().editor.has_folds());
}

#[test]
fn bookmarks_toggle_and_list() {
    let dir = unique_dir("bookmarks");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.txt");
    fs::write(&file, "one\ntwo\nthree\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.run_action("bookmark.toggle");
    assert_eq!(app.bookmarks.len(), 1, "bookmark added");
    app.run_action("bookmark.list");
    assert!(app.location_chooser.is_some(), "bookmark list opened");
    app.on_key(esc());
    app.run_action("bookmark.toggle"); // same line → removes
    assert!(app.bookmarks.is_empty(), "bookmark removed");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn autocomplete_completes_a_buffer_word() {
    let mut app = app_at(Path::new("."));
    // A long word exists earlier; typing its prefix then autocompleting expands it.
    type_str(&mut app, "function\nfun");
    app.run_action("autocomplete");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "function\nfunction"
    );
}

#[test]
fn macro_records_and_replays_editor_keys() {
    let mut app = app_at(Path::new("."));
    app.run_action("macro.record"); // start recording
    assert!(app.macro_recording);
    app.on_key(key('a'));
    app.on_key(key('b'));
    app.run_action("macro.record"); // stop
    assert!(!app.macro_recording);
    assert_eq!(app.editor.active_tab().unwrap().text(), "ab");
    app.run_action("macro.play"); // replays "ab" at the cursor
    assert_eq!(app.editor.active_tab().unwrap().text(), "abab");
}

#[test]
fn accepting_trust_persists_and_loads_automatically_next_launch() {
    let dir = unique_dir("script-trust-accept");
    fs::create_dir_all(dir.join(".vix/scripts")).unwrap();
    fs::write(
        dir.join(".vix/scripts/greet.rhai"),
        r#"register_command("greet", "Greet", "h"); fn h() {}"#,
    )
    .unwrap();
    // Both "launches" share one session file, simulating a real restart --
    // `app_at` deliberately gives every call its own isolated file instead.
    let session_path = isolated_session_path();

    let mut app1 =
        App::new(dir.clone(), Settings::default()).with_session_path(session_path.clone());
    app1.load_scripts();
    app1.maybe_prompt_script_trust();
    assert!(app1.script_trust.is_some(), "should be prompted");
    app1.on_key(key('y'));
    assert!(app1.script_trust.is_none(), "answered");
    assert_eq!(
        loaded_script_command_count(&mut app1),
        1,
        "trusted, so it loads immediately"
    );

    let mut app2 = App::new(dir.clone(), Settings::default()).with_session_path(session_path);
    app2.load_scripts();
    app2.maybe_prompt_script_trust();
    assert!(
        app2.script_trust.is_none(),
        "already trusted -- no reprompt on a later launch"
    );
    assert_eq!(
        loaded_script_command_count(&mut app2),
        1,
        "trust persisted across the \"restart\""
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn declining_trust_persists_but_a_manual_reload_asks_again() {
    let dir = unique_dir("script-trust-decline");
    fs::create_dir_all(dir.join(".vix/scripts")).unwrap();
    fs::write(
        dir.join(".vix/scripts/greet.rhai"),
        r#"register_command("greet", "Greet", "h"); fn h() {}"#,
    )
    .unwrap();
    let session_path = isolated_session_path();

    let mut app1 =
        App::new(dir.clone(), Settings::default()).with_session_path(session_path.clone());
    app1.load_scripts();
    app1.maybe_prompt_script_trust();
    app1.on_key(key('n'));
    assert_eq!(
        loaded_script_command_count(&mut app1),
        0,
        "declined, so it stays unloaded"
    );

    let mut app2 = App::new(dir.clone(), Settings::default()).with_session_path(session_path);
    app2.load_scripts();
    app2.maybe_prompt_script_trust();
    assert!(
        app2.script_trust.is_none(),
        "a plain launch respects an already-recorded \"no\" -- it does not \
         ask every single time, which would defeat persisting the decision \
         at all"
    );
    assert_eq!(loaded_script_command_count(&mut app2), 0);

    // A manual reload, unlike a plain launch, re-asks -- the user's own way
    // to reconsider a workspace they'd declined.
    app2.run_action("script.reload");
    assert!(
        app2.script_trust.is_some(),
        "script.reload re-prompts even after a decline"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn column_ruler_toggles_and_renders() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "some code here\n");
    app.run_action("toggle_ruler");
    assert!(app.show_ruler);
    let mut term = Terminal::new(TestBackend::new(120, 20)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap(); // ruler drawn, must not panic
    app.run_action("toggle_ruler");
    assert!(!app.show_ruler);
}

#[test]
fn overwrite_mode_types_over_characters() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "abc");
    app.run_action("edit.go_first"); // cursor to start of line 0
    app.run_action("toggle_overwrite_mode");
    assert!(app.overwrite);
    app.on_key(key('X'));
    // 'X' overwrites 'a' rather than inserting before it.
    assert_eq!(app.editor.active_tab().unwrap().text(), "Xbc");
    // At end-of-line it inserts normally.
    app.run_action("edit.line_end");
    app.on_key(key('Y'));
    assert_eq!(app.editor.active_tab().unwrap().text(), "XbcY");
}

#[test]
fn spawn_multi_cursor_below_adds_a_caret() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "ab\ncd\nef");
    app.run_action("edit.go_first"); // cursor to line 0, col 0
    app.run_action("spawn_multi_cursor_down");
    assert!(
        app.editor.active_tab().unwrap().editor.has_multi_carets(),
        "a caret was added below"
    );
}

#[test]
fn ctrl_d_adds_a_caret_and_edits_all_occurrences() {
    let mut app = app_at(Path::new("."));
    // The editor core's Ctrl+D. The Apple keymap claims that key for forward
    // delete, so drive this through a keymap that leaves it to the editor.
    app.settings.keymap = "vscode-macos".to_string();
    type_str(&mut app, "foo foo foo");
    // Cursor is at end; move to the start so the first word is "foo".
    app.on_key(ctrl('a')); // select all, then collapse to start via Left
    app.on_key(keycode(KeyCode::Left));
    // Ctrl+D selects the word, then again adds the next occurrence as a caret.
    app.on_key(ctrl('d'));
    app.on_key(ctrl('d'));
    app.on_key(ctrl('d'));
    assert!(
        app.editor.active_tab().unwrap().editor.has_multi_carets(),
        "carets added"
    );
    // Typing replaces every selected occurrence at once.
    type_str(&mut app, "bar");
    assert_eq!(app.editor.active_tab().unwrap().text(), "bar bar bar");
}

#[test]
fn enter_carries_indentation() {
    let mut app = app_at(Path::new("."));
    app.on_key(keycode(KeyCode::Tab));
    type_str(&mut app, "x\ny"); // Tab, 'x', Enter (auto-indent), 'y'
    assert_eq!(
        app.editor.active_tab().unwrap().lines(),
        vec!["    x", "    y"]
    );
}

#[test]
fn alt_down_moves_the_line_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "aaa\nbbb");
    app.on_key(keycode(KeyCode::Up)); // cursor onto line 0
    app.on_key(alt(KeyCode::Down));
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["bbb", "aaa"]);
}

#[test]
fn alt_up_moves_the_line_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "aaa\nbbb"); // cursor on line 1
    app.on_key(alt(KeyCode::Up));
    assert_eq!(app.editor.active_tab().unwrap().lines(), vec!["bbb", "aaa"]);
}

#[test]
fn ctrl_bracket_jumps_to_matching_bracket() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "(x)"); // cursor just after ')'
    app.on_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL));
    assert_eq!(
        app.editor.active_tab().unwrap().editor.get_cursor(),
        0,
        "jumps to '('"
    );
}

#[test]
fn tab_inserts_spaces_by_default() {
    let mut app = app_at(Path::new(".")); // default: spaces, width 4
    app.on_key(keycode(KeyCode::Tab));
    app.on_key(key('x'));
    assert_eq!(
        app.editor.active_tab().unwrap().lines()[0],
        "    x",
        "Tab inserts 4 spaces"
    );
}

#[test]
fn tab_width_setting_controls_space_count() {
    let mut app = app_with(Settings {
        tab_width: 2,
        ..Settings::default()
    });
    app.on_key(keycode(KeyCode::Tab));
    app.on_key(key('y'));
    assert_eq!(
        app.editor.active_tab().unwrap().lines()[0],
        "  y",
        "tab_width=2 inserts 2 spaces"
    );
}

#[test]
fn indent_style_tabs_inserts_a_tab() {
    let mut app = app_with(Settings {
        indent_style: "tabs".to_string(),
        ..Settings::default()
    });
    app.on_key(keycode(KeyCode::Tab));
    app.on_key(key('z'));
    assert_eq!(
        app.editor.active_tab().unwrap().lines()[0],
        "\tz",
        "tabs style inserts a tab"
    );
}

#[test]
fn types_into_buffer_and_marks_dirty() {
    let mut app = app_at(Path::new("."));
    assert!(!app.editor.active_tab().unwrap().dirty);
    for c in "hello".chars() {
        app.on_key(key(c));
    }
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.lines()[0], "hello");
    assert!(tab.dirty, "typing should mark the buffer dirty");
}

#[test]
fn open_edit_save_round_trip() {
    let dir = unique_dir("save");
    let file = dir.join("note.txt");
    fs::write(&file, "first line\nsecond line\n").unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&file.clone());
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "first line");

    // Move to end of the first line and append text.
    app.on_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    for c in "!!!".chars() {
        app.on_key(key(c));
    }
    app.run_action("file.save");
    let saved = fs::read_to_string(&file).unwrap();
    assert!(saved.starts_with("first line!!!"), "got: {saved:?}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn edit_table_opens_edits_and_saves_csv() {
    let dir = unique_dir("table");
    let file = dir.join("data.csv");
    fs::write(&file, "name,age\nalice,30\nbob,25\n").unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&file.clone());

    app.run_action("tools.edit_table");
    assert!(
        app.edit_table.is_some(),
        "table editor opened on the CSV buffer"
    );

    // Move to alice's age cell (row 1, col 1) and change 30 -> 31.
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Right));
    app.on_key(keycode(KeyCode::Enter)); // begin edit, seeded with "30"
    app.on_key(keycode(KeyCode::Backspace));
    app.on_key(keycode(KeyCode::Backspace));
    for c in "31".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter)); // commit

    // Ctrl+S writes the grid back through the normal save flow.
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    let saved = fs::read_to_string(&file).unwrap();
    assert!(saved.contains("alice,31"), "edit persisted; got: {saved:?}");
    assert!(
        saved.contains("bob,25"),
        "other rows intact; got: {saved:?}"
    );

    // Esc closes the editor.
    app.on_key(keycode(KeyCode::Esc));
    assert!(app.edit_table.is_none(), "Esc closes the table editor");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn edit_outline_opens_indents_and_saves() {
    // Distinct tag from `outline_panel_lists_symbols_and_jumps`'s `unique_dir`
    // call below (a different feature -- the prose outline *editor*, not the
    // symbol outline *panel*): `unique_dir` keys solely on tag + process id,
    // so a shared tag is a real same-path race under parallel test threads,
    // not just a naming coincidence (T143 found this the hard way).
    let dir = unique_dir("edit-outline");
    let file = dir.join("notes.txt");
    fs::write(&file, "A\nB\n  B1\nC\n").unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&file.clone());

    app.run_action("tools.edit_outline");
    assert!(
        app.edit_outline.is_some(),
        "outline editor opened on the buffer"
    );

    // Move to B and indent it (with its child B1) under A via Tab.
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Tab));

    // Ctrl+S writes the restructured outline back; indentation is regenerated.
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    let saved = fs::read_to_string(&file).unwrap();
    assert_eq!(
        saved, "A\n  B\n    B1\nC\n",
        "B indented under A; got: {saved:?}"
    );

    // Esc closes the editor.
    app.on_key(keycode(KeyCode::Esc));
    assert!(app.edit_outline.is_none(), "Esc closes the outline editor");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn edit_json_opens_edits_and_saves() {
    let dir = unique_dir("ejson");
    let file = dir.join("data.json");
    fs::write(&file, "{\n  \"a\": 1\n}\n").unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&file.clone());
    app.run_action("tools.edit_json");
    assert!(app.edit_value.is_some(), "JSON editor opened");

    app.on_key(keycode(KeyCode::Down)); // select "a"
    app.on_key(keycode(KeyCode::Enter)); // edit value
    app.on_key(keycode(KeyCode::Backspace));
    app.on_key(key('2'));
    app.on_key(keycode(KeyCode::Enter)); // commit
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

    let saved = fs::read_to_string(&file).unwrap();
    assert!(
        saved.contains("\"a\": 2"),
        "value edit persisted; got: {saved:?}"
    );

    app.on_key(keycode(KeyCode::Esc));
    assert!(app.edit_value.is_none(), "Esc closes the JSON editor");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn edit_bytes_opens_overwrites_and_saves() {
    let dir = unique_dir("ebytes");
    let file = dir.join("b.txt");
    fs::write(&file, "hello").unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&file.clone());
    app.run_action("tools.edit_bytes");
    assert!(app.edit_bytes.is_some(), "byte editor opened");

    // Overwrite the first byte 'h' (0x68) with 0x41 = 'A' by typing "41".
    app.on_key(key('4'));
    app.on_key(key('1'));
    app.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

    let saved = fs::read_to_string(&file).unwrap();
    assert!(
        saved.starts_with("Aello"),
        "byte overwrite persisted; got: {saved:?}"
    );

    app.on_key(keycode(KeyCode::Esc));
    assert!(app.edit_bytes.is_none(), "Esc closes the byte editor");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn insert_lorem_and_datetime_presets() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.lorem.words");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .starts_with("Lorem ipsum"),
        "lorem words inserted"
    );

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.datetime.epoch");
    let epoch = app.editor.active_tab().unwrap().text();
    assert!(
        !epoch.is_empty() && epoch.chars().all(|c| c.is_ascii_digit()),
        "epoch is all digits: {epoch:?}"
    );

    let mut app = app_at(Path::new("."));
    app.run_action("tools.insert.datetime.rfc3339");
    let rfc = app.editor.active_tab().unwrap().text();
    assert!(
        rfc.contains('T') && rfc.contains(':'),
        "rfc3339 date-time shape: {rfc:?}"
    );
}

#[test]
fn qrcode_overlay_generates_and_closes() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "https://example.com");
    app.run_action("tools.qrcode");
    assert!(
        app.qrcode.as_ref().is_some_and(|art| !art.is_empty()),
        "QR overlay rendered from the current line"
    );
    app.on_key(keycode(KeyCode::Esc));
    assert!(app.qrcode.is_none(), "Esc closes the QR overlay");
}

#[test]
fn zen_mode_hides_then_restores_chrome() {
    let mut app = app_at(Path::new("."));
    app.show_explorer = true;
    app.show_messages = true;
    app.show_status_bar = true;
    app.show_bottom_dock = true;

    app.run_action("view.zen");
    assert!(app.is_zen(), "zen mode on");
    assert!(
        !app.show_explorer && !app.show_messages && !app.show_status_bar && !app.show_bottom_dock,
        "zen hides the chrome"
    );

    app.run_action("view.zen");
    assert!(!app.is_zen(), "zen mode off");
    assert!(
        app.show_explorer && app.show_messages && app.show_status_bar && app.show_bottom_dock,
        "zen restores prior visibility"
    );
}

#[test]
fn column_select_block_edits_multiple_lines() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "aa\nbb\ncc");
    app.run_action("edit.go_first"); // cursor to buffer start (line 0, col 0)
    app.on_key(KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::ALT | KeyModifiers::SHIFT,
    ));
    app.on_key(KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::ALT | KeyModifiers::SHIFT,
    ));
    assert!(
        app.editor.active_tab().unwrap().editor.has_multi_carets(),
        "Alt+Shift+Down builds a vertical block of carets"
    );
    app.on_key(key('X')); // type at every caret
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "Xaa\nXbb\nXcc",
        "block insert lands on each line"
    );
}

#[test]
fn select_all_occurrences_creates_multi_carets() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "foo bar foo baz foo");
    app.on_key(keycode(KeyCode::Home)); // cursor onto the first "foo"
    app.run_action("edit.select_all_occurrences");
    assert!(
        app.editor.active_tab().unwrap().editor.has_multi_carets(),
        "every occurrence becomes a caret"
    );
}

#[test]
fn breadcrumb_shows_file_and_enclosing_symbol() {
    let dir = unique_dir("crumb");
    let file = dir.join("m.rs");
    fs::write(&file, "fn alpha() {}\nfn beta() {\n    let x = 1;\n}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Down)); // cursor on line 3, inside beta

    app.run_action("view.breadcrumbs");
    assert!(app.show_breadcrumbs, "breadcrumb bar toggled on");
    let crumb = app.breadcrumb();
    assert!(crumb.starts_with("m.rs"), "shows the file name: {crumb:?}");
    assert!(
        crumb.contains("beta"),
        "shows the enclosing symbol: {crumb:?}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn on_save_toggles_flip_settings() {
    let mut app = app_at(Path::new("."));
    let trim = app.settings.trim_trailing_whitespace;
    app.run_action("view.trim_on_save");
    assert_eq!(
        app.settings.trim_trailing_whitespace, !trim,
        "trim-on-save toggled"
    );

    let nl = app.settings.ensure_final_newline;
    app.run_action("view.final_newline_on_save");
    assert_eq!(
        app.settings.ensure_final_newline, !nl,
        "final-newline-on-save toggled"
    );
}

#[test]
fn smart_home_toggles_first_nonblank_and_column0() {
    let dir = unique_dir("smarthome");
    let file = dir.join("h.txt");
    fs::write(&file, "    hello\n").unwrap(); // four-space indent
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.on_key(keycode(KeyCode::End));
    assert_eq!(app.editor.cursor_1based().1, 10, "end of '    hello'");
    // First Home -> first non-blank (column index 4 -> 1-based 5).
    app.on_key(keycode(KeyCode::Home));
    assert_eq!(
        app.editor.cursor_1based().1,
        5,
        "Home jumps to first non-blank"
    );
    // Second Home -> column 0.
    app.on_key(keycode(KeyCode::Home));
    assert_eq!(
        app.editor.cursor_1based().1,
        1,
        "Home again jumps to column 0"
    );
    // Third Home -> back to first non-blank.
    app.on_key(keycode(KeyCode::Home));
    assert_eq!(
        app.editor.cursor_1based().1,
        5,
        "toggles back to first non-blank"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_trims_trailing_whitespace_by_default() {
    let dir = unique_dir("trim");
    let file = dir.join("t.txt");
    fs::write(&file, "abc\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file.clone());
    app.on_key(keycode(KeyCode::End));
    for _ in 0..3 {
        app.on_key(key(' '));
    }
    app.run_action("file.save");
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "abc\n",
        "trailing spaces trimmed"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_ensures_final_newline_by_default() {
    let dir = unique_dir("newline");
    let file = dir.join("n.txt");
    fs::write(&file, "abc").unwrap(); // no trailing newline
    let mut app = app_at(&dir);
    app.open_initial(&file.clone());
    app.run_action("file.save");
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "abc\n",
        "final newline added"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_respects_disabled_normalization() {
    let dir = unique_dir("rawsave");
    let file = dir.join("r.txt");
    fs::write(&file, "abc").unwrap(); // no trailing newline
    let mut app = app_at(&dir);
    app.settings.trim_trailing_whitespace = false;
    app.settings.ensure_final_newline = false;
    app.open_initial(&file.clone());
    app.on_key(keycode(KeyCode::End));
    for _ in 0..2 {
        app.on_key(key(' '));
    }
    app.run_action("file.save");
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "abc  ",
        "no trim and no final newline when both disabled"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn new_and_close_keep_one_buffer() {
    let mut app = app_at(Path::new("."));
    app.run_action("file.new");
    app.run_action("file.new");
    assert_eq!(app.editor.tabs.len(), 3);
    app.run_action("file.close");
    app.run_action("file.close");
    app.run_action("file.close");
    assert_eq!(app.editor.tabs.len(), 1, "always keeps one buffer open");
}

#[test]
fn goto_line_moves_cursor() {
    let dir = unique_dir("goto");
    let file = dir.join("many.txt");
    fs::write(&file, "a\nb\nc\nd\ne\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.editor.goto(4, Some(1), Rect::new(0, 0, 80, 24));
    assert_eq!(app.editor.cursor_1based().0, 4);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn quit_action_sets_flag() {
    let mut app = app_at(Path::new("."));
    app.on_key(ctrl('q'));
    assert!(app.should_quit);
}

#[test]
fn view_time_zone_action_sets_active_zone() {
    let mut app = app_at(Path::new("."));
    app.run_action("view.time_zone:America/New_York");
    assert_eq!(app.settings.time_zone, "America/New_York");
    app.run_action("view.time_zone:Not/AZone"); // unknown ignored
    assert_eq!(app.settings.time_zone, "America/New_York");
    app.run_action("view.time_zone:UTC"); // restore the process-global active zone
}

#[test]
fn line_number_toggle() {
    let mut app = app_at(Path::new("."));
    let before = app.editor.flags.contains(vix::editor::Flags::LINE_NUMBERS);
    app.run_action("tools.line_numbers");
    assert_ne!(
        before,
        app.editor.flags.contains(vix::editor::Flags::LINE_NUMBERS)
    );
}

#[test]
fn visible_whitespace_toggle() {
    let mut app = app_at(Path::new("."));
    // Off by default; the action toggles it and persists the setting.
    assert!(
        !app.editor
            .flags
            .contains(vix::editor::Flags::SHOW_WHITESPACE)
    );
    assert!(!app.settings.show_whitespace);
    app.run_action("view.whitespace");
    assert!(
        app.editor
            .flags
            .contains(vix::editor::Flags::SHOW_WHITESPACE),
        "toggles visible whitespace on"
    );
    assert!(app.settings.show_whitespace, "persists the new setting");
    app.run_action("view.whitespace");
    assert!(
        !app.editor
            .flags
            .contains(vix::editor::Flags::SHOW_WHITESPACE),
        "toggles back off"
    );
}

#[test]
fn status_bar_info_accessors() {
    let dir = unique_dir("statusinfo");
    let file = dir.join("s.rs");
    fs::write(&file, "fn main() {}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.editor.language(), "rust", "language from extension");
    assert_eq!(tab.editor.line_ending(), "LF");
    assert!(
        tab.editor.selection_span().is_none(),
        "no selection initially"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn line_ending_detects_crlf() {
    let dir = unique_dir("crlf");
    let file = dir.join("c.txt");
    fs::write(&file, "a\r\nb\r\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    assert_eq!(
        app.editor.active_tab().unwrap().editor.line_ending(),
        "CRLF"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn soft_wrap_toggle() {
    let mut app = app_at(Path::new("."));
    assert!(
        !app.editor.flags.contains(vix::editor::Flags::SOFT_WRAP),
        "off by default"
    );
    assert!(!app.settings.soft_wrap);
    app.run_action("view.soft_wrap");
    assert!(
        app.editor.flags.contains(vix::editor::Flags::SOFT_WRAP),
        "toggles soft wrap on"
    );
    assert!(app.settings.soft_wrap, "persists the setting");
    app.run_action("view.soft_wrap");
    assert!(
        !app.editor.flags.contains(vix::editor::Flags::SOFT_WRAP),
        "toggles back off"
    );
}

#[test]
fn toggle_comment_round_trips_and_is_undoable() {
    let mut app = app_at(Path::new("."));
    for c in "hello".chars() {
        app.on_key(key(c));
    }
    // A new (untitled) buffer uses the default `//` token.
    app.run_action("edit.toggle_comment");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "//hello");
    assert!(app.editor.active_tab().unwrap().dirty);
    // Toggling again removes it.
    app.run_action("edit.toggle_comment");
    assert_eq!(
        app.editor.active_tab().unwrap().lines()[0],
        "hello",
        "second toggle uncomments"
    );
    // And the whole thing is a single undoable edit.
    app.run_action("edit.toggle_comment");
    app.run_action("edit.undo");
    assert_eq!(
        app.editor.active_tab().unwrap().lines()[0],
        "hello",
        "undo reverts the comment"
    );
}

#[test]
fn ctrl_slash_toggles_comment() {
    let mut app = app_at(Path::new("."));
    app.on_key(key('x'));
    app.on_key(ctrl('/'));
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "//x");
}

#[test]
fn toggle_comment_uses_language_token() {
    let dir = unique_dir("comment");
    let file = dir.join("c.toml");
    fs::write(&file, "key = 1\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.run_action("edit.toggle_comment");
    assert_eq!(
        app.editor.active_tab().unwrap().lines()[0],
        "#key = 1",
        "TOML uses #"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn goto_symbol_mode_lists_and_jumps() {
    let dir = unique_dir("symbols");
    let file = dir.join("s.rs");
    fs::write(
        &file,
        "fn alpha() {}\nlet x = 1;\nstruct Beta {}\nfn gamma() {}\n",
    )
    .unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("nav.goto_symbol");
    let p = app.palette.as_ref().expect("palette open");
    assert!(
        matches!(p.mode(), vix::palette::Mode::Symbols),
        "@ enters symbols mode"
    );
    assert_eq!(
        p.entries.len(),
        3,
        "three declarations (the `let` is excluded)"
    );

    // Filter to a single symbol, then jump to it.
    for c in "gamma".chars() {
        app.on_key(key(c));
    }
    assert_eq!(app.palette.as_ref().unwrap().entries.len(), 1);
    app.on_key(keycode(KeyCode::Enter));
    assert!(
        app.palette.is_none(),
        "Enter accepts and closes the palette"
    );
    assert_eq!(app.editor.cursor_1based().0, 4, "jumped to gamma's line");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn help_overlay_toggles() {
    let mut app = app_at(Path::new("."));
    assert!(app.help.is_none());
    app.on_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    assert!(app.help.is_some(), "F1 opens the help overlay");
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.help.is_none(), "Esc closes the help overlay");
}

#[test]
fn help_overlay_lists_all_active_shortcuts_and_sorts() {
    use vix::keyboard_shortcut_panel::Column;
    let mut app = app_at(Path::new("."));
    app.on_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    let h = app.help.as_ref().expect("open");
    // Curated global rows and menu accelerators are both present.
    assert!(
        h.rows.iter().any(|s| s.keys == "Ctrl P"),
        "curated global row present"
    );
    assert!(
        h.rows.iter().any(|s| s.keys == "Ctrl Shift T"),
        "menu accelerator (File → Reopen Closed) present"
    );
    // Typing filters both columns; header toggles sort the table.
    let total = h.len();
    for c in "ctrl s".chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    let h = app.help.as_ref().unwrap();
    assert!(h.len() < total, "filter narrows the list");
    let h = app.help.as_mut().unwrap();
    h.query.clear();
    h.toggle_sort(Column::Keys);
    let asc: Vec<String> = h
        .matches()
        .iter()
        .map(|&i| h.rows[i].keys.clone())
        .collect();
    h.toggle_sort(Column::Keys);
    let desc: Vec<String> = h
        .matches()
        .iter()
        .map(|&i| h.rows[i].keys.clone())
        .collect();
    let mut rev = desc.clone();
    rev.reverse();
    assert_eq!(asc, rev, "second click flips the order");
}

#[test]
fn new_overlays_render_without_panicking() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let dir = unique_dir("overlaydraw");
    fs::write(dir.join("a.txt"), "hello").unwrap();
    let mut app = app_at(&dir);
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();

    // File browser (File → Open…).
    app.run_action("file.open");
    assert!(app.file_browser.is_some());
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    assert!(
        app.layout.file_browser.height > 0,
        "browser rows rect recorded"
    );
    app.on_key(esc());

    // Keyboard shortcuts (Help → Keyboard Shortcuts…), sorted by keys.
    app.run_action("help.shortcuts");
    if let Some(h) = app.help.as_mut() {
        h.toggle_sort(vix::keyboard_shortcut_panel::Column::Keys);
    }
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    assert!(
        app.layout.help_headers[0].width > 0 && app.layout.help_headers[1].width > 0,
        "clickable header rects recorded"
    );
    app.on_key(esc());

    // Multi-column recent chooser (File → Open Recent…).
    app.settings
        .recent_files
        .push(dir.join("a.txt").display().to_string());
    app.run_action("file.open_recent");
    assert!(app.recent_chooser.is_some());
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn fuzzy_matches_space_separated_terms() {
    assert!(fuzzy_match("features/groups/view.tsx", "feat group"));
    assert!(fuzzy_match("/etc/hosts", "etc hosts"));
    assert!(fuzzy_match("src/save_file.rs", "save file"));
    assert!(!fuzzy_match("src/main.rs", "zzz"));
}

#[test]
fn parses_path_line_col() {
    assert_eq!(
        parse_path_target("src/main.rs:42:10"),
        ("src/main.rs".to_string(), Some((42, 10)))
    );
    assert_eq!(
        parse_path_target("src/main.rs:42"),
        ("src/main.rs".to_string(), Some((42, 1)))
    );
    assert_eq!(
        parse_path_target("src/main.rs"),
        ("src/main.rs".to_string(), None)
    );
}

#[test]
fn reopen_closed_tab_restores_the_last_closed_file() {
    let dir = unique_dir("reopen");
    fs::write(dir.join("a.txt"), "aaa").unwrap();
    fs::write(dir.join("b.txt"), "bbb").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("a.txt"));
    app.open_initial(&dir.join("b.txt")); // active: b.txt

    // Close b.txt, then reopen it.
    app.run_action("file.close");
    assert!(
        !app.editor
            .tabs
            .iter()
            .any(|t| t.path.as_deref() == Some(dir.join("b.txt").as_path())),
        "b.txt is closed"
    );
    app.run_action("file.reopen_closed");
    assert!(
        app.editor.tabs.iter().any(|t| t.path.as_deref()
            == Some(dir.join("b.txt").canonicalize().unwrap().as_path())),
        "b.txt is reopened"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn close_all_tabs_leaves_one_empty_buffer() {
    let dir = unique_dir("closeall");
    fs::write(dir.join("a.txt"), "aaa").unwrap();
    fs::write(dir.join("b.txt"), "bbb").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("a.txt"));
    app.open_initial(&dir.join("b.txt"));
    assert!(app.editor.tabs.len() >= 2, "two files are open");

    app.run_action("file.close_all");

    assert_eq!(app.editor.tabs.len(), 1, "exactly one buffer remains");
    let t = app.editor.active_tab().unwrap();
    assert!(t.path.is_none(), "the remaining buffer is untitled");
    assert!(t.text().is_empty(), "the remaining buffer is empty");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn iso_formats_have_expected_shape() {
    let now = clock::now_local();
    let utc = clock::utc_iso(&now);
    assert_eq!(utc.len(), 20, "{utc}"); // YYYY-MM-DDTHH:MM:SSZ
    assert!(utc.ends_with('Z'));
    assert_eq!(&utc[4..5], "-");
    assert_eq!(&utc[10..11], "T");

    let week = clock::iso_week_date(&now);
    assert!(week.contains("-W"), "{week}"); // YYYY-Www-D
    let day = week.chars().last().unwrap();
    assert!(('1'..='7').contains(&day), "weekday digit: {week}");

    let clk = clock::local_clock(&now);
    assert_eq!(clk.len(), 8, "{clk}"); // HH:MM:SS

    let local = clock::local_datetime(&now);
    assert_eq!(local.len(), 19, "{local}"); // YYYY-MM-DD HH:MM:SS

    let grid = calendar::month_grid(now.date());
    let count: usize = grid.weeks.iter().flatten().filter(|c| c.is_some()).count();
    assert!((28..=31).contains(&count), "days in month: {count}");
}

#[test]
fn narrow_editor_does_not_panic() {
    // The code editor's focus() underflows on tiny widths; the app clamps the
    // viewport it hands over, so typing into a 5-column editor must not panic.
    let mut app = App::new(PathBuf::from("."), Settings::default());
    app.layout.editor = Rect::new(0, 0, 5, 3);
    for c in "abc".chars() {
        app.on_key(key(c));
    }
    // Go-to-line through the command palette also routes through the clamp.
    app.on_key(ctrl('p'));
    for c in ":1".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "abc");
}

#[test]
fn toggle_status_bar_action_flips_and_persists() {
    let mut app = app_at(Path::new("."));
    assert!(app.show_status_bar, "the status bar is shown by default");
    app.run_action("view.status_bar");
    assert!(!app.show_status_bar, "the action hides the status bar");
    assert!(
        !app.settings.show_status_bar,
        "the choice persists in settings"
    );
    app.run_action("view.status_bar");
    assert!(app.show_status_bar, "toggling again shows it");
}

#[test]
fn toggle_scrollbar_flips_persists_and_reclaims_the_column() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut app = app_at(Path::new("."));
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();

    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    assert!(app.show_scrollbar, "shown by default");
    let with = app.layout.editor.width;
    assert!(app.layout.scrollbar.width > 0, "scrollbar has a column");

    app.run_action("view.scrollbar"); // hide it
    assert!(!app.show_scrollbar);
    assert!(!app.settings.show_scrollbar, "choice persists");
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    assert_eq!(app.layout.scrollbar.width, 0, "scrollbar column collapses");
    assert_eq!(
        app.layout.editor.width,
        with + 1,
        "the text reclaims the column"
    );
}

#[test]
fn clock_box_inserts_a_time_row() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut app = app_at(Path::new("."));
    app.run_action("tools.clock");
    assert!(app.show_clock, "the action opens the clock box");
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let r = app.layout.clock;
    assert!(r.width > 0, "the clock rect was recorded");

    // Click the first row (local date-time): inserts a date-time; the box stays
    // open so several values can be picked.
    app.on_mouse(click(r.x + 1, r.y));
    let text = app.editor.active_tab().unwrap().text();
    assert!(
        text.contains(':') && text.contains('-'),
        "inserted a date-time: {text:?}"
    );
    assert!(app.show_clock, "a row click keeps the clock box open");

    // A click outside the box closes it.
    app.on_mouse(click(0, 23));
    assert!(!app.show_clock, "an outside click closes the clock box");
}

#[test]
fn run_command_streams_output_to_bottom_dock() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.run_command");
    assert!(app.prompt.is_some(), "the action opens a command prompt");
    for c in "echo hello-vix".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));

    assert!(app.prompt.is_none(), "Enter runs and closes the prompt");
    assert!(app.show_bottom_dock, "running shows the bottom dock");

    // The command runs in a background thread; drain it like the event loop does.
    let mut waited = 0;
    while app.command_running() && waited < 300 {
        app.poll_command();
        std::thread::sleep(std::time::Duration::from_millis(10));
        waited += 1;
    }
    app.poll_command();

    let out = app.bottom_dock.lines.join("\n");
    assert!(
        out.contains("$ echo hello-vix"),
        "echoes the command: {out:?}"
    );
    assert!(out.contains("hello-vix"), "shows the output: {out:?}");
    assert!(out.contains("[exit 0]"), "shows the exit code: {out:?}");
}

#[test]
fn cancel_command_kills_a_running_command() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.run_command");
    for c in "sleep 5".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.command_running(), "the command is running");

    app.run_action("tools.cancel_command");
    // A generous budget (10s): killing and reaping a child process can lag
    // well past typical polling on a loaded/shared CI runner, and this test
    // has been observed to flake at a tighter budget on GitHub's runners
    // even though it is instant locally.
    let mut waited = 0;
    while app.command_running() && waited < 1000 {
        app.poll_command();
        std::thread::sleep(std::time::Duration::from_millis(10));
        waited += 1;
    }
    assert!(!app.command_running(), "cancel ended the command");
    let out = app.bottom_dock.lines.join("\n");
    assert!(
        out.contains("[cancelled]"),
        "shows it was cancelled: {out:?}"
    );
}

#[test]
fn clicking_a_dock_location_jumps_to_it() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let dir = unique_dir("dockjump");
    let file = dir.join("hit.txt");
    fs::write(&file, "a\nb\nTARGET\nd\n").unwrap();
    let mut app = app_at(&dir);
    // The bottom dock is shown by default.
    // A grep-style line: path:line:col:text (pointing at line 3).
    app.bottom_dock
        .push(format!("{}:3:1: TARGET", file.display()));
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();

    let r = app.layout.bottom_dock;
    app.on_mouse(click(r.x + 1, r.y + 1)); // first content row = the line

    let tab = app.editor.active_tab().unwrap();
    assert_eq!(
        tab.path.as_deref(),
        Some(file.canonicalize().unwrap().as_path()),
        "opened the file"
    );
    assert_eq!(tab.cursor_1based().0, 3, "jumped to line 3");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn bottom_dock_top_edge_drag_resizes() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut app = app_at(Path::new("."));
    let mut term = Terminal::new(TestBackend::new(80, 40)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();

    let r = app.layout.bottom_dock;
    let before = app.settings.bottom_dock_height;
    // Press the top edge and drag up four rows → taller.
    app.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), r.x + 1, r.y));
    app.on_mouse(mouse(
        MouseEventKind::Drag(MouseButton::Left),
        r.x + 1,
        r.y - 4,
    ));
    assert!(
        app.settings.bottom_dock_height > before,
        "dragging the top edge up grows the dock"
    );
    app.on_mouse(mouse(
        MouseEventKind::Up(MouseButton::Left),
        r.x + 1,
        r.y - 4,
    ));

    // Re-render so the dock rect reflects the new height, then drag down → shorter.
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let grown = app.settings.bottom_dock_height;
    let r2 = app.layout.bottom_dock;
    app.on_mouse(mouse(
        MouseEventKind::Down(MouseButton::Left),
        r2.x + 1,
        r2.y,
    ));
    app.on_mouse(mouse(
        MouseEventKind::Drag(MouseButton::Left),
        r2.x + 1,
        r2.y + 3,
    ));
    assert!(
        app.settings.bottom_dock_height < grown,
        "dragging the top edge down shrinks it"
    );
}

#[test]
fn bottom_dock_focus_and_scroll() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut app = app_at(Path::new("."));
    for i in 0..50 {
        app.bottom_dock.push(format!("line {i}"));
    }
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let r = app.layout.bottom_dock;
    assert!(r.height > 0, "the dock rect was recorded");

    // A click focuses the dock; it starts pinned to the bottom.
    app.on_mouse(click(r.x + 1, r.y + 1));
    assert_eq!(app.focus, vix::app::Focus::BottomDock);
    let pinned = app.bottom_dock.scroll;

    // Up / wheel scroll back through the buffer.
    app.on_key(keycode(KeyCode::Up));
    assert!(app.bottom_dock.scroll < pinned, "Up scrolls back");
    app.on_mouse(mouse(MouseEventKind::ScrollUp, r.x + 1, r.y + 1));
    let after_wheel = app.bottom_dock.scroll;
    app.on_key(keycode(KeyCode::Home));
    assert_eq!(app.bottom_dock.scroll, 0, "Home jumps to the top");
    assert!(after_wheel < pinned);

    // Esc returns focus to the editor.
    app.on_key(esc());
    assert_eq!(app.focus, vix::app::Focus::Editor);
}

#[test]
fn toggle_bottom_dock_flips_persists_and_renders() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut app = app_at(Path::new("."));
    // All three docks (left explorer, right messages, bottom) show by default.
    assert!(app.show_bottom_dock, "shown by default");
    assert!(
        app.show_explorer && app.show_messages,
        "side docks default on"
    );

    app.run_action("view.bottom_dock");
    assert!(!app.show_bottom_dock, "the action hides the bottom dock");
    assert!(!app.settings.show_bottom_dock, "the choice persists");

    app.run_action("view.bottom_dock");
    assert!(app.show_bottom_dock, "toggling again shows it");

    // The dock buffers lines and renders without panicking.
    app.bottom_dock.push("hello from the bottom dock");
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
}

#[test]
fn draw_handles_a_hidden_status_bar() {
    // A full render with the status bar hidden must lay out and paint without
    // panicking (the body row now consumes the freed line).
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut app = app_at(Path::new("."));
    app.run_action("view.status_bar"); // hide it
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
}

#[test]
fn unique_copy_name_suffixes() {
    let dir = unique_dir("uniq");
    fs::write(dir.join("a.txt"), "x").unwrap();
    let got = fileops::unique_copy_name(&dir, &dir.join("a.txt"));
    assert_eq!(got.file_name().unwrap(), "a copy.txt");
    fs::write(dir.join("a copy.txt"), "x").unwrap();
    let got2 = fileops::unique_copy_name(&dir, &dir.join("a.txt"));
    assert_eq!(got2.file_name().unwrap(), "a copy 2.txt");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn position_history_back_and_forward() {
    let dir = unique_dir("nav");
    fs::write(dir.join("a.txt"), "1\n2\n3\n4\n5\n").unwrap();
    fs::write(dir.join("b.txt"), "a\nb\nc\nd\ne\n").unwrap();
    let mut app = app_at(&dir);

    let open_at = |app: &mut App, spec: &str| {
        app.run_action("file.open");
        for c in spec.chars() {
            app.on_key(key(c));
        }
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    };
    let here = |app: &App| -> (String, usize) {
        let t = app.editor.active_tab().unwrap();
        let name = t
            .path
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        (name, app.editor.cursor_1based().0)
    };

    open_at(&mut app, "a.txt:3");
    assert_eq!(here(&app), ("a.txt".into(), 3));
    open_at(&mut app, "b.txt:2");
    assert_eq!(here(&app), ("b.txt".into(), 2));

    // Alt+Left goes back to the previous location.
    app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
    assert_eq!(
        here(&app),
        ("a.txt".into(), 3),
        "Alt+Left → previous position"
    );

    // Alt+Right returns forward.
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
    assert_eq!(here(&app), ("b.txt".into(), 2), "Alt+Right → next position");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn auto_pairs_brackets_and_deletes_empty_pair() {
    let dir = unique_dir("autopair");
    fs::create_dir_all(&dir).unwrap();
    let mut app = app_at(&dir);

    app.on_key(key('('));
    {
        let t = app.editor.active_tab().unwrap();
        assert_eq!(t.editor.get_content(), "()", "closer auto-inserted");
        assert_eq!(t.editor.get_cursor(), 1, "cursor sits between the pair");
    }
    // Typing the closer steps over the auto-inserted one rather than doubling it.
    app.on_key(key(')'));
    assert_eq!(app.editor.active_tab().unwrap().editor.get_content(), "()");
    // Backspace at the caret-between position deletes both halves.
    app.on_key(key('('));
    app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.editor.active_tab().unwrap().editor.get_content(), "()");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn snippet_expands_with_navigable_tabstops() {
    let dir = unique_dir("snippet-tabstops");
    fs::create_dir_all(&dir).unwrap();
    let mut app = app_at(&dir);

    // Open the Snippets picker and choose "Rust function" (index 7).
    app.run_action("tools.snippets");
    for _ in 0..7 {
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    {
        let t = app.editor.active_tab().unwrap();
        assert_eq!(t.editor.get_content(), "fn name() -> () {\n    \n}\n");
        // The first tabstop's placeholder ("name") is selected.
        assert_eq!(t.editor.selection_span(), Some((3, 7)));
    }
    // Typing replaces the selected placeholder.
    for c in "foo".chars() {
        app.on_key(key(c));
    }
    assert_eq!(
        app.editor.active_tab().unwrap().editor.get_content(),
        "fn foo() -> () {\n    \n}\n"
    );
    // Tab jumps to the (empty) parameter tabstop, between the parens.
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.editor.active_tab().unwrap().editor.get_cursor(), 7);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn detects_image_extensions() {
    use vix::editor::is_image_path;
    assert!(is_image_path(Path::new("photos/a.PNG")));
    assert!(is_image_path(Path::new("x.jpeg")));
    assert!(is_image_path(Path::new("y.webp")));
    assert!(!is_image_path(Path::new("z.rs")));
    assert!(!is_image_path(Path::new("notes.md")));
}

#[test]
fn image_open_without_picker_warns_and_does_not_crash() {
    let dir = unique_dir("img");
    fs::write(dir.join("pic.png"), b"\x89PNG not-really").unwrap();
    let mut app = app_at(&dir); // picker is None (no terminal)
    let before = app.messages.items.len();
    app.open_initial(&dir.join("pic.png"));
    // No image tab opened, but the user is told why, and nothing panicked.
    assert!(app.editor.active_tab().is_none_or(|t| !t.is_image()));
    assert!(app.messages.items.len() > before, "a warning was added");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn scrollbar_drag_scrolls_editor() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let dir = unique_dir("scroll");
    let file = dir.join("long.txt");
    let body: String = (1..=200).map(|i| format!("line {i}\n")).collect();
    fs::write(&file, body).unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    // Rectangles are normally set during render; set them directly here.
    app.layout.editor = Rect::new(0, 0, 80, 20);
    app.layout.scrollbar = Rect::new(80, 0, 1, 20);

    // Press at the bottom of the scrollbar track → jump near the last line.
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 80,
        row: 19,
        modifiers: KeyModifiers::NONE,
    });
    assert!(
        app.editor.cursor_1based().0 > 150,
        "dragging to the bottom scrolls near the end (got line {})",
        app.editor.cursor_1based().0
    );

    // Drag back to the top → jump near the first line.
    app.on_mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 80,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        app.editor.cursor_1based().0,
        1,
        "dragging to the top scrolls home"
    );

    fs::remove_dir_all(&dir).ok();
}

// ===========================================================================
// Keyboard shortcuts (global)
// ===========================================================================

#[test]
fn ctrl_n_creates_new_buffer() {
    let mut app = app_at(Path::new("."));
    let before = app.editor.tabs.len();
    app.on_key(ctrl('n'));
    assert_eq!(app.editor.tabs.len(), before + 1);
    assert!(app.editor.active_tab().unwrap().path.is_none());
}

#[test]
fn ctrl_w_closes_active_buffer() {
    let mut app = app_at(Path::new("."));
    app.on_key(ctrl('n'));
    app.on_key(ctrl('n'));
    let before = app.editor.tabs.len();
    app.on_key(ctrl('w'));
    assert_eq!(app.editor.tabs.len(), before - 1);
}

#[test]
fn undo_tree_preserves_a_branch_after_a_new_edit() {
    let mut app = app_at(Path::new("."));
    // Type "A", undo it, then type "B" — the case linear undo would lose.
    type_str(&mut app, "A");
    app.on_key(ctrl('z')); // undo "A" → empty
    type_str(&mut app, "B"); // new branch off the empty root
    assert_eq!(app.editor.active_tab().unwrap().text(), "B");
    // Redo right after the edit does nothing (B is the active tip).
    app.on_key(KeyEvent::new(
        KeyCode::Char('z'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(app.editor.active_tab().unwrap().text(), "B");
    // Undo back to the branch point, switch branches, and redo into the OLD "A"
    // branch — proving it survived the new edit.
    app.on_key(ctrl('z')); // undo "B" → empty (branch point)
    assert_eq!(app.editor.active_tab().unwrap().text(), "");
    app.run_action("edit.undo_branch");
    app.on_key(KeyEvent::new(
        KeyCode::Char('z'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "A",
        "the old branch is still reachable"
    );
}

#[test]
fn ctrl_z_undoes_and_ctrl_shift_z_redoes() {
    let mut app = app_at(Path::new("."));
    for c in "abc".chars() {
        app.on_key(key(c));
    }
    let full = app.editor.active_tab().unwrap().lines()[0].len();
    app.on_key(ctrl('z'));
    let undone = app.editor.active_tab().unwrap().lines()[0].len();
    assert!(undone < full, "Ctrl+Z undoes typing ({undone} < {full})");
    app.on_key(KeyEvent::new(
        KeyCode::Char('z'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    let redone = app.editor.active_tab().unwrap().lines()[0].len();
    assert!(redone > undone, "Ctrl+Shift+Z redoes ({redone} > {undone})");
}

#[test]
fn editor_cut_removes_selection() {
    let mut app = app_at(Path::new("."));
    for c in "hello".chars() {
        app.on_key(key(c));
    }
    // Select the whole word with Shift+Left (Home is intercepted and ignores
    // Shift), then cut.
    for _ in 0..5 {
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT));
    }
    app.on_key(ctrl('x'));
    assert!(
        app.editor.active_tab().unwrap().text().is_empty(),
        "Ctrl+X cuts the selected text"
    );
}

#[test]
fn delete_key_forward_deletes() {
    let mut app = app_at(Path::new("."));
    for c in "abc".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Home));
    app.on_key(keycode(KeyCode::Delete));
    assert_eq!(
        app.editor.active_tab().unwrap().lines()[0],
        "bc",
        "Delete removes the char ahead"
    );
}

#[test]
fn ctrl_tab_switches_tabs() {
    let mut app = app_at(Path::new("."));
    app.run_action("file.new"); // open a second tab
    let before = app.editor.active;
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::CONTROL));
    assert_ne!(app.editor.active, before, "Ctrl+Tab moves to another tab");
}

#[test]
fn ascii_panel_opens_inserts_and_closes() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.ascii");
    assert!(app.ascii_panel.is_some(), "Tools → ASCII opens the panel");

    // Highlight code 65 ('A') and insert it; the panel stays open.
    app.ascii_panel.as_mut().unwrap().selected = 65;
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.ascii_panel.is_some(), "Enter keeps the panel open");
    assert!(
        app.editor.active_tab().unwrap().lines()[0].contains('A'),
        "the highlighted character is inserted into the editor"
    );

    // Esc closes the panel.
    app.on_key(keycode(KeyCode::Esc));
    assert!(app.ascii_panel.is_none(), "Esc closes the panel");
}

#[test]
fn closing_a_dirty_tab_prompts_then_discards() {
    let mut app = app_at(Path::new("."));
    app.on_key(key('x')); // dirties the untitled buffer
    assert!(app.editor.active_tab().unwrap().dirty);
    app.run_action("file.close");
    assert!(app.unsaved.is_some(), "a dirty close prompts to save");
    app.on_key(key('d')); // don't save -> close anyway
    assert!(app.unsaved.is_none(), "the prompt is dismissed");
    assert!(
        !app.editor.active_tab().unwrap().dirty,
        "the buffer was closed (a fresh empty tab remains)"
    );
}

#[test]
fn closing_a_dirty_tab_can_be_cancelled() {
    let mut app = app_at(Path::new("."));
    app.on_key(key('x'));
    app.run_action("file.close");
    assert!(app.unsaved.is_some());
    app.on_key(key('c')); // cancel
    assert!(app.unsaved.is_none());
    assert!(
        app.editor.active_tab().unwrap().dirty,
        "cancelling keeps the unsaved buffer open"
    );
}

#[test]
fn quitting_with_a_dirty_tab_prompts_then_quits_on_discard() {
    let mut app = app_at(Path::new("."));
    app.on_key(key('x'));
    app.run_action("file.quit");
    assert!(app.unsaved.is_some(), "a dirty quit prompts first");
    assert!(
        !app.should_quit,
        "quit is deferred until the tab is resolved"
    );
    app.on_key(key('d')); // discard -> no more dirty tabs -> quit
    assert!(app.should_quit, "discarding the last dirty tab quits");
}

#[test]
fn unsaved_prompt_save_writes_and_closes() {
    let dir = unique_dir("unsaved_save");
    let file = dir.join("note.txt");
    fs::write(&file, "hello\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file.clone());
    app.on_key(keycode(KeyCode::End));
    for c in "!!!".chars() {
        app.on_key(key(c));
    }
    assert!(app.editor.active_tab().unwrap().dirty);
    app.run_action("file.close");
    assert!(app.unsaved.is_some());
    app.on_key(key('s')); // save -> writes -> closes
    assert!(app.unsaved.is_none());
    let saved = fs::read_to_string(&file).unwrap();
    assert!(saved.starts_with("hello!!!"), "got: {saved:?}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn spellcheck_toggle_persists_and_clears_when_off() {
    // Toggling the setting works without a dictionary present (graceful no-op):
    // enabling sets the flag; disabling clears marks and the flag.
    let mut app = app_at(Path::new("."));
    assert!(!app.spellcheck);
    app.run_action("view.spellcheck");
    assert!(app.spellcheck, "toggle enables spellcheck");
    assert!(
        app.settings.spellcheck,
        "the setting is updated for persistence"
    );
    app.run_action("view.spellcheck");
    assert!(!app.spellcheck, "toggle disables spellcheck");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .editor
            .spell_marks()
            .is_none(),
        "disabling clears the underline marks"
    );
}

#[test]
fn select_more_and_less_extend_selection_by_word() {
    let mut app = app_at(Path::new("."));
    for c in "alpha beta gamma".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Home)); // cursor to column 0

    app.run_action("edit.select_more");
    assert_eq!(
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .get_selection_text()
            .as_deref(),
        Some("alpha"),
    );
    app.run_action("edit.select_more");
    assert_eq!(
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .get_selection_text()
            .as_deref(),
        Some("alpha beta"),
    );
    // Select Less retracts the active end leftward by a word.
    app.run_action("edit.select_less");
    assert_eq!(
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .get_selection_text()
            .as_deref(),
        Some("alpha "),
    );
}

#[test]
fn change_case_transforms_the_selection() {
    let mut app = app_at(Path::new("."));
    for c in "foo bar".chars() {
        app.on_key(key(c));
    }
    app.run_action("edit.select_all");
    app.run_action("edit.case_upper");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "FOO BAR");
    // The result stays selected, so the next transform applies to it.
    app.run_action("edit.case_snake");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "foo_bar");
    app.run_action("edit.case_pascal");
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "FooBar");
}

#[test]
fn change_case_without_selection_is_a_noop() {
    let mut app = app_at(Path::new("."));
    for c in "hello".chars() {
        app.on_key(key(c));
    }
    app.run_action("edit.case_upper"); // no selection
    assert_eq!(app.editor.active_tab().unwrap().lines()[0], "hello");
}

#[test]
fn editor_gutter_marks_round_trip() {
    let mut app = app_at(Path::new("."));
    let t = app.editor.active_tab_mut().unwrap();
    t.editor
        .set_gutter_marks(vec![(0, "#3fb950"), (2, "#d29922")]);
    assert_eq!(t.editor.gutter_marks().map(std::vec::Vec::len), Some(2));
    t.editor.clear_gutter_marks();
    assert!(t.editor.gutter_marks().is_none());
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn revert_hunk_restores_committed_text() {
    let dir = unique_dir("reverthunk");
    fs::create_dir_all(&dir).unwrap();
    // Canonicalize so the workspace root and the file share a prefix even when
    // the temp dir lives under a symlink (e.g. macOS /var → /private/var); the
    // diff gutter and revert key the HEAD cache off that shared prefix.
    let dir = dir.canonicalize().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    let file = dir.join("a.txt");
    fs::write(&file, "one\ntwo\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);

    let mut app = app_at(&dir);
    app.refresh_git();
    app.open_initial(&file);
    app.on_key(key('X')); // modify line 0: "one" -> "Xone"
    assert_eq!(app.editor.active_tab().unwrap().text(), "Xone\ntwo\n");

    app.run_action("git.revert_hunk");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "one\ntwo\n",
        "the modified hunk is restored to HEAD"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn stage_hunk_stages_only_the_cursor_hunk() {
    let dir = unique_dir("stagehunk");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    let file = dir.join("a.txt");
    fs::write(&file, "one\ntwo\nthree\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);

    // Change the first line in the working tree.
    fs::write(&file, "ONE\ntwo\nthree\n").unwrap();
    let mut app = app_at(&dir);
    app.refresh_git();
    app.open_initial(&file);
    app.run_action("git.stage_hunk"); // cursor on line 0

    // The staged (index) version now has the change; HEAD still has "one".
    let staged = String::from_utf8(
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(["show", ":a.txt"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(
        staged, "ONE\ntwo\nthree\n",
        "the hunk is staged into the index"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn unstage_hunk_removes_the_cursor_hunk_from_index() {
    let dir = unique_dir("unstagehunk");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    let file = dir.join("a.txt");
    fs::write(&file, "one\ntwo\nthree\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);

    // Change the first line and stage the whole file, then unstage just the hunk.
    fs::write(&file, "ONE\ntwo\nthree\n").unwrap();
    run(&["add", "a.txt"]);
    let mut app = app_at(&dir);
    app.refresh_git();
    app.open_initial(&file);
    app.run_action("git.unstage_hunk"); // cursor on line 0

    // The index now matches HEAD again ("one"); the working tree keeps "ONE".
    let staged = String::from_utf8(
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(["show", ":a.txt"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(
        staged, "one\ntwo\nthree\n",
        "the hunk is removed from the index"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo with branches"]
fn branch_chooser_switches_branches() {
    let dir = unique_dir("gitbranch");
    fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    fs::write(dir.join("a.txt"), "hello\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    run(&["branch", "feature"]);

    let mut app = app_at(&dir);
    app.refresh_git();
    let start = app.git_branch.clone();
    app.run_action("git.switch_branch");
    let chooser = app.branch_chooser.as_ref().expect("branch chooser opens");
    let idx = chooser
        .branches
        .iter()
        .position(|b| Some(b) != start.as_ref())
        .unwrap();
    let target = chooser.branches[idx].clone();
    app.branch_chooser.as_mut().unwrap().selected = idx;
    app.on_key(keycode(KeyCode::Enter));
    app.refresh_git();
    assert_eq!(
        app.git_branch.as_deref(),
        Some(target.as_str()),
        "checked out the chosen branch"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn spell_suggest_is_a_noop_without_a_dictionary() {
    // With spellcheck off (and no dictionary loaded), Ctrl+; just sets a status
    // and does not open the popup.
    let mut app = app_at(Path::new("."));
    app.run_action("spell.suggest");
    assert!(app.spell_suggest.is_none());
}

#[test]
#[ignore = "needs the untracked ./dictionaries set and the Rust grammar"]
fn spellcheck_underlines_a_misspelling_in_a_comment() {
    let dir = unique_dir("spell");
    let file = dir.join("a.rs");
    fs::write(&file, "// helllo wrld\nfn main() {}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.run_action("view.spellcheck");
    app.refresh_spellcheck();
    let marks = app
        .editor
        .active_tab()
        .unwrap()
        .editor
        .spell_marks()
        .cloned()
        .unwrap_or_default();
    assert!(
        !marks.is_empty(),
        "misspelled words in the comment are underlined"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn outline_panel_lists_symbols_and_jumps() {
    let dir = unique_dir("outline");
    let file = dir.join("a.rs");
    fs::write(&file, "fn alpha() {}\n\nstruct Beta;\n\nfn gamma() {}\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("nav.outline");
    let o = app.outline.as_ref().expect("outline opens");
    assert_eq!(o.len(), 3, "alpha, Beta, gamma");

    // Jump to the last symbol (fn gamma, line 5).
    app.on_key(keycode(KeyCode::End));
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.outline.is_none(), "panel closes after a jump");
    assert_eq!(app.editor.cursor_1based().0, 5, "cursor jumps to fn gamma");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn reopening_the_dashboard_while_open_is_idempotent() {
    // Re-invoking the action while the dashboard is already open must not spawn
    // another batch of metric threads / `du` scans. Observable proxy: a resolved
    // metric is preserved rather than reset by a fresh (re)compute.
    let dir = unique_dir("dashboard-idem");
    fs::write(dir.join("a.txt"), "x\n").unwrap();
    fs::write(dir.join("b.txt"), "y\n").unwrap();
    let mut app = app_at(&dir);

    app.run_action("tools.dashboard");
    for _ in 0..200 {
        app.poll_dashboard();
        if app.dashboard.as_ref().unwrap().file_count.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(app.dashboard.as_ref().unwrap().file_count, Some(2));

    // Second invocation while still open: the resolved count must survive (a
    // missing guard would replace the dashboard with a fresh, unresolved one).
    app.run_action("tools.dashboard");
    assert!(app.dashboard.is_some());
    assert_eq!(
        app.dashboard.as_ref().unwrap().file_count,
        Some(2),
        "reopening while open must not reset the in-flight computation"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn system_info_panel_opens_inserts_and_closes() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.system_info");
    assert!(
        app.system_info.is_some(),
        "Tools → System Information opens the panel"
    );

    // Highlight the first row that has an insertable value, then insert it.
    let idx = app
        .system_info
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|r| !r.value.is_empty())
        .expect("the snapshot has at least one value row");
    let value = app.system_info.as_ref().unwrap().rows[idx].value.clone();
    app.system_info.as_mut().unwrap().select_index(idx);
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.system_info.is_some(), "Enter keeps the panel open");
    assert!(
        app.editor.active_tab().unwrap().lines()[0].contains(&value),
        "the highlighted value is inserted into the editor"
    );

    app.on_key(keycode(KeyCode::Esc));
    assert!(app.system_info.is_none(), "Esc closes the panel");
}

#[test]
fn test_panel_toggles_and_parser_builds_results() {
    let mut app = app_at(Path::new("."));
    assert!(!app.show_test_panel);
    app.run_action("tools.test_panel");
    assert!(app.show_test_panel, "Toggle Test Panel shows it");

    // The parser turns runner output into a pass/fail list (used by the panel).
    let results = vix::test_runner::parse("test a::ok ... ok\ntest a::bad ... FAILED\n");
    assert_eq!(vix::test_runner::tally(&results), (1, 1, 0));
}

#[test]
fn debug_breakpoints_toggle_on_the_cursor_line() {
    let dir = unique_dir("breakpoints");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.rs");
    fs::write(
        &file,
        "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n",
    )
    .unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    app.run_action("cursor_down"); // line 2
    app.run_action("run.toggle_breakpoint");
    assert_eq!(
        app.active_breakpoints(),
        vec![2],
        "breakpoint set on line 2"
    );

    app.run_action("cursor_down"); // line 3
    app.run_action("run.toggle_breakpoint");
    assert_eq!(app.active_breakpoints(), vec![2, 3]);

    // Toggling again clears it.
    app.run_action("run.toggle_breakpoint");
    assert_eq!(app.active_breakpoints(), vec![2]);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn outline_sidebar_lists_symbols_and_follows_toggle() {
    let dir = unique_dir("outline-dock");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("lib.rs");
    fs::write(&file, "fn alpha() {}\nfn beta() {}\nstruct Gamma;\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&file);

    // Off by default.
    app.refresh_outline_dock();
    assert!(app.outline_dock.is_none());

    // Toggling on builds the symbol list for the active buffer.
    app.run_action("view.outline_dock");
    app.refresh_outline_dock();
    let o = app.outline_dock.as_ref().expect("outline dock populated");
    assert!(o.entries.iter().any(|e| e.name == "alpha"));
    assert!(o.entries.iter().any(|e| e.name == "Gamma"));

    // Toggling off clears it.
    app.run_action("view.outline_dock");
    assert!(app.outline_dock.is_none());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn click_dropdown_item_runs_its_action() {
    let mut app = app_at(Path::new("."));
    app.layout.menu = Rect::new(0, 0, 100, 1);
    app.on_mouse(click(2, 0)); // open the Vix menu (index 0)
    assert_eq!(app.menu.open, Some(0));
    // The dropdown rect is normally set during render; compute and set it.
    let dd = vix::ui::menu_dropdown_rect(Rect::new(0, 0, 100, 40), app.layout.menu, 0);
    app.layout.menu_dropdown = dd;
    // The first Vix item is "About Vix" → opens its dialog.
    app.on_mouse(click(dd.x + 2, dd.y + 1));
    assert!(app.menu.open.is_none(), "running an item closes the menu");
    assert!(app.dialog.is_some(), "clicking About opens its dialog");
}

#[test]
fn click_editor_focuses_it() {
    let mut app = app_at(Path::new("."));
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.focus = Focus::Explorer;
    app.on_mouse(click(5, 3));
    assert_eq!(app.focus, Focus::Editor, "clicking the editor focuses it");
}

#[test]
fn drag_messages_left_edge_resizes_right_dock() {
    let mut app = app_at(Path::new("."));
    app.show_messages = true;
    app.layout.menu = Rect::new(0, 0, 100, 1);
    app.layout.messages = Rect::new(68, 0, 32, 24); // left border at column 68
    let before = app.settings.messages_width;
    app.on_mouse(click(68, 0)); // grab the left edge
    app.on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 55, 0)); // drag left → wider
    assert!(
        app.settings.messages_width > before,
        "dragging left widens the dock"
    );
    app.on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 55, 0));
}

#[test]
fn click_closes_plain_dialog() {
    let mut app = app_at(Path::new("."));
    app.run_action("vix.about"); // plain (no text field)
    assert!(app.dialog.is_some());
    app.on_mouse(click(0, 0));
    assert!(app.dialog.is_none(), "a click acts as the Ok button");
}

#[test]
fn click_dock_toggle_icons() {
    let mut app = app_at(Path::new("."));
    app.layout.menu = Rect::new(0, 0, 100, 1);
    let (left, right) = vix::ui::dock_toggle_cols(app.layout.menu);
    let explorer_before = app.show_explorer;
    app.on_mouse(click(left, 0));
    assert_ne!(
        app.show_explorer, explorer_before,
        "left dock icon toggles the explorer"
    );
    let messages_before = app.show_messages;
    app.on_mouse(click(right, 0));
    assert_ne!(
        app.show_messages, messages_before,
        "right dock icon toggles the messages"
    );
}

#[test]
fn typing_brackets_auto_pairs_and_steps_over() {
    let mut app = app_at(Path::new("."));
    app.on_key(key('('));
    {
        let t = app.editor.active_tab().unwrap();
        assert_eq!(t.text(), "()", "opener inserts the matching closer");
        assert_eq!(t.editor.get_cursor(), 1, "cursor sits between the pair");
    }
    // Typing the closer steps over the auto-inserted one (no doubling).
    app.on_key(key(')'));
    {
        let t = app.editor.active_tab().unwrap();
        assert_eq!(t.text(), "()");
        assert_eq!(t.editor.get_cursor(), 2);
    }
}

#[test]
fn auto_pair_wraps_a_selection() {
    let mut app = app_at(Path::new("."));
    for c in "abc".chars() {
        app.on_key(key(c));
    }
    app.on_key(ctrl('a')); // select all
    app.on_key(key('('));
    let t = app.editor.active_tab().unwrap();
    assert_eq!(t.text(), "(abc)", "typing an opener wraps the selection");
}

#[test]
fn split_panes_open_focus_and_close() {
    let mut app = app_at(Path::new("."));
    app.run_action("file.new"); // a second tab, so the panes differ
    assert!(!app.editor.is_split());

    app.run_action("view.split_vertical");
    assert!(app.editor.is_split(), "Split Vertical splits the editor");
    let tabs = app.editor.split_layout(Rect::new(0, 0, 80, 24));
    assert_eq!(tabs.len(), 2, "two panes");
    assert_ne!(
        tabs[0].tab, tabs[1].tab,
        "the two panes show different tabs"
    );

    let before = app.editor.active;
    app.run_action("view.focus_other_pane");
    assert_ne!(
        app.editor.active, before,
        "focusing the other pane swaps the active tab"
    );

    // A second split makes a 2x2-style grid (three panes here).
    app.run_action("view.split_horizontal");
    assert_eq!(
        app.editor.split_layout(Rect::new(0, 0, 80, 24)).len(),
        3,
        "nested split adds a pane"
    );

    app.run_action("view.unsplit");
    app.run_action("view.unsplit");
    assert!(!app.editor.is_split(), "unsplitting back to one pane");
}

/// Regression test for the run-command deadlock: a child that closes its stdout
/// but keeps running must still be cancellable. The reader thread reaps the
/// child with non-blocking `try_wait` (releasing the shared lock between polls),
/// so a `kill()` from another thread — the shape of `cancel_command` — acquires
/// the lock and terminates the child promptly instead of blocking forever behind
/// a `lock().wait()`.
///
/// The assertion is on *how the child died*, not on how long anything took: a
/// cancelled child is killed by a signal, while one that outlived a blocked
/// cancel exits normally when its `sleep` ends. An earlier version judged that
/// with a 20-second stopwatch and flaked on a loaded machine — and it flaked for
/// a reason worth keeping in mind: its reader took the lock *inside the `match`
/// scrutinee*, so the guard lived to the end of the `match` and was held across
/// the sleep. That is not what `run_command` does, so the test was modelling a
/// lock discipline the code does not have, and measuring lock starvation (8+
/// seconds, sometimes never) instead of the deadlock it meant to catch. The
/// reader below mirrors the real statement structure; the remaining timeout is
/// only a guard against a true deadlock, which is infinite rather than slow.
#[test]
#[cfg(unix)]
fn running_command_cancel_is_not_blocked_by_a_detached_child() {
    use std::os::unix::process::ExitStatusExt;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    // Close stdout immediately, then sleep: EOF reaches the reader while the
    // process is still alive — exactly the case a blocking `wait()` deadlocks on.
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg("exec 1>&-; sleep 30")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn sh");
    let stdout = child.stdout.take().unwrap();
    let child = Arc::new(Mutex::new(child));

    // Reader thread mirrors run_command: drain stdout, then poll try_wait. It
    // hands back the status it reaped, which is what the test judges.
    let reader_child = Arc::clone(&child);
    let reader = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        for _ in BufReader::new(stdout).lines().map_while(Result::ok) {}
        loop {
            // Bind the result to its own statement, exactly as `run_command`
            // does: the `MutexGuard` is a temporary of *this* statement, so it
            // is dropped here rather than living to the end of a `match` — that
            // is what frees the lock between polls and lets a cancel in.
            let status = reader_child
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .try_wait();
            match status {
                Ok(Some(status)) => return Some(status),
                // An unexpected wait error: stop polling, with nothing to report.
                Err(_) => return None,
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            }
        }
    });

    std::thread::sleep(Duration::from_millis(200)); // let the reader reach reaping

    // Run the cancel (lock + kill) on its own thread and wait for it via a
    // channel. The generous timeout only separates "returned" from "deadlocked
    // forever" — the old bug blocked until `sleep 30` exited, which this would
    // still wait out, and the exit-status assertion below is what catches it.
    let cancel_child = Arc::clone(&child);
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = cancel_child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .kill();
        let _ = done_tx.send(());
    });
    assert!(
        done_rx.recv_timeout(Duration::from_mins(2)).is_ok(),
        "cancel never returned — blocked behind the reader's lock"
    );

    let status = reader.join().unwrap().expect("the child was reaped");
    assert!(
        status.signal().is_some(),
        "the child exited on its own ({status:?}) instead of being killed by the \
         cancel — the cancel was blocked until `sleep 30` finished"
    );
}

#[test]
fn command_key_drives_the_control_bindings_on_macos() {
    let mut app = app_at(Path::new("."));
    let cmd = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::SUPER);

    app.on_key(cmd('f'));
    if !cfg!(target_os = "macos") {
        assert!(
            app.search.is_none(),
            "off macOS the Super modifier is left alone"
        );
        return;
    }
    assert!(app.search.is_some(), "Cmd+F opens Find, like Ctrl+F");
    app.on_key(esc());

    // The rest of the chord survives the fold: Cmd+Z undoes, Cmd+Shift+Z redoes.
    buffer_with(&mut app, "", 0);
    type_str(&mut app, "abc");
    app.on_key(cmd('z'));
    assert_eq!(app.editor.active_tab().unwrap().text(), "ab");
    app.on_key(KeyEvent::new(
        KeyCode::Char('z'),
        KeyModifiers::SUPER | KeyModifiers::SHIFT,
    ));
    assert_eq!(app.editor.active_tab().unwrap().text(), "abc");
}

#[test]
fn bracketed_paste_is_one_edit_and_undoes_in_one_step() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "", 0);

    // A pasted block lands verbatim: no auto-pairing, and no auto-indent
    // cascade re-indenting each line after a newline (what happened when the
    // terminal delivered a paste as individual key events).
    app.on_paste("fn f() {\n    let s = \"hi\";\n}\n");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "fn f() {\n    let s = \"hi\";\n}\n"
    );

    // And it is a single undo step, not one per character.
    app.run_action("edit.undo");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "",
        "undo removes the whole paste"
    );

    // A paste replaces the selection, like pasting the clipboard does.
    buffer_with(&mut app, "keep me", 0);
    app.run_action("edit.select_all");
    app.on_paste("replaced");
    assert_eq!(app.editor.active_tab().unwrap().text(), "replaced");
}

#[test]
fn bracketed_paste_goes_to_the_overlay_taking_keys() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "", 0);

    // With the command palette open the text belongs to its input, not to the
    // buffer behind it.
    app.run_action("tools.palette");
    assert!(app.palette.is_some(), "palette is open");
    app.on_paste("needle");
    assert!(
        app.palette.as_ref().unwrap().query().contains("needle"),
        "the palette got the pasted text"
    );
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "",
        "the buffer is untouched"
    );
    app.on_key(esc());

    // Same for the find bar.
    app.run_action("edit.find");
    app.on_paste("term");
    assert!(app.search.is_some(), "find bar is open");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "",
        "the buffer is untouched"
    );
    app.on_key(esc());

    // Nothing layered over the editor: the paste lands in the buffer again.
    app.on_paste("into the buffer");
    assert_eq!(app.editor.active_tab().unwrap().text(), "into the buffer");
}

#[test]
fn tests_copy_and_paste_through_the_in_memory_clipboard() {
    // Regression guard: the platform clipboard is opt-in (only `main` calls
    // `use_system`), so a test run cannot overwrite what the developer copied.
    // A keymap test used to cut a scratch line onto the real macOS pasteboard.
    assert!(
        !vix::clipboard::is_system(),
        "tests must not touch the system clipboard"
    );

    // Copy and paste still round-trip — through the in-memory clipboard.
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "hello", 0);
    app.run_action("edit.select_all");
    app.run_action("edit.copy");
    app.run_action("escape"); // drop the selection so the paste appends
    app.run_action("edit.go_last");
    app.run_action("edit.paste");
    assert_eq!(app.editor.active_tab().unwrap().text(), "hellohello");
}

#[test]
fn shared_ctrl_backtab_switches_to_the_previous_tab() {
    let mut app = app_at(Path::new("."));
    app.run_action("file.new");
    app.run_action("file.new");
    let active = app.editor.active;
    app.on_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::CONTROL));
    assert_ne!(
        app.editor.active, active,
        "Ctrl+BackTab switches to the previous tab"
    );
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::CONTROL));
    assert_eq!(
        app.editor.active, active,
        "Ctrl+Tab switches back to the next tab"
    );
}

#[test]
fn key_override_of_an_unbound_token_does_not_report_a_shadow() {
    let mut app = app_at(Path::new("."));
    let before = app.messages.items.len();
    app.apply_key_overrides(vec![vix_keybindings::Override {
        key_token: "C-j".to_string(),
        action_id: "file.new".to_string(),
        source: vix_keybindings::Source::User,
    }]);
    // Apple's built-in table has no Ctrl+J binding, so nothing is shadowed.
    assert_eq!(
        app.messages.items.len(),
        before,
        "no shadow (or conflict) to report for a single, unbound override"
    );
    let tabs_before = app.editor.tabs.len();
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
    assert_eq!(app.editor.tabs.len(), tabs_before + 1, "the override ran");
}

#[test]
fn two_key_overrides_on_the_same_token_are_both_rejected() {
    let mut app = app_at(Path::new("."));
    app.apply_key_overrides(vec![
        vix_keybindings::Override {
            key_token: "C-j".to_string(),
            action_id: "file.new".to_string(),
            source: vix_keybindings::Source::User,
        },
        vix_keybindings::Override {
            key_token: "C-j".to_string(),
            action_id: "file.close".to_string(),
            source: vix_keybindings::Source::Script("demo".to_string()),
        },
    ]);
    assert!(
        app.messages
            .items
            .iter()
            .any(|m| matches!(m.level, vix::messages::Level::Error)),
        "a token claimed twice is reported as an error"
    );
    // Neither override applies -- Ctrl+J is simply unbound on Apple, so
    // the key is left unclaimed (no tab created or closed).
    let tabs_before = app.editor.tabs.len();
    app.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
    assert_eq!(
        app.editor.tabs.len(),
        tabs_before,
        "a rejected conflict runs neither action"
    );
}

#[test]
fn wrap_fills_the_cursor_paragraph_or_the_selection() {
    let settings = Settings {
        wrap_column: 10,
        ..Settings::default()
    };
    let mut app = app_with(settings);

    // No selection: only the paragraph holding the cursor is refilled.
    buffer_with(&mut app, "one two three four\n\nleave me alone\n", 0);
    app.run_action("edit.wrap");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "one two\nthree four\n\nleave me alone\n"
    );

    // Already wrapped → no-op, with a status note.
    app.status.clear();
    app.run_action("edit.wrap");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "one two\nthree four\n\nleave me alone\n"
    );
    assert!(!app.status.is_empty(), "reports there was nothing to wrap");

    // With a selection, the selected text is what gets wrapped.
    buffer_with(&mut app, "aa bb cc dd ee\nkeep\n", 0);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, 14);
    app.run_action("edit.wrap");
    assert_eq!(
        app.editor.active_tab().unwrap().text(),
        "aa bb cc\ndd ee\nkeep\n"
    );
}

#[test]
fn wrap_uses_the_wrap_column_setting() {
    // The same paragraph wraps differently at 20 columns than at the default 80.
    let text = "alpha beta gamma delta epsilon\n";
    let mut wide = app_with(Settings::default());
    buffer_with(&mut wide, text, 0);
    wide.run_action("edit.wrap");
    assert_eq!(
        wide.editor.active_tab().unwrap().text(),
        text,
        "already inside the default 80-column wrap"
    );

    let mut narrow = app_with(Settings {
        wrap_column: 20,
        ..Settings::default()
    });
    buffer_with(&mut narrow, text, 0);
    narrow.run_action("edit.wrap");
    assert_eq!(
        narrow.editor.active_tab().unwrap().text(),
        "alpha beta gamma\ndelta epsilon\n"
    );
}

#[test]
fn welcome_dialog_shows_on_the_first_launch_only() {
    // Two launches against one config file in a temp dir, so the user's real
    // config is never touched.
    let dir = unique_dir("welcome");
    let config = dir.join("config.toml");
    fs::remove_file(&config).ok();

    // First launch: no config file yet, so the dialog is enabled and opens.
    let first = Settings::load_from(&config);
    assert!(
        first.show_welcome_dialog,
        "a fresh config starts with the welcome dialog enabled"
    );
    let mut app = App::new(dir.clone(), first).with_settings_path(&config);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.maybe_show_welcome();
    assert!(
        app.welcome.is_some(),
        "the welcome dialog opens on the first launch"
    );
    assert!(
        !app.settings.show_welcome_dialog,
        "showing it turns the setting off"
    );

    // Quit: dismiss the dialog and drop the app. Nothing is saved on the way
    // out — the setting was already written to disk when the dialog appeared.
    app.on_key(keycode(KeyCode::Esc));
    assert!(app.welcome.is_none(), "Esc closes the welcome dialog");
    drop(app);

    // Second launch: the saved config now has the dialog turned off.
    let second = Settings::load_from(&config);
    assert!(
        !second.show_welcome_dialog,
        "the first show persisted show_welcome_dialog = false"
    );
    let mut app = App::new(dir.clone(), second).with_settings_path(&config);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.maybe_show_welcome();
    assert!(
        app.welcome.is_none(),
        "no welcome dialog on the second launch"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn open_stdin_buffer_loads_the_exact_piped_content() {
    // T208's `vix -`: no header line injected, unlike the plain New Scratch
    // Buffer action - the caller may want to save or otherwise act on
    // precisely what it piped in.
    let mut app = app_at(Path::new("."));
    app.open_stdin_buffer("piped\ncontent\n");
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.text(), "piped\ncontent\n");
    assert!(tab.path.is_none(), "unsaved, like any other new buffer");
}

#[test]
fn open_diff_files_compares_two_files_directly() {
    // T208's `--diff OLD NEW`: independent of any open buffer, unlike
    // Tools -> Compare With File...
    let dir = unique_dir("cli-diff");
    let old = dir.join("old.txt");
    let new = dir.join("new.txt");
    fs::write(&old, "one\ntwo\nthree\n").unwrap();
    fs::write(&new, "one\nTWO\nthree\n").unwrap();
    let mut app = app_at(&dir);
    app.open_diff_files(&old, &new);
    let view = app.diff_view.as_ref().expect("diff overlay opened");
    assert!(view.title.contains("old.txt"));
    assert!(view.title.contains("new.txt"));
    assert!(!view.lines.is_empty(), "one line differs");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn open_diff_files_reports_identical_files_without_opening_the_overlay() {
    let dir = unique_dir("cli-diff-same");
    let old = dir.join("old.txt");
    let new = dir.join("new.txt");
    fs::write(&old, "same\n").unwrap();
    fs::write(&new, "same\n").unwrap();
    let mut app = app_at(&dir);
    app.open_diff_files(&old, &new);
    assert!(app.diff_view.is_none());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn open_diff_files_reports_a_missing_file_without_panicking() {
    let dir = unique_dir("cli-diff-missing");
    let old = dir.join("missing.txt");
    let new = dir.join("new.txt");
    fs::write(&new, "here\n").unwrap();
    let mut app = app_at(&dir);
    app.open_diff_files(&old, &new);
    assert!(
        app.diff_view.is_none(),
        "should not open an overlay for a failed read"
    );
    fs::remove_dir_all(&dir).ok();
}
