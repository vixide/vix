//! The IM dock panel and its dialogs (T752): the conversation list, the
//! messages of the selected conversation, the input line and a key hint.
//!
//! Each message is one logical line in reading order -- `time author: text`
//! -- so a screen reader reads it naturally; unread counts and mentions are
//! written as text (`3 unread`, `@`), never shown by colour alone. Remote text
//! was sanitized when it entered the inbox; nothing here turns it into an
//! escape sequence or a hyperlink.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthChar as _;
use vix_im::{ConversationId, Message, MessageKind};

use crate::app::im::ImDialog;
use crate::app::{App, Focus};
use crate::theme;

use super::ClampFit as _;

/// `HH:MM` in the system time zone for a UTC millisecond time.
fn clock(time_utc: i64) -> String {
    jiff::Timestamp::from_millisecond(time_utc)
        .map(|t| {
            t.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// A message as display rows before wrapping: the logical line, then (for a
/// snippet) the code lines. The flag marks rows drawn dim.
fn message_rows(app: &App, id: &ConversationId, m: &Message) -> Vec<(String, bool)> {
    let time = clock(m.time_utc);
    let author = if app.im.unverified(id, &m.author) {
        format!("{} ({})", m.author, t!("ui.im_unverified"))
    } else {
        m.author.clone()
    };
    let mut rows = vec![match &m.kind {
        MessageKind::System => (format!("{time} -- {}", m.body), true),
        MessageKind::Action => (format!("{time} * {author} {}", m.body), false),
        _ => (format!("{time} {author}: {}", m.body), false),
    }];
    if let MessageKind::Snippet { code, .. } = &m.kind {
        rows.extend(code.lines().map(|l| (format!("    {l}"), true)));
    }
    rows
}

/// Break `text` into rows at most `width` columns wide, by display width.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = vec![String::new()];
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width && used > 0 {
            rows.push(String::new());
            used = 0;
        }
        if let Some(last) = rows.last_mut() {
            last.push(c);
        }
        used += w;
    }
    rows
}

/// One conversation-list row: a marker for the selection, the title, and
/// its unread count and mention marker as text.
fn list_row(app: &App, index: usize) -> String {
    let Some(c) = app.im.inbox.conversations().get(index) else {
        return String::new();
    };
    let marker = if index == app.im.panel.selected_index() {
        "> "
    } else {
        "  "
    };
    let mut row = format!("{marker}{}", app.im.title(&c.id));
    if c.unread > 0 {
        row.push_str("  ");
        row.push_str(&t!("ui.im_unread", count = c.unread));
    }
    if c.mentioned {
        row.push_str(" @");
    }
    row
}

/// The IM dock: a bordered column beside the editor.
pub(super) fn draw_im_dock(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Im;
    let block = Block::default()
        .style(theme::region_base(theme::Region::RightDock))
        .borders(Borders::TOP | Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(theme::region_title(theme::Region::RightDock, focused))
        .title(format!(" {} ", t!("menu.im")));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.layout.im = area;
    if inner.height < 4 || inner.width < 4 {
        return;
    }
    let count = app.im.inbox.conversations().len();
    let list_h = u16::try_from(count)
        .unwrap_or(u16::MAX)
        .clamp_fit(1, (inner.height / 4).max(1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(list_h),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
    draw_list(app, frame, chunks[0]);
    draw_messages(app, frame, chunks[1]);
    draw_input(app, frame, chunks[2], focused);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            t!("ui.im_hint").to_string(),
            theme::dim(),
        ))),
        chunks[3],
    );
}

