//! Small floating info boxes anchored near the top-right corner: the
//! calendar (Tools → Calendar…), the clock (Tools → Clock…), and the
//! workspace dashboard (Tools → Workspace Dashboard…).

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::app::App;
use crate::calendar;
use crate::clock;
use crate::theme::{self, icon};

/// Previous-month arrow glyph, at column 0 of the calendar's month-header row.
pub const CAL_PREV: char = '\u{25c0}';
/// Next-month arrow glyph, at column 20 of the calendar's month-header row.
pub const CAL_NEXT: char = '\u{25b6}';

pub(super) fn draw_calendar(app: &mut App, frame: &mut Frame, area: Rect) {
    // The month area follows the user's navigation (see `App::calendar`). Live
    // date/time strings now live in the separate clock box (Tools → Clock…).
    let width = 28u16.min(area.width);
    let height = 11u16.min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width + 1),
        y: area.y + 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::CALENDAR, t!("ui.calendar")));
    let inner = block.inner(rect);
    // Record the inner rect so a click can hit-test the month-nav arrows and day
    // cells.
    app.layout.calendar = inner;
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // month header + nav arrows
            Constraint::Min(6),    // weekday header + weeks
            Constraint::Length(1), // help
        ])
        .split(inner);

    // Month header: a left arrow, the centered month title, and a right arrow
    // (`◀`/`▶`). The arrows are clickable (see `App::calendar_mouse`) and mirror
    // the Left/Right keys.
    let header = Line::from(format!("{CAL_PREV}{:^19}{CAL_NEXT}", app.calendar.title()));
    frame.render_widget(Paragraph::new(header), rows[0]);
    frame.render_widget(Paragraph::new(month_lines(&app.calendar)), rows[1]);

    let help = Line::from(Span::styled(
        t!("ui.calendar_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(help), rows[2]);
}

pub(super) fn draw_clock(app: &mut App, frame: &mut Frame, area: Rect) {
    let now = clock::now_local();
    let rows_data = app.clock.rows(&now);
    let zone = crate::time_zone_model::active_name();

    let width = 38u16.min(area.width);
    let height = (u16::try_from(rows_data.len()).unwrap_or(u16::MAX) + 3).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width + 1),
        y: area.y + 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::CLOCK, t!("ui.clock")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let items: Vec<ListItem> = rows_data
        .iter()
        .map(|r| {
            let label = match r.key {
                "local" => t!("ui.clock_local").to_string(),
                "utc" => t!("ui.clock_utc").to_string(),
                "iso_week" => t!("ui.clock_iso_week").to_string(),
                _ => t!("ui.clock_zone", zone = zone).to_string(),
            };
            ListItem::new(Line::from(format!(" {label:<10} {}", r.value)))
        })
        .collect();
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(app.clock.selected));
    frame.render_stateful_widget(list, rows[0], &mut state);
    app.layout.clock = rows[0];

    let help = Line::from(Span::styled(t!("ui.clock_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(help), rows[1]);
}

pub(super) fn draw_dashboard(app: &App, frame: &mut Frame, area: Rect) {
    let Some(d) = app.dashboard.as_ref() else {
        return;
    };
    let pending = t!("ui.dashboard_computing");
    let num = |n: Option<u64>| n.map_or_else(|| pending.to_string(), |v| v.to_string());
    let rows = [
        (t!("ui.dashboard_folder"), d.folder.clone()),
        (
            t!("ui.dashboard_disk"),
            d.disk_usage.clone().unwrap_or_else(|| pending.to_string()),
        ),
        (t!("ui.dashboard_files"), num(d.file_count)),
        (t!("ui.dashboard_commits"), num(d.commit_count)),
    ];
    let width = 52u16.min(area.width);
    let height = (u16::try_from(rows.len()).unwrap_or(u16::MAX) + 4).min(area.height);
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
        .title(format!(" {} {} ", icon::INFO, t!("ui.dashboard")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let lines: Vec<Line> = rows
        .iter()
        .map(|(label, value)| {
            Line::from(vec![
                Span::styled(format!("  {label:<14} "), theme::dim()),
                Span::raw(value.clone()),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), chunks[0]);

    let hint = Line::from(Span::styled(
        t!("ui.dashboard_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
}

fn month_lines(cal: &calendar::Calendar) -> Vec<Line<'static>> {
    let grid = cal.grid();
    let selected = cal.selected_day_in_shown();
    let mut lines = vec![Line::from(Span::styled(t!("ui.weekdays"), theme::dim()))];
    for week in &grid.weeks {
        let mut spans = Vec::with_capacity(7);
        for (i, cell) in week.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            match cell {
                // The selected day (keyboard cursor) is reverse-highlighted;
                // today (when not selected) is underlined.
                Some(d) if selected == Some(*d) => {
                    spans.push(Span::styled(format!("{d:>2}"), theme::selected()));
                }
                Some(d) if grid.today == Some(*d) => {
                    spans.push(Span::styled(
                        format!("{d:>2}"),
                        Style::default().add_modifier(Modifier::UNDERLINED),
                    ));
                }
                Some(d) => spans.push(Span::raw(format!("{d:>2}"))),
                None => spans.push(Span::raw("  ")),
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}
