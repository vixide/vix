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

use crate::app::info_panel::InfoSlot;
use crate::geo::{Geo, chunk};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::draw_scrollbar;
use crate::app::App;
use crate::theme::{self, icon};

/// Draw the contacts list panel, sized to its row count.
pub(super) fn draw_contacts(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(total) = app.contacts.as_ref().map(crate::contact_panel::Panel::len) else {
        return;
    };
    let width = 40u16.min(area.width);
    let max_rows = area.height.minus(3).max(1);
    let rows = u16::try_from(total.max(1))
        .unwrap_or(u16::MAX)
        .min(max_rows);
    let height = rows.plus(3).min(area.height);
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
        .title(format!(" {} {} ", icon::FOLDER, t!("ui.contacts")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let view_h = usize::from(chunk(&chunks, 0).height);
    if let Some(p) = app.contacts.as_mut() {
        p.ensure_visible(view_h);
    }
    let Some(p) = app.contacts.as_ref() else {
        return;
    };
    let show_bar = total > view_h && chunk(&chunks, 0).width > 1;
    let list_area = if show_bar {
        Rect {
            width: chunk(&chunks, 0).width.minus(1),
            ..chunk(&chunks, 0)
        }
    } else {
        chunk(&chunks, 0)
    };
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    if p.is_empty() {
        lines.push(Line::from(Span::styled(
            t!("ui.no_contacts").to_string(),
            theme::dim(),
        )));
    } else {
        for idx in p.scroll..p.scroll.plus(view_h).min(total) {
            let Some(contact) = p.contacts.get(idx) else {
                continue;
            };
            let text = format!("  {}", contact.name);
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
            x: chunk(&chunks, 0).x.plus(chunk(&chunks, 0).width).minus(1),
            ..chunk(&chunks, 0)
        };
        draw_scrollbar(frame, sb_area, p.selected, total.minus(1));
    }
    let hint = Line::from(Span::styled(
        t!("ui.contacts_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));
    app.layout.contacts = Rect {
        x: chunk(&chunks, 0).x,
        y: chunk(&chunks, 0).y,
        width: list_area.width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunk(&chunks, 0).height),
    };
}

/// Draw the vCard viewer panel, titled with the card's name.
pub(super) fn draw_vcard(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(total) = app.vcard.as_ref().map(crate::vcard_panel::Panel::len) else {
        return;
    };
    let title = app
        .vcard
        .as_ref()
        .map_or_default(crate::vcard_panel::Panel::title);
    let width = 60u16.min(area.width);
    let max_rows = area.height.minus(3).max(1);
    let rows = u16::try_from(total.max(1))
        .unwrap_or(u16::MAX)
        .min(max_rows);
    let height = rows.plus(3).min(area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(3)),
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
    let view_h = usize::from(chunk(&chunks, 0).height);
    if let Some(p) = app.vcard.as_mut() {
        p.ensure_visible(view_h);
    }
    let Some(p) = app.vcard.as_ref() else { return };
    let show_bar = total > view_h && chunk(&chunks, 0).width > 1;
    let list_area = if show_bar {
        Rect {
            width: chunk(&chunks, 0).width.minus(1),
            ..chunk(&chunks, 0)
        }
    } else {
        chunk(&chunks, 0)
    };
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in p.scroll..p.scroll.plus(view_h).min(total) {
        let Some(row) = p.rows.get(idx) else {
            continue;
        };
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
            x: chunk(&chunks, 0).x.plus(chunk(&chunks, 0).width).minus(1),
            ..chunk(&chunks, 0)
        };
        draw_scrollbar(frame, sb_area, p.selected, total.minus(1));
    }
    let hint = Line::from(Span::styled(t!("ui.vcard_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));
    app.layout.vcard = Rect {
        x: chunk(&chunks, 0).x,
        y: chunk(&chunks, 0).y,
        width: list_area.width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunk(&chunks, 0).height),
    };
}

/// Draw the snippet browser panel.
pub(super) fn draw_snippets(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.snippets.is_none() {
        return;
    }
    let width = 60u16.min(area.width).max(24);
    let height = area.height.minus(4).max(6).min(area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(4)),
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let Some(picker) = app.snippets.as_ref() else {
        return;
    };
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
    frame.render_widget(Paragraph::new(filter), chunk(&chunks, 0));

    let view_h = usize::from(chunk(&chunks, 1).height);
    let lib = &app.snippet_library;
    if let Some(p) = app.snippets.as_mut() {
        p.ensure_visible(view_h, lib);
    }
    let Some(picker) = app.snippets.as_ref() else {
        return;
    };
    let filtered = picker.matches(lib);
    let total = filtered.len();
    let colw = usize::from(chunk(&chunks, 1).width);
    let mut rows: Vec<Line> = Vec::with_capacity(view_h);
    let shown = filtered.iter().enumerate().skip(picker.scroll).take(view_h);
    for (row, s) in shown.filter_map(|(row, &i)| Some((row, lib.get(i)?))) {
        let prefix = s
            .prefixes
            .first()
            .map_or_else(String::new, |p| format!("[{p}] "));
        let text = format!(" {}{}  ", prefix, s.name);
        let scope = s.scope.label();
        let pad = colw.minus(text.chars().count().plus(scope.chars().count()).plus(1));
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
    let show_bar = total > view_h && chunk(&chunks, 1).width > 1;
    let row_area = if show_bar {
        Rect {
            width: chunk(&chunks, 1).width.minus(1),
            ..chunk(&chunks, 1)
        }
    } else {
        chunk(&chunks, 1)
    };
    frame.render_widget(Paragraph::new(rows), row_area);
    if show_bar {
        let sb = Rect {
            x: chunk(&chunks, 1).x.plus(chunk(&chunks, 1).width).minus(1),
            ..chunk(&chunks, 1)
        };
        draw_scrollbar(frame, sb, picker.selected, total.minus(1));
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            t!("ui.snippets_hint").to_string(),
            theme::dim(),
        ))),
        chunk(&chunks, 2),
    );
    app.layout.snippets = row_area;
}

/// Draw the Markdown preview as a large centred reading pane (at most 80
/// columns wide).
pub(super) fn draw_markdown_preview(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(panel) = app.markdown_preview.as_mut() else {
        return;
    };
    // A large centered reading pane.
    let width = 80u16.min(area.width.minus(2)).max(20);
    let height = area.height.minus(2).max(6);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(2)),
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

    let view_h = usize::from(inner.height);
    let max_scroll = panel.lines.len().minus(view_h);
    if panel.scroll > max_scroll {
        panel.scroll = max_scroll;
    }
    let end = panel.scroll.plus(view_h).min(panel.lines.len());
    let lines: Vec<Line> = panel
        .lines
        .get(panel.scroll..end)
        .unwrap_or(&[])
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
    // Links render as `text (url)`; make the URL clickable (T628).
    if app.terminal_caps.hyperlinks {
        let urls = super::hyperlink::urls_in(
            panel
                .lines
                .get(panel.scroll..end)
                .unwrap_or(&[])
                .iter()
                .map(String::as_str),
        );
        if !urls.is_empty() {
            app.layout.hyperlink_regions.push((inner, urls));
        }
    }
    if panel.lines.len() > view_h {
        let sb = Rect {
            x: rect.x.plus(rect.width).minus(1),
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
    let max_rows = area.height.minus(3).max(1);
    let rows = u16::try_from(n).unwrap_or(u16::MAX).min(max_rows);
    let height = rows.plus(3).min(area.height);
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
        .title(format!(" {} {} ", icon::LIST, t!("ui.markdown_toc")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view_h = usize::from(chunk(&chunks, 0).height);
    if let Some(o) = app.markdown_toc.as_mut() {
        o.ensure_visible(view_h);
    }
    let Some(o) = app.markdown_toc.as_ref() else {
        return;
    };
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in o.scroll..o.scroll.plus(view_h).min(o.len()) {
        let Some(e) = o.entries.get(idx) else {
            continue;
        };
        // `kind` is the heading's `#`-run; indent nested levels under it.
        let indent = "  ".repeat(e.kind.len().minus(1));
        let text = format!("  {indent}{}", e.name);
        if idx == o.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(text));
        }
    }
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));

    let hint = Line::from(Span::styled(
        t!("ui.markdown_toc_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));
}

/// Draw the text-information panel (counts and statistics for the buffer).
pub(super) fn draw_text_info(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(n) = app
        .info
        .text_info()
        .map(crate::text_information_panel::Panel::len)
    else {
        return;
    };
    let width = 40u16.min(area.width).max(24);
    let height = u16::try_from(n)
        .unwrap_or(u16::MAX)
        .plus(3)
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
        .title(format!(" {} {} ", icon::INFO, t!("ui.text_info")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let Some(p) = app.info.text_info() else {
        return;
    };
    let view_h = usize::from(chunk(&chunks, 0).height);
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for (idx, row) in p.rows.iter().take(view_h).enumerate() {
        let text = format!("  {:<12} {}", row.label, row.value);
        if idx == p.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(Span::raw(text)));
        }
    }
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            t!("ui.system_info_hint").to_string(),
            theme::dim(),
        ))),
        chunk(&chunks, 1),
    );
    app.layout.text_info = Rect {
        x: chunk(&chunks, 0).x,
        y: chunk(&chunks, 0).y,
        width: chunk(&chunks, 0).width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunk(&chunks, 0).height),
    };
}

