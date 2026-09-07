//! The file browser overlay (File → Open…): a `walkdir`-backed listing with
//! fuzzy/glob/extension filtering, sort, and hidden-file toggling.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::{draw_scrollbar, trunc, unix_secs_label};
use crate::app::App;
use crate::theme::{self, icon};

/// The file browser overlay (File → Open…): root + filter lines, the entry
/// rows (name, size, modified date), and status + hint lines. Records the
/// row-list rectangle for mouse hit-testing and paging.
pub(super) fn draw_file_browser(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.file_browser.is_none() {
        return;
    }
    let width = 76u16.min(area.width);
    let height = (area.height.saturating_sub(4)).max(8).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::FOLDER_OPEN, t!("ui.file_browser")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let fb = app.file_browser.as_ref().unwrap();
    draw_file_browser_header(fb, frame, chunks[0], chunks[1]);

    let view_h = chunks[2].height as usize;
    if let Some(fb) = app.file_browser.as_mut() {
        fb.ensure_visible(view_h);
    }
    let fb = app.file_browser.as_ref().unwrap();
    let filtered = fb.matches();
    let total = filtered.len();
    // Fixed size (9) and date (16) columns; the path takes the rest.
    let path_w = (chunks[2].width as usize).saturating_sub(9 + 16 + 5).max(8);
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for (row, &idx) in filtered.iter().enumerate().skip(fb.scroll).take(view_h) {
        let text = file_browser_row(&fb.entries[idx], path_w);
        let line = if row == fb.selected {
            Line::from(Span::styled(text, theme::selected()))
        } else {
            Line::from(Span::raw(text))
        };
        lines.push(line);
    }
    let show_bar = total > view_h && chunks[2].width > 1;
    let row_area = if show_bar {
        Rect {
            width: chunks[2].width - 1,
            ..chunks[2]
        }
    } else {
        chunks[2]
    };
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect {
            x: chunks[2].x + chunks[2].width - 1,
            ..chunks[2]
        };
        draw_scrollbar(frame, sb_area, fb.selected, total.saturating_sub(1));
    }

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            file_browser_status(fb, total),
            theme::dim(),
        ))),
        chunks[3],
    );

    let hint = Line::from(Span::styled(
        t!("ui.file_browser_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[4]);

    app.layout.file_browser = Rect {
        x: chunks[2].x,
        y: chunks[2].y,
        width: row_area.width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[2].height),
    };
}

/// The file browser's root line (the directory being browsed) and filter line
/// (the typed query, or a dim prompt when empty).
fn draw_file_browser_header(
    fb: &crate::file_browser_panel::Panel,
    frame: &mut Frame,
    root_area: Rect,
    filter_area: Rect,
) {
    let root = Line::from(vec![
        Span::styled(format!("{} ", icon::FOLDER), theme::dim()),
        Span::raw(fb.root.display().to_string()),
    ]);
    frame.render_widget(Paragraph::new(root), root_area);

    let filter = if fb.query.is_empty() {
        Line::from(Span::styled(
            t!("ui.file_browser_filter").to_string(),
            theme::dim(),
        ))
    } else {
        Line::from(vec![
            Span::styled("/ ", theme::dim()),
            Span::raw(fb.query.clone()),
        ])
    };
    frame.render_widget(Paragraph::new(filter), filter_area);
}

/// One file-browser row: relative path (directories get a `/` suffix), size,
/// and modified date, in fixed columns.
fn file_browser_row(e: &crate::file_browser_panel::Entry, path_w: usize) -> String {
    let shown = if e.is_dir {
        format!("{}/", e.rel)
    } else {
        e.rel.clone()
    };
    let size = if e.is_dir {
        String::new()
    } else {
        crate::file_browser_panel::size_label(e.size)
    };
    format!(
        " {:<path_w$} {:>9}  {:<16}",
        trunc(&shown, path_w),
        size,
        unix_secs_label(e.modified),
    )
}

/// The file-browser status line: sort column + direction, match count, and
/// the hidden-files / truncation flags when set.
fn file_browser_status(fb: &crate::file_browser_panel::Panel, total: usize) -> String {
    let arrow = if fb.ascending { "↑" } else { "↓" };
    let mut parts = vec![
        format!("{} {arrow}", t!(fb.sort.label_key())),
        t!("ui.file_browser_count", count = total).to_string(),
    ];
    if fb.show_hidden {
        parts.push(t!("ui.file_browser_hidden").to_string());
    }
    if fb.truncated {
        parts.push(t!("ui.file_browser_truncated").to_string());
    }
    parts.join(" · ")
}
