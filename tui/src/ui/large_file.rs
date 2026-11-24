//! The Large File Viewer overlay (T733): a byte-offset gutter, the visible
//! lines, a status line (size, position, line, index and search progress),
//! and a hint, input, or notice line. The viewer reads only what is drawn;
//! see `crates/vix-large-file/spec/index.md`.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use crate::geo::{Geo, chunk};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::app::App;
use crate::large_file::{InputKind, Notice, Status, ViewLine, Viewer};
use crate::theme::{self, icon};

/// `bytes` as a short size with a binary unit: `512 B`, `3.4 MiB`, `4.00 GiB`.
pub(super) fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut unit = 0;
    // Hundredths of the unit, in integers (no float casts).
    let mut scaled = u128::from(bytes) * 100;
    while scaled >= 1024 * 100 && unit.plus(1) < UNITS.len() {
        scaled /= 1024;
        unit = unit.plus(1);
    }
    if unit == 0 {
        return format!("{bytes} B");
    }
    format!(
        "{}.{:02} {}",
        scaled.quot(100),
        scaled % 100,
        UNITS.get(unit).copied().unwrap_or_default()
    )
}

/// One row: the dimmed byte offset, then the text, the cursor line and a
/// selection reversed, and the search match reversed against its line.
fn row_line(row: &ViewLine, gutter: usize, more_left: bool) -> Line<'static> {
    let line_style = if row.cursor || row.selected {
        theme::selected()
    } else {
        theme::base()
    };
    let hit_style = if row.cursor || row.selected {
        theme::base()
    } else {
        theme::selected()
    };
    let mut spans = vec![Span::styled(
        format!(
            "{:>gutter$} {}",
            row.offset,
            if more_left { '‹' } else { ' ' }
        ),
        theme::dim(),
    )];
    match row.hit {
        Some((a, b)) => {
            let chars: Vec<char> = row.text.chars().collect();
            let (a, b) = (a.min(chars.len()), b.min(chars.len()));
            spans.push(Span::styled(
                chars
                    .get(..a)
                    .unwrap_or_default()
                    .iter()
                    .collect::<String>(),
                line_style,
            ));
            spans.push(Span::styled(
                chars
                    .get(a..b)
                    .unwrap_or_default()
                    .iter()
                    .collect::<String>(),
                hit_style,
            ));
            spans.push(Span::styled(
                chars
                    .get(b..)
                    .unwrap_or_default()
                    .iter()
                    .collect::<String>(),
                line_style,
            ));
        }
        None => spans.push(Span::styled(row.text.clone(), line_style)),
    }
    if row.more_right {
        spans.push(Span::styled("›", theme::dim()));
    }
    Line::from(spans)
}

/// The status line: position, line, and background progress.
fn status_text(st: &Status) -> String {
    let mut parts = vec![
        t!(
            "ui.large_file_position",
            offset = st.offset,
            size = human_size(st.len),
            percent = st.percent
        )
        .to_string(),
    ];
    match (st.line, st.total_lines) {
        (Some(line), Some(total)) => {
            parts.push(t!("ui.large_file_line_of", line = line, total = total).to_string());
        }
        (Some(line), None) => parts.push(t!("ui.large_file_line", line = line).to_string()),
        _ => {}
    }
    if st.total_lines.is_none() {
        parts.push(t!("ui.large_file_indexing", percent = st.indexed).to_string());
    }
    if let Some(p) = st.searching {
        parts.push(t!("ui.large_file_searching", percent = p).to_string());
    }
    if st.selecting {
        parts.push(t!("ui.large_file_selecting").to_string());
    }
    parts.join("   ")
}

/// The translated text for a notice.
fn notice_text(n: &Notice) -> String {
    match n {
        Notice::NotFound => t!("ui.large_file_not_found").to_string(),
        Notice::SearchCancelled => t!("ui.large_file_search_cancelled").to_string(),
        Notice::GotoPending(line) => t!("ui.large_file_goto_pending", line = line).to_string(),
        Notice::BadGoto => t!("ui.large_file_bad_goto").to_string(),
        Notice::BeyondEnd(lines) => t!("ui.large_file_beyond_end", lines = lines).to_string(),
        Notice::Copied(bytes) => t!("ui.large_file_copied", size = human_size(*bytes)).to_string(),
        Notice::CopyTooLarge => t!(
            "ui.large_file_copy_too_large",
            size = human_size(crate::large_file::COPY_LIMIT)
        )
        .to_string(),
        Notice::ReadFailed(e) => t!("ui.large_file_read_failed", error = e).to_string(),
        Notice::IndexFailed => t!("ui.large_file_index_failed").to_string(),
    }
}

