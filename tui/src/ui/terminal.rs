//! The PTY-backed integrated terminal panel, and a Cowork guest's view of a
//! host's shared terminal (T697), drawn the same way.

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

use crate::geo::Geo;
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
    let (rows, cols) = (rows.get(), cols.get());
    let mut lines: Vec<Line> = Vec::with_capacity(usize::from(rows));
    for row in 0..rows.min(inner.height) {
        let mut spans: Vec<Span> = Vec::with_capacity(usize::from(cols));
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
            frame.set_cursor_position((inner.x.plus(ccol), inner.y.plus(crow)));
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
        inner.y = inner.y.plus(1);
        inner.height = inner.height.minus(1);
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
        let mut label = format!(" {} ", i.plus(1));
        if shared == Some(id) {
            label = format!(" {} ({}) ", i.plus(1), t!("ui.terminal_shared_tab"));
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn nz(n: u16) -> std::num::NonZeroU16 {
        std::num::NonZeroU16::new(n).unwrap()
    }

    /// A parser over a 3x8 screen that has been fed `bytes`.
    fn screen_of(bytes: &[u8]) -> vt100::Parser {
        let mut p = vt100::Parser::new(nz(3), nz(8), 0);
        p.process(bytes);
        p
    }

    fn render(
        p: &vt100::Parser,
        w: u16,
        h: u16,
        cursor: bool,
        sel: Option<Selection>,
    ) -> Terminal<TestBackend> {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| draw_screen(f, Rect::new(0, 0, w, h), p.screen(), cursor, sel))
            .unwrap();
        term
    }

    #[test]
    fn vt_colors_map_to_ratatui_colors() {
        assert_eq!(vt_color(vt100::Color::Default), Color::Reset);
        assert_eq!(vt_color(vt100::Color::Idx(9)), Color::Indexed(9));
        assert_eq!(vt_color(vt100::Color::Rgb(1, 2, 3)), Color::Rgb(1, 2, 3));
    }

    #[test]
    fn cells_carry_text_colors_and_attributes() {
        // bold red "B", italic "I", underlined "U", truecolor background "R".
        let p = screen_of(b"\x1b[1;31mB\x1b[0m\x1b[3mI\x1b[0m\x1b[4mU\x1b[0m\x1b[48;2;10;20;30mR");
        let term = render(&p, 8, 3, false, None);
        let buf = term.backend().buffer();
        assert_eq!(buf[(0, 0)].symbol(), "B");
        assert_eq!(buf[(0, 0)].fg, Color::Indexed(1));
        assert!(buf[(0, 0)].modifier.contains(Modifier::BOLD));
        assert!(buf[(1, 0)].modifier.contains(Modifier::ITALIC));
        assert!(buf[(2, 0)].modifier.contains(Modifier::UNDERLINED));
        assert_eq!(buf[(3, 0)].bg, Color::Rgb(10, 20, 30));
        assert_eq!(buf[(5, 0)].symbol(), " ", "empty cells draw as spaces");
    }

    #[test]
    fn selection_reverses_cells_and_cancels_existing_reverse_video() {
        // Column 0 is already inverse; the selection covers columns 0-1.
        let p = screen_of(b"\x1b[7mA\x1b[0mB");
        let sel = Selection {
            anchor: (0, 0),
            head: (0, 1),
        };
        let term = render(&p, 8, 3, false, Some(sel));
        let buf = term.backend().buffer();
        assert!(
            !buf[(0, 0)].modifier.contains(Modifier::REVERSED),
            "inverse + selected shows normally"
        );
        assert!(buf[(1, 0)].modifier.contains(Modifier::REVERSED));
        assert!(!buf[(2, 0)].modifier.contains(Modifier::REVERSED));
        // Without a selection the inverse cell stays reversed.
        let plain = render(&p, 8, 3, false, None);
        assert!(
            plain.backend().buffer()[(0, 0)]
                .modifier
                .contains(Modifier::REVERSED)
        );
    }

    #[test]
    fn the_real_cursor_follows_the_shell_when_shown_and_in_bounds() {
        let p = screen_of(b"ab\r\nc");
        let mut term = render(&p, 8, 3, true, None);
        term.backend_mut().assert_cursor_position((1, 1));
        // Hidden by the shell (DECTCEM off): never placed.
        let hidden = screen_of(b"ab\x1b[?25l");
        let mut term = render(&hidden, 8, 3, true, None);
        assert_eq!(
            term.backend_mut().get_cursor_position().unwrap(),
            Position::new(0, 0)
        );
        // An area narrower than the cursor column leaves it alone.
        let wide = screen_of(b"abcdef");
        let mut term = render(&wide, 4, 3, true, None);
        assert_eq!(
            term.backend_mut().get_cursor_position().unwrap(),
            Position::new(0, 0)
        );
    }

    #[test]
    fn the_screen_is_clipped_to_the_inner_area() {
        let p = screen_of(b"abcdefgh");
        let term = render(&p, 3, 1, false, None);
        let buf = term.backend().buffer();
        assert_eq!(buf[(2, 0)].symbol(), "c");
    }

    #[test]
    fn the_frame_is_titled_and_never_smaller_than_two_cells() {
        let mut term = Terminal::new(TestBackend::new(20, 4)).unwrap();
        let mut inner = Rect::default();
        term.draw(|f| inner = draw_frame(f, Rect::new(0, 0, 20, 4), "Shell"))
            .unwrap();
        assert_eq!(inner, Rect::new(1, 1, 18, 2));
        let top: String = (0..20)
            .map(|x| term.backend().buffer()[(x, 0)].symbol().to_string())
            .collect();
        assert!(top.contains("Shell"), "{top}");
        // A zero-size area is lifted to the 2x2 minimum.
        term.draw(|f| inner = draw_frame(f, Rect::new(0, 0, 0, 0), "x"))
            .unwrap();
        assert_eq!(inner, Rect::new(1, 1, 0, 0));
    }
}
