//! The center pane's chrome above the editor: the tab bar and the optional
//! breadcrumb bar (active file name + enclosing symbol), plus the layout
//! split that carves their rows out of the center column.

use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Tabs};

use crate::app::App;
use crate::theme;

// Split the center column into the tab bar, an optional breadcrumb bar, and the
// editor cell. Returns (tabs, breadcrumb, editor).
pub(super) fn center_split(area: Rect, breadcrumbs: bool) -> (Rect, Option<Rect>, Rect) {
    let dir = Direction::Vertical;
    if breadcrumbs {
        let c = Layout::default()
            .direction(dir)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(area);
        (c[0], Some(c[1]), c[2])
    } else {
        let c = Layout::default()
            .direction(dir)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area);
        (c[0], None, c[1])
    }
}

// Render the breadcrumb bar: the active file name and the enclosing symbol.
pub(super) fn draw_breadcrumb(app: &App, frame: &mut Frame, area: Rect) {
    let line = Line::from(Span::styled(format!(" {}", app.breadcrumb()), theme::dim()));
    frame.render_widget(Paragraph::new(line).style(theme::base()), area);
}

/// The x of the first tab's title: one cell in from the left edge, or, in a
/// mirrored layout, where the tab strip's right-aligned block begins (T564).
pub fn tabs_origin(app: &App, area: Rect) -> u16 {
    if !super::layout_mirrored(app) {
        return area.x + 1;
    }
    let n = app.editor.tabs.len();
    let titles: usize = app
        .editor
        .tabs
        .iter()
        .map(|t| t.title().chars().count())
        .sum();
    // Each title is padded by one cell either side; dividers sit between.
    let total = titles + 3 * n.saturating_sub(1) + 2;
    let total = u16::try_from(total).unwrap_or(u16::MAX).min(area.width);
    area.right().saturating_sub(total) + 1
}

pub(super) fn draw_tabs(app: &App, frame: &mut Frame, area: Rect) {
    let strip = Rect {
        x: tabs_origin(app, area) - 1,
        width: area.right() - (tabs_origin(app, area) - 1),
        ..area
    };
    frame.render_widget(
        Paragraph::new("").style(theme::region_base(theme::Region::Editor)),
        area,
    );
    let titles: Vec<Line> = app
        .editor
        .tabs
        .iter()
        .map(|t| {
            if t.preview {
                Line::from(Span::styled(t.title(), theme::dim()))
            } else {
                Line::from(t.title())
            }
        })
        .collect();
    let tabs = Tabs::new(titles)
        // Paint the bar in the editor region's background; otherwise the Tabs
        // widget resets its area to the terminal default, which shows through as
        // the wrong color (e.g. white) when the theme background differs.
        .style(theme::region_base(theme::Region::Editor))
        .select(app.editor.active)
        // Mark the active tab with an underline rather than reversed video, so it
        // keeps the editor's (e.g. dark) background instead of flipping to a light
        // one.
        .highlight_style(
            theme::region_base(theme::Region::Editor).add_modifier(Modifier::UNDERLINED),
        )
        .divider(Span::styled("│", theme::dim()));
    frame.render_widget(tabs, strip);
}
