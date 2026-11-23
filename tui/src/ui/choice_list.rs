//! The generic pick-one list overlay (Local History, run configurations,
//! definition peek): a titled frame, the rows with the selection highlighted,
//! and a key hint underneath.

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
    let height = (u16::try_from(open.panel.len()).unwrap_or(u16::MAX))
        .plus(4)
        .clamp(6, area.height.minus(4).max(6))
        .min(area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(4)),
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

    let view_h = chunk(&chunks, 0).height as usize;
    if let Some(o) = app.choice_list.as_mut() {
        o.panel.ensure_visible(view_h);
    }
    let Some(p) = app.choice_list.as_ref().map(|o| &o.panel) else {
        return;
    };
    let total = p.len();
    let show_bar = total > view_h && chunk(&chunks, 0).width > 1;
    let row_area = if show_bar {
        Rect {
            width: chunk(&chunks, 0).width.minus(1),
            ..chunk(&chunks, 0)
        }
    } else {
        chunk(&chunks, 0)
    };
    let text_w = usize::from(row_area.width);
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
            x: chunk(&chunks, 0).x.plus(chunk(&chunks, 0).width).minus(1),
            ..chunk(&chunks, 0)
        };
        draw_scrollbar(frame, sb_area, p.selected, total.minus(1));
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(p.hint.clone(), theme::dim()))),
        chunk(&chunks, 1),
    );
    app.layout.choice_list = Rect {
        x: chunk(&chunks, 0).x,
        y: chunk(&chunks, 0).y,
        width: row_area.width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunk(&chunks, 0).height),
    };
}
