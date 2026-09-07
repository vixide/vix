//! Small standalone tool panels: the pomodoro timer, the welcome screen, the
//! generic modal dialog (About/Website/Email), the color converter, the
//! regex tester, the calculator, and the unit converter.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::draw_scrollbar;
use crate::app::App;
use crate::theme::{self, icon};

pub(super) fn draw_pomodoro(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::pomodoro_tool::Phase;
    let Some(timer) = app.pomodoro.as_ref() else {
        return;
    };
    let phase = timer.phase;

    let (title, hint) = if phase == Phase::Break {
        (t!("ui.pomodoro_break_label"), t!("ui.pomodoro_break_hint"))
    } else {
        (t!("menu.item.tools.pomodoro"), t!("ui.pomodoro_hint"))
    };
    let button = match phase {
        Phase::Idle => t!("ui.pomodoro_start"),
        Phase::Work => t!("ui.pomodoro_stop"),
        Phase::Break => t!("ui.pomodoro_cancel"),
    };
    let big = timer.label();
    let width = 36u16.min(area.width.saturating_sub(2)).max(24);
    let height = 7u16.min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {title} "));
    let inner = block.inner(rect);
    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // timer
            Constraint::Length(1), // button
            Constraint::Length(1), // spacer
            Constraint::Min(1),    // hint
        ])
        .split(inner);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(big, theme::selected())))
            .alignment(Alignment::Center),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("[ {button} ]"),
            theme::selected(),
        )))
        .alignment(Alignment::Center),
        rows[1],
    );
    app.layout.pomodoro_button = rows[1];
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint.to_string(), theme::dim())))
            .alignment(Alignment::Center),
        rows[3],
    );
}

pub(super) fn draw_welcome(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.welcome.is_none() {
        return;
    }
    let width = 72u16.min(area.width.saturating_sub(2)).max(24);
    let height = area.height.saturating_sub(2).clamp(6, 24);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::INFO, t!("ui.welcome")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let body = chunks[0];
    let view_h = body.height as usize;

    // Wrap the paragraphs to the text width (reserving the scrollbar column), so
    // the lines below are already soft-wrapped; then clamp the scroll to them.
    let text_width = body.width.saturating_sub(1).max(1) as usize;
    if let Some(w) = app.welcome.as_mut() {
        w.wrap_to(text_width);
        w.clamp(view_h);
    }
    let total = app
        .welcome
        .as_ref()
        .map_or(0, crate::welcome_panel::Panel::len);
    let scroll = app.welcome.as_ref().map_or(0, |w| w.scroll);
    let show_bar = total > view_h && body.width > 1;
    let text_area = if show_bar {
        Rect {
            width: body.width - 1,
            ..body
        }
    } else {
        body
    };
    let visible: Vec<Line> = app
        .welcome
        .as_ref()
        .map(|w| {
            w.lines()[scroll..(scroll + view_h).min(total)]
                .iter()
                .map(|l| Line::from(Span::raw(l.clone())))
                .collect()
        })
        .unwrap_or_default();
    frame.render_widget(Paragraph::new(visible), text_area);
    if show_bar {
        let sb_area = Rect {
            x: body.x + body.width - 1,
            ..body
        };
        draw_scrollbar(frame, sb_area, scroll, total.saturating_sub(view_h));
    }

    let hint = Line::from(Span::styled(
        t!("ui.welcome_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.welcome = body;
}

pub(super) fn draw_dialog(app: &mut App, frame: &mut Frame, area: Rect) {
    let (title, body, has_editor) = match app.dialog.as_ref() {
        Some(d) => (d.title.clone(), d.body.clone(), d.editor.is_some()),
        None => return,
    };
    let content_w =
        u16::try_from(body.chars().count().max(title.chars().count())).unwrap_or(u16::MAX);
    let width = (content_w + 6).clamp(16, area.width);
    let height = 5u16.min(area.height); // border + body + blank + Ok + border
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {title} "));
    let inner = block.inner(rect);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
    // Record the body rect so mouse selection can hit-test it.
    app.layout.dialog_body = rows[0];

    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);
    if has_editor {
        // A selectable/copyable text field (Website/Email).
        if let Some(ed) = app.dialog.as_ref().and_then(|d| d.editor.as_ref()) {
            frame.render_widget(ed, rows[0]);
        }
    } else {
        frame.render_widget(Paragraph::new(body).alignment(Alignment::Center), rows[0]);
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("[ {} ]", t!("ui.ok")),
            theme::selected(),
        )))
        .alignment(Alignment::Center),
        rows[2],
    );
}

