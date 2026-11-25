//! The core editor rendering: the editor region (single pane or a split
//! tree), one pane's text + scrollbar + ruler guide, and the un-split
//! "center" case (text area with an optional minimap and horizontal
//! scrollbar).

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

use crate::geo::{Geo, chunk};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

use super::ghost::draw_ghost;
#[cfg(feature = "git")]
use super::review_panes::draw_review_panes;

/// Without the `git` feature there is no branch review to draw.
#[cfg(not(feature = "git"))]
fn draw_review_panes(_app: &App, _frame: &mut Frame, _area: Rect, _tab: usize) -> bool {
    false
}
use super::scroll_markers;
use super::{RULER_COLUMN, draw_hscrollbar, draw_minimap, draw_scrollbar_marked};
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
        let (inner, minimap_area) = if app.settings.viewport.show_minimap && inner.width > 40 {
            let s = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(1), Constraint::Length(MINIMAP_WIDTH)])
                .split(inner);
            (chunk(&s, 0), chunk(&s, 1))
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
        let (editor_area, scrollbar_area) = if app.visible.contains(crate::app::Visible::SCROLLBAR)
        {
            let mirrored = super::layout_mirrored(app);
            let s = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(if mirrored {
                    [Constraint::Length(1), Constraint::Min(1)]
                } else {
                    [Constraint::Min(1), Constraint::Length(1)]
                })
                .split(inner);
            (
                chunk(&s, usize::from(mirrored)),
                chunk(&s, usize::from(!mirrored)),
            )
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
                (Some(chunk(&r, 0)), chunk(&r, 1))
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
            draw_minimap(app, frame, minimap_area, usize::from(editor_area.height));
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
            let row = "─".repeat(usize::from(divider.width));
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
    let (text, sb) = if app.visible.contains(crate::app::Visible::SCROLLBAR) && area.width > 1 {
        let s = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        (chunk(&s, 0), chunk(&s, 1))
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
            render_image(frame, text, proto);
        }
        return text;
    }
    if draw_review_panes(app, frame, text, tab_index) {
        return text;
    }
    if let Some(tab) = app.editor.tabs.get(tab_index) {
        frame.render_widget(&tab.editor, text);
        if tab_index == app.editor.active {
            draw_ghost(app, frame, text);
        }
        if app.visible.contains(crate::app::Visible::RULER) {
            tint_ruler(frame, text, &tab.editor);
        }
        if sb.width > 0 {
            let total = tab.line_count().max(1);
            let pos = tab.cursor_1based().0.minus(1);
            let markers = scrollbar_markers(app, tab, total, sb.height);
            draw_scrollbar_marked(frame, sb, pos, total.minus(1), &markers);
        }
    }
    text
}

/// The per-row marker colors for `tab`'s vertical scroll bar, or none when
/// `show_scrollbar_markers` is off (T578).
fn scrollbar_markers(
    app: &App,
    tab: &crate::editor::Tab,
    total_lines: usize,
    height: u16,
) -> Vec<Option<Color>> {
    if !app.settings.scrollbar.show_scrollbar_markers {
        return Vec::new();
    }
    scroll_markers::bucket(
        &scroll_markers::collect(tab),
        total_lines,
        usize::from(height),
    )
}

/// Draw a faint vertical guide at [`RULER_COLUMN`] over an already-rendered
/// editor pane, accounting for the line-number gutter and horizontal scroll.
fn tint_ruler(frame: &mut Frame, text: Rect, editor: &crate::editor::CodeEditor) {
    let off = editor.get_offset_x();
    if RULER_COLUMN < off {
        return; // scrolled past the guide
    }
    let gutter = u16::try_from(editor.gutter_width()).unwrap_or(u16::MAX);
    let Ok(rel) = u16::try_from(RULER_COLUMN.minus(off)) else {
        return;
    };
    let x = text.x.plus(gutter).plus(rel);
    if x < text.x || x >= text.x.plus(text.width) {
        return;
    }
    let buf = frame.buffer_mut();
    for y in text.y..text.y.plus(text.height) {
        if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(x, y)) {
            if cell.symbol() == " " {
                cell.set_symbol("│");
            }
            cell.set_style(theme::dim());
        }
    }
}

