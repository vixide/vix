//! The top menu bar and its dropdowns: the bar itself, up to three nested
//! dropdown levels (menu → submenu → subsubmenu), and the help tooltip for
//! the currently-highlighted entry.
//!
//! `dock_toggle_cols`, `menu_dropdown_rect`, and `dropdown_scroll` stay in
//! the parent `ui` module — `App`'s mouse-click handling reaches them via
//! `crate::ui::dropdown_scroll`/`crate::ui::dock_toggle_cols`, a path this
//! module moving would have broken. `dropdown_width` is `pub(super)` for the
//! same reason: `ui::menu_dropdown_rect` (also staying) still calls it.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use super::{draw_scrollbar, dropdown_scroll};
use crate::app::App;
use crate::menu::menus;
use crate::theme::{self, icon};

pub(super) fn draw_menu_bar(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = Vec::new();
    for (i, m) in menus().iter().enumerate() {
        let open = app.menu.open == Some(i);
        let style = if open {
            theme::selected()
        } else {
            Style::default().fg(theme::region_fg(theme::Region::MenuBar))
        };
        spans.push(Span::styled(format!(" {} ", m.title()), style));
    }
    let bar = Paragraph::new(Line::from(spans)).style(theme::region_base(theme::Region::MenuBar));
    frame.render_widget(bar, area);

    // Right-aligned dock open/close toggles: bright when open, dim when closed.
    // Click handling lives in `App::menu_click` (see `dock_toggle_cols`).
    let dock_style = |open: bool| {
        if open {
            theme::title(true)
        } else {
            theme::dim()
        }
    };
    let docks = Line::from(vec![
        Span::styled(icon::FOLDER, dock_style(app.show_explorer)),
        Span::raw(" "),
        Span::styled(icon::BELL, dock_style(app.show_messages)),
        Span::raw(" "),
    ]);
    frame.render_widget(
        Paragraph::new(docks)
            .alignment(Alignment::Right)
            .style(theme::region_base(theme::Region::MenuBar)),
        area,
    );
}

/// Right-aligned indicator for a dropdown item: a `▸` arrow for a submenu parent,
/// otherwise its keyboard shortcut.
fn item_right(it: &crate::menu::Item) -> String {
    if it.has_submenu() {
        "\u{25b8}".to_string()
    } else {
        it.shortcut.to_string()
    }
}

/// Width budget for a dropdown holding `items`: 2 borders + leading + trailing
/// space (= 4), plus a 1-column gap so the label and the right indicator never
/// touch. Minimum 14.
pub(super) fn dropdown_width(items: &[crate::menu::Item]) -> u16 {
    let w = items
        .iter()
        .map(|it| {
            if it.is_separator() {
                return 0;
            }
            let right = item_right(it).chars().count();
            let gap = usize::from(it.has_submenu() || !it.shortcut.is_empty());
            it.label().chars().count() + right + 4 + gap
        })
        .max()
        .unwrap_or(12)
        .max(14);
    u16::try_from(w).unwrap_or(u16::MAX)
}

