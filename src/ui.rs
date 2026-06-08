//! All rendering. `draw` lays out the frame, records pane rectangles for mouse
//! hit-testing, and delegates to per-pane helpers.

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
    ScrollbarOrientation, ScrollbarState, Tabs, Wrap,
};

use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::StatefulImage;

use crate::app::{App, Focus};
use crate::datetime;
use crate::menu::MENUS;
use crate::messages::Level;
use crate::search::Field;
use crate::theme::{self, icon};

pub fn draw(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // menu bar
            Constraint::Min(1),    // body
            Constraint::Length(1), // status bar
        ])
        .split(area);
    app.layout.menu = rows[0];

    // Body columns: explorer | center | messages.
    let mut constraints = Vec::new();
    if app.show_explorer {
        constraints.push(Constraint::Length(30));
    }
    constraints.push(Constraint::Min(20));
    if app.show_messages {
        constraints.push(Constraint::Length(32));
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(rows[1]);

    let mut ci = 0;
    let explorer_rect = if app.show_explorer {
        let r = cols[ci];
        ci += 1;
        Some(r)
    } else {
        None
    };
    let center_rect = cols[ci];
    ci += 1;
    let messages_rect = if app.show_messages { Some(cols[ci]) } else { None };

    // Center: tab bar over editor+scrollbar.
    let center = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(center_rect);
    app.layout.tabs = center[0];

    let editor_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(app.focus == Focus::Editor));
    let editor_inner = editor_block.inner(center[1]);
    let editor_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(editor_inner);
    app.layout.editor = editor_split[0];

    if let Some(r) = explorer_rect {
        app.layout.explorer = r;
    }
    if let Some(r) = messages_rect {
        app.layout.messages = r;
    }

    // Keep the explorer selection within its viewport (also used by mouse).
    if let Some(r) = explorer_rect {
        let h = r.height.saturating_sub(2) as usize;
        app.explorer.ensure_visible(h);
    }

    // ----- render (immutable reads from here) -----
    draw_menu_bar(app, frame, rows[0]);
    if let Some(r) = explorer_rect {
        draw_explorer(app, frame, r);
    }
    draw_tabs(app, frame, center[0]);
    frame.render_widget(editor_block, center[1]);
    draw_center(app, frame, editor_split[0], editor_split[1]);
    if let Some(r) = messages_rect {
        draw_messages(app, frame, r);
    }
    draw_status_bar(app, frame, rows[2]);

    // Overlays.
    if app.show_calendar {
        draw_calendar(frame, area);
    }
    if app.menu.is_open() {
        draw_menu_dropdown(app, frame, rows[0]);
    }
    if app.search.is_some() {
        draw_search(app, frame, area);
    }
    if app.palette.is_some() {
        draw_palette(app, frame, area);
    }
    if app.project_search.is_some() {
        draw_project_search(app, frame, area);
    }
    if app.prompt.is_some() {
        draw_prompt(app, frame, area);
    }
    if app.query_replace.is_some() {
        draw_query_replace(app, frame, area);
    }
    if app.confirm.is_some() {
        draw_confirm(app, frame, area);
    }
    if app.paste.as_ref().map(|p| p.conflict.is_some()).unwrap_or(false) {
        draw_paste_conflict(app, frame, area);
    }
    if app.show_help {
        draw_help(frame, area);
    }
}

fn draw_confirm(app: &App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.confirm.as_ref() else { return };
    let width = (c.message.chars().count() as u16 + 6).min(area.width);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + area.height / 3,
        width,
        height: 3,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::ERR))
        .title(" Confirm ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(Line::from(c.message.clone())), inner);
}

