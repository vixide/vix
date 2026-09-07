//! Small LSP-driven popups anchored near the cursor or centered over the
//! editor: the completion list, the hover tooltip, and the code-action /
//! code-lens choosers (sharing one generic single-column chooser box).

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
};

use crate::app::App;
use crate::theme;

/// The active editor cursor's screen position within the editor text area, as
/// `(x, y)`, accounting for vertical scroll. The x is approximate (it does not
/// subtract the line-number gutter), which is fine for anchoring a popup.
fn cursor_screen_yx(app: &App) -> Option<(u16, u16)> {
    let area = app.layout.editor;
    let t = app.editor.active_tab()?;
    let (line, col) = app.editor.cursor_1based();
    let off_y = t.editor.get_offset_y();
    let y = area.y + u16::try_from(line.saturating_sub(1).saturating_sub(off_y)).unwrap_or(0);
    let x = area.x
        + u16::try_from(col.saturating_sub(1))
            .unwrap_or(0)
            .min(area.width);
    Some((
        x.min(area.x + area.width.saturating_sub(1)),
        y.min(area.y + area.height.saturating_sub(1)),
    ))
}

/// Draw the LSP completion popup as a small list anchored at the cursor.
pub(super) fn draw_completion(app: &App, frame: &mut Frame) {
    let Some(popup) = app.completion.as_ref() else {
        return;
    };
    if popup.items.is_empty() {
        return;
    }
    let area = app.layout.editor;
    if area.width < 16 || area.height < 4 {
        return;
    }
    let max_rows = 10.min(popup.items.len());
    // Scroll so the highlighted row stays visible.
    let start = if popup.selected >= max_rows {
        popup.selected + 1 - max_rows
    } else {
        0
    };
    let end = (start + max_rows).min(popup.items.len());

    // Width from the widest visible label (+detail), capped to the area.
    let widest = popup.items[start..end]
        .iter()
        .map(|it| {
            it.label.chars().count() + it.detail.as_ref().map_or(0, |d| d.chars().count() + 3)
        })
        .max()
        .unwrap_or(10);
    let width =
        (u16::try_from(widest).unwrap_or(u16::MAX) + 2).clamp(16, area.width.saturating_sub(2));
    let height = u16::try_from(max_rows).unwrap_or(u16::MAX) + 2;

    let (cx, cy) = cursor_screen_yx(app).unwrap_or((area.x + 2, area.y));
    // Below the cursor if it fits, else above.
    let y = if cy + 1 + height <= area.y + area.height {
        cy + 1
    } else {
        cy.saturating_sub(height)
    };
    let x = cx.min(area.x + area.width.saturating_sub(width));
    let rect = Rect {
        x,
        y,
        width,
        height,
    };

    frame.render_widget(Clear, rect);
    let rows: Vec<ListItem> = popup.items[start..end]
        .iter()
        .map(|it| {
            let mut spans = vec![Span::raw(it.label.clone())];
            if let Some(detail) = &it.detail {
                spans.push(Span::styled(format!("  {detail}"), theme::dim()));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let list = List::new(rows)
        .block(
            Block::default()
                .style(theme::base())
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::title(true))
                .title(format!(" {} ", t!("ui.completion"))),
        )
        .highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(popup.selected - start));
    frame.render_stateful_widget(list, rect, &mut state);
}

/// Draw the LSP hover tooltip as a wrapped, bordered box near the cursor.
pub(super) fn draw_hover(app: &App, frame: &mut Frame) {
    let Some(h) = app.hover.as_ref() else { return };
    let area = app.layout.editor;
    if area.width < 12 || area.height < 4 {
        return;
    }
    let text = h.text.replace('\r', "");
    let width = 64u16.min(area.width.saturating_sub(2)).max(20);
    let inner_w = width.saturating_sub(2).max(1) as usize;
    // Estimate wrapped height.
    let mut rows = 0usize;
    for line in text.lines() {
        rows += line.chars().count() / inner_w + 1;
    }
    let height = (u16::try_from(rows).unwrap_or(u16::MAX) + 2).clamp(3, 12.min(area.height));

    let (cx, cy) = cursor_screen_yx(app).unwrap_or((area.x + 2, area.y));
    let y = if cy + 1 + height <= area.y + area.height {
        cy + 1
    } else {
        cy.saturating_sub(height)
    };
    let x = cx.min(area.x + area.width.saturating_sub(width));
    let rect = Rect {
        x,
        y,
        width,
        height,
    };

    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.hover")));
    let para = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    frame.render_widget(para, rect);
}

pub(super) fn draw_code_actions(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(menu) = app.code_actions.as_ref() else {
        return;
    };
    let titles: Vec<&str> = menu.actions.iter().map(|(t, _)| t.as_str()).collect();
    draw_chooser(
        frame,
        area,
        &t!("menu.item.lsp.code_action"),
        &titles,
        menu.selected,
    );
}

pub(super) fn draw_code_lens(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(menu) = app.code_lens.as_ref() else {
        return;
    };
    let titles: Vec<&str> = menu.lenses.iter().map(|(_, t, _, _)| t.as_str()).collect();
    draw_chooser(
        frame,
        area,
        &t!("menu.item.lsp.code_lens"),
        &titles,
        menu.selected,
    );
}

/// A centered single-column chooser: a bordered list of `titles` with `selected`
/// highlighted. Shared by the code-action and code-lens menus.
fn draw_chooser(frame: &mut Frame, area: Rect, title: &str, titles: &[&str], selected: usize) {
    let longest = titles.iter().map(|t| t.chars().count()).max().unwrap_or(20);
    let width = u16::try_from(longest)
        .unwrap_or(u16::MAX)
        .saturating_add(4)
        .clamp(24, area.width);
    let rows_n = u16::try_from(titles.len()).unwrap_or(u16::MAX);
    let height = rows_n.saturating_add(2).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {title} "));
    let inner = block.inner(rect);
    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);
    let rows: Vec<Line> = titles
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let style = if i == selected {
                theme::selected()
            } else {
                theme::base()
            };
            Line::from(Span::styled(format!(" {t} "), style))
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), inner);
}
