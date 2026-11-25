//! Round-4 behavior tests for `app/doc_links.rs` (test-only; a child module
//! so it can read the private per-file color and link state).

use super::*;
use crate::app::cov4::{app_at, buffer_with, temp};
use crate::lsp_core::document_extras::{parse_color_presentations, parse_colors, parse_links};

fn cursor_line(app: &App) -> usize {
    app.editor.active_tab().unwrap().editor.cursor_line()
}

// ----- document colors and links --------------------------------------

fn lsp_range(line: u32, from: u32, to: u32) -> serde_json::Value {
    serde_json::json!({
        "start": {"line": line, "character": from},
        "end": {"line": line, "character": to},
    })
}

/// The file link's target inside workspace `dir`.
fn link_target(dir: &std::path::Path) -> String {
    format!("file://{}#L3", dir.join("r4-target.txt").display())
}

/// A file buffer with a color on line 0 and two links on line 2.
fn doc_app(tag: &str) -> (App, std::path::PathBuf) {
    let dir = temp(tag);
    let file = dir.join("doc.txt");
    std::fs::write(
        &file,
        "color: #ff0000 here\nplain\nsee a and b end\nnothing\n",
    )
    .unwrap();
    let mut a = app_at(&dir);
    a.open_path(&file, false);
    let path = a.active_path().unwrap();
    let colors = parse_colors(&serde_json::json!([{
        "range": lsp_range(0, 7, 14),
        "color": {"red": 1.0, "green": 0.0, "blue": 0.0, "alpha": 1.0},
    }]));
    let links = parse_links(&serde_json::json!([
        {"range": lsp_range(2, 4, 5), "target": link_target(&dir)},
        {"range": lsp_range(2, 10, 11), "target": "ftp://nope"},
    ]));
    a.on_document_colors(&path, colors);
    a.on_document_links(&path, links);
    (a, dir)
}

/// Colors draw as swatches while the setting is on and disappear when off.
#[test]
fn colors_become_swatches_only_when_the_setting_is_on() {
    let (mut a, _dir) = doc_app("r4-doc-swatch");
    let path = a.active_path().unwrap();
    let swatches = |a: &App| {
        a.editor
            .tabs
            .iter()
            .find(|t| t.path.as_deref() == Some(path.as_path()))
            .map(|t| {
                t.editor
                    .eol_texts(crate::editor_core::virtual_text::VirtualTextLayer::ColorSwatches)
                    .len()
            })
    };
    a.settings.language_features.document_colors = true;
    a.on_document_colors(&path, a.doc_extras[&path].colors.clone());
    assert_eq!(swatches(&a), Some(1));
    a.settings.language_features.document_colors = false;
    a.on_document_colors(&path, a.doc_extras[&path].colors.clone());
    assert_eq!(swatches(&a), Some(0));
}

/// `lsp.edit_color` with no color under the cursor, on a line without
/// exactly one color, or with no server says so; with a server-less buffer
/// the request is reported unsupported.
#[test]
fn edit_color_reports_why_it_cannot_ask() {
    let (mut a, _dir) = doc_app("r4-doc-color");
    // Cursor on line 1 (no color there).
    buffer_with(&mut a, "color: #ff0000 here\nplain\n", 22);
    a.run_doc_link_action("lsp.edit_color");
    assert_eq!(a.status, t!("status.color_none").to_string());
    // Cursor inside the color: no language server handles the file.
    buffer_with(&mut a, "color: #ff0000 here\nplain\n", 9);
    a.run_doc_link_action("lsp.edit_color");
    assert_eq!(a.status, t!("status.color_unsupported").to_string());
    assert!(
        a.doc_extras[&a.active_path().unwrap()]
            .pending_color
            .is_none()
    );
    // Cursor at the end of the colored line still finds the only color on it.
    buffer_with(&mut a, "color: #ff0000 here\nplain\n", 19);
    a.run_doc_link_action("lsp.edit_color");
    assert_eq!(a.status, t!("status.color_unsupported").to_string());
    // With no tab at all.
    a.editor.tabs.clear();
    a.run_doc_link_action("lsp.edit_color");
    assert_eq!(a.status, t!("status.lsp_inactive").to_string());
}

