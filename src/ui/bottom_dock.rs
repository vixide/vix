//! The bottom dock: the output/log strip beneath the main body, with
//! vertical and horizontal scroll.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::{draw_hscrollbar, draw_scrollbar};
use crate::app::{App, Focus};
use crate::theme::{self, icon};

/// The longest line width (in chars) among `lines`.
fn max_line_width<'a>(lines: impl Iterator<Item = &'a str>) -> usize {
    lines.map(|l| l.chars().count()).max().unwrap_or(0)
}

/// Slice `line` to the horizontal window `[offset, offset + width)` by character.
fn hslice(line: &str, offset: usize, width: usize) -> String {
    line.chars().skip(offset).take(width).collect()
}

pub(super) fn draw_bottom_dock(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::BottomDock;
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::TOP)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(focused))
        .title(format!(" {} {} ", icon::INFO, t!("ui.bottom_dock")));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let total = app.bottom_dock.lines.len();
    let h = inner.height as usize;
    let allow_bars = app.settings.show_scrollbar && inner.width > 1 && inner.height > 1;
    let vbar = allow_bars && total > h;

    // The visible rows, and whether they overflow horizontally → a bottom hbar.
    let view_h = if allow_bars {
        inner.height.saturating_sub(1)
    } else {
        inner.height
    } as usize;
    let visible: Vec<String> = app.bottom_dock.visible(view_h).to_vec();
    let content_w = max_line_width(visible.iter().map(String::as_str));
    let text_w_full = if vbar { inner.width - 1 } else { inner.width } as usize;
    let hbar = allow_bars && content_w > text_w_full;

    let text_w = text_w_full;
    let body_h = if hbar { inner.height - 1 } else { inner.height };
    let text_area = Rect {
        width: u16::try_from(text_w).unwrap_or(u16::MAX),
        height: body_h,
        ..inner
    };

    let hmax = content_w.saturating_sub(text_w);
    app.bottom_hmax = hmax;
    app.bottom_hscroll = app.bottom_hscroll.min(hmax);
    let off = app.bottom_hscroll;

    let lines: Vec<Line> = if app.bottom_dock.is_empty() {
        vec![Line::from(Span::styled(
            t!("ui.bottom_dock_empty").to_string(),
            theme::dim(),
        ))]
    } else {
        visible
            .iter()
            .map(|l| Line::from(hslice(l, off, text_w)))
            .collect()
    };
    frame.render_widget(Paragraph::new(lines), text_area);

    if vbar {
        let sb = Rect {
            x: inner.x + inner.width - 1,
            y: inner.y,
            width: 1,
            height: body_h,
        };
        draw_scrollbar(
            frame,
            sb,
            app.bottom_dock.scroll,
            total.saturating_sub(view_h),
        );
    }
    app.layout.bottom_hscrollbar = if hbar {
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