pub(super) fn draw_color_converter(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::color_converter_tool::Field;
    let Some(conv) = app.color_converter.as_ref() else {
        return;
    };

    let title = t!("menu.item.tools.color_converter");
    let hint = t!("ui.color_converter_hint");
    let width = 44u16.min(area.width.saturating_sub(2)).max(24);
    // border + 3 field rows + blank + swatch + blank + hint + border.
    let height = 9u16.min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {title} "));
    let inner = block.inner(rect);
    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // HEX
            Constraint::Length(1), // RGB
            Constraint::Length(1), // HSL
            Constraint::Length(1), // blank
            Constraint::Length(1), // swatch
            Constraint::Min(1),    // hint
        ])
        .split(inner);

    for (i, field) in Field::ALL.iter().enumerate() {
        let focused = conv.focus == *field;
        let text = &conv.fields[field.index()];
        let style = if focused {
            theme::selected()
        } else {
            theme::base()
        };
        let caret = if focused { "_" } else { "" };
        let line = Line::from(vec![
            Span::styled(format!(" {:<4}", field.label()), theme::dim()),
            Span::styled(format!("{text}{caret}"), style),
        ]);
        frame.render_widget(Paragraph::new(line), rows[i]);
        app.layout.color_converter_rows[i] = rows[i];
    }

    // A swatch of the current color, when the focused field parses.
    if let Some(c) = conv.color() {
        let swatch = Block::default().style(Style::default().bg(Color::Rgb(c.r, c.g, c.b)));
        frame.render_widget(swatch, rows[4]);
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint.to_string(), theme::dim()))),
        rows[5],
    );
}

pub(super) fn draw_regex_tester(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::regex_tool::{Field, Outcome};
    let Some(t) = app.regex_tester.as_ref() else {
        return;
    };

    let title = t!("menu.item.tools.regex_tester");
    let hint = t!("ui.regex_tester_hint");
    let width = 60u16.min(area.width.saturating_sub(2)).max(28);
    let height = 12u16.min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {title} "));
    let inner = block.inner(rect);
    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // pattern
            Constraint::Length(1), // subject
            Constraint::Length(1), // blank
            Constraint::Min(1),    // results
            Constraint::Length(1), // hint
        ])
        .split(inner);

    let field = |label: &str, text: &str, focused: bool| {
        let style = if focused {
            theme::selected()
        } else {
            theme::base()
        };
        let caret = if focused { "_" } else { "" };
        Line::from(vec![
            Span::styled(format!(" {label:<8}"), theme::dim()),
            Span::styled(format!("{text}{caret}"), style),
        ])
    };
    frame.render_widget(
        Paragraph::new(field("pattern", &t.pattern, t.focus == Field::Pattern)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(field("subject", &t.subject, t.focus == Field::Subject)),
        rows[1],
    );
    app.layout.regex_tester_rows = [rows[0], rows[1]];

    let result_lines: Vec<Line> = match t.result() {
        Outcome::Error(e) => vec![Line::from(Span::styled(format!(" {e}"), theme::dim()))],
        Outcome::Matches(m) if m.is_empty() => {
            vec![Line::from(Span::styled(
                t!("status.no_matches").to_string(),
                theme::dim(),
            ))]
        }
        Outcome::Matches(m) => {
            let mut lines = vec![Line::from(Span::styled(
                t!("status.matches_n", n = m.len()).to_string(),
                theme::dim(),
            ))];
            let view = rows[3].height.saturating_sub(1) as usize;
            for s in m.iter().take(view) {
                lines.push(Line::from(format!("  {s}")));
            }
            lines
        }
    };
    frame.render_widget(Paragraph::new(result_lines), rows[3]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint.to_string(), theme::dim()))),
        rows[4],
    );
}

