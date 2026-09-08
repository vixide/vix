//! The core editor rendering: the editor region (single pane or a split
//! tree), one pane's text + scrollbar + ruler guide, and the un-split
//! "center" case (text area with an optional minimap and horizontal
//! scrollbar).

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};
use ratatui_image::StatefulImage;
use ratatui_image::protocol::StatefulProtocol;

use super::{RULER_COLUMN, draw_hscrollbar, draw_minimap, draw_scrollbar};
use crate::app::App;
use crate::theme;

/// Width (cells) of the code-overview minimap column.
const MINIMAP_WIDTH: u16 = 16;

/// Render the editor region: a single pane, or two split panes with a divider.
pub(super) fn draw_editor_region(app: &mut App, frame: &mut Frame, inner: Rect) {
    use crate::editor::SplitDir;
    app.layout.editor_region = inner;
    let panes = app.editor.split_layout(inner);
    if panes.is_empty() {
        // Carve a minimap column from the right edge (when enabled and there's
        // room); the remainder splits into editor text + scrollbar as before.
        let (inner, minimap_area) = if app.settings.show_minimap && inner.width > 40 {
            let s = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(1), Constraint::Length(MINIMAP_WIDTH)])
                .split(inner);
            (s[0], s[1])
        } else {
            (
                inner,
                Rect {
                    width: 0,
                    height: 0,
                    ..inner
                },
            )
        };
        app.layout.minimap = minimap_area;
        let (editor_area, scrollbar_area) = if app.show_scrollbar {
            let s = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(inner);
            (s[0], s[1])
        } else {
            (
                inner,
                Rect {
                    width: 0,
                    height: 0,
                    ..inner
                },
            )
        };
        // Sticky scroll: reserve the top row for the enclosing scope's header.
        let header = if editor_area.height > 1 {
            app.sticky_header()
        } else {
            None
        };
        let (header_area, editor_area) = match &header {
            Some(_) => {
                let r = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Min(1)])
                    .split(editor_area);
                (Some(r[0]), r[1])
            }
            None => (None, editor_area),
        };
        app.layout.editor = editor_area;
        app.layout.scrollbar = scrollbar_area;
        draw_center(app, frame, editor_area, scrollbar_area);
        if let (Some(hrect), Some(text)) = (header_area, header) {
            let style = theme::region_title(theme::Region::Editor, true);
            frame.render_widget(Block::default().style(style), hrect);
            frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), hrect);
        }
        if minimap_area.width > 0 {
            draw_minimap(app, frame, minimap_area, editor_area.height as usize);
        }
        return;
    }

    // Draw split dividers (one per internal tree node).
    let dstyle = theme::region_title(theme::Region::Editor, true);
    for (dir, divider) in app.editor.split_dividers(inner) {
        if dir == SplitDir::Vertical {
            let col: Vec<Line> = (0..divider.height)
                .map(|_| Line::from(Span::styled("│", dstyle)))
                .collect();
            frame.render_widget(Paragraph::new(col), divider);
        } else {
            let row = "─".repeat(divider.width as usize);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(row, dstyle))),
                divider,
            );
        }
    }

    // Draw each pane; the focused leaf drives cursor/mouse mapping.
    let focused = app.editor.focused_leaf();
    let mut focused_rect = inner;
    for pane in panes {
        let text = draw_pane(app, frame, pane.rect, pane.tab);
        if pane.leaf == focused {
            focused_rect = text;
        }
    }
    app.layout.editor = focused_rect;
    app.layout.scrollbar = Rect::default();
    app.layout.editor_hscrollbar = Rect::default();
}