fn draw_paste_conflict(app: &App, frame: &mut Frame, area: Rect) {
    let Some(op) = app.paste.as_ref() else { return };
    let Some(src) = op.conflict.as_ref() else { return };
    let name = src
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lines = vec![
        Line::from(format!("\"{name}\" already exists in the destination.")),
        Line::from(Span::styled(
            "(o)verwrite  (O) all  (s)kip  (S) all  (c)ancel",
            theme::dim(),
        )),
    ];
    let width = 56u16.min(area.width);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + area.height / 3,
        width,
        height: 4,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(" Paste conflict ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_query_replace(app: &App, frame: &mut Frame, area: Rect) {
    let Some(qr) = app.query_replace.as_ref() else {
        return;
    };
    let bar = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(2),
        width: area.width,
        height: 1,
    };
    frame.render_widget(Clear, bar);
    let line = Line::from(vec![
        Span::styled(
            format!(" {} Query replace ", icon::SEARCH),
            Style::default().bg(theme::WARN).fg(Color::Black).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" «{}» ", qr.label), Style::default().fg(theme::WARN)),
        Span::raw("  "),
        Span::styled("y", theme::title(true)),
        Span::raw(" replace  "),
        Span::styled("n", theme::title(true)),
        Span::raw(" skip  "),
        Span::styled("!", theme::title(true)),
        Span::raw(" rest  "),
        Span::styled("q", theme::title(true)),
        Span::raw(" quit   "),
        Span::styled(format!("(replaced {})", qr.replaced), theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(line).style(Style::default().bg(Color::Indexed(236))), bar);
}

fn menu_offsets() -> Vec<u16> {
    let mut offsets = Vec::new();
    let mut pos: u16 = 1;
    for m in MENUS {
        offsets.push(pos);
        pos += m.name.chars().count() as u16 + 2;
    }
    offsets
}

fn draw_menu_bar(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = vec![Span::raw(" ")];
    for (i, m) in MENUS.iter().enumerate() {
        let open = app.menu.open == Some(i);
        let style = if open {
            theme::selected()
        } else {
            Style::default().fg(Color::White)
        };
        spans.push(Span::styled(format!(" {} ", m.name), style));
    }
    spans.push(Span::raw("   "));
    spans.push(Span::styled(format!("{} STRIDE", icon::PALETTE), theme::dim()));
    let bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Indexed(236)));
    frame.render_widget(bar, area);
}

fn draw_menu_dropdown(app: &App, frame: &mut Frame, bar: Rect) {
    let Some(i) = app.menu.open else { return };
    let def = &MENUS[i];
    let x = bar.x + menu_offsets()[i];
    let width = def
        .items
        .iter()
        .map(|it| it.label.chars().count() + it.shortcut.chars().count() + 4)
        .max()
        .unwrap_or(12)
        .max(14) as u16;
    let height = def.items.len() as u16 + 2;
    let y = bar.y + 1;
    let area = Rect {
        x: x.min(frame.area().width.saturating_sub(width)),
        y,
        width: width.min(frame.area().width),
        height: height.min(frame.area().height.saturating_sub(y)),
    };
    frame.render_widget(Clear, area);
    let items: Vec<ListItem> = def
        .items
        .iter()
        .map(|it| {
            let pad = (width as usize)
                .saturating_sub(it.label.chars().count() + it.shortcut.chars().count() + 4);
            let line = Line::from(vec![
                Span::raw(format!(" {}", it.label)),
                Span::raw(" ".repeat(pad)),
                Span::styled(format!("{} ", it.shortcut), theme::dim()),
            ]);
            ListItem::new(line)
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::title(true))
                .title(def.name),
        )
        .highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(app.menu.item));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_explorer(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Explorer;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(focused))
        .title(format!(" {} Explorer ", icon::FOLDER));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let h = inner.height as usize;
    let top = app.explorer.top.min(app.explorer.nodes.len());
    let end = (top + h).min(app.explorer.nodes.len());
    let items: Vec<ListItem> = app.explorer.nodes[top..end]
        .iter()
        .map(|n| {
            let indent = "  ".repeat(n.depth);
            let glyph = if n.is_dir {
                if n.expanded {
                    icon::FOLDER_OPEN
                } else {
                    icon::FOLDER
                }
            } else {
                theme::file_icon(&n.name)
            };
            let mut style = if n.is_dir {
                Style::default().fg(theme::ACCENT)
            } else {
                Style::default()
            };
            let marked = app.explorer.marked.contains(&n.path);
            let cut_pending = app.clip_cut && app.clip.contains(&n.path);
            if cut_pending {
                style = style.add_modifier(Modifier::DIM);
            }
            let mark = if marked { "● " } else { "" };
            let mut item = ListItem::new(Line::from(vec![
                Span::raw(indent),
                Span::styled(format!("{mark}{glyph} {}", n.name), style),
            ]));
            if marked {
                item = item.style(Style::default().bg(Color::Indexed(238)));
            }
            item
        })
        .collect();

    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    if app.explorer.selected >= top && app.explorer.selected < end {
        state.select(Some(app.explorer.selected - top));
    }
    frame.render_stateful_widget(list, inner, &mut state);
}

