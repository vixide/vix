//! The generic pick-one list overlay (Local History, run configurations,
//! definition peek): a titled frame, the rows with the selection highlighted,
//! and a key hint underneath.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::{draw_scrollbar, trunc};
use crate::app::App;
use crate::theme::{self, icon};

/// Draw the overlay when one is open; records the row rectangle in
/// `app.layout.choice_list` for click-to-select.
pub(super) fn draw_choice_list(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(open) = app.choice_list.as_ref() else {
        return;
    };
    let width = 100u16.min(area.width);
    let height = (u16::try_from(open.panel.len()).unwrap_or(u16::MAX) + 4)
        .clamp(6, area.height.saturating_sub(4).max(6))
        .min(area.height);
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
        .title(format!(" {} {} ", icon::LIST, open.panel.title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view_h = chunks[0].height as usize;
    if let Some(o) = app.choice_list.as_mut() {
        o.panel.ensure_visible(view_h);
    }
    let Some(p) = app.choice_list.as_ref().map(|o| &o.panel) else {
        return;
    };
    let total = p.len();
    let show_bar = total > view_h && chunks[0].width > 1;
    let row_area = if show_bar {
        Rect {
            width: chunks[0].width - 1,
            ..chunks[0]
        }
    } else {
        chunks[0]
    };
    let text_w = row_area.width as usize;
    let lines: Vec<Line> = p
        .rows
        .iter()
        .enumerate()
        .skip(p.scroll)
        .take(view_h)
        .map(|(i, row)| {
            let text = trunc(row, text_w);
            if i == p.selected {
                Line::from(Span::styled(text, theme::selected()))
            } else {
                Line::from(Span::raw(text))
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect {
            x: chunks[0].x + chunks[0].width - 1,
            ..chunks[0]
        };
        draw_scrollbar(frame, sb_area, p.selected, total.saturating_sub(1));
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(p.hint.clone(), theme::dim()))),
        chunks[1],
    );
    app.layout.choice_list = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: row_area.width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[0].height),
    };
}