/// Render one split pane (tab `tab_index`) into `area` with its own vertical
/// scrollbar; returns the text rectangle (for mouse hit-testing).
fn draw_pane(app: &mut App, frame: &mut Frame, area: Rect, tab_index: usize) -> Rect {
    let (text, sb) = if app.show_scrollbar && area.width > 1 {
        let s = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        (s[0], s[1])
    } else {
        (
            area,
            Rect {
                width: 0,
                height: 0,
                ..area
            },
        )
    };

    if app
        .editor
        .tabs
        .get(tab_index)
        .is_some_and(crate::editor::Tab::is_image)
    {
        if let Some(tab) = app.editor.tabs.get_mut(tab_index)
            && let Some(proto) = tab.image.as_mut()
        {
            frame.render_stateful_widget(StatefulImage::<StatefulProtocol>::new(), text, proto);
        }
        return text;
    }
    if let Some(tab) = app.editor.tabs.get(tab_index) {
        frame.render_widget(&tab.editor, text);
        if app.show_ruler {
            tint_ruler(frame, text, &tab.editor);
        }
        if sb.width > 0 {
            let total = tab.line_count().max(1);
            let pos = tab.cursor_1based().0.saturating_sub(1);
            draw_scrollbar(frame, sb, pos, total.saturating_sub(1));
        }
    }
    text
}

/// Draw a faint vertical guide at [`RULER_COLUMN`] over an already-rendered
/// editor pane, accounting for the line-number gutter and horizontal scroll.
fn tint_ruler(frame: &mut Frame, text: Rect, editor: &crate::editor::CodeEditor) {
    let off = editor.get_offset_x();
    if RULER_COLUMN < off {
        return; // scrolled past the guide
    }
    let gutter = u16::try_from(editor.gutter_width()).unwrap_or(u16::MAX);
    let Ok(rel) = u16::try_from(RULER_COLUMN - off) else {
        return;
    };
    let x = text.x + gutter + rel;
    if x < text.x || x >= text.x + text.width {
        return;
    }
    let buf = frame.buffer_mut();
    for y in text.y..text.y + text.height {
        if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(x, y)) {
            if cell.symbol() == " " {
                cell.set_symbol("│");
            }
            cell.set_style(theme::dim());
        }
    }
}

fn draw_center(app: &mut App, frame: &mut Frame, text: Rect, scrollbar: Rect) {
    let is_image = app
        .editor
        .active_tab()
        .is_some_and(crate::editor::Tab::is_image);
    if is_image {
        if let Some(tab) = app.editor.active_tab_mut()
            && let Some(proto) = tab.image.as_mut()
        {
            frame.render_stateful_widget(StatefulImage::<StatefulProtocol>::new(), text, proto);
        }
        return;
    }
    let mut hbar_rect: Option<Rect> = None;
    if let Some(tab) = app.editor.active_tab() {
        let soft = tab.editor.soft_wrap_enabled();
        let gutter = tab.editor.gutter_width();
        let maxw = tab.editor.max_line_width();
        let off = tab.editor.get_offset_x();
        let text_visible = (text.width as usize).saturating_sub(gutter);
        // A horizontal scrollbar appears when not soft-wrapping and a line
        // overflows the visible text width (and the scrollbar is enabled).
        let hbar = app.settings.show_scrollbar && !soft && text.height > 1 && maxw > text_visible;
        let editor_area = if hbar {
            Rect {
                height: text.height - 1,
                ..text
            }
        } else {
            text
        };
        let vsb = if hbar {
            Rect {
                height: scrollbar.height.saturating_sub(1),
                ..scrollbar
            }
        } else {
            scrollbar
        };
        frame.render_widget(&tab.editor, editor_area);

        if vsb.width > 0 {
            let total = app.editor.active_line_count().max(1);
            let pos = app.editor.cursor_1based().0.saturating_sub(1);
            draw_scrollbar(frame, vsb, pos, total.saturating_sub(1));
        }
        if hbar {
            let gutter_w = u16::try_from(gutter).unwrap_or(u16::MAX);
            let hb = Rect {
                x: text.x + gutter_w,
                y: text.y + text.height - 1,
                width: text.width - gutter_w,
                height: 1,
            };
            draw_hscrollbar(frame, hb, off, maxw.saturating_sub(text_visible));
            hbar_rect = Some(hb);
            app.layout.editor = editor_area;
            app.editor_hmax = maxw.saturating_sub(text_visible);
        }
    }
    if hbar_rect.is_none() {
        app.editor_hmax = 0;
    }
    app.layout.editor_hscrollbar = hbar_rect.unwrap_or_default();
}