/// Render one dropdown (Clear + bordered list) at `area`, highlighting `selected`
/// and scrolling so it stays visible. A `●` scrollbar marks the right edge when
/// the items overflow.
fn render_dropdown(
    frame: &mut Frame,
    area: Rect,
    items: &[crate::menu::Item],
    selected: Option<usize>,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let inner_h = inner.height as usize;
    let offset = dropdown_scroll(selected, inner_h, items.len());
    let end = (offset + inner_h).min(items.len());
    let text_w = inner.width as usize;
    let rows: Vec<Line> = items[offset..end]
        .iter()
        .enumerate()
        .map(|(vis, it)| {
            if it.is_separator() {
                return Line::from(Span::styled("─".repeat(text_w), theme::dim()));
            }
            let label = it.label();
            let right = item_right(it);
            let pad = text_w.saturating_sub(label.chars().count() + right.chars().count() + 2);
            let style = if selected == Some(offset + vis) {
                theme::selected()
            } else {
                theme::base()
            };
            Line::from(vec![
                Span::styled(format!(" {label}"), style),
                Span::styled(" ".repeat(pad), style),
                Span::styled(
                    format!("{right} "),
                    if selected == Some(offset + vis) {
                        style
                    } else {
                        theme::dim()
                    },
                ),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), inner);

    if items.len() > inner_h {
        // Draw the thumb over the right border column.
        let sb = Rect {
            x: area.x + area.width - 1,
            y: inner.y,
            width: 1,
            height: inner.height,
        };
        draw_scrollbar(
            frame,
            sb,
            selected.unwrap_or(0),
            items.len().saturating_sub(1),
        );
    }
}

pub(super) fn draw_menu_dropdown(app: &mut App, frame: &mut Frame) {
    let Some(i) = app.menu.open else { return };
    let area = app.layout.menu_dropdown;
    render_dropdown(frame, area, menus()[i].items, app.menu.item);

    // An open submenu is drawn to the right of its parent item. It may be open
    // with nothing highlighted yet (`app.menu.sub == None`).
    if app.menu.submenu_open()
        && let Some(subitems) = app.menu.submenu_items()
    {
        let fa = frame.area();
        let sub_w = dropdown_width(subitems);
        let sub_h = u16::try_from(subitems.len()).unwrap_or(u16::MAX) + 2;
        let sub_x = (area.x + area.width).min(fa.width.saturating_sub(sub_w));
        let parent_row = u16::try_from(app.menu.item.unwrap_or(0)).unwrap_or(u16::MAX);
        let sub_y = (area.y + parent_row).min(fa.height.saturating_sub(sub_h));
        let sub_area = Rect {
            x: sub_x,
            y: sub_y,
            width: sub_w.min(fa.width),
            height: sub_h.min(fa.height.saturating_sub(sub_y)),
        };
        app.layout.submenu_dropdown = sub_area;
        render_dropdown(frame, sub_area, subitems, app.menu.sub);

        // A third-level submenu is drawn to the right of its parent row.
        if app.menu.subsubmenu_open()
            && let Some(ssitems) = app.menu.subsubmenu_items()
        {
            let ss_w = dropdown_width(ssitems);
            let ss_h = u16::try_from(ssitems.len()).unwrap_or(u16::MAX) + 2;
            let ss_x = (sub_area.x + sub_area.width).min(fa.width.saturating_sub(ss_w));
            let prow = u16::try_from(app.menu.sub.unwrap_or(0)).unwrap_or(u16::MAX);
            let ss_y = (sub_area.y + prow).min(fa.height.saturating_sub(ss_h));
            let ss_area = Rect {
                x: ss_x,
                y: ss_y,
                width: ss_w.min(fa.width),
                height: ss_h.min(fa.height.saturating_sub(ss_y)),
            };
            app.layout.subsubmenu_dropdown = ss_area;
            render_dropdown(frame, ss_area, ssitems, app.menu.subsub);
        }
    }
    draw_menu_tooltip(app, frame);
}

/// Absolute screen row of dropdown item `idx` in dropdown `rect`, accounting for
/// scroll given the current `selected` highlight and item count `len`.
fn menu_row_y(rect: Rect, idx: usize, selected: Option<usize>, len: usize) -> u16 {
    let inner_h = rect.height.saturating_sub(2) as usize;
    let offset = dropdown_scroll(selected, inner_h, len);
    rect.y + 1 + u16::try_from(idx.saturating_sub(offset)).unwrap_or(0)
}

/// The deepest currently-highlighted menu entry's help text, the dropdown it
/// lives in, and the screen row of that entry. When a menu is open but nothing is
/// highlighted yet, falls back to the top-level menu's own help at the dropdown's
/// top. `None` when there is no help to show.
fn menu_tooltip_target(app: &App) -> Option<(String, Rect, u16)> {
    let mi = app.menu.open?;
    if app.menu.subsubmenu_open()
        && let Some(items) = app.menu.subsubmenu_items()
        && let Some(idx) = app.menu.subsub
    {
        let rect = app.layout.subsubmenu_dropdown;
        return items
            .get(idx)
            .and_then(crate::menu::Item::help)
            .map(|h| (h, rect, menu_row_y(rect, idx, app.menu.subsub, items.len())));
    }
    if app.menu.submenu_open()
        && let Some(items) = app.menu.submenu_items()
        && let Some(idx) = app.menu.sub
    {
        let rect = app.layout.submenu_dropdown;
        return items
            .get(idx)
            .and_then(crate::menu::Item::help)
            .map(|h| (h, rect, menu_row_y(rect, idx, app.menu.sub, items.len())));
    }
    if let Some(idx) = app.menu.item {
        let items = menus()[mi].items;
        let rect = app.layout.menu_dropdown;
        return items
            .get(idx)
            .and_then(crate::menu::Item::help)
            .map(|h| (h, rect, menu_row_y(rect, idx, app.menu.item, items.len())));
    }
    let rect = app.layout.menu_dropdown;
    menus()[mi].help().map(|h| (h, rect, rect.y))
}

/// Draw a help tooltip for the deepest currently-highlighted menu entry, placed
/// beside its dropdown (to the right, or the left when there is no room) and
/// aligned with the highlighted row.
fn draw_menu_tooltip(app: &App, frame: &mut Frame) {
    if !app.settings.show_menu_tooltips {
        return;
    }
    let Some((text, anchor, row_y)) = menu_tooltip_target(app) else {
        return;
    };
    let fa = frame.area();
    if fa.width < 16 {
        return;
    }
    let width = u16::try_from(text.chars().count() + 2)
        .unwrap_or(u16::MAX)
        .clamp(12, 44)
        .min(fa.width);
    let inner_w = width.saturating_sub(2).max(1) as usize;
    let rows = text.chars().count().div_ceil(inner_w).max(1);
    let height = (u16::try_from(rows).unwrap_or(u16::MAX) + 2).min(fa.height);
    // Prefer the right of the dropdown; fall back to its left; else clamp.
    let x = if anchor.x + anchor.width + width <= fa.width {
        anchor.x + anchor.width
    } else if anchor.x >= width {
        anchor.x - width
    } else {
        fa.width.saturating_sub(width)
    };
    let y = row_y.min(fa.height.saturating_sub(height));
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
        .border_style(theme::title(true));
    let para = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    frame.render_widget(para, rect);
}