fn draw_tabs(app: &App, frame: &mut Frame, area: Rect) {
    let titles: Vec<Line> = app
        .editor
        .tabs
        .iter()
        .map(|t| {
            if t.preview {
                Line::from(Span::styled(
                    t.title(),
                    Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC),
                ))
            } else {
                Line::from(t.title())
            }
        })
        .collect();
    let tabs = Tabs::new(titles)
        .select(app.editor.active)
        .highlight_style(theme::selected())
        .divider(Span::styled("│", theme::dim()));
    frame.render_widget(tabs, area);
}

fn draw_center(app: &mut App, frame: &mut Frame, text: Rect, scrollbar: Rect) {
    let is_image = app.editor.active_tab().map(|t| t.is_image()).unwrap_or(false);
    if is_image {
        if let Some(tab) = app.editor.active_tab_mut() {
            if let Some(proto) = tab.image.as_mut() {
                frame.render_stateful_widget(StatefulImage::<StatefulProtocol>::new(), text, proto);
            }
        }
        return;
    }
    if let Some(tab) = app.editor.active_tab() {
        frame.render_widget(&tab.editor, text);

        let total = app.editor.active_line_count().max(1);
        let pos = app.editor.cursor_1based().0.saturating_sub(1);
        let mut sb_state = ScrollbarState::new(total).position(pos);
        let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("\u{2191}"))
            .end_symbol(Some("\u{2193}"))
            .style(theme::dim());
        frame.render_stateful_widget(bar, scrollbar, &mut sb_state);
    }
}

fn draw_messages(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Messages;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(focused))
        .title(format!(" {} Messages ", icon::BELL));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.messages.items.is_empty() {
        let hint = Paragraph::new("No messages.")
            .style(theme::dim())
            .wrap(Wrap { trim: true });
        frame.render_widget(hint, inner);
        return;
    }

    let items: Vec<ListItem> = app
        .messages
        .items
        .iter()
        .map(|m| {
            let (sym, color) = match m.level {
                Level::Info => (icon::INFO, Color::White),
                Level::Advice => (icon::INFO, theme::ACCENT),
                Level::Warn => (icon::BELL, theme::WARN),
                Level::Error => (icon::CLOSE, theme::ERR),
            };
            let line = Line::from(vec![
                Span::styled(format!("{sym} "), Style::default().fg(color)),
                Span::raw(m.text.clone()),
                Span::styled(format!("  {}", icon::CLOSE), theme::dim()),
            ]);
            ListItem::new(line)
        })
        .collect();
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(app.messages.selected));
    frame.render_stateful_widget(list, inner, &mut state);
}

fn draw_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    let (line, col) = app.editor.cursor_1based();
    let path = app
        .editor
        .active_tab()
        .map(|t| t.display_path())
        .unwrap_or_default();
    let dirty = app.editor.active_tab().map(|t| t.dirty).unwrap_or(false);
    let dirty_flag = if dirty {
        format!(" {}", icon::FILE_DIRTY)
    } else {
        String::new()
    };

    let left = format!(" {}{}  \u{2014}  {}", path, dirty_flag, app.status);
    let right = format!("Ln {line}:Col {col}   {} ", icon::CALENDAR);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(right.chars().count() as u16 + 1),
        ])
        .split(area);

    let bg = Style::default().bg(theme::ACCENT).fg(Color::Black);
    frame.render_widget(Paragraph::new(left).style(bg).alignment(Alignment::Left), cols[0]);
    frame.render_widget(Paragraph::new(right).style(bg).alignment(Alignment::Right), cols[1]);
}

