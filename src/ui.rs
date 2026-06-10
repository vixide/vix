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
use crate::calendar;
use crate::menu::MENUS;
use crate::messages::Level;
use crate::search::Field;
use crate::theme::{self, icon};

/// Render the whole frame: lay out panes, record their rectangles for mouse
/// hit-testing, draw each pane, then draw any active overlay on top.
pub fn draw(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    // Paint the whole frame in the theme's background so every pane (and the gaps
    // between them) shares one background — important for the light theme.
    frame.render_widget(Block::default().style(theme::base()), area);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // menu bar
            Constraint::Min(1),    // body
            Constraint::Length(1), // status bar
        ])
        .split(area);
    app.layout.menu = rows[0];

    // Body columns: explorer | center | messages. The dock widths are
    // user-adjustable (drag the inner edges); clamp them so the editor keeps room.
    let dock_max = rows[1].width.saturating_sub(20).max(12);
    let mut constraints = Vec::new();
    if app.show_explorer {
        constraints.push(Constraint::Length(app.settings.explorer_width.clamp(12, dock_max)));
    }
    constraints.push(Constraint::Min(20));
    if app.show_messages {
        constraints.push(Constraint::Length(app.settings.messages_width.clamp(12, dock_max)));
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
        .style(theme::region_base(theme::Region::Editor))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(app.focus == Focus::Editor));
    let editor_inner = editor_block.inner(center[1]);
    let editor_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(editor_inner);
    app.layout.editor = editor_split[0];
    app.layout.scrollbar = editor_split[1];

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
        draw_calendar(app, frame, area);
    }
    if app.menu.is_open() {
        if let Some(i) = app.menu.open {
            app.layout.menu_dropdown = menu_dropdown_rect(area, rows[0], i);
        }
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
    if app.theme_chooser.is_some() {
        draw_theme_chooser(app, frame, area);
    }
    if app.locale_chooser.is_some() {
        draw_locale_chooser(app, frame, area);
    }
    if app.keyway_chooser.is_some() {
        draw_keyway_chooser(app, frame, area);
    }
    if app.paste.as_ref().map(|p| p.conflict.is_some()).unwrap_or(false) {
        draw_paste_conflict(app, frame, area);
    }
    if app.show_help {
        draw_help(frame, area);
    }
    if app.dialog.is_some() {
        draw_dialog(app, frame, area);
    }
}

fn draw_dialog(app: &mut App, frame: &mut Frame, area: Rect) {
    let (title, body, has_editor) = match app.dialog.as_ref() {
        Some(d) => (d.title.clone(), d.body.clone(), d.editor.is_some()),
        None => return,
    };
    let content_w = body.chars().count().max(title.chars().count()) as u16;
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
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
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
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.confirm")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(Line::from(c.message.clone())), inner);
}

/// Render a centered list-chooser overlay (theme/locale/keyway): a titled box
/// with one row per `labels` entry, the `selected` row highlighted, and a hint
/// line. Returns the list's rectangle so the caller can record it for mouse
/// hit-testing.
fn draw_list_chooser(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    labels: &[String],
    selected: usize,
) -> Rect {
    let width = 34u16.min(area.width);
    let height = (labels.len() as u16 + 4).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + area.height / 3,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::PALETTE, title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let items: Vec<ListItem> = labels
        .iter()
        .map(|label| ListItem::new(Line::from(format!("  {label}"))))
        .collect();
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(list, rows[0], &mut state);

    let hint = Line::from(Span::styled(t!("ui.theme_hint"), theme::dim()));
    frame.render_widget(Paragraph::new(hint), rows[1]);
    rows[0]
}

fn draw_theme_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(tc) = app.theme_chooser.as_ref() else { return };
    let selected = tc.selected;
    let labels: Vec<String> = tc
        .choices
        .iter()
        .map(|c| match c.builtin() {
            Some(m) => t!(m.label()).to_string(),
            None => c.custom_name().unwrap_or_default().to_string(),
        })
        .collect();
    app.layout.chooser = draw_list_chooser(frame, area, &t!("ui.theme"), &labels, selected);
}

