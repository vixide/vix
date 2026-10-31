//! The undo tree overlay (T638): the graph from `vix_undo_tree_panel`, one
//! row per state with its `seq` and local time, the selection highlighted.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::draw_scrollbar;
use crate::app::App;
use crate::theme::{self, icon};

/// Unix seconds as a local 24-hour `HH:MM:SS`; empty when unknown (`0`).
fn clock(secs: u64) -> String {
    if secs == 0 {
        return String::new();
    }
    i64::try_from(secs)
        .ok()
        .and_then(|s| jiff::Timestamp::from_second(s).ok())
        .map(|t| {
            t.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%H:%M:%S")
                .to_string()
        })
        .unwrap_or_default()
}

/// Draw the undo tree panel, centered, and record its row area for clicks.
pub(super) fn draw_undo_tree(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(state) = app.undo_tree.as_ref() else {
        return;
    };
    let total = state.panel.len();
    let width = 48u16.min(area.width);
    let max_rows = area.height.saturating_sub(4).max(1);
    let rows = u16::try_from(total).unwrap_or(u16::MAX).min(max_rows);
    let height = (rows + 4).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 3,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::BRANCH, t!("ui.undo_tree_title")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let view_h = chunks[0].height as usize;
    if let Some(s) = app.undo_tree.as_mut() {
        s.panel.ensure_visible(view_h);
    }
    let Some(state) = app.undo_tree.as_ref() else {
        return;
    };
    let p = &state.panel;
    let graph_w = p.graph_width();
    let selected = p.selected_row();
    let lines: Vec<Line> = p
        .rows
        .iter()
        .enumerate()
        .skip(p.scroll)
        .take(view_h)
        .map(|(i, row)| {
            let graph = &row.graph;
            let text = match row.seq {
                Some(0) => format!(" {graph:graph_w$}     0  {}", t!("ui.undo_tree_original")),
                Some(seq) => format!(" {graph:graph_w$}  {seq:>4}  {}", clock(row.timestamp)),
                None => format!(" {graph}"),
            };
            if i == selected {
                Line::from(Span::styled(text, theme::selected()))
            } else if row.seq.is_none() {
                Line::from(Span::styled(text, theme::dim()))
            } else {
                Line::from(text)
            }
        })
        .collect();
    let show_bar = total > view_h && chunks[0].width > 1;
    let row_area = if show_bar {
        Rect {
            width: chunks[0].width - 1,
            ..chunks[0]
        }
    } else {
        chunks[0]
    };
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect {
            x: chunks[0].x + chunks[0].width - 1,
            ..chunks[0]
        };
        draw_scrollbar(frame, sb_area, selected, total.saturating_sub(1));
    }
    let hint = Line::from(Span::styled(
        t!("ui.undo_tree_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.undo_tree = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: row_area.width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[0].height),
    };
}
