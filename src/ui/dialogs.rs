//! Small modal dialogs: the generic yes/no confirm box, the workspace-script
//! trust prompt, the find/replace confirmation viewer, the unsaved-changes
//! prompt, the paste-name-conflict prompt, and the interactive query-replace
//! status bar.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use crate::app::App;
use crate::theme::{self, icon};

pub(super) fn draw_confirm(app: &App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.confirm.as_ref() else {
        return;
    };
    let width = (u16::try_from(c.message.chars().count()).unwrap_or(u16::MAX) + 6).min(area.width);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + area.height / 3,
        width,
        height: 3,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.confirm")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(Line::from(c.message.clone())), inner);
}

/// The "trust this workspace's scripts?" prompt (T132).
pub(super) fn draw_script_trust(app: &App, frame: &mut Frame, area: Rect) {
    let Some(p) = app.script_trust.as_ref() else {
        return;
    };
    let width = (area.width * 3 / 5).clamp(40, 64).min(area.width);
    let height = 6u16.min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + area.height / 3,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.script_trust_title")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let message = t!("ui.script_trust_message", count = p.count).to_string();
    frame.render_widget(Paragraph::new(message).wrap(Wrap { trim: true }), chunks[0]);
    let hint = Line::from(Span::styled(
        t!("ui.script_trust_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
}

pub(super) fn draw_replace_confirm(app: &App, frame: &mut Frame, area: Rect) {
    let Some(rc) = app.replace_confirm.as_ref() else {
        return;
    };
    let width = (area.width * 7 / 10).clamp(30, area.width);
    let height = (area.height * 6 / 10).clamp(8, area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 3,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let title = format!(
        " {} {} ",
        icon::SEARCH,
        t!(
            "ui.replace_confirm_title",
            replaced = rc.replaced,
            files = rc.plan.len()
        )
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
    let view_h = chunks[0].height as usize;
    let start = rc.scroll.min(rc.lines.len().saturating_sub(1));
    let lines: Vec<Line> = rc
        .lines
        .iter()
        .skip(start)
        .take(view_h)
        .map(|l| Line::from(Span::raw(l.clone())))
        .collect();
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    let hint = Line::from(Span::styled(
        t!("ui.replace_confirm_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
}

pub(super) fn draw_unsaved(app: &App, frame: &mut Frame, area: Rect) {
    let Some(u) = app.unsaved.as_ref() else {
        return;
    };
    let message = t!("ui.unsaved_prompt", name = u.name).to_string();
    let choices = t!("ui.unsaved_choices");
    let width = (u16::try_from(message.chars().count().max(choices.chars().count()))
        .unwrap_or(u16::MAX)
        + 6)
    .min(area.width);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height / 3,
        width,
        height: 4,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.unsaved_title")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let lines = vec![
        Line::from(message),
        Line::from(Span::styled(choices.to_string(), theme::dim())),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn draw_paste_conflict(app: &App, frame: &mut Frame, area: Rect) {
    let Some(op) = app.paste.as_ref() else { return };
    let Some(src) = op.conflict.as_ref() else {
        return;
    };
    let name = src
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lines = vec![
        Line::from(t!("ui.paste_exists", name = name).to_string()),
        Line::from(Span::styled(t!("ui.paste_choices"), theme::dim())),
    ];
    let width = 56u16.min(area.width);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + area.height / 3,
        width,
        height: 4,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.paste_conflict")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn draw_query_replace(app: &App, frame: &mut Frame, area: Rect) {
    let Some(qr) = app.query_replace.as_ref() else {
        return;
    };
    let bar = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(2),
        width: area.width,
        height: 1,
    };
    frame.render_widget(Clear, bar);
    let line = Line::from(vec![
        Span::styled(
            format!(" {} {} ", icon::SEARCH, t!("ui.qr_label")),
            Style::default(),
        ),
        Span::styled(format!(" «{}» ", qr.label), Style::default()),
        Span::raw("  "),
        Span::styled("y", theme::title(true)),
        Span::raw(format!(" {}  ", t!("ui.qr_replace"))),
        Span::styled("n", theme::title(true)),
        Span::raw(format!(" {}  ", t!("ui.qr_skip"))),
        Span::styled("!", theme::title(true)),
        Span::raw(format!(" {}  ", t!("ui.qr_rest"))),
        Span::styled("q", theme::title(true)),
        Span::raw(format!(" {}   ", t!("ui.qr_quit"))),
        Span::styled(t!("ui.qr_replaced", count = qr.replaced), theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme::base()), bar);
}
