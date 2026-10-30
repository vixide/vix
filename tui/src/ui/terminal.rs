//! The PTY-backed integrated terminal panel, and a Cowork guest's view of a
//! host's shared terminal (T697), drawn the same way.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::app::App;
use crate::theme::{self, icon};

/// Map a `vt100` color to a ratatui color.
fn vt_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Clear `area` and draw the rounded, titled frame; returns its inside.
fn draw_frame(frame: &mut Frame, area: Rect, title: &str) -> Rect {
    let rect = Rect {
        x: area.x,
        y: area.y,
        width: area.width.max(2),
        height: area.height.max(2),
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {title} ", icon::CODE));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

/// Draw a `vt100` screen into `inner` (cells clipped to it) and, when
/// `cursor` is set and the shell shows one, place the real cursor there.
fn draw_screen(frame: &mut Frame, inner: Rect, screen: &vt100::Screen, cursor: bool) {
    let (rows, cols) = screen.size();
    let mut lines: Vec<Line> = Vec::with_capacity(rows as usize);
    for row in 0..rows.min(inner.height) {
        let mut spans: Vec<Span> = Vec::with_capacity(cols as usize);
        for col in 0..cols.min(inner.width) {
            let (text, mut style) = match screen.cell(row, col) {
                Some(cell) => {
                    let contents = cell.contents();
                    let text = if contents.is_empty() {
                        " ".to_string()
                    } else {
                        contents
                    };
                    let mut s = Style::default()
                        .fg(vt_color(cell.fgcolor()))
                        .bg(vt_color(cell.bgcolor()));
                    if cell.bold() {
                        s = s.add_modifier(Modifier::BOLD);
                    }
                    if cell.italic() {
                        s = s.add_modifier(Modifier::ITALIC);
                    }
                    if cell.underline() {
                        s = s.add_modifier(Modifier::UNDERLINED);
                    }
                    (text, s)
                }
                None => (" ".to_string(), Style::default()),
            };
            if screen.cell(row, col).is_some_and(vt100::Cell::inverse) {
                style = style.add_modifier(Modifier::REVERSED);
            }
            spans.push(Span::styled(text, style));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);

    // Place the real cursor where the shell put it.
    if cursor && !screen.hide_cursor() {
        let (crow, ccol) = screen.cursor_position();
        if crow < inner.height && ccol < inner.width {
            frame.set_cursor_position((inner.x + ccol, inner.y + crow));
        }
    }
}

pub(super) fn draw_terminal(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.terminal.is_none() {
        return;
    }
    let inner = draw_frame(frame, area, &t!("ui.terminal"));
    // Match the PTY grid to the visible area so the shell wraps correctly.
    if let Some(term) = app.terminal.as_mut() {
        term.resize(inner.height, inner.width);
    }
    let Some(term) = app.terminal.as_ref() else {
        return;
    };
    let parser = term.lock();
    draw_screen(frame, inner, parser.screen(), true);
}

/// A Cowork guest's view of the host's shared terminal (T697). The host's
/// size wins (a smaller view clips it); the title says whether this guest
/// may type. The caret is shown only when it may.
#[cfg(feature = "cowork")]
pub(super) fn draw_cowork_terminal(app: &App, frame: &mut Frame, area: Rect) {
    let Some((screen, can_type)) = app.cowork_terminal_view() else {
        return;
    };
    let title = if can_type {
        t!("ui.cowork_terminal_input")
    } else {
        t!("ui.cowork_terminal_read_only")
    };
    let inner = draw_frame(frame, area, &title);
    draw_screen(frame, inner, screen, can_type);
}
