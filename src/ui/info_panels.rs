//! Rendering for the small info/reference panels: the Contacts vCard
//! view, the file-info and text-info panels, Markdown preview, the
//! Snippets picker, and the System Info panel -- the drawing side of
//! `vix_app::info_panels` (T141).
//!
//! Moved out of `ui.rs` verbatim (T142, slice 1). Stays in the `vix`
//! crate, same as every other `src/ui/*.rs` submodule -- rendering
//! lives only in `src/ui.rs` and its submodules, never in a panel's own
//! crate (see `AGENTS.md`'s non-negotiable convention, and T142's own
//! task text after it was revised to match).

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::draw_scrollbar;
use crate::app::App;
use crate::theme::{self, icon};

pub(super) fn draw_contacts(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(total) = app.contacts.as_ref().map(crate::contact_panel::Panel::len) else {
        return;
    };
    let width = 40u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = u16::try_from(total.max(1))
        .unwrap_or(u16::MAX)
        .min(max_rows);
    let height = (rows + 3).min(area.height);
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
        .title(format!(" {} {} ", icon::FOLDER, t!("ui.contacts")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let view_h = chunks[0].height as usize;
    if let Some(p) = app.contacts.as_mut() {
        p.ensure_visible(view_h);
    }
    let p = app.contacts.as_ref().unwrap();
    let show_bar = total > view_h && chunks[0].width > 1;
    let list_area = if show_bar {
        Rect {
            width: chunks[0].width - 1,
            ..chunks[0]
        }
    } else {
        chunks[0]
    };
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    if p.is_empty() {
        lines.push(Line::from(Span::styled(
            t!("ui.no_contacts").to_string(),
            theme::dim(),
        )));
    } else {
        for idx in p.scroll..(p.scroll + view_h).min(total) {
            let text = format!("  {}", p.contacts[idx].name);
            if idx == p.selected {
                lines.push(Line::from(Span::styled(text, theme::selected())));
            } else {
                lines.push(Line::from(Span::raw(text)));
            }
        }
    }
    frame.render_widget(Paragraph::new(lines), list_area);
    if show_bar {
        let sb_area = Rect {
            x: chunks[0].x + chunks[0].width - 1,
            ..chunks[0]
        };
        draw_scrollbar(frame, sb_area, p.selected, total.saturating_sub(1));
    }
    let hint = Line::from(Span::styled(
        t!("ui.contacts_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
    app.layout.contacts = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: list_area.width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[0].height),
    };
}

pub(super) fn draw_vcard(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(total) = app.vcard.as_ref().map(crate::vcard_panel::Panel::len) else {
        return;
    };
    let title = app
        .vcard
        .as_ref()
        .map(crate::vcard_panel::Panel::title)
        .unwrap_or_default();
    let width = 60u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = u16::try_from(total.max(1))
        .unwrap_or(u16::MAX)
        .min(max_rows);
    let height = (rows + 3).min(area.height);
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
        .title(format!(" {} {} ", icon::INFO, title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let view_h = chunks[0].height as usize;
    if let Some(p) = app.vcard.as_mut() {
        p.ensure_visible(view_h);
    }
    let p = app.vcard.as_ref().unwrap();
    let show_bar = total > view_h && chunks[0].width > 1;
    let list_area = if show_bar {
        Rect {
            width: chunks[0].width - 1,
            ..chunks[0]
        }
    } else {
        chunks[0]
    };
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in p.scroll..(p.scroll + view_h).min(total) {
        let row = &p.rows[idx];
        let text = format!("  {:<14} {}", row.label, row.value);
        if idx == p.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(Span::raw(text)));
        }
    }
    frame.render_widget(Paragraph::new(lines), list_area);
    if show_bar {
        let sb_area = Rect {
            x: chunks[0].x + chunks[0].width - 1,
            ..chunks[0]
        };
        draw_scrollbar(frame, sb_area, p.selected, total.saturating_sub(1));
    }
    let hint = Line::from(Span::styled(t!("ui.vcard_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
    app.layout.vcard = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: list_area.width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[0].height),
    };
}

pub(super) fn draw_snippets(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.snippets.is_none() {
        return;
    }
    let width = 60u16.min(area.width).max(24);
    let height = area.height.saturating_sub(4).max(6).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let picker = app.snippets.as_ref().unwrap();
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.snippets")));
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

    let filter = if picker.query.is_empty() {
        Line::from(Span::styled(
            t!("ui.snippets_filter").to_string(),
            theme::dim(),
        ))
    } else {
        Line::from(vec![
            Span::styled("/ ", theme::dim()),
            Span::raw(picker.query.clone()),
        ])
    };
    frame.render_widget(Paragraph::new(filter), chunks[0]);

    let view_h = chunks[1].height as usize;
    let lib = &app.snippet_library;
    if let Some(p) = app.snippets.as_mut() {
        p.ensure_visible(view_h, lib);
    }
    let picker = app.snippets.as_ref().unwrap();
    let filtered = picker.matches(lib);
    let total = filtered.len();
    let colw = chunks[1].width as usize;
    let mut rows: Vec<Line> = Vec::with_capacity(view_h);
    for (row, &i) in filtered.iter().enumerate().skip(picker.scroll).take(view_h) {
        let s = &lib[i];
        let prefix = s
            .prefixes
            .first()
            .map_or_else(String::new, |p| format!("[{p}] "));
        let text = format!(" {}{}  ", prefix, s.name);
        let scope = s.scope.label();
        let pad = colw.saturating_sub(text.chars().count() + scope.chars().count() + 1);
        let body_line = format!("{text}{}{scope} ", " ".repeat(pad));
        if row == picker.selected {
            rows.push(Line::from(Span::styled(body_line, theme::selected())));
        } else {
            rows.push(Line::from(vec![
                Span::raw(text),
                Span::styled(format!("{}{scope} ", " ".repeat(pad)), theme::dim()),
            ]));
        }
    }
    let show_bar = total > view_h && chunks[1].width > 1;
    let row_area = if show_bar {
        Rect {
            width: chunks[1].width - 1,
            ..chunks[1]
        }
    } else {
        chunks[1]
    };
    frame.render_widget(Paragraph::new(rows), row_area);
    if show_bar {
        let sb = Rect {
            x: chunks[1].x + chunks[1].width - 1,
            ..chunks[1]
        };
        draw_scrollbar(frame, sb, picker.selected, total.saturating_sub(1));
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            t!("ui.snippets_hint").to_string(),
            theme::dim(),
        ))),
        chunks[2],
    );
    app.layout.snippets = row_area;
}

pub(super) fn draw_markdown_preview(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(panel) = app.markdown_preview.as_mut() else {
        return;
    };
    // A large centered reading pane.
    let width = 80u16.min(area.width.saturating_sub(2)).max(20);
    let height = area.height.saturating_sub(2).max(6);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.markdown_preview")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let view_h = inner.height as usize;
    let max_scroll = panel.lines.len().saturating_sub(view_h);
    if panel.scroll > max_scroll {
        panel.scroll = max_scroll;
    }
    let end = (panel.scroll + view_h).min(panel.lines.len());
    let lines: Vec<Line> = panel.lines[panel.scroll..end]
        .iter()
        .map(|l| {
            // Heading underline rules and the thematic break render dim.
            let dim = l.chars().all(|c| matches!(c, '=' | '-' | '─')) && !l.is_empty();
            if dim {
                Line::from(Span::styled(l.clone(), theme::dim()))
            } else {
                Line::from(l.clone())
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
    if panel.lines.len() > view_h {
        let sb = Rect {
            x: rect.x + rect.width - 1,
            y: inner.y,
            width: 1,
            height: inner.height,
        };
        draw_scrollbar(frame, sb, panel.scroll, max_scroll);
    }
}

/// The Markdown preview's table of contents (T206), drawn over the preview
/// the same way `draw_outline` draws over the editor -- see that function
/// for the shared layout shape.
pub(super) fn draw_markdown_toc(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(n) = app.markdown_toc.as_ref().map(crate::app::Outline::len) else {
        return;
    };
    let width = 48u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = u16::try_from(n).unwrap_or(u16::MAX).min(max_rows);
    let height = (rows + 3).min(area.height);
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
        .title(format!(" {} {} ", icon::LIST, t!("ui.markdown_toc")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view_h = chunks[0].height as usize;
    if let Some(o) = app.markdown_toc.as_mut() {
        o.ensure_visible(view_h);
    }
    let o = app.markdown_toc.as_ref().unwrap();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in o.scroll..(o.scroll + view_h).min(o.len()) {
        let e = &o.entries[idx];
        // `kind` is the heading's `#`-run; indent nested levels under it.
        let indent = "  ".repeat(e.kind.len().saturating_sub(1));
        let text = format!("  {indent}{}", e.name);
        if idx == o.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(text));
        }
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);

    let hint = Line::from(Span::styled(
        t!("ui.markdown_toc_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
}

pub(super) fn draw_text_info(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(n) = app
        .text_info
        .as_ref()
        .map(crate::text_information_panel::Panel::len)
    else {
        return;
    };
    let width = 40u16.min(area.width).max(24);
    let height = (u16::try_from(n).unwrap_or(u16::MAX) + 3).min(area.height);
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
        .title(format!(" {} {} ", icon::INFO, t!("ui.text_info")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let p = app.text_info.as_ref().unwrap();
    let view_h = chunks[0].height as usize;
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for (idx, row) in p.rows.iter().take(view_h).enumerate() {
        let text = format!("  {:<12} {}", row.label, row.value);
        if idx == p.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(Span::raw(text)));
        }
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            t!("ui.system_info_hint").to_string(),
            theme::dim(),
        ))),
        chunks[1],
    );
    app.layout.text_info = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: chunks[0].width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[0].height),
    };
}

pub(super) fn draw_file_info(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.file_info.is_none() {
        return;
    }
    let n = app.file_info.as_ref().unwrap().len();
    let width = 64u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = u16::try_from(n).unwrap_or(u16::MAX).min(max_rows);
    let height = (rows + 3).min(area.height);
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
        .title(format!(" {} {} ", icon::INFO, t!("ui.file_info")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view_h = chunks[0].height as usize;
    if let Some(p) = app.file_info.as_mut() {
        p.ensure_visible(view_h);
    }
    let p = app.file_info.as_ref().unwrap();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in p.scroll..(p.scroll + view_h).min(p.len()) {
        let row = &p.rows[idx];
        let text = format!("  {:<14} {}", row.label, row.value);
        if idx == p.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(Span::raw(text)));
        }
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);

    let hint = Line::from(Span::styled(
        t!("ui.system_info_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.file_info = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: chunks[0].width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[0].height),
    };
}

pub(super) fn draw_system_info(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.system_info.is_none() {
        return;
    }
    let n = app.system_info.as_ref().unwrap().len();
    let width = 60u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = u16::try_from(n).unwrap_or(u16::MAX).min(max_rows);
    let height = (rows + 3).min(area.height);
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
        .title(format!(" {} {} ", icon::INFO, t!("ui.system_info")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view_h = chunks[0].height as usize;
    if let Some(p) = app.system_info.as_mut() {
        p.ensure_visible(view_h);
    }
    let p = app.system_info.as_ref().unwrap();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in p.scroll..(p.scroll + view_h).min(p.len()) {
        let row = &p.rows[idx];
        let line = if row.is_heading() {
            Line::from(Span::styled(row.label.clone(), theme::title(true)))
        } else {
            let text = format!("  {:<16} {}", row.label, row.value);
            if idx == p.selected {
                Line::from(Span::styled(text, theme::selected()))
            } else {
                Line::from(Span::raw(text))
            }
        };
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);

    let hint = Line::from(Span::styled(
        t!("ui.system_info_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.system_info = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: chunks[0].width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[0].height),
    };
}
