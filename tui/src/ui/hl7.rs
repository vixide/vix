//! Rendering for the HL7 message view overlay (Tools → HL7 View): a filter
//! line, the numbered segment/field rows, and a hint line.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::{draw_scrollbar, trunc};
use crate::app::App;
use crate::hl7::RowKind;
use crate::theme::{self, icon};

/// The overlay's rectangle: centred, wide enough for a typical field value.
fn overlay_rect(area: Rect) -> Rect {
    let width = 78u16.min(area.width);
    let height = (area.height.saturating_sub(4)).max(6).min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    }
}

pub(super) fn draw_hl7_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.hl7_panel.is_none() {
        return;
    }
    let rect = overlay_rect(area);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::CODE, t!("ui.hl7_view")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let view_h = chunks[1].height as usize;
    if let Some(p) = app.hl7_panel.as_mut() {
        p.ensure_visible(view_h);
    }
    let Some(p) = app.hl7_panel.as_ref() else {
        return;
    };
    let filter = if p.query.is_empty() {
        Line::from(Span::styled(
            t!("ui.hl7_view_filter").to_string(),
            theme::dim(),
        ))
    } else {
        Line::from(vec![
            Span::styled("/ ", theme::dim()),
            Span::raw(p.query.clone()),
        ])
    };
    frame.render_widget(Paragraph::new(filter), chunks[0]);

    let matches = p.matches();
    let total = matches.len();
    let text_w = (chunks[1].width as usize).saturating_sub(1);
    let lines: Vec<Line> = matches
        .iter()
        .enumerate()
        .skip(p.scroll)
        .take(view_h)
        .map(|(row, &idx)| {
            let text = trunc(&p.rows[idx].display(), text_w);
            let style = if row == p.selected {
                theme::selected()
            } else if p.rows[idx].kind == RowKind::Segment {
                theme::title(true)
            } else {
                Style::default()
            };
            Line::from(Span::styled(text, style))
        })
        .collect();
    let show_bar = total > view_h && chunks[1].width > 1;
    let row_area = Rect {
        width: chunks[1].width.saturating_sub(u16::from(show_bar)),
        ..chunks[1]
    };
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect {
            x: chunks[1].x + chunks[1].width - 1,
            width: 1,
            ..chunks[1]
        };
        draw_scrollbar(frame, sb_area, p.selected, total.saturating_sub(1));
    }
    let hint = Line::from(Span::styled(
        t!("ui.hl7_view_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[2]);
    app.layout.hl7_panel = Rect {
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[1].height),
        ..row_area
    };
}