/// A `colorPresentation` reply opens a chooser; picking a row applies its
/// edits; an empty reply says there is nothing; a reply nobody asked for is
/// ignored.
#[test]
fn color_presentations_open_a_chooser_and_apply() {
    let (mut a, _dir) = doc_app("r4-doc-present");
    let path = a.active_path().unwrap();
    let presentations = parse_color_presentations(&serde_json::json!([
        {"label": "red"},
        {"label": "#f00", "textEdit": {"range": lsp_range(0, 7, 14), "newText": "#f00"}},
    ]));
    // Unsolicited: ignored.
    a.on_color_presentations(&path, &presentations);
    assert!(a.choice_list.is_none());
    // Solicited with nothing on offer.
    a.doc_extras.get_mut(&path).unwrap().pending_color = Some(
        parse_colors(&serde_json::json!([{
            "range": lsp_range(0, 7, 14),
            "color": {"red": 1.0, "green": 0.0, "blue": 0.0},
        }]))[0]
            .range,
    );
    a.on_color_presentations(&path, &[]);
    assert_eq!(a.status, t!("status.color_presentations_none").to_string());
    assert!(a.choice_list.is_none());
    assert!(a.doc_extras[&path].pending_color.is_none(), "consumed");
    // Solicited with two presentations: a chooser lists both labels.
    let range = parse_colors(&serde_json::json!([{
        "range": lsp_range(0, 7, 14),
        "color": {"red": 1.0, "green": 0.0, "blue": 0.0},
    }]))[0]
        .range;
    a.doc_extras.get_mut(&path).unwrap().pending_color = Some(range);
    a.on_color_presentations(&path, &presentations);
    let list = a.choice_list.as_ref().expect("a chooser");
    assert!(matches!(
        list.purpose,
        crate::app::ide_parity::ChoicePurpose::ColorPresentation { .. }
    ));
    // Applying a row writes its text over the color; an out-of-range row is
    // ignored.
    let edits: Vec<_> = presentations.iter().map(|p| p.edits(range)).collect();
    a.status.clear();
    a.apply_color_presentation(&path, &edits, 9);
    assert_eq!(a.status, "");
    a.apply_color_presentation(&path, &edits, 1);
    assert_eq!(a.status, t!("status.color_applied").to_string());
}

/// Links open from the cursor, from the only link on the line, or by click;
/// unsupported targets and missing links are reported.
#[test]
fn links_open_from_the_cursor_and_report_failures() {
    let (mut a, dir) = doc_app("r4-doc-link");
    std::fs::write(dir.join("r4-target.txt"), "one\ntwo\nthree\nfour\n").unwrap();
    // On a line with no link.
    buffer_with(
        &mut a,
        "color: #ff0000 here\nplain\nsee a and b end\nnothing\n",
        22,
    );
    a.run_doc_link_action("lsp.open_link");
    assert_eq!(a.status, t!("status.link_none").to_string());
    // Inside the second (unsupported) link.
    let off = "color: #ff0000 here\nplain\nsee a and ".chars().count();
    buffer_with(
        &mut a,
        "color: #ff0000 here\nplain\nsee a and b end\nnothing\n",
        off,
    );
    a.run_doc_link_action("lsp.open_link");
    assert_eq!(
        a.status,
        t!("status.link_unsupported", target = "ftp://nope").to_string()
    );
    // A line with two links and the cursor between them is ambiguous.
    let between = "color: #ff0000 here\nplain\nsee a an".chars().count();
    buffer_with(
        &mut a,
        "color: #ff0000 here\nplain\nsee a and b end\nnothing\n",
        between,
    );
    a.run_doc_link_action("lsp.open_link");
    assert_eq!(a.status, t!("status.link_none").to_string());
    // Inside the file link: it opens the target file at its line.
    let first = "color: #ff0000 here\nplain\nsee ".chars().count();
    buffer_with(
        &mut a,
        "color: #ff0000 here\nplain\nsee a and b end\nnothing\n",
        first,
    );
    a.run_doc_link_action("lsp.open_link");
    assert_eq!(
        a.status,
        t!("status.link_opened", target = link_target(&dir)).to_string()
    );
    assert!(
        a.active_path()
            .is_some_and(|p| p.ends_with("r4-target.txt"))
    );
    assert_eq!(cursor_line(&a), 2);
}

/// A click outside any link is not consumed; with no tab it is not either.
#[test]
fn clicking_where_there_is_no_link_is_not_consumed() {
    let (mut a, _dir) = doc_app("r4-doc-click");
    a.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
    assert!(!a.open_link_at_mouse(60, 10), "empty area");
    a.editor.tabs.clear();
    assert!(!a.open_link_at_mouse(1, 1));
}

/// Unsupported targets are classified without touching the system opener.
#[test]
fn unsupported_link_targets_are_reported() {
    let mut a = app_at(&temp("r4-doc-unsupported"));
    a.open_link_target("gopher://x");
    assert_eq!(
        a.status,
        t!("status.link_unsupported", target = "gopher://x").to_string()
    );
}

/// The per-tick refresh asks nothing of a buffer no server handles, and a
/// save marks the refresh due without needing one.
#[test]
fn doc_extras_polling_and_save_without_a_server() {
    let (mut a, _dir) = doc_app("r4-doc-poll");
    let path = a.active_path().unwrap();
    a.poll_doc_extras();
    assert!(a.doc_extras.contains_key(&path));
    a.doc_extras_saved(&path);
    a.poll_doc_extras();
    assert_eq!(a.doc_extras[&path].retries, 0, "no server: nothing retried");
    a.doc_extras_saved(Path::new("/not/open.txt")); // unknown file: no-op
    assert!(!a.doc_extras.contains_key(Path::new("/not/open.txt")));
    a.editor.tabs.clear();
    a.poll_doc_extras();
}