/// The bottom line: the input being typed, else a notice, else the key hint.
fn bottom_line(v: &Viewer) -> Line<'static> {
    if let Some((kind, text)) = v.input() {
        let label = match kind {
            InputKind::Goto => t!("ui.large_file_goto"),
            InputKind::SearchForward => t!("ui.large_file_search"),
            InputKind::SearchBackward => t!("ui.large_file_search_back"),
        };
        return Line::from(vec![
            Span::styled(format!("{label} "), theme::title(true)),
            Span::styled(format!("{text}▏"), theme::base()),
        ]);
    }
    if let Some(n) = v.notice() {
        return Line::from(Span::styled(notice_text(n), theme::base()));
    }
    Line::from(Span::styled(
        t!("ui.large_file_hint").to_string(),
        theme::dim(),
    ))
}

/// Render the Large File Viewer overlay over `area`.
pub(super) fn draw_large_file(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(v) = app.large_file.as_mut() else {
        return;
    };
    frame.render_widget(Clear, area);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(
            " {} {} — {} ",
            icon::FILE,
            t!("ui.large_file"),
            v.path().display()
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
    let st = v.status();
    let gutter = st.len.to_string().len();
    let text_width = usize::from(chunk(&chunks, 0).width).minus(gutter.plus(3));
    let rows = v.view(usize::from(chunk(&chunks, 0).height), text_width);
    let lines: Vec<Line> = rows
        .iter()
        .map(|r| row_line(r, gutter, st.hscroll > 0))
        .collect();
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));
    // The status after `view`, which may have scrolled.
    let st = v.status();
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            status_text(&st),
            theme::title(true),
        ))),
        chunk(&chunks, 1),
    );
    frame.render_widget(Paragraph::new(bottom_line(v)), chunk(&chunks, 2));
    app.layout.large_file = chunk(&chunks, 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use std::fmt::Write as _;
    use vix_input::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1.00 KiB");
        assert_eq!(human_size(1536), "1.50 KiB");
        assert_eq!(human_size(4 * 1024 * 1024 * 1024), "4.00 GiB");
    }

    fn status(line: Option<u64>, total: Option<u64>) -> Status {
        Status {
            len: 2048,
            offset: 10,
            percent: 7,
            line,
            total_lines: total,
            indexed: 42,
            searching: None,
            selecting: false,
            hscroll: 0,
        }
    }

    #[test]
    fn the_status_line_reports_position_line_and_progress() {
        let position = t!(
            "ui.large_file_position",
            offset = 10,
            size = "2.00 KiB",
            percent = 7
        )
        .to_string();
        // Fully indexed: "line N of M" and no indexing progress.
        let done = status_text(&status(Some(3), Some(9)));
        assert_eq!(
            done,
            format!(
                "{position}   {}",
                t!("ui.large_file_line_of", line = 3, total = 9)
            )
        );
        // Still indexing: the bare line and the progress.
        let partial = status_text(&status(Some(3), None));
        assert_eq!(
            partial,
            format!(
                "{position}   {}   {}",
                t!("ui.large_file_line", line = 3),
                t!("ui.large_file_indexing", percent = 42)
            )
        );
        // No line yet, a running search and a selection.
        let mut busy = status(None, None);
        busy.searching = Some(55);
        busy.selecting = true;
        assert_eq!(
            status_text(&busy),
            format!(
                "{position}   {}   {}   {}",
                t!("ui.large_file_indexing", percent = 42),
                t!("ui.large_file_searching", percent = 55),
                t!("ui.large_file_selecting")
            )
        );
    }

    #[test]
    fn every_notice_has_its_own_translated_text() {
        assert_eq!(
            notice_text(&Notice::NotFound),
            t!("ui.large_file_not_found")
        );
        assert_eq!(
            notice_text(&Notice::SearchCancelled),
            t!("ui.large_file_search_cancelled")
        );
        assert_eq!(
            notice_text(&Notice::GotoPending(12)),
            t!("ui.large_file_goto_pending", line = 12)
        );
        assert_eq!(notice_text(&Notice::BadGoto), t!("ui.large_file_bad_goto"));
        assert_eq!(
            notice_text(&Notice::BeyondEnd(4)),
            t!("ui.large_file_beyond_end", lines = 4)
        );
        assert_eq!(
            notice_text(&Notice::Copied(1536)),
            t!("ui.large_file_copied", size = "1.50 KiB")
        );
        assert_eq!(
            notice_text(&Notice::CopyTooLarge),
            t!(
                "ui.large_file_copy_too_large",
                size = human_size(crate::large_file::COPY_LIMIT)
            )
        );
        assert_eq!(
            notice_text(&Notice::ReadFailed("boom".into())),
            t!("ui.large_file_read_failed", error = "boom")
        );
        assert_eq!(
            notice_text(&Notice::IndexFailed),
            t!("ui.large_file_index_failed")
        );
    }

    fn row(text: &str, cursor: bool, selected: bool) -> ViewLine {
        ViewLine {
            offset: 0,
            text: text.to_string(),
            cursor,
            selected,
            hit: None,
            more_right: false,
        }
    }

    #[test]
    fn rows_mark_the_cursor_and_the_search_hit_and_the_hidden_edges() {
        let plain = row_line(&row("hello world", false, false), 4, false);
        let joined: String = plain.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(joined.ends_with("hello world"), "{joined}");

        let mut hit = row("hello world", false, false);
        hit.hit = Some((6, 11));
        hit.more_right = true;
        let line = row_line(&hit, 4, true);
        let texts: Vec<String> = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(texts.contains(&"world".to_string()), "{texts:?}");
        assert!(texts.contains(&"hello ".to_string()), "{texts:?}");
        assert!(
            texts.contains(&"›".to_string()),
            "a row cut at the right shows the arrow"
        );
        // The hit is drawn in a different style from the rest of its row.
        let style_of = |needle: &str| {
            line.spans
                .iter()
                .find(|s| s.content == needle)
                .map(|s| s.style)
        };
        assert_ne!(style_of("world"), style_of("hello "));

        // On the cursor line the styles swap: the hit is the plain style.
        let mut on_cursor = row("hello world", true, false);
        on_cursor.hit = Some((6, 11));
        let cur = row_line(&on_cursor, 4, false);
        let cur_style = |needle: &str| {
            cur.spans
                .iter()
                .find(|s| s.content == needle)
                .map(|s| s.style)
        };
        assert_ne!(cur_style("world"), cur_style("hello "));
        assert_ne!(cur_style("hello "), style_of("hello "));
    }

    fn draw_overlay(app: &mut App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(220, 12)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_large_file(app, frame, area);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn press(app: &mut App, code: KeyCode) {
        let v = app.large_file.as_mut().unwrap();
        v.handle_key(KeyEvent::new(code, KeyModifiers::NONE), 8);
    }

    #[test]
    fn the_overlay_shows_lines_then_the_goto_prompt_then_a_notice() {
        let dir = crate::app::cov4::temp("large-file");
        let path = dir.join("big.txt");
        let mut body = String::new();
        for n in 1..=20 {
            writeln!(body, "line number {n}").unwrap();
        }
        std::fs::write(&path, &body).unwrap();
        let mut app = crate::app::cov4::app_at(&dir);
        app.large_file = Some(Viewer::open(&path).unwrap());

        // Nothing but the file, the position status and the key hint.
        let drawn = draw_overlay(&mut app);
        assert!(drawn.contains("line number 1"), "{drawn}");
        assert!(drawn.contains(&t!("ui.large_file").to_string()));
        assert!(
            drawn.contains(&t!("ui.large_file_hint").to_string()),
            "{drawn}"
        );
        assert!(
            app.layout.large_file.height > 0,
            "the text area is recorded"
        );

        // `:` opens the goto input; typed text shows after its label.
        press(&mut app, KeyCode::Char(':'));
        press(&mut app, KeyCode::Char('z'));
        let drawn = draw_overlay(&mut app);
        assert!(
            drawn.contains(&t!("ui.large_file_goto").to_string()),
            "{drawn}"
        );
        assert!(drawn.contains("z▏"), "{drawn}");

        // An unparsable line number becomes a notice replacing the hint.
        press(&mut app, KeyCode::Enter);
        let drawn = draw_overlay(&mut app);
        assert!(
            drawn.contains(&t!("ui.large_file_bad_goto").to_string()),
            "{drawn}"
        );
        assert!(!drawn.contains(&t!("ui.large_file_hint").to_string()));

        // The search prompts have their own labels.
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('/'));
        assert!(draw_overlay(&mut app).contains(&t!("ui.large_file_search").to_string()));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('?'));
        assert!(draw_overlay(&mut app).contains(&t!("ui.large_file_search_back").to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn drawing_without_a_viewer_leaves_the_screen_alone() {
        let mut app = crate::app::cov4::app_at(&std::env::temp_dir());
        let drawn = draw_overlay(&mut app);
        assert!(drawn.chars().all(|c| c == ' ' || c == '\n'));
    }
}
