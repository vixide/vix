//! The center pane's chrome above the editor: the tab bar and the optional
//! breadcrumb bar (active file name + enclosing symbol), plus the layout
//! split that carves their rows out of the center column.

#![warn(clippy::pedantic)]

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

pub(super) fn draw_tabs(app: &App, frame: &mut Frame, area: Rect) {
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
    frame.render_widget(tabs, area);
}