fn draw_locale_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(lc) = app.locale_chooser.as_ref() else { return };
    let selected = lc.selected;
    let labels: Vec<String> = vix_locale_chooser::LOCALES
        .iter()
        .map(|l| l.name.to_string())
        .collect();
    app.layout.chooser = draw_list_chooser(frame, area, &t!("ui.locale"), &labels, selected);
}

fn draw_keyway_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(kc) = app.keyway_chooser.as_ref() else { return };
    let selected = kc.selected;
    let labels: Vec<String> = vix_keyway_chooser::KEYWAYS
        .iter()
        .map(|k| format!("{}  —  {}", k.name, k.tooltip))
        .collect();
    app.layout.chooser = draw_list_chooser(frame, area, &t!("ui.keyway"), &labels, selected);
}

fn draw_paste_conflict(app: &App, frame: &mut Frame, area: Rect) {
    let Some(op) = app.paste.as_ref() else { return };
    let Some(src) = op.conflict.as_ref() else { return };
    let name = src
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lines = vec![
        Line::from(t!("ui.paste_exists", name = name).to_string()),
        Line::from(Span::styled(
            t!("ui.paste_choices"),
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
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.paste_conflict")));
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
            format!(" {} {} ", icon::SEARCH, t!("ui.qr_label")),
            Style::default(),
        ),
        Span::styled(format!(" «{}» ", qr.label), Style::default()),
        Span::raw("  "),
        Span::styled("y", theme::title(true)),
        Span::raw(format!(" {}  ", t!("ui.qr_replace"))),
        Span::styled("n", theme::title(true)),
        Span::raw(format!(" {}  ", t!("ui.qr_skip"))),
        Span::styled("!", theme::title(true)),
        Span::raw(format!(" {}  ", t!("ui.qr_rest"))),
        Span::styled("q", theme::title(true)),
        Span::raw(format!(" {}   ", t!("ui.qr_quit"))),
        Span::styled(t!("ui.qr_replaced", count = qr.replaced), theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme::base()), bar);
}

fn menu_offsets() -> Vec<u16> {
    let mut offsets = Vec::new();
    let mut pos: u16 = 1;
    for m in MENUS {
        offsets.push(pos);
        pos += m.title().chars().count() as u16 + 2;
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
            Style::default().fg(theme::region_fg(theme::Region::MenuBar))
        };
        spans.push(Span::styled(format!(" {} ", m.title()), style));
    }
    let bar = Paragraph::new(Line::from(spans)).style(theme::region_base(theme::Region::MenuBar));
    frame.render_widget(bar, area);

    // Right-aligned dock open/close toggles: bright when open, dim when closed.
    // Click handling lives in `App::menu_click` (see `dock_toggle_cols`).
    let dock_style = |open: bool| {
        if open {
            theme::title(true)
        } else {
            theme::dim()
        }
    };
    let docks = Line::from(vec![
        Span::styled(icon::FOLDER, dock_style(app.show_explorer)),
        Span::raw(" "),
        Span::styled(icon::BELL, dock_style(app.show_messages)),
        Span::raw(" "),
    ]);
    frame.render_widget(
        Paragraph::new(docks)
            .alignment(Alignment::Right)
            .style(theme::region_base(theme::Region::MenuBar)),
        area,
    );
}

/// Columns (within the menu-bar rect) of the left- and right-dock toggle icons,
/// matching the right-aligned layout drawn by `draw_menu_bar`.
pub fn dock_toggle_cols(menu: Rect) -> (u16, u16) {
    let right = menu.x + menu.width;
    // Layout from the right edge: FOLDER, space, BELL, space.
    (right.saturating_sub(4), right.saturating_sub(2))
}