/// Draw the file-information panel for the active file.
pub(super) fn draw_file_info(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(n) = app
        .info
        .file_info()
        .map(vix_file_information_panel::Panel::len)
    else {
        return;
    };
    let width = 64u16.min(area.width);
    let max_rows = area.height.minus(3).max(1);
    let rows = u16::try_from(n).unwrap_or(u16::MAX).min(max_rows);
    let height = rows.plus(3).min(area.height);
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
        .title(format!(" {} {} ", icon::INFO, t!("ui.file_info")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view_h = usize::from(chunk(&chunks, 0).height);
    if let Some(p) = app.info.file_info_mut() {
        p.ensure_visible(view_h);
    }
    let Some(p) = app.info.file_info() else {
        return;
    };
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in p.scroll..p.scroll.plus(view_h).min(p.len()) {
        let Some(row) = p.rows.get(idx) else {
            continue;
        };
        let text = format!("  {:<14} {}", row.label, row.value);
        if idx == p.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(Span::raw(text)));
        }
    }
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));

    let hint = Line::from(Span::styled(
        t!("ui.system_info_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));

    app.layout.file_info = Rect {
        x: chunk(&chunks, 0).x,
        y: chunk(&chunks, 0).y,
        width: chunk(&chunks, 0).width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunk(&chunks, 0).height),
    };
}

/// Draw the system-information panel.
pub(super) fn draw_system_info(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(n) = app
        .info
        .system_info()
        .map(vix_system_information_panel::Panel::len)
    else {
        return;
    };
    let width = 60u16.min(area.width);
    let max_rows = area.height.minus(3).max(1);
    let rows = u16::try_from(n).unwrap_or(u16::MAX).min(max_rows);
    let height = rows.plus(3).min(area.height);
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
        .title(format!(" {} {} ", icon::INFO, t!("ui.system_info")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view_h = usize::from(chunk(&chunks, 0).height);
    if let Some(p) = app.info.system_info_mut() {
        p.ensure_visible(view_h);
    }
    let Some(p) = app.info.system_info() else {
        return;
    };
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in p.scroll..p.scroll.plus(view_h).min(p.len()) {
        let Some(row) = p.rows.get(idx) else {
            continue;
        };
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
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));

    let hint = Line::from(Span::styled(
        t!("ui.system_info_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));

    app.layout.system_info = Rect {
        x: chunk(&chunks, 0).x,
        y: chunk(&chunks, 0).y,
        width: chunk(&chunks, 0).width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunk(&chunks, 0).height),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
    use vix_input::{KeyCode, KeyEvent, KeyModifiers};

    fn app() -> App {
        let mut a = App::new(std::env::temp_dir(), Settings::default());
        a.layout.editor = Rect::new(0, 0, 80, 24);
        a
    }

    fn render(app: &mut App, w: u16, h: u16, f: fn(&mut App, &mut Frame, Rect)) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                f(app, frame, area);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn text(buf: &Buffer) -> String {
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn blank(buf: &Buffer) -> bool {
        text(buf).chars().all(|c| c == ' ' || c == '\n')
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn contacts(n: usize) -> Vec<crate::contact_panel::Contact> {
        (0..n)
            .map(|i| crate::contact_panel::Contact {
                name: format!("Person {i:02}"),
                path: std::path::PathBuf::from(format!("/nowhere/{i}.vcf")),
            })
            .collect()
    }

    #[test]
    fn contacts_panel_lists_names_and_records_its_hit_area() {
        let mut a = app();
        assert!(blank(&render(&mut a, 60, 20, draw_contacts)));
        a.contacts = Some(crate::contact_panel::Panel::open(contacts(3)));
        let buf = render(&mut a, 60, 20, draw_contacts);
        let t = text(&buf);
        assert!(t.contains(&t!("ui.contacts").to_string()));
        assert!(t.contains("Person 00") && t.contains("Person 02"));
        assert!(t.contains(&t!("ui.contacts_hint").to_string()));
        assert_eq!(a.layout.contacts.height, 3);
        assert_eq!(a.layout.contacts.width, 38);
    }

    #[test]
    fn contacts_panel_scrolls_with_a_scrollbar_when_long() {
        let mut a = app();
        let mut p = crate::contact_panel::Panel::open(contacts(40));
        p.selected = 30;
        a.contacts = Some(p);
        let buf = render(&mut a, 60, 12, draw_contacts);
        let t = text(&buf);
        assert!(
            t.contains("Person 30"),
            "the selection is scrolled into view"
        );
        assert!(!t.contains("Person 00"));
        // A scrollbar takes the last column of the list, so rows are one narrower.
        assert_eq!(a.layout.contacts.width, 37);
    }

    #[test]
    fn contacts_panel_with_no_contacts_says_so() {
        let mut a = app();
        a.contacts = Some(crate::contact_panel::Panel::open(Vec::new()));
        let t = text(&render(&mut a, 60, 12, draw_contacts));
        assert!(t.contains(&t!("ui.no_contacts").to_string()));
    }

    fn vcard() -> crate::vcard_panel::Panel {
        crate::vcard_panel::Panel::open(crate::vcard_parser::parse(
            "BEGIN:VCARD\nVERSION:3.0\nFN:Ada Lovelace\nEMAIL:ada@example.org\nTEL:555-0100\nEND:VCARD\n",
        ))
    }

    #[test]
    fn vcard_panel_shows_the_card_rows_under_its_name() {
        let mut a = app();
        assert!(blank(&render(&mut a, 80, 20, draw_vcard)));
        a.vcard = Some(vcard());
        let t = text(&render(&mut a, 80, 20, draw_vcard));
        assert!(t.contains("Ada Lovelace"));
        assert!(t.contains("ada@example.org"));
        assert!(t.contains("555-0100"));
        assert!(t.contains(&t!("ui.vcard_hint").to_string()));
        assert!(a.layout.vcard.height >= 2);
    }

    #[test]
    fn vcard_panel_gets_a_scrollbar_when_taller_than_the_screen() {
        let mut a = app();
        a.vcard = Some(vcard());
        let rows = a.vcard.as_ref().unwrap().len();
        assert!(rows >= 3);
        // A screen too short for every row scrolls instead of growing.
        let _ = render(&mut a, 80, 5, draw_vcard);
        assert!(usize::from(a.layout.vcard.height) < rows);
        assert_eq!(a.layout.vcard.width, 57);
    }

    #[test]
    fn snippets_panel_filters_and_marks_the_selected_row() {
        let mut a = app();
        assert!(blank(&render(&mut a, 80, 24, draw_snippets)));
        a.run_action("tools.snippets");
        let t = text(&render(&mut a, 80, 24, draw_snippets));
        assert!(t.contains(&t!("ui.snippets").to_string()));
        assert!(t.contains(&t!("ui.snippets_filter").to_string()));
        assert!(t.contains(&t!("ui.snippets_hint").to_string()));
        assert!(a.layout.snippets.height > 0);

        // Typing narrows the list and shows the query instead of the hint.
        for c in "zzzz".chars() {
            a.on_key(key(KeyCode::Char(c)));
        }
        let t = text(&render(&mut a, 80, 24, draw_snippets));
        assert!(t.contains("/ zzzz"));
        assert!(!t.contains(&t!("ui.snippets_filter").to_string()));
    }

    #[test]
    fn snippets_panel_scrolls_a_long_library_with_a_scrollbar() {
        let mut a = app();
        a.run_action("tools.snippets");
        let total = a.snippet_library.len();
        assert!(total > 5, "bundled snippets exist");
        let before = a.layout.snippets;
        let _ = render(&mut a, 80, 10, draw_snippets);
        assert!(a.layout.snippets.height < u16::try_from(total).unwrap());
        assert_ne!(before, a.layout.snippets);
        // Walking to the end keeps the selection inside the visible rows.
        for _ in 0..total {
            a.on_key(key(KeyCode::Down));
        }
        let t = text(&render(&mut a, 80, 10, draw_snippets));
        assert!(t.contains(&t!("ui.snippets_hint").to_string()));
    }

    fn markdown_app(n_sections: usize) -> App {
        let mut a = app();
        let mut body = String::new();
        for i in 0..n_sections {
            std::fmt::Write::write_fmt(
                &mut body,
                format_args!("# Section {i}\n\nSome text for section {i}.\n\n"),
            )
            .unwrap();
        }
        a.editor.new_tab_with_content(&body);
        a.run_action("tools.markdown_preview");
        assert!(a.markdown_preview.is_some());
        a
    }

    #[test]
    fn markdown_preview_renders_rules_dim_and_scrolls_with_a_scrollbar() {
        let mut a = app();
        assert!(blank(&render(&mut a, 90, 24, draw_markdown_preview)));
        let mut a = markdown_app(30);
        let t = text(&render(&mut a, 90, 20, draw_markdown_preview));
        assert!(t.contains(&t!("ui.markdown_preview").to_string()));
        assert!(t.contains("Section 0"));
        // Scrolling far past the end clamps to the last page.
        a.markdown_preview.as_mut().unwrap().scroll = usize::MAX / 2;
        let t = text(&render(&mut a, 90, 20, draw_markdown_preview));
        assert!(t.contains("Section 29"));
        assert!(!t.contains("Section 0"));
        let max = a.markdown_preview.as_ref().unwrap().scroll;
        assert!(max < usize::MAX / 2);
    }

    #[test]
    fn markdown_preview_records_urls_when_the_terminal_has_hyperlinks() {
        let mut a = app();
        a.editor
            .new_tab_with_content("see [the site](https://example.org/page) now\n");
        a.run_action("tools.markdown_preview");
        a.terminal_caps.hyperlinks = true;
        let _ = render(&mut a, 90, 20, draw_markdown_preview);
        assert_eq!(a.layout.hyperlink_regions.len(), 1);
        assert!(
            a.layout
                .hyperlink_regions
                .iter()
                .any(|(_, urls)| urls.iter().any(|u| u.contains("example.org")))
        );
    }

    #[test]
    fn markdown_toc_lists_headings_indented_by_level() {
        let mut a = app();
        assert!(blank(&render(&mut a, 90, 24, draw_markdown_toc)));
        a.editor
            .new_tab_with_content("# Top\n\ntext\n\n## Child\n\nmore\n\n### Grand\n");
        a.run_action("tools.markdown_preview");
        a.on_key(key(KeyCode::Char('t')));
        assert!(a.markdown_toc.is_some());
        let buf = render(&mut a, 90, 24, draw_markdown_toc);
        let t = text(&buf);
        assert!(t.contains(&t!("ui.markdown_toc").to_string()));
        let line_of = |needle: &str| {
            text(&buf)
                .lines()
                .find(|l| l.contains(needle))
                .map(|l| l.find(needle).unwrap())
                .unwrap()
        };
        assert!(line_of("Child") > line_of("Top"));
        assert!(line_of("Grand") > line_of("Child"));
        assert!(t.contains(&t!("ui.markdown_toc_hint").to_string()));
    }

    #[test]
    fn text_info_panel_shows_statistics_for_the_buffer() {
        let mut a = app();
        assert!(blank(&render(&mut a, 80, 24, draw_text_info)));
        a.editor.new_tab_with_content("one two three\nfour\n");
        a.run_action("tools.text_info");
        let t = text(&render(&mut a, 80, 24, draw_text_info));
        assert!(t.contains(&t!("ui.text_info").to_string()));
        let first = a
            .info
            .text_info()
            .unwrap()
            .rows
            .first()
            .unwrap()
            .label
            .clone();
        assert!(t.contains(&first), "the first statistic row is drawn");
        assert!(a.layout.text_info.height > 0);
    }

    #[test]
    fn file_info_panel_shows_the_active_file_facts() {
        let dir = std::env::temp_dir().join(format!("vix-r2c-fileinfo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("facts.txt");
        std::fs::write(&file, "hello world\n").unwrap();
        let mut a = App::new(dir.clone(), Settings::default());
        a.layout.editor = Rect::new(0, 0, 80, 24);
        a.open_initial(&file);
        assert!(blank(&render(&mut a, 90, 24, draw_file_info)));
        a.run_action("tools.file_info");
        let t = text(&render(&mut a, 90, 24, draw_file_info));
        assert!(t.contains(&t!("ui.file_info").to_string()));
        assert!(t.contains("facts.txt"));
        assert!(a.layout.file_info.height > 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn system_info_panel_has_headings_and_scrolls() {
        let mut a = app();
        assert!(blank(&render(&mut a, 90, 24, draw_system_info)));
        a.run_action("tools.system_info");
        let t = text(&render(&mut a, 90, 24, draw_system_info));
        assert!(t.contains(&t!("ui.system_info").to_string()));
        assert!(t.contains(&t!("ui.system_info_hint").to_string()));
        // A short screen shows only part of the rows, and moving down scrolls.
        let _ = render(&mut a, 90, 8, draw_system_info);
        let first = a.info.system_info().unwrap().scroll;
        for _ in 0..a.info.system_info().unwrap().len() {
            a.on_key(key(KeyCode::Down));
        }
        let _ = render(&mut a, 90, 8, draw_system_info);
        assert!(a.info.system_info().unwrap().scroll > first);
    }
}
