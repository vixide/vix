//! The PTY-backed integrated terminal panel, and a Cowork guest's view of a
//! host's shared terminal (T697), drawn the same way.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::app::App;
use crate::theme::{self, icon};
use vix_terminal::Selection;

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

/// Draw a `vt100` screen into `inner` (cells clipped to it), the cells of
/// `selection` reversed, and, when `cursor` is set and the shell shows one,
/// place the real cursor there.
fn draw_screen(
    frame: &mut Frame,
    inner: Rect,
    screen: &vt100::Screen,
    cursor: bool,
    selection: Option<Selection>,
) {
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
                        contents.to_string()
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
            let inverse = screen.cell(row, col).is_some_and(vt100::Cell::inverse);
            // A selected cell flips its reverse video so it shows either way.
            if inverse != selection.is_some_and(|s| s.contains(row, col)) {
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

/// The shown integrated terminals (T631): the active one's screen, with a
/// tab strip on top when more than one is open (the Cowork-shared one
/// marked) and the title saying how far back the view is scrolled.
pub(super) fn draw_terminal(app: &mut App, frame: &mut Frame, area: Rect) {
    if !app.terminals.is_visible() {
        return;
    }
    let offset = app
        .terminals
        .active()
        .map_or(0, crate::terminal::Terminal::scroll_offset);
    let mut title = t!("ui.terminal").to_string();
    if offset > 0 {
        title = format!("{title} · {}", t!("ui.terminal_scrolled", n = offset));
    }
    if let Some(term) = app.terminals.active() {
        if !term.alive() {
            title = format!("{title} · {}", t!("ui.terminal_exited"));
        }
        if !term.capture() {
            title = format!("{title} · {}", t!("ui.terminal_capture_off"));
        }
    }
    let mut inner = draw_frame(frame, area, &title);
    if app.terminals.len() > 1 && inner.height > 1 {
        let strip = Rect { height: 1, ..inner };
        draw_tab_strip(app, frame, strip);
        inner.y += 1;
        inner.height -= 1;
    }
    app.layout.terminal = inner;
    // Match the PTY grid to the visible area so the shell wraps correctly.
    let Some(term) = app.terminals.active_mut() else {
        return;
    };
    term.resize(inner.height, inner.width);
    let selection = term.selection();
    let parser = term.lock();
    // The caret only on the live screen, not while reading the scrollback.
    draw_screen(frame, inner, parser.screen(), offset == 0, selection);
}

/// One row of terminal tabs, `1`, `2`, …, the active one highlighted and
/// the one shared with a Cowork session marked.
fn draw_tab_strip(app: &App, frame: &mut Frame, strip: Rect) {
    let shared = app.cowork_shared_terminal();
    let active = app.terminals.active_index();
    let mut spans: Vec<Span> = Vec::new();
    for (i, (id, _)) in app.terminals.iter().enumerate() {
        let mut label = format!(" {} ", i + 1);
        if shared == Some(id) {
            label = format!(" {} ({}) ", i + 1, t!("ui.terminal_shared_tab"));
        }
        let style = if i == active {
            theme::title(true).add_modifier(Modifier::REVERSED)
        } else {
            theme::base()
        };
        spans.push(Span::styled(label, style));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::base()),
        strip,
    );
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
    draw_screen(frame, inner, screen, can_type, None);
}
