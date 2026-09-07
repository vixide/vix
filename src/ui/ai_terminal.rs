//! AI-related overlays and the integrated terminal: the reviewable AI-diff
//! hunk viewer, the PTY-backed terminal panel, and the persistent AI chat
//! panel.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::app::App;
use crate::theme::{self, icon};

#[allow(clippy::too_many_lines)]
pub(super) fn draw_ai_diff(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::ai_diff::Seg;
    let Some(review) = app.ai_diff_review() else {
        return;
    };
    let width = (area.width * 8 / 10).clamp(30, area.width);
    let height = (area.height * 8 / 10).clamp(8, area.height);
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

/// Map a `vt100` color to a ratatui color.
fn vt_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn draw_terminal(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.terminal.is_none() {
        return;
    }
    let width = area.width.max(2);
    let height = area.height.max(2);
    let rect = Rect {
        x: area.x,
        y: area.y,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::CODE, t!("ui.terminal")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    // Match the PTY grid to the visible area so the shell wraps correctly.
    if let Some(term) = app.terminal.as_mut() {
        term.resize(inner.height, inner.width);
    }
    let Some(term) = app.terminal.as_ref() else {
        return;
    };
    let parser = term.lock();
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    let mut lines: Vec<Line> = Vec::with_capacity(rows as usize);
    for row in 0..rows.min(inner.height) {
        let mut spans: Vec<Span> = Vec::with_capacity(cols as usize);
        for col in 0..cols.min(inner.width) {
            let (text, mut style) = match screen.cell(row, col) {
                Some(cell) => {
                    let contents = cell.contents();
                    let text = if contents.is_empty() {
                        " ".to_string()
                    } else {
                        contents
                    };
                    let mut s = Style::default()
                        .fg(vt_color(cell.fgcolor()))
                        .bg(vt_color(cell.bgcolor()));
                    if cell.bold() {
                        s = s.add_modifier(Modifier::BOLD);
                    }
                    if cell.italic() {
                        s = s.add_modifier(Modifier::ITALIC);
                    }
                    if cell.underline() {
                        s = s.add_modifier(Modifier::UNDERLINED);
                    }
                    (text, s)
                }
                None => (" ".to_string(), Style::default()),
            };
            if screen.cell(row, col).is_some_and(vt100::Cell::inverse) {
                style = style.add_modifier(Modifier::REVERSED);
            }
            spans.push(Span::styled(text, style));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);

    // Place the real cursor where the shell put it.
    if !screen.hide_cursor() {
        let (crow, ccol) = screen.cursor_position();
        if crow < inner.height && ccol < inner.width {
            frame.set_cursor_position((inner.x + ccol, inner.y + crow));
        }
    }
}

pub(super) fn draw_ai_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::ai_panel::Role;
    if app.ai_panel.is_none() {
        return;
    }
    let width = (area.width * 7 / 10).clamp(30, area.width);
    let height = (area.height * 7 / 10).clamp(8, area.height);
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
