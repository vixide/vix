//! The file explorer (left dock): the directory tree, flattened to rows,
//! with git-status coloring, vertical and horizontal scroll.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState};

use super::{draw_hscrollbar, draw_scrollbar, git_change_color, hslice_spans, span_line_width};
use crate::app::{App, Focus};
use crate::theme::{self, icon};

/// Build the styled spans for explorer rows `top..end`: indent, type glyph,
/// optional cut-dimming and mark dot, and a trailing git-change letter. Split
/// out of [`draw_explorer`] to keep it within the line limit.
fn explorer_rows(app: &App, top: usize, end: usize) -> Vec<Vec<Span<'static>>> {
    app.explorer.nodes[top..end]
        .iter()
        .map(|n| {
            let indent = "  ".repeat(n.depth);
            let glyph = if n.is_symlink {
                icon::LINK
            } else if n.is_dir {
                if n.expanded {
                    icon::FOLDER_OPEN
                } else {
                    icon::FOLDER
                }
            } else {
                theme::file_icon(&n.name)
            };
            let mut style = Style::default();
            let cut_pending = app.clip_cut && app.clip.contains(&n.path);
            if cut_pending {
                style = style.add_modifier(Modifier::DIM);
            }
            let mark = if app.explorer.marked.contains(&n.path) {
                "● "
            } else {
                ""
            };
            let mut spans = vec![
                Span::raw(indent),
                Span::styled(format!("{mark}{glyph} {}", n.name), style),
            ];
            if !n.is_dir
                && let Some(change) = app.git_change_for(&n.path)
            {
                spans.push(Span::styled(
                    format!("  {}", change.letter()),
                    Style::default().fg(git_change_color(change)),
                ));
            }
            spans
        })
        .collect()
}

pub(super) fn draw_explorer(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Explorer;
    let block = Block::default()
        .style(theme::region_base(theme::Region::LeftDock))
        // The left dock keeps only its top and right borders.
        .borders(Borders::TOP | Borders::RIGHT)
        .border_type(BorderType::Rounded)
        .border_style(theme::region_title(theme::Region::LeftDock, focused))
        .title(if app.explorer.has_filter() {
            format!(
                " {} {}  {} ",
                icon::FOLDER,
                t!("ui.explorer"),
                t!("ui.explorer_filtered")
            )
        } else {
            format!(" {} {} ", icon::FOLDER, t!("ui.explorer"))
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let total = app.explorer.nodes.len();
    let allow_bars = app.settings.show_scrollbar && inner.width > 1 && inner.height > 1;
    let vbar = allow_bars && total > inner.height as usize;
    let text_w = if vbar { inner.width - 1 } else { inner.width } as usize;

    let top = app.explorer.top.min(total);
    // Build the full (unsliced) styled rows for the visible window.
    let win_h = inner.height as usize;
    let end = (top + win_h).min(total);
    let rows = explorer_rows(app, top, end);

    let content_w = rows.iter().map(|s| span_line_width(s)).max().unwrap_or(0);
    let hbar = allow_bars && content_w > text_w;
    let body_h = if hbar { inner.height - 1 } else { inner.height } as usize;
    let hmax = content_w.saturating_sub(text_w);
    app.explorer_hmax = hmax;
    app.explorer_hscroll = app.explorer_hscroll.min(hmax);
    let off = app.explorer_hscroll;

    let visible = body_h.min(rows.len());
    let items: Vec<ListItem> = rows[..visible]
        .iter()
        .map(|spans| ListItem::new(Line::from(hslice_spans(spans, off, text_w))))
        .collect();
    let list_area = Rect {
        width: u16::try_from(text_w).unwrap_or(u16::MAX),
        height: u16::try_from(body_h).unwrap_or(u16::MAX),
        ..inner
    };
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    if app.explorer.selected >= top && app.explorer.selected < top + visible {
        state.select(Some(app.explorer.selected - top));
    }
    frame.render_stateful_widget(list, list_area, &mut state);

    if vbar {
        let sb = Rect {
            x: inner.x + inner.width - 1,
            y: inner.y,
            width: 1,
            height: u16::try_from(body_h).unwrap_or(u16::MAX),
        };
        draw_scrollbar(frame, sb, app.explorer.selected, total.saturating_sub(1));
    }
    app.layout.explorer_hscrollbar = if hbar {
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