/// Draw the centre column's content: an image preview for image tabs,
/// otherwise the text editor with its scrollbar.
fn draw_center(app: &mut App, frame: &mut Frame, text: Rect, scrollbar: Rect) {
    let is_image = app
        .editor
        .active_tab()
        .is_some_and(crate::editor::Tab::is_image);
    if is_image {
        if let Some(tab) = app.editor.active_tab_mut()
            && let Some(proto) = tab.image.as_mut()
        {
            render_image(frame, text, proto);
        }
        return;
    }
    // The review's two-pane view replaces the text editor (T665).
    if draw_review_panes(app, frame, text, app.editor.active) {
        app.editor_hmax = 0;
        app.layout.editor_hscrollbar = Rect::default();
        return;
    }
    let mut hbar_rect: Option<Rect> = None;
    if let Some(tab) = app.editor.active_tab() {
        let soft = tab.editor.soft_wrap_enabled();
        let gutter = tab.editor.gutter_width();
        let maxw = tab.editor.max_line_width();
        let off = tab.editor.get_offset_x();
        let text_visible = (usize::from(text.width)).minus(gutter);
        // A horizontal scrollbar appears when not soft-wrapping and a line
        // overflows the visible text width (and the scrollbar is enabled).
        let hbar =
            app.settings.viewport.show_scrollbar && !soft && text.height > 1 && maxw > text_visible;
        let editor_area = if hbar {
            Rect {
                height: text.height.minus(1),
                ..text
            }
        } else {
            text
        };
        let vsb = if hbar {
            Rect {
                height: scrollbar.height.minus(1),
                ..scrollbar
            }
        } else {
            scrollbar
        };
        frame.render_widget(&tab.editor, editor_area);
        draw_ghost(app, frame, editor_area);

        if vsb.width > 0 {
            let total = app.editor.active_line_count().max(1);
            let pos = app.editor.cursor_1based().0.minus(1);
            let markers = scrollbar_markers(app, tab, total, vsb.height);
            draw_scrollbar_marked(frame, vsb, pos, total.minus(1), &markers);
        }
        if hbar {
            let gutter_w = u16::try_from(gutter).unwrap_or(u16::MAX);
            // Saturating: a window narrower than the gutter has no scrollable
            // width left (T575).
            let hb = Rect {
                x: text.x.plus(gutter_w),
                y: text.y.plus(text.height).minus(1),
                width: text.width.minus(gutter_w),
                height: 1,
            };
            draw_hscrollbar(frame, hb, off, maxw.minus(text_visible));
            hbar_rect = Some(hb);
            app.layout.editor = editor_area;
            app.editor_hmax = maxw.minus(text_visible);
        }
    }
    if hbar_rect.is_none() {
        app.editor_hmax = 0;
    }
    app.layout.editor_hscrollbar = hbar_rect.unwrap_or_default();
}

/// Draw an image tab's terminal-graphics protocol into `area`.
#[cfg(feature = "image")]
fn render_image(frame: &mut Frame, area: Rect, proto: &mut crate::editor::ImageProtocol) {
    frame.render_stateful_widget(
        ratatui_image::StatefulImage::<crate::editor::ImageProtocol>::new(),
        area,
        proto,
    );
}

