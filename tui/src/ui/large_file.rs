//! The Large File Viewer overlay (T733): a byte-offset gutter, the visible
//! lines, a status line (size, position, line, index and search progress),
//! and a hint, input, or notice line. The viewer reads only what is drawn;
//! see `crates/vix-large-file/spec/index.md`.

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
    while scaled >= 1024 * 100 && unit + 1 < UNITS.len() {
        scaled /= 1024;
        unit += 1;
    }
    if unit == 0 {
        return format!("{bytes} B");
    }
    format!("{}.{:02} {}", scaled / 100, scaled % 100, UNITS[unit])
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
                chars[..a].iter().collect::<String>(),
                line_style,
            ));
            spans.push(Span::styled(
                chars[a..b].iter().collect::<String>(),
                hit_style,
            ));
            spans.push(Span::styled(
                chars[b..].iter().collect::<String>(),
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
    let text_width = usize::from(chunks[0].width).saturating_sub(gutter + 3);
    let rows = v.view(usize::from(chunks[0].height), text_width);
    let lines: Vec<Line> = rows
        .iter()
        .map(|r| row_line(r, gutter, st.hscroll > 0))
        .collect();
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    // The status after `view`, which may have scrolled.
    let st = v.status();
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            status_text(&st),
            theme::title(true),
        ))),
        chunks[1],
    );
    frame.render_widget(Paragraph::new(bottom_line(v)), chunks[2]);
    app.layout.large_file = chunks[0];
}

#[cfg(test)]
mod tests {
    use super::human_size;

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1.00 KiB");
        assert_eq!(human_size(1536), "1.50 KiB");
        assert_eq!(human_size(4 * 1024 * 1024 * 1024), "4.00 GiB");
    }
}
