//! Round-3 tests of the right-dock panels' drawing: the test-run tree, the
//! outline dock and the messages drawer, each in its empty and populated
//! states, checked by the text that reaches the buffer.

use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::{draw_messages, draw_outline_dock, draw_test_panel};
use crate::app::App;
use crate::app::cov4::{app_at, temp};
use crate::test_runner::{Status, TestResult};

fn render(app: &mut App, w: u16, h: u16, f: fn(&mut App, &mut Frame, Rect)) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            f(app, frame, area);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn rows(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

fn text(buf: &Buffer) -> String {
    rows(buf).join("\n")
}

fn result(name: &str, status: Status) -> TestResult {
    TestResult {
        name: name.into(),
        status,
        location: None,
    }
}

#[test]
fn the_test_panel_hints_when_idle_and_lists_results_with_a_tally() {
    let mut app = app_at(&temp("dock-tests"));
    let idle = text(&render(&mut app, 50, 8, draw_test_panel));
    assert!(idle.contains(&t!("ui.tests_idle").to_string()), "{idle}");
    assert!(idle.contains("0/0/0"));

    app.test_results = vec![
        result("mod::passes", Status::Pass),
        result("mod::fails", Status::Fail),
        result("mod::skipped", Status::Ignore),
    ];
    app.test_selected = 1;
    let buf = render(&mut app, 50, 8, draw_test_panel);
    let shown = text(&buf);
    assert!(shown.contains("1/1/1"), "{shown}");
    assert!(shown.contains("\u{2713} mod::passes"));
    assert!(shown.contains("\u{2717} mod::fails"));
    assert!(shown.contains("\u{25cb} mod::skipped"));
    assert!(!shown.contains(&t!("ui.tests_idle").to_string()));
    assert!(
        app.layout.test_panel.width > 0,
        "the panel records where it drew"
    );
    // More results than rows: only the visible ones are drawn.
    app.test_results = (0..30)
        .map(|i| result(&format!("t{i}"), Status::Pass))
        .collect();
    let shown = text(&render(&mut app, 50, 5, draw_test_panel));
    assert!(shown.contains("t0") && !shown.contains("t29"));
}

#[test]
fn the_outline_dock_shows_a_hint_or_the_symbols_with_the_selection_marked() {
    let mut app = app_at(&temp("dock-outline"));
    let empty = text(&render(&mut app, 40, 8, draw_outline_dock));
    assert!(
        empty.contains(&t!("status.outline_empty").to_string()),
        "{empty}"
    );

    let entries = vec![
        vix_outline_panel::Entry {
            kind: "fn".into(),
            name: "main".into(),
            line: 1,
        },
        vix_outline_panel::Entry {
            kind: String::new(),
            name: "Heading".into(),
            line: 9,
        },
        vix_outline_panel::Entry {
            kind: "struct".into(),
            name: "Thing".into(),
            line: 20,
        },
    ];
    let mut outline = vix_outline_panel::Outline::new(entries);
    outline.selected = 1;
    app.outline_dock = Some(outline);
    let buf = render(&mut app, 40, 8, draw_outline_dock);
    let shown = text(&buf);
    assert!(shown.contains("fn     main"), "{shown}");
    assert!(shown.contains("Heading"));
    assert!(shown.contains("struct Thing"));
    assert!(app.layout.outline_dock.width > 0);
    // The selected row is drawn with the selection style, the others are not.
    let style_at = |needle: &str| {
        let all = rows(&buf);
        let (y, line) = all
            .iter()
            .enumerate()
            .find(|(_, r)| r.contains(needle))
            .unwrap();
        let byte = line.find(needle).unwrap();
        let x = u16::try_from(line[..byte].chars().count()).unwrap();
        buf[(x, u16::try_from(y).unwrap())].style()
    };
    assert_ne!(style_at("Heading"), style_at("Thing"));
}

#[test]
fn the_messages_drawer_lists_every_message_and_scrolls_long_ones() {
    let mut app = app_at(&temp("dock-messages"));
    app.messages.items.clear();
    let empty = text(&render(&mut app, 40, 8, draw_messages));
    assert!(empty.contains(&t!("ui.no_messages").to_string()), "{empty}");

    app.messages.info("plain info".to_string());
    app.messages.warn("a warning".to_string());
    app.messages.error("an error".to_string());
    let shown = text(&render(&mut app, 40, 8, draw_messages));
    for needle in ["plain info", "a warning", "an error"] {
        assert!(shown.contains(needle), "{shown}");
    }
    assert_eq!(
        app.messages_hmax, 0,
        "short lines need no horizontal scroll"
    );

    // A very long line makes the drawer horizontally scrollable.
    app.messages.error("x".repeat(200));
    let _ = render(&mut app, 40, 8, draw_messages);
    assert!(app.messages_hmax > 0);
    assert!(
        app.layout.messages_hscrollbar.width > 0,
        "the horizontal bar is drawn"
    );
    app.messages_hscroll = 10_000;
    let _ = render(&mut app, 40, 8, draw_messages);
    assert_eq!(
        app.messages_hscroll, app.messages_hmax,
        "the offset is clamped"
    );

    // More messages than rows: the vertical scrollbar column appears.
    for i in 0..30 {
        app.messages.info(format!("message {i}"));
    }
    app.settings.viewport.show_scrollbar = true;
    let buf = render(&mut app, 40, 8, draw_messages);
    let right_col: String = (1..buf.area.height)
        .map(|y| buf[(39, y)].symbol().to_string())
        .collect();
    assert!(
        !right_col.trim().is_empty(),
        "a scrollbar glyph is drawn: {right_col:?}"
    );
}