fn draw_calendar(frame: &mut Frame, area: Rect) {
    let now = datetime::now_local();
    let width = 28u16.min(area.width);
    let height = 14u16.min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width + 1),
        y: area.y + 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} Calendar ", icon::CLOCK));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(1), Constraint::Min(6)])
        .split(inner);

    let info = vec![
        Line::from(vec![
            Span::styled(format!("{} ", icon::CLOCK), theme::dim()),
            Span::raw(datetime::local_clock(&now)),
        ]),
        Line::from(Span::raw(datetime::utc_iso(&now))),
        Line::from(Span::styled(datetime::iso_week_date(&now), theme::dim())),
    ];
    frame.render_widget(Paragraph::new(info), rows[0]);

    let header = Line::from(Span::styled(
        format!("{:^21}", datetime::month_title(&now)),
        Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(Paragraph::new(header), rows[1]);
    frame.render_widget(Paragraph::new(month_lines(&now)), rows[2]);
}

fn month_lines(now: &jiff::Zoned) -> Vec<Line<'static>> {
    let grid = datetime::month_grid(now);
    let mut lines = vec![Line::from(Span::styled("Mo Tu We Th Fr Sa Su", theme::dim()))];
    for week in &grid.weeks {
        let mut spans = Vec::with_capacity(7);
        for (i, cell) in week.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            match cell {
                Some(d) if *d == grid.today => {
                    spans.push(Span::styled(format!("{d:>2}"), theme::selected()));
                }
                Some(d) => spans.push(Span::raw(format!("{d:>2}"))),
                None => spans.push(Span::raw("  ")),
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn centered(area: Rect, pct_x: u16, pct_y: u16) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(v[1])[1]
}

fn draw_palette(app: &App, frame: &mut Frame, area: Rect) {
    let Some(p) = app.palette.as_ref() else { return };
    let rect = centered(area, 70, 70);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} Command Palette [{}] ", icon::SEARCH, p.mode().label()));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let input = Line::from(vec![
        Span::styled("\u{276f} ", theme::title(true)),
        Span::raw(p.input.clone()),
        Span::styled("\u{2588}", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(input), rows[0]);

    let items: Vec<ListItem> = p
        .entries
        .iter()
        .map(|e| ListItem::new(Line::from(e.label.clone())))
        .collect();
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    if !p.entries.is_empty() {
        state.select(Some(p.selected));
    }
    frame.render_stateful_widget(list, rows[1], &mut state);

    let hint = Line::from(Span::styled(
        "prefixes:  (none) files   > commands   # buffers   : line     Tab accept   Esc close",
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), rows[2]);
}

fn draw_project_search(app: &App, frame: &mut Frame, area: Rect) {
    let Some(ps) = app.project_search.as_ref() else { return };
    let rect = centered(area, 80, 80);
    frame.render_widget(Clear, rect);
    let title = if ps.replacing {
        format!(" {} Search & Replace in Project ", icon::SEARCH)
    } else {
        format!(" {} Search in Project ", icon::SEARCH)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let head = if ps.replacing { 4 } else { 3 };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(head), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let mut header = Vec::new();
    let q_focus = !ps.replacing || ps.field == Field::Query;
    header.push(field_line("Find   ", &ps.query, q_focus));
    if ps.replacing {
        header.push(field_line("Replace", &ps.replace, ps.field == Field::Replace));
    }
    let toggle = |on: bool, label: &str| {
        let style = if on { theme::selected() } else { theme::dim() };
        Span::styled(format!(" {label} "), style)
    };
    header.push(Line::from(vec![
        toggle(ps.case_sensitive, "Case (Alt+C)"),
        Span::raw(" "),
        toggle(ps.regex, "Regex (Alt+R)"),
    ]));
    header.push(Line::from(Span::styled(ps.status.clone(), theme::dim())));
    frame.render_widget(Paragraph::new(header), rows[0]);

    let items: Vec<ListItem> = ps
        .hits
        .iter()
        .map(|h| ListItem::new(Line::from(h.display.clone())))
        .collect();
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    if !ps.hits.is_empty() {
        state.select(Some(ps.selected));
    }
    frame.render_stateful_widget(list, rows[1], &mut state);

    let hint = if ps.replacing {
        "Enter: open match   Alt+Enter / Replace-field Enter: replace all   Tab: switch field   Esc: close"
    } else {
        "Enter: open match   \u{2191}\u{2193}: navigate   Esc: close"
    };
    frame.render_widget(Paragraph::new(Line::from(Span::styled(hint, theme::dim()))), rows[2]);
}

fn draw_search(app: &App, frame: &mut Frame, area: Rect) {
    let Some(s) = app.search.as_ref() else { return };
    let height = if s.replacing { 5 } else { 4 };
    let width = (area.width as f32 * 0.7) as u16;
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + 1,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let title = if s.interactive {
        format!(" {} Query Replace ", icon::SEARCH)
    } else if s.replacing {
        format!(" {} Find & Replace ", icon::SEARCH)
    } else {
        format!(" {} Find ", icon::SEARCH)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines = Vec::new();
    let q_focus = !s.replacing || s.field == Field::Query;
    lines.push(field_line("Find   ", &s.query, q_focus));
    if s.replacing {
        lines.push(field_line("Replace", &s.replace, s.field == Field::Replace));
    }
    let toggle = |on: bool, label: &str| {
        let style = if on { theme::selected() } else { theme::dim() };
        Span::styled(format!(" {label} "), style)
    };
    lines.push(Line::from(vec![
        toggle(s.case_sensitive, "Case (Alt+C)"),
        Span::raw(" "),
        toggle(s.whole_word, "Word (Alt+W)"),
        Span::raw(" "),
        toggle(s.regex, "Regex (Alt+R)"),
    ]));
    let status = if !s.status.is_empty() {
        s.status.clone()
    } else if s.interactive {
        "Enter: begin step-through (y/n/!/q)   Esc: close".to_string()
    } else {
        "Enter: next   Alt+Enter / Replace-field Enter: replace all   Esc: close".to_string()
    };
    lines.push(Line::from(Span::styled(status, theme::dim())));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn field_line<'a>(label: &'a str, value: &'a str, focused: bool) -> Line<'a> {
    let marker = if focused { "\u{276f}" } else { " " };
    let lstyle = if focused { theme::title(true) } else { theme::dim() };
    Line::from(vec![
        Span::styled(format!("{marker} {label} "), lstyle),
        Span::raw(value.to_string()),
    ])
}

fn draw_prompt(app: &App, frame: &mut Frame, area: Rect) {
    let Some(p) = app.prompt.as_ref() else { return };
    let width = (area.width as f32 * 0.6) as u16;
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + area.height / 3,
        width,
        height: 3,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", p.title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let line = Line::from(vec![
        Span::styled("\u{276f} ", theme::title(true)),
        Span::raw(p.input.clone()),
        Span::styled("\u{2588}", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(line), inner);
}

const HELP_ROWS: &[(&str, &str)] = &[
    ("Ctrl+P", "Command palette"),
    ("Ctrl+O", "Open file…"),
    ("Ctrl+S / Ctrl+Shift+S", "Save / Save As"),
    ("Ctrl+N / Ctrl+W", "New / Close tab"),
    ("Ctrl+Q", "Quit"),
    ("Ctrl+Z / Ctrl+Y", "Undo / Redo"),
    ("Ctrl+X / Ctrl+C / Ctrl+V", "Cut / Copy / Paste"),
    ("Ctrl+A", "Select all"),
    ("Ctrl+F / Ctrl+R", "Find / Find & Replace"),
    ("F3 / Shift+F3", "Find next / previous"),
    ("Ctrl+B / Ctrl+E", "Toggle / focus explorer"),
    ("F10 / Alt+F,E,T,H", "Menu bar"),
    ("F1", "This help"),
    ("Mouse", "Click to place cursor, drag to select, wheel to scroll"),
];

fn draw_help(frame: &mut Frame, area: Rect) {
    let rect = centered(area, 60, 70);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(" Keyboard Shortcuts  (Esc to close) ");
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let key_w = HELP_ROWS.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    let lines: Vec<Line> = HELP_ROWS
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!(" {k:<key_w$} "), theme::title(true)),
                Span::raw(format!("  {d}")),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