/// Geometry of the dropdown for the menu at `index`. Shared by the renderer and
/// by mouse hit-testing (`App::on_mouse`) so clicks land on the right item.
pub fn menu_dropdown_rect(frame_area: Rect, bar: Rect, index: usize) -> Rect {
    let def = &MENUS[index];
    let x = bar.x + menu_offsets()[index];
    let width = def
        .items
        .iter()
        .map(|it| it.label().chars().count() + it.shortcut.chars().count() + 4)
        .max()
        .unwrap_or(12)
        .max(14) as u16;
    let height = def.items.len() as u16 + 2;
    let y = bar.y + 1;
    Rect {
        x: x.min(frame_area.width.saturating_sub(width)),
        y,
        width: width.min(frame_area.width),
        height: height.min(frame_area.height.saturating_sub(y)),
    }
}

fn draw_menu_dropdown(app: &App, frame: &mut Frame, bar: Rect) {
    let Some(i) = app.menu.open else { return };
    let def = &MENUS[i];
    let area = menu_dropdown_rect(frame.area(), bar, i);
    frame.render_widget(Clear, area);
    let items: Vec<ListItem> = def
        .items
        .iter()
        .map(|it| {
            let label = it.label();
            let pad = (area.width as usize)
                .saturating_sub(label.chars().count() + it.shortcut.chars().count() + 4);
            let line = Line::from(vec![
                Span::raw(format!(" {label}")),
                Span::raw(" ".repeat(pad)),
                Span::styled(format!("{} ", it.shortcut), theme::dim()),
            ]);
            ListItem::new(line)
        })
        .collect();
    // The dropdown shows no title — the open menu is already indicated in the
    // menu bar, and a title here would otherwise display the raw i18n key.
    let list = List::new(items)
        .block(
            Block::default()
                .style(theme::base())
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::title(true)),
        )
        .highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(app.menu.item));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_explorer(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Explorer;
    let block = Block::default()
        .style(theme::region_base(theme::Region::LeftDock))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(focused))
        .title(format!(" {} {} ", icon::FOLDER, t!("ui.explorer")));
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
            // Directories are distinguished by their folder glyph, not a font
            // style (the built-in themes use no bold).
            let mut style = Style::default();
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
                item = item.style(Style::default());
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
        .style(theme::region_base(theme::Region::RightDock))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(focused))
        .title(format!(" {} {} ", icon::BELL, t!("ui.messages")));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.messages.items.is_empty() {
        let hint = Paragraph::new(t!("ui.no_messages").to_string())
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
            let (sym, sym_style) = match m.level {
                Level::Info => (icon::INFO, Style::default()),
                Level::Advice => (icon::INFO, Style::default()),
                Level::Warn => (icon::BELL, Style::default()),
                Level::Error => (icon::CLOSE, Style::default()),
            };
            let line = Line::from(vec![
                Span::styled(format!("{sym} "), sym_style),
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

    let mode = app
        .mode_indicator()
        .map(|m| format!("{m}   "))
        .unwrap_or_default();
    let left = format!(" {}{}{}  \u{2014}  {}", mode, path, dirty_flag, app.status);
    let right = format!("Ln {line}:Col {col}   {} ", icon::CALENDAR);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(right.chars().count() as u16 + 1),
        ])
        .split(area);

    let bg = theme::region_base(theme::Region::StatusBar);
    frame.render_widget(Paragraph::new(left).style(bg).alignment(Alignment::Left), cols[0]);
    frame.render_widget(Paragraph::new(right).style(bg).alignment(Alignment::Right), cols[1]);
}

fn draw_calendar(app: &App, frame: &mut Frame, area: Rect) {
    // The date/time area always reflects the present; the month area follows the
    // user's navigation (see `App::calendar`).
    let now = calendar::now_local();
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
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::CLOCK, t!("ui.calendar")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Length(1), Constraint::Min(6)])
        .split(inner);

    let info = vec![
        // Local date/time (no leading icon), then UTC ISO, then the commercial
        // (ISO week) date in the foreground color, then a blank spacer line.
        Line::from(Span::raw(calendar::local_datetime(&now))),
        Line::from(Span::raw(calendar::utc_iso(&now))),
        Line::from(Span::raw(calendar::iso_week_date(&now))),
        Line::from(""),
    ];
    frame.render_widget(Paragraph::new(info), rows[0]);

    let header = Line::from(Span::styled(
        format!("{:^21}", app.calendar.title()),
        Style::default(),
    ));
    frame.render_widget(Paragraph::new(header), rows[1]);
    frame.render_widget(Paragraph::new(month_lines(&app.calendar)), rows[2]);
}

