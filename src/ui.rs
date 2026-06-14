//! All rendering. `draw` lays out the frame, records pane rectangles for mouse
//! hit-testing, and delegates to per-pane helpers.

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap,
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
    // Refresh misspelled-word underlines before painting (event-driven redraw, so
    // this recomputes once per input rather than continuously).
    if app.spellcheck {
        app.refresh_spellcheck();
    }
    if app.git_repo {
        app.refresh_git_gutter();
    }
    let area = frame.area();
    // Paint the whole frame in the theme's background so every pane (and the gaps
    // between them) shares one background — important for the light theme.
    frame.render_widget(Block::default().style(theme::base()), area);
    let mut vconstraints = vec![
        Constraint::Length(1), // menu bar
        Constraint::Min(1),    // body
    ];
    if app.show_status_bar {
        vconstraints.push(Constraint::Length(2)); // status bar (top border + content)
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vconstraints)
        .split(area);
    app.layout.menu = rows[0];
    let status_row = rows.get(2).copied();

    // The bottom dock (when shown) takes a fixed-height strip at the bottom of the
    // body; the rest is the main body (explorer | center | messages).
    let (body, bottom_dock_rect) = if app.show_bottom_dock {
        // Height is user-adjustable (drag the dock's top edge); keep at least 3
        // rows for the main body above it.
        let max_h = rows[1].height.saturating_sub(3).max(3);
        let h = app.settings.bottom_dock_height.clamp(3, max_h);
        let v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(h)])
            .split(rows[1]);
        (v[0], Some(v[1]))
    } else {
        (rows[1], None)
    };

    // Body columns: explorer | center | messages. The dock widths are
    // user-adjustable (drag the inner edges); clamp them so the editor keeps room.
    let dock_max = body.width.saturating_sub(20).max(12);
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
        .split(body);

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
        // The center editor keeps only its top border (no left/right/bottom).
        .borders(Borders::TOP)
        .border_type(BorderType::Rounded)
        .border_style(theme::region_title(theme::Region::Editor, app.focus == Focus::Editor));
    let editor_inner = editor_block.inner(center[1]);
    // The right-side scroll bar is optional; when hidden, the text reclaims its
    // column and the scrollbar rect collapses to zero (so hit-testing/drawing skip
    // it).
    let (editor_area, scrollbar_area) = if app.show_scrollbar {
        let s = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(editor_inner);
        (s[0], s[1])
    } else {
        (editor_inner, Rect { width: 0, height: 0, ..editor_inner })
    };
    app.layout.editor = editor_area;
    app.layout.scrollbar = scrollbar_area;

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
    draw_center(app, frame, editor_area, scrollbar_area);
    if let Some(r) = messages_rect {
        draw_messages(app, frame, r);
    }
    if let Some(r) = bottom_dock_rect {
        app.layout.bottom_dock = r;
        draw_bottom_dock(app, frame, r);
    }
    if let Some(r) = status_row {
        draw_status_bar(app, frame, r);
    }

    // Overlays.
    if app.show_calendar {
        draw_calendar(app, frame, area);
    }
    if app.menu.is_open() {
        if let Some(i) = app.menu.open {
            app.layout.menu_dropdown = menu_dropdown_rect(area, rows[0], i);
        }
        draw_menu_dropdown(app, frame);
    }
    if app.search.is_some() {
        draw_search(app, frame, area);
    }
    if app.palette.is_some() {
        draw_palette(app, frame, area);
    }
    if app.workspace_search.is_some() {
        draw_workspace_search(app, frame, area);
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
    if app.unsaved.is_some() {
        draw_unsaved(app, frame, area);
    }
    if app.spell_suggest.is_some() {
        draw_spell_suggest(app, frame, area);
    }
    if app.context_menu.is_some() {
        draw_context_menu(app, frame, area);
    }
    if app.git_panel.is_some() {
        draw_git_panel(app, frame, area);
    }
    if app.branch_chooser.is_some() {
        draw_branch_chooser(app, frame, area);
    }
    if app.theme_chooser.is_some() {
        draw_theme_chooser(app, frame, area);
    }
    if app.locale_chooser.is_some() {
        draw_locale_chooser(app, frame, area);
    }
    if app.keymap_chooser.is_some() {
        draw_keymap_chooser(app, frame, area);
    }
    if app.recent_chooser.is_some() {
        draw_recent_chooser(app, frame, area);
    }
    if app.nerd_palette.is_some() {
        draw_nerd_palette(app, frame, area);
    }
    if app.ascii_panel.is_some() {
        draw_ascii_panel(app, frame, area);
    }
    if app.x11_panel.is_some() {
        draw_x11_panel(app, frame, area);
    }
    if app.html_panel.is_some() {
        draw_html_panel(app, frame, area);
    }
    if app.system_info.is_some() {
        draw_system_info(app, frame, area);
    }
    if app.file_info.is_some() {
        draw_file_info(app, frame, area);
    }
    if app.contacts.is_some() {
        draw_contacts(app, frame, area);
    }
    if app.vcard.is_some() {
        draw_vcard(app, frame, area);
    }
    if app.dashboard.is_some() {
        draw_dashboard(app, frame, area);
    }
    if app.outline.is_some() {
        draw_outline(app, frame, area);
    }
    if app.completion.is_some() {
        draw_completion(app, frame);
    }
    if app.hover.is_some() {
        draw_hover(app, frame);
    }
    if app.paste.as_ref().is_some_and(|p| p.conflict.is_some()) {
        draw_paste_conflict(app, frame, area);
    }
    if app.show_help {
        draw_help(frame, area);
    }
    if app.dialog.is_some() {
        draw_dialog(app, frame, area);
    }
    if app.welcome.is_some() {
        draw_welcome(app, frame, area);
    }
}

