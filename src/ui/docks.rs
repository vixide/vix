//! Dock panels along the bottom-strip family: the test-run tree, the debug
//! (DAP) panel, the outline dock (document structure sidebar), and the
//! notifications drawer.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap};

use super::{draw_hscrollbar, draw_scrollbar, hslice_spans, span_line_width};
use crate::app::{App, Focus};
use crate::messages::Level;
use crate::theme::{self, icon};

pub(super) fn draw_test_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::test_runner::Status;
    let (pass, fail, ignore) = crate::test_runner::tally(&app.test_results);
    let block = Block::default()
        .style(theme::region_base(theme::Region::RightDock))
        .borders(Borders::TOP | Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(theme::region_title(theme::Region::RightDock, false))
        .title(format!(
            " {} {} {pass}/{fail}/{ignore} ",
            icon::CODE,
            t!("ui.tests")
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.layout.test_panel = inner;

    if app.test_results.is_empty() {
        let hint = Paragraph::new(t!("ui.tests_idle").to_string())
            .style(theme::dim())
            .wrap(Wrap { trim: true });
        frame.render_widget(hint, inner);
        return;
    }
    let view_h = inner.height as usize;
    let lines: Vec<Line> = app
        .test_results
        .iter()
        .enumerate()
        .take(view_h)
        .map(|(i, r)| {
            let (icon, style) = match r.status {
                Status::Pass => ("\u{2713} ", Style::default().fg(Color::Green)),
                Status::Fail => ("\u{2717} ", Style::default().fg(Color::Red)),
                Status::Ignore => ("\u{25cb} ", theme::dim()),
            };
            let row = Line::from(vec![Span::styled(icon, style), Span::raw(r.name.clone())]);
            if i == app.test_selected {
                row.style(theme::selected())
            } else {
                row
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn draw_debug_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .style(theme::region_base(theme::Region::RightDock))
        .borders(Borders::TOP | Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(theme::region_title(theme::Region::RightDock, false))
        .title(format!(" {} {} ", icon::CODE, t!("ui.debug")));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    let header = |lines: &mut Vec<Line>, text: String| {
        lines.push(Line::from(Span::styled(
            text,
            theme::title(true).add_modifier(Modifier::BOLD),
        )));
    };
    if !app.dap.is_active() {
        let hint = Paragraph::new(t!("ui.debug_idle").to_string())
            .style(theme::dim())
            .wrap(Wrap { trim: true });
        frame.render_widget(hint, inner);
        return;
    }
    header(&mut lines, t!("ui.debug_call_stack").to_string());
    if app.dap_stack.is_empty() {
        lines.push(Line::from(Span::styled("  —", theme::dim())));
    }
    for f in &app.dap_stack {
        let loc = f
            .path
            .as_deref()
            .and_then(|p| p.rsplit('/').next())
            .map_or(String::new(), |n| format!("  {n}:{}", f.line));
        lines.push(Line::from(vec![
            Span::raw(format!("  {}", f.name)),
            Span::styled(loc, theme::dim()),
        ]));
    }
    lines.push(Line::from(""));
    header(&mut lines, t!("ui.debug_variables").to_string());
    if app.dap_variables.is_empty() {
        lines.push(Line::from(Span::styled("  —", theme::dim())));
    }
    for v in &app.dap_variables {
        lines.push(Line::from(vec![
            Span::raw(format!("  {} = ", v.name)),
            Span::styled(v.value.clone(), theme::dim()),
        ]));
    }
    if !app.dap_watches.is_empty() {
        lines.push(Line::from(""));
        header(&mut lines, t!("ui.debug_watch").to_string());
        for (expr, result) in &app.dap_watches {
            lines.push(Line::from(vec![
                Span::raw(format!("  {expr} = ")),
                Span::styled(result.clone(), theme::dim()),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

pub(super) fn draw_outline_dock(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .style(theme::region_base(theme::Region::RightDock))
        .borders(Borders::TOP | Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(theme::region_title(theme::Region::RightDock, false))
        .title(format!(" {} {} ", icon::CODE, t!("ui.outline")));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.layout.outline_dock = inner;

    let Some(o) = app.outline_dock.as_mut() else {
        let hint = Paragraph::new(t!("status.outline_empty").to_string())
            .style(theme::dim())
            .wrap(Wrap { trim: true });
        frame.render_widget(hint, inner);
        return;
    };
    let view_h = inner.height as usize;
    o.ensure_visible(view_h);
    let total = o.len();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in o.scroll..(o.scroll + view_h).min(total) {
        let e = &o.entries[idx];
        let kind = if e.kind.is_empty() {
            String::new()
        } else {
            format!("{:<6} ", e.kind)
        };
        if idx == o.selected {
            lines.push(Line::from(Span::styled(
                format!("{kind}{}", e.name),
                theme::selected(),
            )));
        } else {
            lines.push(Line::from(vec![
                Span::styled(kind, theme::dim()),
                Span::raw(e.name.clone()),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn draw_messages(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Messages;
    let block = Block::default()
        .style(theme::region_base(theme::Region::RightDock))
        // The right dock keeps only its top and left borders.
        .borders(Borders::TOP | Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(theme::region_title(theme::Region::RightDock, focused))
        .title(format!(" {} {} ", icon::BELL, t!("ui.messages")));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.messages.items.is_empty() {
        let hint = Paragraph::new(t!("ui.no_messages").to_string())
            .style(theme::dim())
            .wrap(Wrap { trim: true });
        frame.render_widget(hint, inner);
        return;
    }

    let rows: Vec<Vec<Span<'static>>> = app
        .messages
        .items
        .iter()
        .map(|m| {
            let (sym, sym_style) = match m.level {
                Level::Info | Level::Advice => (icon::INFO, Style::default()),
                Level::Warn => (icon::BELL, Style::default()),
                Level::Error => (icon::CLOSE, Style::default()),
            };
            vec![
                Span::styled(format!("{sym} "), sym_style),
                Span::raw(m.text.clone()),
                Span::styled(format!("  {}", icon::CLOSE), theme::dim()),
            ]
        })
        .collect();
    let total = app.messages.items.len();
    let allow_bars = app.settings.show_scrollbar && inner.width > 1 && inner.height > 1;
    let vbar = allow_bars && total > inner.height as usize;
    let text_w = if vbar { inner.width - 1 } else { inner.width } as usize;
    let content_w = rows.iter().map(|s| span_line_width(s)).max().unwrap_or(0);
    let hbar = allow_bars && content_w > text_w;
    let body_h = if hbar { inner.height - 1 } else { inner.height };
    let hmax = content_w.saturating_sub(text_w);
    app.messages_hmax = hmax;
    app.messages_hscroll = app.messages_hscroll.min(hmax);
    let off = app.messages_hscroll;

    let items: Vec<ListItem> = rows
        .iter()
        .map(|s| ListItem::new(Line::from(hslice_spans(s, off, text_w))))
        .collect();
    let list_area = Rect {
        width: u16::try_from(text_w).unwrap_or(u16::MAX),
        height: body_h,
        ..inner
    };
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(app.messages.selected));
    frame.render_stateful_widget(list, list_area, &mut state);
    if vbar {
        let sb = Rect {
            x: inner.x + inner.width - 1,
            y: inner.y,
            width: 1,
            height: body_h,
        };
        draw_scrollbar(frame, sb, app.messages.selected, total.saturating_sub(1));
    }
    app.layout.messages_hscrollbar = if hbar {
        let hb = Rect {
            x: inner.x,
            y: inner.y + inner.height - 1,
            width: u16::try_from(text_w).unwrap_or(u16::MAX),
            height: 1,
        };
        draw_hscrollbar(frame, hb, off, hmax);
        hb
    } else {
        Rect::default()
    };
}