fn month_lines(cal: &calendar::Calendar) -> Vec<Line<'static>> {
    let grid = cal.grid();
    let mut lines = vec![Line::from(Span::styled(t!("ui.weekdays"), theme::dim()))];
    for week in &grid.weeks {
        let mut spans = Vec::with_capacity(7);
        for (i, cell) in week.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            match cell {
                Some(d) if grid.today == Some(*d) => {
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
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} [{}] ", icon::SEARCH, t!("ui.command_palette"), p.mode().label()));
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
        t!("ui.palette_prefixes"),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), rows[2]);
}

fn draw_project_search(app: &App, frame: &mut Frame, area: Rect) {
    let Some(ps) = app.project_search.as_ref() else { return };
    let rect = centered(area, 80, 80);
    frame.render_widget(Clear, rect);
    let title = if ps.static_results {
        format!(" {} {} ", icon::SEARCH, t!("ui.goto_definition"))
    } else if ps.replacing {
        format!(" {} {} ", icon::SEARCH, t!("ui.search_replace_project"))
    } else {
        format!(" {} {} ", icon::SEARCH, t!("ui.search_project"))
    };
    let block = Block::default()
        .style(theme::base())
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
    header.push(field_line(&t!("ui.field_find"), &ps.query, q_focus));
    if ps.replacing {
        header.push(field_line(&t!("ui.field_replace"), &ps.replace, ps.field == Field::Replace));
    }
    let toggle = |on: bool, label: &str| {
        let style = if on { theme::selected() } else { theme::dim() };
        Span::styled(format!(" {label} "), style)
    };
    header.push(Line::from(vec![
        toggle(ps.case_sensitive, &t!("ui.toggle_case")),
        Span::raw(" "),
        toggle(ps.regex, &t!("ui.toggle_regex")),
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
        t!("ui.ps_hint_replace")
    } else {
        t!("ui.ps_hint")
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
        format!(" {} {} ", icon::SEARCH, t!("ui.query_replace"))
    } else if s.replacing {
        format!(" {} {} ", icon::SEARCH, t!("ui.find_replace"))
    } else {
        format!(" {} {} ", icon::SEARCH, t!("ui.find"))
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines = Vec::new();
    let q_focus = !s.replacing || s.field == Field::Query;
    lines.push(field_line(&t!("ui.field_find"), &s.query, q_focus));
    if s.replacing {
        lines.push(field_line(&t!("ui.field_replace"), &s.replace, s.field == Field::Replace));
    }
    let toggle = |on: bool, label: &str| {
        let style = if on { theme::selected() } else { theme::dim() };
        Span::styled(format!(" {label} "), style)
    };
    lines.push(Line::from(vec![
        toggle(s.case_sensitive, &t!("ui.toggle_case")),
        Span::raw(" "),
        toggle(s.whole_word, &t!("ui.toggle_word")),
        Span::raw(" "),
        toggle(s.regex, &t!("ui.toggle_regex")),
    ]));
    let status = if !s.status.is_empty() {
        s.status.clone()
    } else if s.interactive {
        t!("ui.search_hint_interactive").to_string()
    } else {
        t!("ui.search_hint").to_string()
    };
    lines.push(Line::from(Span::styled(status, theme::dim())));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn field_line(label: &str, value: &str, focused: bool) -> Line<'static> {
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
        .style(theme::base())
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

fn draw_help(frame: &mut Frame, area: Rect) {
    let rect = centered(area, 60, 70);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.keyboard_shortcuts")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let rows = vix_keyboard_shortcut_panel::ROWS;
    let key_w = rows.iter().map(|r| r.keys.len()).max().unwrap_or(0);
    let lines: Vec<Line> = rows
        .iter()
        .map(|r| {
            Line::from(vec![
                Span::styled(format!(" {:<key_w$} ", r.keys), theme::title(true)),
                Span::raw(format!("  {}", t!(r.desc))),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