/// Without the `image` feature no image tab can exist (`ImageProtocol` is
/// uninhabited), so there is nothing to draw.
#[cfg(not(feature = "image"))]
fn render_image(_frame: &mut Frame, _area: Rect, proto: &mut crate::editor::ImageProtocol) {
    match *proto {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Visible;
    use crate::app::cov4::{app_at, temp};
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    fn split_app(tag: &str) -> App {
        let mut app = app_at(&temp(tag));
        app.editor
            .new_tab_with_content(&format!("{}\nshort\n", "w".repeat(120)));
        app.run_action("view.split_vertical");
        assert!(app.editor.is_split());
        app.visible.set(Visible::EXPLORER, false);
        app.visible.set(Visible::MESSAGES, false);
        app
    }

    fn draw(app: &mut App, w: u16, h: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| crate::ui::draw(app, f)).unwrap();
        terminal.backend().buffer().clone()
    }

    /// Without a scrollbar a split pane keeps its whole width, and the layout
    /// reports no scrollbar rectangles.
    #[test]
    fn split_panes_without_a_scrollbar_use_the_full_width() {
        let mut with_bar = split_app("r3-split-bar");
        let mut without = split_app("r3-split-nobar");
        without.visible.set(Visible::SCROLLBAR, false);
        draw(&mut with_bar, 100, 24);
        draw(&mut without, 100, 24);
        assert!(without.layout.editor.width > with_bar.layout.editor.width);
        assert_eq!(without.layout.scrollbar, Rect::default());
        assert_eq!(without.layout.editor_hscrollbar, Rect::default());
    }

    /// The ruler guide is drawn in split panes over empty cells at the ruler
    /// column (past the gutter), and not at all once the pane is scrolled
    /// past it or is too narrow to reach it.
    #[test]
    fn the_ruler_guide_is_drawn_in_split_panes_until_scrolled_past() {
        let mut app = split_app("r3-ruler");
        app.visible.set(Visible::RULER, false);
        let off = draw(&mut app, 200, 24);
        app.visible.set(Visible::RULER, true);
        let on = draw(&mut app, 200, 24);
        let changed: Vec<(u16, u16)> = (0..24)
            .flat_map(|y| (0..200).map(move |x| (x, y)))
            .filter(|&(x, y)| on[(x, y)].symbol() != off[(x, y)].symbol())
            .collect();
        assert!(!changed.is_empty(), "the guide is drawn");
        assert!(
            changed
                .iter()
                .all(|&(x, y)| on[(x, y)].symbol() == "│" && y > 0)
        );
        let columns: std::collections::BTreeSet<u16> = changed.iter().map(|&(x, _)| x).collect();
        assert_eq!(columns.len(), 2, "one vertical guide per pane: {columns:?}");
        let gutter = u16::try_from(app.editor.active_tab().unwrap().editor.gutter_width()).unwrap();
        let focused_guide = usize::from(app.layout.editor.x) + usize::from(gutter) + RULER_COLUMN;
        assert!(
            columns.iter().any(|&x| usize::from(x) == focused_guide),
            "{columns:?} vs {focused_guide}"
        );

        // Scrolled past the ruler column: on and off look the same.
        for tab in &mut app.editor.tabs {
            tab.editor.set_offset_x(RULER_COLUMN + 10);
        }
        app.visible.set(Visible::RULER, false);
        let past_off = draw(&mut app, 200, 24);
        app.visible.set(Visible::RULER, true);
        let past_on = draw(&mut app, 200, 24);
        assert_eq!(past_on, past_off);
        // A pane too narrow to reach the column draws no guide either.
        for tab in &mut app.editor.tabs {
            tab.editor.set_offset_x(0);
        }
        let narrow_on = draw(&mut app, 60, 24);
        app.visible.set(Visible::RULER, false);
        let narrow_off = draw(&mut app, 60, 24);
        assert_eq!(narrow_on, narrow_off);
    }

    /// An image in a split pane renders through its protocol without
    /// disturbing the pane layout.
    #[cfg(feature = "image")]
    #[test]
    fn an_image_in_a_split_pane_renders_and_returns_its_text_area() {
        let mut app = split_app("r3-split-image");
        let picker = ratatui_image::picker::Picker::halfblocks();
        let proto = picker.new_resize_protocol(image::DynamicImage::new_rgb8(8, 8));
        app.editor
            .open_image(std::path::Path::new("/tmp/vix-r3.png"), proto);
        assert!(app.editor.active_tab().unwrap().is_image());
        let buf = draw(&mut app, 100, 24);
        assert!(app.layout.editor.width > 0);
        assert_eq!(buf.area.width, 100);
    }
}