pub(super) fn draw_calculator(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::calculator_tool::{Focus, Outcome};
    let Some(calc) = app.calculator.as_ref() else {
        return;
    };

    let title = t!("menu.item.tools.calculator");
    let hint = t!("ui.calculator_hint");
    let width = 50u16.min(area.width.saturating_sub(2)).max(28);
    let height = 8u16.min(area.height); // border + input + blank + buttons + blank + result + hint + border
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {title} "));
    let inner = block.inner(rect);
    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // input
            Constraint::Length(1), // buttons
            Constraint::Length(1), // result
            Constraint::Min(1),    // hint
        ])
        .split(inner);

    let input_style = if calc.focus == Focus::Input {
        theme::selected()
    } else {
        theme::base()
    };
    let caret = if calc.focus == Focus::Input { "_" } else { "" };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}{caret}", calc.input),
            input_style,
        ))),
        rows[0],
    );
    app.layout.calculator_rects[0] = rows[0];

    // Buttons: [ Run ] [ Insert ].
    let btn = |label: String, focused: bool| {
        let style = if focused {
            theme::selected()
        } else {
            theme::dim()
        };
        Span::styled(format!("[ {label} ] "), style)
    };
    let buttons = Line::from(vec![
        Span::raw(" "),
        btn(
            t!("ui.calculator_run").to_string(),
            calc.focus == Focus::Run,
        ),
        btn(
            t!("ui.calculator_insert").to_string(),
            calc.focus == Focus::Insert,
        ),
    ]);
    frame.render_widget(Paragraph::new(buttons), rows[1]);
    // Approximate button hit rects: split the buttons row in two halves.
    let half = rows[1].width / 2;
    app.layout.calculator_rects[1] = Rect {
        x: rows[1].x,
        y: rows[1].y,
        width: half,
        height: 1,
    };
    app.layout.calculator_rects[2] = Rect {
        x: rows[1].x + half,
        y: rows[1].y,
        width: rows[1].width - half,
        height: 1,
    };

    // Result or error line.
    let result_line = match &calc.outcome {
        Some(Outcome::Ok(v)) => Line::from(Span::styled(format!(" = {v}"), theme::base())),
        Some(Outcome::Err(e)) => Line::from(Span::styled(format!(" {e}"), theme::dim())),
        None => Line::from(""),
    };
    frame.render_widget(Paragraph::new(result_line), rows[2]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint.to_string(), theme::dim()))),
        rows[3],
    );
}

pub(super) fn draw_unit_converter(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::unit_converter_tool::{Focus, UNITS};
    let Some(conv) = app.unit_converter.as_ref() else {
        return;
    };

    let title = t!("menu.item.tools.convert.unit");
    let hint = t!("ui.unit_converter_hint");
    let width = 46u16.min(area.width.saturating_sub(2)).max(28);
    let height = 8u16.min(area.height); // border + value + from + to + blank + hint + border
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {title} "));
    let inner = block.inner(rect);
    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // value
            Constraint::Length(1), // from
            Constraint::Length(1), // to + output
            Constraint::Length(1), // blank
            Constraint::Min(1),    // hint
        ])
        .split(inner);

    let field_style = |focused: bool| {
        if focused {
            theme::selected()
        } else {
            theme::base()
        }
    };

    // Value field.
    let caret = if conv.focus == Focus::Value { "_" } else { "" };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {:<8}", t!("ui.unit_value")), theme::dim()),
            Span::styled(
                format!("{}{caret}", conv.value),
                field_style(conv.focus == Focus::Value),
            ),
        ])),
        rows[0],
    );
    // From selector.
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {:<8}", t!("ui.unit_from")), theme::dim()),
            Span::styled(
                format!("‹ {} ›", UNITS[conv.from].label),
                field_style(conv.focus == Focus::From),
            ),
        ])),
        rows[1],
    );
    // To selector, with the live output to its right.
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {:<8}", t!("ui.unit_to")), theme::dim()),
            Span::styled(
                format!("‹ {} ›", UNITS[conv.to].label),
                field_style(conv.focus == Focus::To),
            ),
            Span::styled(format!("   = {}", conv.output_text()), theme::base()),
        ])),
        rows[2],
    );
    app.layout.unit_converter_rows = [rows[0], rows[1], rows[2]];

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint.to_string(), theme::dim()))),
        rows[4],
    );
}