/// The conversation list, scrolled to keep the selection in view.
fn draw_list(app: &mut App, frame: &mut Frame, area: Rect) {
    let rows = area.height as usize;
    let selected = app.im.panel.selected_index();
    let top = selected.saturating_sub(rows.saturating_sub(1));
    app.im.list_top = top;
    app.layout.im_list = area;
    let lines: Vec<Line> = (top..app.im.inbox.conversations().len())
        .take(rows)
        .map(|i| {
            let line = Line::from(list_row(app, i));
            if i == selected {
                line.style(theme::selected())
            } else {
                line
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// The selected conversation's messages, newest at the bottom. With a
/// message highlighted the view keeps it on screen.
fn draw_messages(app: &mut App, frame: &mut Frame, area: Rect) {
    let height = area.height as usize;
    let width = area.width as usize;
    app.im.panel.ensure_visible(&app.im.inbox, height);
    let Some(conversation) = app.im.panel.conversation(&app.im.inbox) else {
        return;
    };
    if conversation.messages.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                t!("ui.im_empty").to_string(),
                theme::dim(),
            ))),
            area,
        );
        return;
    }
    let id = conversation.id.clone();
    let highlighted = app.im.panel.selected_message();
    let anchor = highlighted.unwrap_or(conversation.messages.len() - 1);
    // Each message's wrapped rows, from the panel's scroll offset on; then
    // start later while the anchor's last row would fall off the bottom.
    let wrapped: Vec<Vec<(String, bool)>> = conversation
        .messages
        .iter()
        .map(|m| {
            message_rows(app, &id, m)
                .into_iter()
                .flat_map(|(text, dim)| wrap(&text, width).into_iter().map(move |r| (r, dim)))
                .collect()
        })
        .collect();
    let mut start = app.im.panel.scroll().min(anchor);
    while start < anchor && wrapped[start..=anchor].iter().map(Vec::len).sum::<usize>() > height {
        start += 1;
    }
    let mut lines: Vec<Line> = Vec::new();
    for (i, rows) in wrapped.iter().enumerate().skip(start) {
        for (text, dim) in rows {
            let style = if Some(i) == highlighted {
                theme::selected()
            } else if *dim {
                theme::dim()
            } else {
                theme::base()
            };
            lines.push(Line::from(Span::styled(text.clone(), style)));
        }
        if lines.len() >= height {
            break;
        }
    }
    lines.truncate(height);
    frame.render_widget(Paragraph::new(lines), area);
}

/// The input line with a caret when the panel has the keyboard.
fn draw_input(app: &App, frame: &mut Frame, area: Rect, focused: bool) {
    let input = app.im.panel.input();
    let cursor = app.im.panel.cursor().min(input.len());
    let (before, after) = input.split_at(cursor);
    let mut spans = vec![
        Span::styled("› ", theme::title(focused)),
        Span::raw(before.to_string()),
    ];
    if focused {
        spans.push(Span::styled("▏", theme::dim()));
    }
    spans.push(Span::raw(after.to_string()));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// An IM dialog over the editor: the account picker, or a share or clear
/// confirmation that shows exactly what will happen.
pub(super) fn draw_im_dialog(app: &App, frame: &mut Frame, area: Rect) {
    let Some(dialog) = app.im_dialog.as_ref() else {
        return;
    };
    let (title, body): (String, Vec<Line>) = match dialog {
        ImDialog::Accounts { names, selected } => (
            t!("menu.item.im.connect").to_string(),
            names
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    let line = Line::from(format!(" {n} "));
                    if i == *selected {
                        line.style(theme::selected())
                    } else {
                        line
                    }
                })
                .collect(),
        ),
        ImDialog::Share { title, message, .. } => {
            let mut lines = vec![
                Line::from(t!("ui.im_send_to", target = title.as_str()).to_string()),
                Line::from(message.body.clone()),
            ];
            if let MessageKind::Snippet { code, .. } = &message.kind {
                lines.extend(
                    code.lines()
                        .map(|l| Line::from(Span::styled(format!("  {l}"), theme::dim()))),
                );
            }
            lines.push(Line::from(Span::styled(
                t!("ui.im_confirm_hint").to_string(),
                theme::dim(),
            )));
            (t!("menu.item.im.share_selection").to_string(), lines)
        }
        ImDialog::Clear { title, .. } => (
            t!("menu.item.im.clear_history").to_string(),
            vec![
                Line::from(t!("ui.im_clear_question", name = title.as_str()).to_string()),
                Line::from(Span::styled(
                    t!("ui.im_confirm_hint").to_string(),
                    theme::dim(),
                )),
            ],
        ),
    };
    let widest = body.iter().map(Line::width).max().unwrap_or(0);
    let width = u16::try_from(widest + 4)
        .unwrap_or(u16::MAX)
        .clamp_fit(30, area.width);
    let height = u16::try_from(body.len() + 2)
        .unwrap_or(u16::MAX)
        .clamp_fit(3, area.height);
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
        .title(format!(" {title} "));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(body), inner);
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn wrap_breaks_by_display_width() {
        assert_eq!(wrap("abcdef", 4), vec!["abcd", "ef"]);
        // Wide characters take two columns each.
        assert_eq!(wrap("漢字漢字", 4), vec!["漢字", "漢字"]);
        assert_eq!(wrap("", 4), vec![""]);
    }
}