fn draw_welcome(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.welcome.is_none() {
        return;
    }
    let total = app.welcome.as_ref().map_or(0, vix_welcome_panel::Panel::len);
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

    if let Some(w) = app.welcome.as_mut() {
        w.clamp(view_h);
    }
    let scroll = app.welcome.as_ref().map_or(0, |w| w.scroll);
    let show_bar = total > view_h && body.width > 1;
    let text_area = if show_bar {
        Rect { width: body.width - 1, ..body }
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
    frame.render_widget(Paragraph::new(visible).wrap(Wrap { trim: false }), text_area);
    if show_bar {
        let sb_area = Rect { x: body.x + body.width - 1, ..body };
        draw_scrollbar(frame, sb_area, scroll, total.saturating_sub(view_h));
    }

    let hint = Line::from(Span::styled(t!("ui.welcome_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.welcome = body;
}

/// The active editor cursor's screen position within the editor text area, as
/// `(x, y)`, accounting for vertical scroll. The x is approximate (it does not
/// subtract the line-number gutter), which is fine for anchoring a popup.
fn cursor_screen_yx(app: &App) -> Option<(u16, u16)> {
    let area = app.layout.editor;
    let t = app.editor.active_tab()?;
    let (line, col) = app.editor.cursor_1based();
    let off_y = t.editor.get_offset_y();
    let y = area.y + u16::try_from(line.saturating_sub(1).saturating_sub(off_y)).unwrap_or(0);
    let x = area.x + u16::try_from(col.saturating_sub(1)).unwrap_or(0).min(area.width);
    Some((x.min(area.x + area.width.saturating_sub(1)), y.min(area.y + area.height.saturating_sub(1))))
}

/// Draw the LSP completion popup as a small list anchored at the cursor.
fn draw_completion(app: &App, frame: &mut Frame) {
    let Some(popup) = app.completion.as_ref() else { return };
    if popup.items.is_empty() {
        return;
    }
    let area = app.layout.editor;
    if area.width < 16 || area.height < 4 {
        return;
    }
    let max_rows = 10.min(popup.items.len());
    // Scroll so the highlighted row stays visible.
    let start = if popup.selected >= max_rows { popup.selected + 1 - max_rows } else { 0 };
    let end = (start + max_rows).min(popup.items.len());

    // Width from the widest visible label (+detail), capped to the area.
    let widest = popup.items[start..end]
        .iter()
        .map(|it| it.label.chars().count() + it.detail.as_ref().map_or(0, |d| d.chars().count() + 3))
        .max()
        .unwrap_or(10);
    let width = (widest as u16 + 2).clamp(16, area.width.saturating_sub(2));
    let height = max_rows as u16 + 2;

    let (cx, cy) = cursor_screen_yx(app).unwrap_or((area.x + 2, area.y));
    // Below the cursor if it fits, else above.
    let y = if cy + 1 + height <= area.y + area.height {
        cy + 1
    } else {
        cy.saturating_sub(height)
    };
    let x = cx.min(area.x + area.width.saturating_sub(width));
    let rect = Rect { x, y, width, height };

    frame.render_widget(Clear, rect);
    let rows: Vec<ListItem> = popup.items[start..end]
        .iter()
        .map(|it| {
            let mut spans = vec![Span::raw(it.label.clone())];
            if let Some(detail) = &it.detail {
                spans.push(Span::styled(format!("  {detail}"), theme::dim()));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let list = List::new(rows)
        .block(
            Block::default()
                .style(theme::base())
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::title(true))
                .title(format!(" {} ", t!("ui.completion"))),
        )
        .highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(popup.selected - start));
    frame.render_stateful_widget(list, rect, &mut state);
}

/// Draw the LSP hover tooltip as a wrapped, bordered box near the cursor.
fn draw_hover(app: &App, frame: &mut Frame) {
    let Some(h) = app.hover.as_ref() else { return };
    let area = app.layout.editor;
    if area.width < 12 || area.height < 4 {
        return;
    }
    let text = h.text.replace('\r', "");
    let width = 64u16.min(area.width.saturating_sub(2)).max(20);
    let inner_w = width.saturating_sub(2).max(1) as usize;
    // Estimate wrapped height.
    let mut rows = 0usize;
    for line in text.lines() {
        rows += line.chars().count() / inner_w + 1;
    }
    let height = (rows as u16 + 2).clamp(3, 12.min(area.height));

    let (cx, cy) = cursor_screen_yx(app).unwrap_or((area.x + 2, area.y));
    let y = if cy + 1 + height <= area.y + area.height {
        cy + 1
    } else {
        cy.saturating_sub(height)
    };
    let x = cx.min(area.x + area.width.saturating_sub(width));
    let rect = Rect { x, y, width, height };

    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.hover")));
    let para = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    frame.render_widget(para, rect);
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

fn draw_unsaved(app: &App, frame: &mut Frame, area: Rect) {
    let Some(u) = app.unsaved.as_ref() else { return };
    let message = t!("ui.unsaved_prompt", name = u.name).to_string();
    let choices = t!("ui.unsaved_choices");
    let width = (message.chars().count().max(choices.chars().count()) as u16 + 6).min(area.width);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
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
        .title(format!(" {} ", t!("ui.unsaved_title")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let lines = vec![
        Line::from(message),
        Line::from(Span::styled(choices.to_string(), theme::dim())),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_branch_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.branch_chooser.as_ref() else { return };
    let hint = t!("ui.branch_hint");
    app.layout.chooser =
        draw_list_chooser(frame, area, &t!("ui.branch"), &hint, &c.branches, c.selected);
}

fn draw_git_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.git_panel.is_none() {
        return;
    }
    let selected = app.git_panel.as_ref().unwrap().selected;
    let rows = app.git_status.len().max(1);
    let width = 64u16.min(area.width);
    let max_rows = area.height.saturating_sub(4).max(1);
    let visible = (rows as u16).min(max_rows);
    let height = (visible + 4).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 3,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let title = match app.git_branch.as_deref() {
        Some(b) => format!(" {} {} — {} ", icon::BRANCH, t!("ui.git_changes"), b),
        None => format!(" {} {} ", icon::BRANCH, t!("ui.git_changes")),
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    if app.git_status.is_empty() {
        let clean = Line::from(Span::styled(t!("ui.git_clean").to_string(), theme::dim()));
        frame.render_widget(Paragraph::new(clean), chunks[0]);
    } else {
        let items: Vec<ListItem> = app
            .git_status
            .iter()
            .map(|s| {
                // "[x] M  path" — [x] when staged; the letter is colored by change.
                let staged = if s.is_staged() { "[\u{2713}]" } else { "[ ]" };
                let change = s.primary();
                let letter = change.map_or(' ', vix_git::Change::letter);
                let color = change.map_or(Color::Gray, git_change_color);
                ListItem::new(Line::from(vec![
                    Span::raw(format!("  {staged} ")),
                    Span::styled(format!("{letter} "), Style::default().fg(color)),
                    Span::raw(s.path.clone()),
                ]))
            })
            .collect();
        let list = List::new(items).highlight_style(theme::selected());
        let mut state = ListState::default();
        state.select(Some(selected.min(app.git_status.len() - 1)));
        frame.render_stateful_widget(list, chunks[0], &mut state);
    }

    let hint = Line::from(Span::styled(t!("ui.git_changes_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.git_panel = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: chunks[0].width,
        height: (app.git_status.len() as u16).min(chunks[0].height),
    };
}

fn draw_context_menu(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::app::CONTEXT_ITEMS;
    let Some(cm) = app.context_menu.as_ref() else { return };
    let labels: Vec<String> =
        CONTEXT_ITEMS.iter().map(|&(label, action)| {
            if action == "menu.separator" { String::new() } else { t!(label).to_string() }
        }).collect();
    let width = (labels.iter().map(|l| l.chars().count()).max().unwrap_or(8) as u16 + 4).min(area.width);
    let height = (CONTEXT_ITEMS.len() as u16 + 2).min(area.height);
    // Clamp so the menu stays on screen near the click.
    let x = cm.x.min(area.right().saturating_sub(width));
    let y = cm.y.min(area.bottom().saturating_sub(height));
    let rect = Rect { x, y, width, height };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let lines: Vec<Line> = CONTEXT_ITEMS
        .iter()
        .enumerate()
        .map(|(i, (_, action))| {
            if *action == "menu.separator" {
                Line::from(Span::styled("─".repeat(inner.width as usize), theme::dim()))
            } else {
                let text = format!(" {} ", labels[i]);
                if i == cm.selected {
                    Line::from(Span::styled(text, theme::selected()))
                } else {
                    Line::from(Span::raw(text))
                }
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
    app.layout.context_menu = rect;
}

fn draw_spell_suggest(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(p) = app.spell_suggest.as_ref() else { return };
    let title = t!("ui.spell_suggest", word = p.word).to_string();
    let hint = t!("ui.spell_suggest_hint");
    let rows = p.suggestions.len().max(1);
    let widest = p
        .suggestions
        .iter()
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(0)
        .max(title.chars().count())
        .max(hint.chars().count());
    let width = (widest as u16 + 6).min(area.width);
    // Borders (2) + suggestion rows + hint (1).
    let height = (rows as u16 + 3).min(area.height);
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
        .title(format!(" {title} "));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    if p.suggestions.is_empty() {
        let none = Line::from(Span::styled(t!("ui.spell_no_suggestions").to_string(), theme::dim()));
        frame.render_widget(Paragraph::new(none), chunks[0]);
    } else {
        let items: Vec<ListItem> = p
            .suggestions
            .iter()
            .map(|s| ListItem::new(Line::from(format!("  {s}"))))
            .collect();
        let list = List::new(items).highlight_style(theme::selected());
        let mut state = ListState::default();
        state.select(Some(p.selected));
        frame.render_stateful_widget(list, chunks[0], &mut state);
    }

    let hint = Line::from(Span::styled(hint.to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.spell_suggest = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: chunks[0].width,
        height: (p.suggestions.len() as u16).min(chunks[0].height),
    };
}

/// Render a centered list-chooser overlay (theme/locale/keymap): a titled box
/// with one row per `labels` entry, the `selected` row highlighted, and a hint
/// line. Returns the list's rectangle so the caller can record it for mouse
/// hit-testing.
fn draw_list_chooser(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    hint: &str,
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

    let hint = Line::from(Span::styled(hint.to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), rows[1]);
    rows[0]
}

fn draw_theme_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(tc) = app.theme_chooser.as_ref() else { return };
    let selected = tc.selected;
    let labels: Vec<String> = tc.choices.iter().map(|c| c.name.clone()).collect();
    let hint = t!("ui.theme_hint");
    app.layout.chooser = draw_list_chooser(frame, area, &t!("ui.theme"), &hint, &labels, selected);
}

fn draw_locale_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(lc) = app.locale_chooser.as_ref() else { return };
    let selected = lc.selected;
    let labels: Vec<String> = vix_locale_chooser::LOCALES
        .iter()
        .map(|l| l.name.to_string())
        .collect();
    let hint = t!("ui.theme_hint");
    app.layout.chooser = draw_list_chooser(frame, area, &t!("ui.locale"), &hint, &labels, selected);
}

fn draw_keymap_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(kc) = app.keymap_chooser.as_ref() else { return };
    let selected = kc.selected;
    let labels: Vec<String> = vix_keymap_chooser::KEYMAPS
        .iter()
        .map(|k| format!("{}  —  {}", k.name, k.tooltip))
        .collect();
    let hint = t!("ui.theme_hint");
    app.layout.chooser = draw_list_chooser(frame, area, &t!("ui.keymap"), &hint, &labels, selected);
}

fn draw_recent_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(rc) = app.recent_chooser.as_ref() else { return };
    let selected = rc.selected;
    // Show the file name first (survives truncation), then its directory.
    let labels: Vec<String> = rc
        .entries
        .iter()
        .map(|p| {
            let name = p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            match p.parent() {
                Some(dir) if !dir.as_os_str().is_empty() => {
                    format!("{name}  —  {}", dir.display())
                }
                _ => name,
            }
        })
        .collect();
    let hint = t!("ui.recent_hint");
    app.layout.chooser = draw_list_chooser(frame, area, &t!("ui.recent"), &hint, &labels, selected);
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
#[must_use] 
pub fn dock_toggle_cols(menu: Rect) -> (u16, u16) {
    let right = menu.x + menu.width;
    // Layout from the right edge: FOLDER, space, BELL, space.
    (right.saturating_sub(4), right.saturating_sub(2))
}

/// Right-aligned indicator for a dropdown item: a `▸` arrow for a submenu parent,
/// otherwise its keyboard shortcut.
fn item_right(it: &crate::menu::Item) -> String {
    if it.has_submenu() {
        "\u{25b8}".to_string()
    } else {
        it.shortcut.to_string()
    }
}

/// Width budget for a dropdown holding `items`: 2 borders + leading + trailing
/// space (= 4), plus a 1-column gap so the label and the right indicator never
/// touch. Minimum 14.
fn dropdown_width(items: &[crate::menu::Item]) -> u16 {
    items
        .iter()
        .map(|it| {
            if it.is_separator() {
                return 0;
            }
            let right = item_right(it).chars().count();
            let gap = usize::from(it.has_submenu() || !it.shortcut.is_empty());
            it.label().chars().count() + right + 4 + gap
        })
        .max()
        .unwrap_or(12)
        .max(14) as u16
}

/// Geometry of the dropdown for the menu at `index`. Shared by the renderer and
/// by mouse hit-testing (`App::on_mouse`) so clicks land on the right item.
#[must_use]
pub fn menu_dropdown_rect(frame_area: Rect, bar: Rect, index: usize) -> Rect {
    let def = &MENUS[index];
    let x = bar.x + menu_offsets()[index];
    let width = dropdown_width(def.items);
    let height = def.items.len() as u16 + 2;
    let y = bar.y + 1;
    Rect {
        x: x.min(frame_area.width.saturating_sub(width)),
        y,
        width: width.min(frame_area.width),
        height: height.min(frame_area.height.saturating_sub(y)),
    }
}

/// Render one dropdown (Clear + bordered list) at `area`, highlighting `selected`.
fn render_dropdown(frame: &mut Frame, area: Rect, items: &[crate::menu::Item], selected: Option<usize>) {
    frame.render_widget(Clear, area);
    let rows: Vec<ListItem> = items
        .iter()
        .map(|it| {
            if it.is_separator() {
                let w = (area.width as usize).saturating_sub(2);
                return ListItem::new(Line::from(Span::styled("─".repeat(w), theme::dim())));
            }
            let label = it.label();
            let right = item_right(it);
            let pad = (area.width as usize)
                .saturating_sub(label.chars().count() + right.chars().count() + 4);
            let line = Line::from(vec![
                Span::raw(format!(" {label}")),
                Span::raw(" ".repeat(pad)),
                Span::styled(format!("{right} "), theme::dim()),
            ]);
            ListItem::new(line)
        })
        .collect();
    // No title — the open menu is already indicated in the bar, and a title would
    // otherwise display the raw i18n key.
    let list = List::new(rows)
        .block(
            Block::default()
                .style(theme::base())
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::title(true)),
        )
        .highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(selected);
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_menu_dropdown(app: &mut App, frame: &mut Frame) {
    let Some(i) = app.menu.open else { return };
    let area = app.layout.menu_dropdown;
    render_dropdown(frame, area, MENUS[i].items, app.menu.item);

    // An open submenu is drawn to the right of its parent item.
    if let (Some(sidx), Some(subitems)) = (app.menu.sub, app.menu.submenu_items()) {
        let fa = frame.area();
        let sub_w = dropdown_width(subitems);
        let sub_h = subitems.len() as u16 + 2;
        let sub_x = (area.x + area.width).min(fa.width.saturating_sub(sub_w));
        let parent_row = app.menu.item.unwrap_or(0) as u16;
        let sub_y = (area.y + parent_row).min(fa.height.saturating_sub(sub_h));
        let sub_area = Rect {
            x: sub_x,
            y: sub_y,
            width: sub_w.min(fa.width),
            height: sub_h.min(fa.height.saturating_sub(sub_y)),
        };
        app.layout.submenu_dropdown = sub_area;
        render_dropdown(frame, sub_area, subitems, Some(sidx));
    }
}

/// The badge color for a git change in the file explorer.
fn git_change_color(change: vix_git::Change) -> Color {
    use vix_git::Change;
    match change {
        Change::Added | Change::Untracked => Color::Green,
        Change::Modified => Color::Yellow,
        Change::Deleted => Color::Red,
        Change::Renamed => Color::Cyan,
        Change::Conflicted => Color::Magenta,
    }
}

fn draw_explorer(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Explorer;
    let block = Block::default()
        .style(theme::region_base(theme::Region::LeftDock))
        // The left dock keeps only its top and right borders.
        .borders(Borders::TOP | Borders::RIGHT)
        .border_type(BorderType::Rounded)
        .border_style(theme::region_title(theme::Region::LeftDock, focused))
        .title(if app.explorer.has_filter() {
            format!(" {} {}  {} ", icon::FOLDER, t!("ui.explorer"), t!("ui.explorer_filtered"))
        } else {
            format!(" {} {} ", icon::FOLDER, t!("ui.explorer"))
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let h = inner.height as usize;
    let total = app.explorer.nodes.len();
    // Reserve a one-column gutter on the right for a scrollbar when the tree
    // overflows the viewport and the scrollbar is enabled.
    let show_bar = app.settings.show_scrollbar && total > h && inner.width > 1;
    let list_area = if show_bar {
        Rect { width: inner.width - 1, ..inner }
    } else {
        inner
    };
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
            let mut spans = vec![
                Span::raw(indent),
                Span::styled(format!("{mark}{glyph} {}", n.name), style),
            ];
            // Git status badge (a colored letter) for changed, tracked files.
            if !n.is_dir {
                if let Some(change) = app.git_change_for(&n.path) {
                    spans.push(Span::styled(
                        format!("  {}", change.letter()),
                        Style::default().fg(git_change_color(change)),
                    ));
                }
            }
            let mut item = ListItem::new(Line::from(spans));
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
    frame.render_stateful_widget(list, list_area, &mut state);

    if show_bar {
        let sb_area = Rect { x: inner.x + inner.width - 1, y: inner.y, width: 1, height: inner.height };
        draw_scrollbar(frame, sb_area, app.explorer.selected, total.saturating_sub(1));
    }
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
    let is_image = app.editor.active_tab().is_some_and(super::editor::Tab::is_image);
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

        if scrollbar.width > 0 {
            let total = app.editor.active_line_count().max(1);
            let pos = app.editor.cursor_1based().0.saturating_sub(1);
            draw_scrollbar(frame, scrollbar, pos, total.saturating_sub(1));
        }
    }
}

fn draw_messages(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Messages;
    let block = Block::default()
        .style(theme::region_base(theme::Region::RightDock))
        // The right dock keeps only its top and left borders.
        .borders(Borders::TOP | Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(theme::region_title(theme::Region::RightDock, focused))
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
                Level::Info | Level::Advice => (icon::INFO, Style::default()),
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
    let total = app.messages.items.len();
    let h = inner.height as usize;
    let show_bar = app.settings.show_scrollbar && total > h && inner.width > 1;
    let list_area = if show_bar {
        Rect { width: inner.width - 1, ..inner }
    } else {
        inner
    };
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(app.messages.selected));
    frame.render_stateful_widget(list, list_area, &mut state);
    if show_bar {
        let sb_area = Rect { x: inner.x + inner.width - 1, y: inner.y, width: 1, height: inner.height };
        draw_scrollbar(frame, sb_area, app.messages.selected, total.saturating_sub(1));
    }
}

/// Vix's one-character scrollbar, drawn into the vertical one-column `area`: a
/// dim track and a single `●` thumb positioned **proportionally** to `pos`
/// within `0..=max`. The thumb is always one cell tall (never proportional
/// height) and the track spans the whole `area` (no end-cap arrows). For
/// cursor/selection views pass `pos = selected`, `max = total - 1` (so the thumb
/// reaches the bottom only on the last item); for scroll views pass
/// `pos = scroll`, `max = total - viewport`.
fn draw_scrollbar(frame: &mut Frame, area: Rect, pos: usize, max: usize) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let h = area.height as usize;
    let frac = if max == 0 { 0.0 } else { pos.min(max) as f64 / max as f64 };
    let thumb_glyph = Span::styled("●", theme::title(true));
    let track_glyph = || Span::styled("│", theme::dim());
    let thumb = (frac * h.saturating_sub(1) as f64).round() as usize;
    let mut lines: Vec<Line> = Vec::with_capacity(h);
    for r in 0..h {
        lines.push(Line::from(if r == thumb { thumb_glyph.clone() } else { track_glyph() }));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Map a mouse `row` within a scrollbar `area` to a position in `0..=max`. The
/// track spans the whole `area` (no end-cap arrows), so the row maps
/// proportionally. Used for click and drag.
#[must_use]
pub fn scrollbar_pos_from_row(area: Rect, row: u16, max: usize) -> usize {
    if max == 0 || area.height == 0 {
        return 0;
    }
    let h = area.height;
    let rel = f64::from(row.saturating_sub(area.y));
    let pos = (rel / f64::from((h - 1).max(1)) * max as f64).round() as usize;
    pos.min(max)
}

fn draw_bottom_dock(app: &App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::BottomDock;
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::TOP)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(focused))
        .title(format!(" {} {} ", icon::INFO, t!("ui.bottom_dock")));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    // Reserve a one-column gutter for a scrollbar when the buffer overflows.
    let total = app.bottom_dock.lines.len();
    let h = inner.height as usize;
    let show_bar = app.settings.show_scrollbar && total > h && inner.width > 1;
    let text_area = if show_bar {
        Rect { width: inner.width - 1, ..inner }
    } else {
        inner
    };
    let lines: Vec<Line> = if app.bottom_dock.is_empty() {
        vec![Line::from(Span::styled(
            t!("ui.bottom_dock_empty").to_string(),
            theme::dim(),
        ))]
    } else {
        app.bottom_dock
            .visible(text_area.height as usize)
            .iter()
            .map(|l| Line::from(l.clone()))
            .collect()
    };
    frame.render_widget(Paragraph::new(lines), text_area);
    if show_bar {
        let sb_area = Rect { x: inner.x + inner.width - 1, y: inner.y, width: 1, height: inner.height };
        draw_scrollbar(frame, sb_area, app.bottom_dock.scroll, total.saturating_sub(inner.height as usize));
    }
}

fn draw_status_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    let (line, col) = app.editor.cursor_1based();
    let path = app
        .editor
        .active_tab()
        .map(super::editor::Tab::display_path)
        .unwrap_or_default();
    let dirty = app.editor.active_tab().is_some_and(|t| t.dirty);
    let dirty_flag = if dirty {
        format!(" {}", icon::FILE_DIRTY)
    } else {
        String::new()
    };

    let mode = app
        .mode_indicator()
        .map(|m| format!("{m}   "))
        .unwrap_or_default();
    // Editor info (language · line ending · encoding · selection) for text tabs.
    let info = app
        .editor
        .active_tab()
        .filter(|t| !t.is_image())
        .map(|t| {
            let lang = match t.editor.language() {
                "unknown" | "" => "text",
                other => other,
            };
            let sel = t.editor.selection_span().map(|(s, e)| {
                let code = t.editor.code_ref();
                (e - s, code.char_to_line(e) - code.char_to_line(s) + 1)
            });
            vix_status_bar_panel::info_segment(Some(lang), t.editor.line_ending(), sel)
        })
        .unwrap_or_default();

    let git = vix_status_bar_panel::git_segment(app.git_branch.as_deref(), icon::BRANCH, app.git_dirty());
    let left = vix_status_bar_panel::left_segment(&mode, &path, &dirty_flag, &app.status);
    let right = vix_status_bar_panel::right_segment(&format!("{git}{info}"), line, col, icon::CALENDAR);

    let bg = theme::region_base(theme::Region::StatusBar);
    // A top border separates the status bar from the body above it.
    let block = Block::default()
        .style(bg)
        .borders(Borders::TOP)
        .border_style(theme::region_title(theme::Region::StatusBar, false));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(right.chars().count() as u16 + 1),
        ])
        .split(inner);

    frame.render_widget(Paragraph::new(left).style(bg).alignment(Alignment::Left), cols[0]);
    frame.render_widget(Paragraph::new(right).style(bg).alignment(Alignment::Right), cols[1]);

    // Record the git/branch segment's rectangle (the leftmost part of the
    // right-aligned right segment, after its 1-cell padding) so a click on the
    // branch indicator opens the Git panel.
    let git_w = git.chars().count() as u16;
    app.layout.git_status_bar = if git_w > 0 {
        Rect { x: cols[1].x + 1, y: cols[1].y, width: git_w.min(cols[1].width), height: 1 }
    } else {
        Rect::default()
    };
}

fn draw_calendar(app: &mut App, frame: &mut Frame, area: Rect) {
    // The date/time area always reflects the present; the month area follows the
    // user's navigation (see `App::calendar`).
    let now = calendar::now_local();
    let width = 28u16.min(area.width);
    let height = 16u16.min(area.height);
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
    // Record the inner rect so a click can hit-test info lines, the month-nav
    // arrows, and day cells.
    app.layout.calendar = inner;
    frame.render_widget(block, rect);

    // The calendar (month nav + grid) sits above the date/time entries.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // month header + nav arrows
            Constraint::Min(6),    // weekday header + weeks
            Constraint::Length(4), // info
            Constraint::Length(1), // help
        ])
        .split(inner);

    // Month header: a left arrow, the centered month title, and a right arrow
    // (`◀`/`▶`). The arrows are clickable (see `App::calendar_mouse`) and mirror
    // the Left/Right keys.
    let header = Line::from(format!("{CAL_PREV}{:^19}{CAL_NEXT}", app.calendar.title()));
    frame.render_widget(Paragraph::new(header), rows[0]);
    frame.render_widget(Paragraph::new(month_lines(&app.calendar)), rows[1]);

    let info = vec![
        // A blank spacer, then local date/time, UTC ISO, and the commercial
        // (ISO week) date.
        Line::from(""),
        Line::from(Span::raw(calendar::local_datetime(&now))),
        Line::from(Span::raw(calendar::utc_iso(&now))),
        Line::from(Span::raw(calendar::iso_week_date(&now))),
    ];
    frame.render_widget(Paragraph::new(info), rows[2]);

    let help = Line::from(Span::styled(t!("ui.calendar_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(help), rows[3]);
}

/// Previous-month arrow glyph, at column 0 of the calendar's month-header row.
pub const CAL_PREV: char = '\u{25c0}';
/// Next-month arrow glyph, at column 20 of the calendar's month-header row.
pub const CAL_NEXT: char = '\u{25b6}';

/// Columns each glyph cell occupies in the Nerd Font palette grid. The mouse
/// hit-test in [`crate::app::App::nerd_mouse`] divides by this, so the renderer
/// and the hit-test must agree on it.
pub const NERD_CELL_W: u16 = 4;

fn draw_nerd_palette(app: &mut App, frame: &mut Frame, area: Rect) {
    use vix_nerd_font_picker::{COLS, GLYPHS};
    let Some(p) = app.nerd_palette.as_ref() else {
        return;
    };
    let grid_w = COLS as u16 * NERD_CELL_W;
    let width = (grid_w + 2).min(area.width);
    let grid_rows = p.rows() as u16;
    // Borders (2) + glyph rows + the selected-name row (1) + the hint row (1).
    let height = (grid_rows + 4).min(area.height);
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
        .title(format!(" {} {} ", icon::PALETTE, t!("ui.nerd_palette")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    // The glyph grid: COLS cells per row, each NERD_CELL_W wide so the column a
    // click lands in is `(x - grid_x) / NERD_CELL_W`. The highlighted cell is
    // drawn reversed (theme::selected), mirroring the other choosers.
    let mut lines: Vec<Line> = Vec::with_capacity(p.rows());
    for row in 0..p.rows() {
        let mut spans = Vec::with_capacity(COLS);
        for col in 0..COLS {
            let idx = row * COLS + col;
            if idx >= GLYPHS.len() {
                spans.push(Span::raw(" ".repeat(NERD_CELL_W as usize)));
                continue;
            }
            let cell = format!(" {}  ", GLYPHS[idx].ch);
            if idx == p.selected {
                spans.push(Span::styled(cell, theme::selected()));
            } else {
                spans.push(Span::raw(cell));
            }
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);

    let name = Line::from(Span::raw(format!("  {}", p.selected_name())));
    frame.render_widget(Paragraph::new(name), chunks[1]);

    let hint = Line::from(Span::styled(t!("ui.nerd_palette_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[2]);

    // Record just the glyph rows for mouse hit-testing.
    app.layout.nerd_palette = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: grid_w.min(chunks[0].width),
        height: grid_rows.min(chunks[0].height),
    };
}

fn draw_ascii_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    use vix_ascii_character_picker::{self as ascii, LEN};
    if app.ascii_panel.is_none() {
        return;
    }
    let width = 26u16.min(area.width);
    // Borders (2) + header (1) + rows + hint (1); cap rows so the box fits.
    let max_rows = area.height.saturating_sub(4).max(1);
    let rows = (LEN as u16).min(max_rows);
    let height = (rows + 4).min(area.height);
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
        .title(format!(" {} {} ", icon::TABLE, t!("ui.ascii")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let header = Line::from(Span::styled(t!("ui.ascii_header").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(header), chunks[0]);

    // Sync the scroll window to the highlighted row, then render that window.
    let view_h = chunks[1].height as usize;
    if let Some(p) = app.ascii_panel.as_mut() {
        p.ensure_visible(view_h);
    }
    let p = app.ascii_panel.as_ref().unwrap();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in p.scroll..(p.scroll + view_h).min(LEN) {
        let code = idx as u8;
        let text = format!("  {:>3}  {:>2}   {}", ascii::dec(code), ascii::hex(code), ascii::label(code));
        if idx == p.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(Span::raw(text)));
        }
    }
    frame.render_widget(Paragraph::new(lines), chunks[1]);

    let hint = Line::from(Span::styled(t!("ui.ascii_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[2]);

    // Record just the row window for mouse hit-testing.
    app.layout.ascii_panel = Rect {
        x: chunks[1].x,
        y: chunks[1].y,
        width: chunks[1].width,
        height: (view_h as u16).min(chunks[1].height),
    };
}

fn draw_x11_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let colors = vix_x11_color_picker::colors();
    let total = colors.len();
    if app.x11_panel.is_none() || total == 0 {
        return;
    }
    let width = 36u16.min(area.width);
    let max_rows = area.height.saturating_sub(4).max(1);
    let rows = (total as u16).min(max_rows);
    let height = (rows + 4).min(area.height);
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
        .title(format!(" {} {} ", icon::PALETTE, t!("ui.x11")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let header = Line::from(Span::styled(t!("ui.x11_header").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(header), chunks[0]);

    let view_h = chunks[1].height as usize;
    if let Some(p) = app.x11_panel.as_mut() {
        p.ensure_visible(view_h);
    }
    let p = app.x11_panel.as_ref().unwrap();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for c in &colors[p.scroll..(p.scroll + view_h).min(total)] {
        let swatch = Span::styled("██", Style::default().fg(Color::Rgb(c.r, c.g, c.b)));
        let text = format!(" {:7} {}", c.hex, c.name);
        let idx = p.scroll + lines.len();
        let label = if idx == p.selected {
            Span::styled(text, theme::selected())
        } else {
            Span::raw(text)
        };
        lines.push(Line::from(vec![Span::raw(" "), swatch, label]));
    }
    // Reserve a one-column gutter for the scrollbar when the table overflows.
    let show_bar = total > view_h && chunks[1].width > 1;
    let row_area = if show_bar {
        Rect { width: chunks[1].width - 1, ..chunks[1] }
    } else {
        chunks[1]
    };
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect { x: chunks[1].x + chunks[1].width - 1, ..chunks[1] };
        draw_scrollbar(frame, sb_area, p.selected, total.saturating_sub(1));
    }

    let hint = Line::from(Span::styled(t!("ui.x11_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[2]);

    app.layout.x11_panel = Rect {
        x: chunks[1].x,
        y: chunks[1].y,
        width: row_area.width,
        height: (view_h as u16).min(chunks[1].height),
    };
}

fn draw_html_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let entities = vix_html_character_picker::entities();
    let total = entities.len();
    if app.html_panel.is_none() || total == 0 {
        return;
    }
    let width = 46u16.min(area.width);
    let max_rows = area.height.saturating_sub(4).max(1);
    let rows = (total as u16).min(max_rows);
    let height = (rows + 4).min(area.height);
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
        .title(format!(" {} {} ", icon::TABLE, t!("ui.html")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let header = Line::from(Span::styled(t!("ui.html_header").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(header), chunks[0]);

    let view_h = chunks[1].height as usize;
    if let Some(p) = app.html_panel.as_mut() {
        p.ensure_visible(view_h);
    }
    let p = app.html_panel.as_ref().unwrap();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for (i, e) in entities[p.scroll..(p.scroll + view_h).min(total)].iter().enumerate() {
        let text = format!("  {:2}  {:26}  {}", e.glyph, e.name, e.code);
        let idx = p.scroll + i;
        if idx == p.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(Span::raw(text)));
        }
    }
    // Reserve a one-column gutter for the scrollbar when the table overflows.
    let show_bar = total > view_h && chunks[1].width > 1;
    let row_area = if show_bar {
        Rect { width: chunks[1].width - 1, ..chunks[1] }
    } else {
        chunks[1]
    };
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect { x: chunks[1].x + chunks[1].width - 1, ..chunks[1] };
        draw_scrollbar(frame, sb_area, p.selected, total.saturating_sub(1));
    }

    let hint = Line::from(Span::styled(t!("ui.html_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[2]);

    app.layout.html_panel = Rect {
        x: chunks[1].x,
        y: chunks[1].y,
        width: row_area.width,
        height: (view_h as u16).min(chunks[1].height),
    };
}

fn draw_outline(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.outline.is_none() {
        return;
    }
    let n = app.outline.as_ref().unwrap().len();
    let width = 48u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = (n as u16).min(max_rows);
    let height = (rows + 3).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::CODE, t!("ui.outline")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view_h = chunks[0].height as usize;
    if let Some(o) = app.outline.as_mut() {
        o.ensure_visible(view_h);
    }
    let o = app.outline.as_ref().unwrap();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in o.scroll..(o.scroll + view_h).min(o.len()) {
        let e = &o.entries[idx];
        let kind = if e.kind.is_empty() {
            String::new()
        } else {
            format!("{:<7} ", e.kind)
        };
        let text = format!("  {kind}{}", e.name);
        if idx == o.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!("  {kind}"), theme::dim()),
                Span::raw(e.name.clone()),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);

    let hint = Line::from(Span::styled(t!("ui.outline_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.outline = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: chunks[0].width,
        height: (view_h as u16).min(chunks[0].height),
    };
}

fn draw_dashboard(app: &App, frame: &mut Frame, area: Rect) {
    let Some(d) = app.dashboard.as_ref() else { return };
    let pending = t!("ui.dashboard_computing");
    let num = |n: Option<u64>| n.map_or_else(|| pending.to_string(), |v| v.to_string());
    let rows = [
        (t!("ui.dashboard_folder"), d.folder.clone()),
        (t!("ui.dashboard_disk"), d.disk_usage.clone().unwrap_or_else(|| pending.to_string())),
        (t!("ui.dashboard_files"), num(d.file_count)),
        (t!("ui.dashboard_commits"), num(d.commit_count)),
    ];
    let width = 52u16.min(area.width);
    let height = (rows.len() as u16 + 4).min(area.height);
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

    let hint = Line::from(Span::styled(t!("ui.dashboard_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
}

fn draw_contacts(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(total) = app.contacts.as_ref().map(vix_contact_panel::Panel::len) else { return };
    let width = 40u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = (total.max(1) as u16).min(max_rows);
    let height = (rows + 3).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::FOLDER, t!("ui.contacts")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let view_h = chunks[0].height as usize;
    if let Some(p) = app.contacts.as_mut() {
        p.ensure_visible(view_h);
    }
    let p = app.contacts.as_ref().unwrap();
    let show_bar = total > view_h && chunks[0].width > 1;
    let list_area = if show_bar { Rect { width: chunks[0].width - 1, ..chunks[0] } } else { chunks[0] };
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    if p.is_empty() {
        lines.push(Line::from(Span::styled(t!("ui.no_contacts").to_string(), theme::dim())));
    } else {
        for idx in p.scroll..(p.scroll + view_h).min(total) {
            let text = format!("  {}", p.contacts[idx].name);
            if idx == p.selected {
                lines.push(Line::from(Span::styled(text, theme::selected())));
            } else {
                lines.push(Line::from(Span::raw(text)));
            }
        }
    }
    frame.render_widget(Paragraph::new(lines), list_area);
    if show_bar {
        let sb_area = Rect { x: chunks[0].x + chunks[0].width - 1, ..chunks[0] };
        draw_scrollbar(frame, sb_area, p.selected, total.saturating_sub(1));
    }
    let hint = Line::from(Span::styled(t!("ui.contacts_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
    app.layout.contacts = Rect { x: chunks[0].x, y: chunks[0].y, width: list_area.width, height: (view_h as u16).min(chunks[0].height) };
}

fn draw_vcard(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(total) = app.vcard.as_ref().map(vix_vcard_panel::Panel::len) else { return };
    let title = app.vcard.as_ref().map(vix_vcard_panel::Panel::title).unwrap_or_default();
    let width = 60u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = (total.max(1) as u16).min(max_rows);
    let height = (rows + 3).min(area.height);
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
        .title(format!(" {} {} ", icon::INFO, title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let view_h = chunks[0].height as usize;
    if let Some(p) = app.vcard.as_mut() {
        p.ensure_visible(view_h);
    }
    let p = app.vcard.as_ref().unwrap();
    let show_bar = total > view_h && chunks[0].width > 1;
    let list_area = if show_bar { Rect { width: chunks[0].width - 1, ..chunks[0] } } else { chunks[0] };
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in p.scroll..(p.scroll + view_h).min(total) {
        let row = &p.rows[idx];
        let text = format!("  {:<14} {}", row.label, row.value);
        if idx == p.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(Span::raw(text)));
        }
    }
    frame.render_widget(Paragraph::new(lines), list_area);
    if show_bar {
        let sb_area = Rect { x: chunks[0].x + chunks[0].width - 1, ..chunks[0] };
        draw_scrollbar(frame, sb_area, p.selected, total.saturating_sub(1));
    }
    let hint = Line::from(Span::styled(t!("ui.vcard_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
    app.layout.vcard = Rect { x: chunks[0].x, y: chunks[0].y, width: list_area.width, height: (view_h as u16).min(chunks[0].height) };
}

fn draw_file_info(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.file_info.is_none() {
        return;
    }
    let n = app.file_info.as_ref().unwrap().len();
    let width = 64u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = (n as u16).min(max_rows);
    let height = (rows + 3).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::INFO, t!("ui.file_info")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view_h = chunks[0].height as usize;
    if let Some(p) = app.file_info.as_mut() {
        p.ensure_visible(view_h);
    }
    let p = app.file_info.as_ref().unwrap();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in p.scroll..(p.scroll + view_h).min(p.len()) {
        let row = &p.rows[idx];
        let text = format!("  {:<14} {}", row.label, row.value);
        if idx == p.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(Span::raw(text)));
        }
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);

    let hint = Line::from(Span::styled(t!("ui.system_info_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.file_info = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: chunks[0].width,
        height: (view_h as u16).min(chunks[0].height),
    };
}

fn draw_system_info(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.system_info.is_none() {
        return;
    }
    let n = app.system_info.as_ref().unwrap().len();
    let width = 60u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = (n as u16).min(max_rows);
    let height = (rows + 3).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::INFO, t!("ui.system_info")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view_h = chunks[0].height as usize;
    if let Some(p) = app.system_info.as_mut() {
        p.ensure_visible(view_h);
    }
    let p = app.system_info.as_ref().unwrap();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in p.scroll..(p.scroll + view_h).min(p.len()) {
        let row = &p.rows[idx];
        let line = if row.is_heading() {
            Line::from(Span::styled(row.label.clone(), theme::title(true)))
        } else {
            let text = format!("  {:<16} {}", row.label, row.value);
            if idx == p.selected {
                Line::from(Span::styled(text, theme::selected()))
            } else {
                Line::from(Span::raw(text))
            }
        };
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);

    let hint = Line::from(Span::styled(t!("ui.system_info_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.system_info = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: chunks[0].width,
        height: (view_h as u16).min(chunks[0].height),
    };
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

fn draw_workspace_search(app: &App, frame: &mut Frame, area: Rect) {
    let Some(ps) = app.workspace_search.as_ref() else { return };
    let rect = centered(area, 80, 80);
    frame.render_widget(Clear, rect);
    let title = if ps.static_results {
        format!(" {} {} ", icon::SEARCH, t!("ui.goto_definition"))
    } else if ps.replacing {
        format!(" {} {} ", icon::SEARCH, t!("ui.search_replace_workspace"))
    } else {
        format!(" {} {} ", icon::SEARCH, t!("ui.search_workspace"))
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    // find (+ replace) + include-path + exclude-path + toggles + status.
    let head = if ps.replacing { 6 } else { 5 };
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
    header.push(field_line(&t!("ui.field_include"), &ps.include_path, ps.field == Field::IncludePath));
    header.push(field_line(&t!("ui.field_exclude"), &ps.exclude_path, ps.field == Field::ExcludePath));
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

fn draw_search(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(s) = app.search.as_ref() else { return };
    let height = if s.replacing { 5 } else { 4 };
    let width = (f32::from(area.width) * 0.7) as u16;
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
    app.layout.search = inner;

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
    } else if s.replacing {
        t!("ui.search_hint_replace").to_string()
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
    // The workspace→dock search prompt shows case/regex toggles on a second line.
    let toggles = matches!(p.kind, crate::app::PromptKind::SearchToDock);
    let width = (f32::from(area.width) * 0.6) as u16;
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + area.height / 3,
        width,
        height: if toggles { 4 } else { 3 },
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

    let input = Line::from(vec![
        Span::styled("\u{276f} ", theme::title(true)),
        Span::raw(p.input.clone()),
        Span::styled("\u{2588}", theme::dim()),
    ]);
    if toggles {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(inner);
        frame.render_widget(Paragraph::new(input), rows[0]);
        let on = |b: bool| if b { "on" } else { "off" };
        let hint = format!("Alt C case: {}   Alt R regex: {}", on(p.case_sensitive), on(p.regex));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(hint, theme::dim()))),
            rows[1],
        );
    } else {
        frame.render_widget(Paragraph::new(input), inner);
    }
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
