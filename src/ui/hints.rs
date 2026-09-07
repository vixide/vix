//! Transient keyboard-mode overlays drawn over the editor: the which-key
//! popup (candidate keys for a pending prefix) and the jump-to-line labels
//! (leap-style navigation).

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::app::App;
use crate::theme;

/// Draw the which-key popup (candidate keys for a pending prefix) anchored at the
/// bottom of `area`. No-op when no prefix is pending or there are no candidates.
pub(super) fn draw_which_key(app: &App, frame: &mut Frame, area: Rect) {
    let Some((title, rows)) = app.which_key() else {
        return;
    };
    if rows.is_empty() {
        return;
    }
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, a)| {
            Line::from(vec![
                Span::styled(format!(" {k:<4}"), theme::selected()),
                Span::styled(format!(" {a} "), theme::dim()),
            ])
        })
        .collect();
    let width = rows
        .iter()
        .map(|(k, a)| k.len() + a.len() + 7)
        .max()
        .unwrap_or(20)
        .clamp(20, area.width.max(20) as usize);
    let width = u16::try_from(width).unwrap_or(20).min(area.width);
    let height = (u16::try_from(rows.len()).unwrap_or(1) + 2).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width),
        y: area.y + area.height.saturating_sub(height),
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(format!(" {title} "));
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

/// Draw the jump-to-line labels at the left edge of each labeled visible row.
pub(super) fn draw_jump_labels(app: &App, frame: &mut Frame) {
    let Some(jm) = app.jump.as_ref() else { return };
    let ed = app.layout.editor;
    let top = app.editor.top_visible_line();
    let style = theme::selected();
    for (label, line) in &jm.labels {
        // The label's row within the editor viewport (non-wrapped mapping).
        let Some(row_off) = line.checked_sub(top) else {
            continue;
        };
        let y = ed.y + u16::try_from(row_off).unwrap_or(u16::MAX);
        if y >= ed.y + ed.height {
            continue;
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(label.clone(), style))),
            Rect {
                x: ed.x,
                y,
                width: u16::try_from(label.len()).unwrap_or(2).min(ed.width),
                height: 1,
            },
        );
    }
}
