//! AI overlays: the reviewable AI-diff hunk viewer and the persistent AI chat
//! panel.

use super::ClampFit;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::app::App;
use crate::theme::{self, icon};

#[allow(
    clippy::too_many_lines,
    reason = "a single linear frame-drawing routine for one overlay"
)]
pub(super) fn draw_ai_diff(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::ai_diff::Seg;
    let Some(review) = app.ai_diff_review() else {
        return;
    };
    let width = (area.width * 8 / 10).clamp_fit(30, area.width);
    let height = (area.height * 8 / 10).clamp_fit(8, area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let title = format!(
        " {} {} ({}/{}) ",
        icon::INFO,
        t!("ui.ai_diff"),
        review.accepted_count(),
        review.change_count()
    );
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let body = chunks[0];
    let positions = review.change_positions();
    let selected_seg = positions.get(review.selected).copied();

    let mut lines: Vec<Line> = Vec::new();
    for (i, seg) in review.segs.iter().enumerate() {
        match seg {
            Seg::Equal(ls) => {
                for l in ls {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", l.trim_end_matches('\n')),
                        theme::dim(),
                    )));
                }
            }
            Seg::Change { old, new, accepted } => {
                let here = Some(i) == selected_seg;
                let marker = if here { "▸" } else { " " };
                for l in old {
                    let style = Style::default().fg(Color::Red).add_modifier(Modifier::DIM);
                    lines.push(Line::from(Span::styled(
                        format!("{marker} - {}", l.trim_end_matches('\n')),
                        style,
                    )));
                }
                for l in new {
                    let mut style = Style::default().fg(Color::Green);
                    if !accepted {
                        style = style.add_modifier(Modifier::DIM | Modifier::CROSSED_OUT);
                    } else if here {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    lines.push(Line::from(Span::styled(
                        format!("{marker} + {}", l.trim_end_matches('\n')),
                        style,
                    )));
                }
            }
        }
    }
    // Scroll so the selected hunk stays visible: anchor the view near it.
    let view_h = body.height as usize;
    let anchor = selected_seg.map_or(0, |sid| {
        review.segs[..sid].iter().map(seg_line_count).sum::<usize>()
    });
    let start = anchor
        .saturating_sub(view_h / 3)
        .min(lines.len().saturating_sub(view_h));
    let window: Vec<Line> = lines.into_iter().skip(start).take(view_h).collect();
    frame.render_widget(Paragraph::new(window), body);

    let hint = Line::from(Span::styled(
        t!("ui.ai_diff_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
}

/// Number of rendered lines a diff segment occupies (old + new for a change).
fn seg_line_count(seg: &crate::ai_diff::Seg) -> usize {
    match seg {
        crate::ai_diff::Seg::Equal(ls) => ls.len(),
        crate::ai_diff::Seg::Change { old, new, .. } => old.len() + new.len(),
    }
}

pub(super) fn draw_ai_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::ai_panel::Role;
    if app.ai_panel.is_none() {
        return;
    }
    let width = (area.width * 7 / 10).clamp_fit(30, area.width);
    let height = (area.height * 7 / 10).clamp_fit(8, area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let busy = app.ai_panel.as_ref().is_some_and(|p| p.busy);
    let title = if busy {
        format!(" {} {} ", icon::INFO, t!("ui.ai_thinking"))
    } else {
        format!(" {} {} ", icon::INFO, t!("menu.ai"))
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
    let body = chunks[0];
    let view_h = body.height as usize;
    let view_w = body.width as usize;
    let lines: Vec<Line> = {
        let p = app.ai_panel.as_mut().unwrap();
        let visible = p.visible(view_w, view_h);
        visible
            .into_iter()
            .map(|(role, text)| {
                let style = match role {
                    Role::User => theme::base().add_modifier(Modifier::BOLD),
                    Role::Assistant => theme::base(),
                    Role::Error => theme::dim().add_modifier(Modifier::ITALIC),
                };
                Line::from(Span::styled(text, style))
            })
            .collect()
    };
    frame.render_widget(Paragraph::new(lines), body);
    app.layout.ai_panel = body;

    // Input line: a leading prompt glyph then the in-progress text plus a caret.
    let input = app
        .ai_panel
        .as_ref()
        .map(|p| p.input.clone())
        .unwrap_or_default();
    let input_line = Line::from(vec![
        Span::styled("› ", theme::title(true).add_modifier(Modifier::BOLD)),
        Span::raw(input),
        Span::styled("▏", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(input_line), chunks[1]);

    let hint = Line::from(Span::styled(
        t!("ui.ai_panel_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[2]);
}
