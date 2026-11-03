//! Rendering for the FHIR Bundle viewer overlay (T684): one row per entry,
//! with the resource reference, its search mode or response status, and a
//! short label drawn from the resource.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::{draw_scrollbar, trunc};
use crate::app::App;
use crate::theme::{self, icon};

/// The centered overlay rectangle.
fn overlay_rect(area: Rect) -> Rect {
    let width = 90u16.min(area.width);
    let height = (area.height.saturating_sub(4)).max(6).min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    }
}

/// The info line: Bundle type, entries on this page, and the server's total.
fn info_line(b: &crate::fhir::Bundle) -> String {
    match b.total {
        Some(total) => t!(
            "ui.fhir_bundle_info_total",
            kind = b.bundle_type,
            count = b.entries.len(),
            total = total
        )
        .to_string(),
        None => t!(
            "ui.fhir_bundle_info",
            kind = b.bundle_type,
            count = b.entries.len()
        )
        .to_string(),
    }
}

/// Draw the FHIR Bundle viewer overlay: one row per entry. No-op when none is open.
pub(super) fn draw_fhir_bundle(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.fhir_bundle.is_none() {
        return;
    }
    let rect = overlay_rect(area);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::CODE, t!("ui.fhir_bundle")));
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
    if let Some(p) = app.fhir_bundle.as_mut() {
        p.ensure_visible(view_h);
    }
    let Some(p) = app.fhir_bundle.as_ref() else {
        return;
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(info_line(&p.bundle), theme::dim()))),
        chunks[0],
    );

    let lines: Vec<Line> = if p.is_empty() {
        vec![Line::from(Span::styled(
            t!("ui.fhir_bundle_empty").to_string(),
            theme::dim(),
        ))]
    } else {
        p.bundle
            .entries
            .iter()
            .enumerate()
            .skip(p.scroll)
            .take(view_h)
            .map(|(row, e)| {
                let tag = e.status.as_deref().or(e.search_mode.as_deref());
                let text = format!(
                    " {:30} {:14} {}",
                    trunc(&e.reference(), 30),
                    trunc(tag.unwrap_or(""), 14),
                    e.summary
                );
                if row == p.selected {
                    Line::from(Span::styled(text, theme::selected()))
                } else {
                    Line::from(Span::raw(text))
                }
            })
            .collect()
    };
    let total = p.len();
    let show_bar = total > view_h && chunks[1].width > 1;
    let row_area = if show_bar {
        Rect {
            width: chunks[1].width - 1,
            ..chunks[1]
        }
    } else {
        chunks[1]
    };
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect {
            x: chunks[1].x + chunks[1].width - 1,
            ..chunks[1]
        };
        draw_scrollbar(frame, sb_area, p.selected, total.saturating_sub(1));
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            t!("ui.fhir_bundle_hint").to_string(),
            theme::dim(),
        ))),
        chunks[2],
    );
    app.layout.fhir_bundle = Rect {
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[1].height),
        width: row_area.width,
        ..chunks[1]
    };
}
