//! All rendering. `draw` lays out the frame, records pane rectangles for mouse
//! hit-testing, and delegates to per-pane helpers.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap,
};

use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::StatefulImage;

use crate::app::{App, Focus};
use crate::calendar;
use crate::clock;
use crate::menu::menus;
use crate::messages::Level;
use crate::search::Field;
use crate::theme::{self, icon};

/// The body's column rectangles: file explorer, center editor, message drawer,
/// and outline sidebar. `None` for any dock that is hidden.
struct BodyColumns {
    explorer: Option<Rect>,
    center: Rect,
    messages: Option<Rect>,
    outline: Option<Rect>,
    debug: Option<Rect>,
    test: Option<Rect>,
}

/// Split the body area into explorer | center | messages | outline | debug | test
/// columns based on which docks are shown. Dock widths come from settings,
/// clamped so the editor keeps room.
fn body_columns(app: &App, body: Rect) -> BodyColumns {
    let dock_max = body.width.saturating_sub(20).max(12);
    let mut constraints = Vec::new();
    if app.show_explorer {
        constraints.push(Constraint::Length(app.settings.explorer_width.clamp(12, dock_max)));
    }
    constraints.push(Constraint::Min(20));
    if app.show_messages {
        constraints.push(Constraint::Length(app.settings.messages_width.clamp(12, dock_max)));
    }
    if app.settings.show_outline_dock {
        constraints.push(Constraint::Length(app.settings.outline_width.clamp(12, dock_max)));
    }
    if app.show_debug_panel {
        constraints.push(Constraint::Length(app.settings.debug_width.clamp(12, dock_max)));
    }
    if app.show_test_panel {
        constraints.push(Constraint::Length(app.settings.test_width.clamp(12, dock_max)));
    }
    let cols = Layout::default().direction(Direction::Horizontal).constraints(constraints).split(body);
    let mut ci = 0;
    let take = |ci: &mut usize| {
        let r = cols[*ci];
        *ci += 1;
        r
    };
    let explorer_rect = app.show_explorer.then(|| take(&mut ci));
    let center_rect = take(&mut ci);
    let messages_rect = app.show_messages.then(|| take(&mut ci));
    let outline_rect = app.settings.show_outline_dock.then(|| take(&mut ci));
    let debug_rect = app.show_debug_panel.then(|| take(&mut ci));
    let test_rect = app.show_test_panel.then(|| take(&mut ci));
    BodyColumns {
        explorer: explorer_rect,
        center: center_rect,
        messages: messages_rect,
        outline: outline_rect,
        debug: debug_rect,
        test: test_rect,
    }
}

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

    let BodyColumns {
        explorer: explorer_rect,
        center: center_rect,
        messages: messages_rect,
        outline: outline_rect,
        debug: debug_rect,
        test: test_rect,
    } = body_columns(app, body);

    // Center: tab bar, optional breadcrumb bar, then editor+scrollbar.
    let (tabs_rect, breadcrumb_rect, editor_cell) = center_split(center_rect, app.show_breadcrumbs);
    app.layout.tabs = tabs_rect;

    let editor_block = Block::default()
        .style(theme::region_base(theme::Region::Editor))
        // The center editor keeps only its top border (no left/right/bottom).
        .borders(Borders::TOP)
        .border_type(BorderType::Rounded)
        .border_style(theme::region_title(theme::Region::Editor, app.focus == Focus::Editor));
    let editor_inner = editor_block.inner(editor_cell);

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
    draw_tabs(app, frame, tabs_rect);
    if let Some(r) = breadcrumb_rect {
        draw_breadcrumb(app, frame, r);
    }
    frame.render_widget(editor_block, editor_cell);
    draw_editor_region(app, frame, editor_inner);
    if let Some(r) = messages_rect {
        draw_messages(app, frame, r);
    }
    if let Some(r) = outline_rect {
        draw_outline_dock(app, frame, r);
    }
    if let Some(r) = debug_rect {
        draw_debug_panel(app, frame, r);
    }
    if let Some(r) = test_rect {
        draw_test_panel(app, frame, r);
    }
    if let Some(r) = bottom_dock_rect {
        app.layout.bottom_dock = r;
        draw_bottom_dock(app, frame, r);
    }
    if let Some(r) = status_row {
        draw_status_bar(app, frame, r);
    }

    draw_overlays(app, frame, area, rows[0]);
}

/// Draw any active overlay on top of the base frame. Split out of `draw` to keep
/// each function focused; behavior is identical to inlining this dispatch.
fn draw_overlays(app: &mut App, frame: &mut Frame, area: Rect, menu_bar: Rect) {
    // Overlays.
    if app.show_calendar {
        draw_calendar(app, frame, area);
    }
    if app.show_clock {
        draw_clock(app, frame, area);
    }
    if app.menu.is_open() {
        if let Some(i) = app.menu.open {
            app.layout.menu_dropdown = menu_dropdown_rect(area, menu_bar, i);
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
    if app.replace_confirm.is_some() {
        draw_replace_confirm(app, frame, area);
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
    if app.task_chooser.is_some() {
        draw_task_chooser(app, frame, area);
    }
    if app.diff_view.is_some() {
        draw_diff_view(app, frame, area);
    }
    if app.location_chooser.is_some() {
        draw_location_chooser(app, frame, area);
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
    if app.edit_table.is_some() {
        draw_edit_table(app, frame, area);
    }
    if app.edit_outline.is_some() {
        draw_edit_outline(app, frame, area);
    }
    if app.edit_value.is_some() {
        draw_edit_value(app, frame, area);
    }
    if app.edit_bytes.is_some() {
        draw_edit_bytes(app, frame, area);
    }
    if app.qrcode.is_some() {
        draw_qrcode(app, frame, area);
    }
    if app.ai_panel.is_some() {
        draw_ai_panel(app, frame, area);
    }
    if app.terminal.is_some() {
        draw_terminal(app, frame, area);
    }
    if app.ai_diff_review().is_some() {
        draw_ai_diff(app, frame, area);
    }
    if app.x11_panel.is_some() {
        draw_x11_panel(app, frame, area);
    }
    if app.html_panel.is_some() {
        draw_html_panel(app, frame, area);
    }
    draw_overlays_aux(app, frame, area);
}

/// Second half of the overlay dispatch (split from `draw_overlays` to satisfy the
/// per-function line limit). Behavior is identical to inlining this dispatch.
fn draw_overlays_aux(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.edit_sql.is_some() {
        draw_edit_sql(app, frame, area);
    }
    if app.media_type_panel.is_some() {
        draw_media_type_panel(app, frame, area);
    }
    if app.macro_chooser.is_some() {
        draw_macro_chooser(app, frame, area);
    }
    if app.workspace_chooser.is_some() {
        draw_workspace_chooser(app, frame, area);
    }
    if app.system_info.is_some() {
        draw_system_info(app, frame, area);
    }
    if app.file_info.is_some() {
        draw_file_info(app, frame, area);
    }
    if app.text_info.is_some() {
        draw_text_info(app, frame, area);
    }
    if app.markdown_preview.is_some() {
        draw_markdown_preview(app, frame, area);
    }
    if app.snippets.is_some() {
        draw_snippets(app, frame, area);
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
        draw_help(app, frame, area);
    }
    if app.dialog.is_some() {
        draw_dialog(app, frame, area);
    }
    if app.color_converter.is_some() {
        draw_color_converter(app, frame, area);
    }
    if app.unit_converter.is_some() {
        draw_unit_converter(app, frame, area);
    }
    if app.calculator.is_some() {
        draw_calculator(app, frame, area);
    }
    if app.regex_tester.is_some() {
        draw_regex_tester(app, frame, area);
    }
    if app.code_actions.is_some() {
        draw_code_actions(app, frame, area);
    }
    if app.code_lens.is_some() {
        draw_code_lens(app, frame, area);
    }
    if app.pomodoro_open {
        draw_pomodoro(app, frame, area);
    }
    if app.welcome.is_some() {
        draw_welcome(app, frame, area);
    }
}

fn draw_pomodoro(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::pomodoro_tool::Phase;
    let Some(timer) = app.pomodoro.as_ref() else { return };
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
        Paragraph::new(Line::from(Span::styled(big, theme::selected()))).alignment(Alignment::Center),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(format!("[ {button} ]"), theme::selected())))
            .alignment(Alignment::Center),
        rows[1],
    );
    app.layout.pomodoro_button = rows[1];
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint.to_string(), theme::dim()))).alignment(Alignment::Center),
        rows[3],
    );
}

fn draw_welcome(app: &mut App, frame: &mut Frame, area: Rect) {
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
    let total = app.welcome.as_ref().map_or(0, crate::welcome_panel::Panel::len);
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
    frame.render_widget(Paragraph::new(visible), text_area);
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
    let width = (u16::try_from(widest).unwrap_or(u16::MAX) + 2).clamp(16, area.width.saturating_sub(2));
    let height = u16::try_from(max_rows).unwrap_or(u16::MAX) + 2;

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
    let height = (u16::try_from(rows).unwrap_or(u16::MAX) + 2).clamp(3, 12.min(area.height));

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
    let content_w = u16::try_from(body.chars().count().max(title.chars().count())).unwrap_or(u16::MAX);
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

fn draw_color_converter(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::color_converter_tool::Field;
    let Some(conv) = app.color_converter.as_ref() else { return };

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
        let style = if focused { theme::selected() } else { theme::base() };
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

fn draw_code_actions(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(menu) = app.code_actions.as_ref() else { return };
    let titles: Vec<&str> = menu.actions.iter().map(|(t, _)| t.as_str()).collect();
    draw_chooser(frame, area, &t!("menu.item.lsp.code_action"), &titles, menu.selected);
}

fn draw_code_lens(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(menu) = app.code_lens.as_ref() else { return };
    let titles: Vec<&str> = menu.lenses.iter().map(|(_, t, _, _)| t.as_str()).collect();
    draw_chooser(frame, area, &t!("menu.item.lsp.code_lens"), &titles, menu.selected);
}

/// A centered single-column chooser: a bordered list of `titles` with `selected`
/// highlighted. Shared by the code-action and code-lens menus.
fn draw_chooser(frame: &mut Frame, area: Rect, title: &str, titles: &[&str], selected: usize) {
    let longest = titles.iter().map(|t| t.chars().count()).max().unwrap_or(20);
    let width = u16::try_from(longest).unwrap_or(u16::MAX).saturating_add(4).clamp(24, area.width);
    let rows_n = u16::try_from(titles.len()).unwrap_or(u16::MAX);
    let height = rows_n.saturating_add(2).min(area.height);
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
    let rows: Vec<Line> = titles
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let style = if i == selected { theme::selected() } else { theme::base() };
            Line::from(Span::styled(format!(" {t} "), style))
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), inner);
}

fn draw_regex_tester(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::regex_tool::{Field, Outcome};
    let Some(t) = app.regex_tester.as_ref() else { return };

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
        let style = if focused { theme::selected() } else { theme::base() };
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
            vec![Line::from(Span::styled(t!("status.no_matches").to_string(), theme::dim()))]
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

fn draw_calculator(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::calculator_tool::{Focus, Outcome};
    let Some(calc) = app.calculator.as_ref() else { return };

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

    let input_style = if calc.focus == Focus::Input { theme::selected() } else { theme::base() };
    let caret = if calc.focus == Focus::Input { "_" } else { "" };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(format!(" {}{caret}", calc.input), input_style))),
        rows[0],
    );
    app.layout.calculator_rects[0] = rows[0];

    // Buttons: [ Run ] [ Insert ].
    let btn = |label: String, focused: bool| {
        let style = if focused { theme::selected() } else { theme::dim() };
        Span::styled(format!("[ {label} ] "), style)
    };
    let buttons = Line::from(vec![
        Span::raw(" "),
        btn(t!("ui.calculator_run").to_string(), calc.focus == Focus::Run),
        btn(t!("ui.calculator_insert").to_string(), calc.focus == Focus::Insert),
    ]);
    frame.render_widget(Paragraph::new(buttons), rows[1]);
    // Approximate button hit rects: split the buttons row in two halves.
    let half = rows[1].width / 2;
    app.layout.calculator_rects[1] = Rect { x: rows[1].x, y: rows[1].y, width: half, height: 1 };
    app.layout.calculator_rects[2] =
        Rect { x: rows[1].x + half, y: rows[1].y, width: rows[1].width - half, height: 1 };

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

fn draw_unit_converter(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::unit_converter_tool::{Focus, UNITS};
    let Some(conv) = app.unit_converter.as_ref() else { return };

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

    let field_style = |focused: bool| if focused { theme::selected() } else { theme::base() };

    // Value field.
    let caret = if conv.focus == Focus::Value { "_" } else { "" };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {:<8}", t!("ui.unit_value")), theme::dim()),
            Span::styled(format!("{}{caret}", conv.value), field_style(conv.focus == Focus::Value)),
        ])),
        rows[0],
    );
    // From selector.
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {:<8}", t!("ui.unit_from")), theme::dim()),
            Span::styled(format!("‹ {} ›", UNITS[conv.from].label), field_style(conv.focus == Focus::From)),
        ])),
        rows[1],
    );
    // To selector, with the live output to its right.
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {:<8}", t!("ui.unit_to")), theme::dim()),
            Span::styled(format!("‹ {} ›", UNITS[conv.to].label), field_style(conv.focus == Focus::To)),
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

fn draw_confirm(app: &App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.confirm.as_ref() else { return };
    let width = (u16::try_from(c.message.chars().count()).unwrap_or(u16::MAX) + 6).min(area.width);
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

fn draw_replace_confirm(app: &App, frame: &mut Frame, area: Rect) {
    let Some(rc) = app.replace_confirm.as_ref() else { return };
    let width = (area.width * 7 / 10).clamp(30, area.width);
    let height = (area.height * 6 / 10).clamp(8, area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 3,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let title = format!(
        " {} {} ",
        icon::SEARCH,
        t!("ui.replace_confirm_title", replaced = rc.replaced, files = rc.plan.len())
    );
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
    let view_h = chunks[0].height as usize;
    let start = rc.scroll.min(rc.lines.len().saturating_sub(1));
    let lines: Vec<Line> = rc
        .lines
        .iter()
        .skip(start)
        .take(view_h)
        .map(|l| Line::from(Span::raw(l.clone())))
        .collect();
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    let hint = Line::from(Span::styled(t!("ui.replace_confirm_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
}

fn draw_unsaved(app: &App, frame: &mut Frame, area: Rect) {
    let Some(u) = app.unsaved.as_ref() else { return };
    let message = t!("ui.unsaved_prompt", name = u.name).to_string();
    let choices = t!("ui.unsaved_choices");
    let width = (u16::try_from(message.chars().count().max(choices.chars().count())).unwrap_or(u16::MAX) + 6).min(area.width);
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

fn draw_diff_view(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::diff_view::Kind;
    let Some(d) = app.diff_view.as_ref() else { return };
    let width = (area.width * 8 / 10).clamp(30, area.width);
    let height = (area.height * 8 / 10).clamp(8, area.height);
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
        .title(format!(" {} {} ", icon::INFO, d.title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let view_h = chunks[0].height as usize;
    let start = d.scroll.min(d.lines.len().saturating_sub(1));
    let lines: Vec<Line> = d
        .lines
        .iter()
        .skip(start)
        .take(view_h)
        .map(|l| {
            let (prefix, style) = match l.kind {
                Kind::Add => ("+ ", Style::default().fg(Color::Green)),
                Kind::Del => ("- ", Style::default().fg(Color::Red)),
                Kind::Context => ("  ", theme::dim()),
                Kind::Sep => ("  ", theme::dim().add_modifier(Modifier::DIM)),
            };
            Line::from(Span::styled(format!("{prefix}{}", l.text), style))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    let hint = Line::from(Span::styled(t!("ui.diff_view_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
}

fn draw_workspace_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.workspace_chooser.as_ref() else { return };
    let hint = t!("ui.projects_hint");
    app.layout.chooser = draw_list_chooser(frame, area, &t!("ui.projects"), &hint, &c.roots, c.selected);
}

fn draw_macro_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.macro_chooser.as_ref() else { return };
    let labels: Vec<String> = c.macros.iter().map(|m| format!("{} ({} keys)", m.name, m.keys.len())).collect();
    let hint = t!("ui.macros_hint");
    app.layout.chooser = draw_list_chooser(frame, area, &t!("ui.macros"), &hint, &labels, c.selected);
}

fn draw_task_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(c) = app.task_chooser.as_ref() else { return };
    // Show "name — command" so the action is clear before running it.
    let labels: Vec<String> = c.tasks.iter().map(|t| format!("{} — {}", t.name, t.command)).collect();
    let hint = t!("ui.tasks_hint");
    app.layout.chooser = draw_list_chooser(frame, area, &t!("ui.tasks"), &hint, &labels, c.selected);
}

fn draw_git_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.git_panel.is_none() {
        return;
    }
    let selected = app.git_panel.as_ref().unwrap().selected;
    let rows = app.git_status.len().max(1);
    let width = 64u16.min(area.width);
    let max_rows = area.height.saturating_sub(4).max(1);
    let visible = u16::try_from(rows).unwrap_or(u16::MAX).min(max_rows);
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
                let letter = change.map_or(' ', crate::git::Change::letter);
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
        height: u16::try_from(app.git_status.len()).unwrap_or(u16::MAX).min(chunks[0].height),
    };
}

fn draw_context_menu(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::app::CONTEXT_ITEMS;
    let Some(cm) = app.context_menu.as_ref() else { return };
    let labels: Vec<String> =
        CONTEXT_ITEMS.iter().map(|&(label, action)| {
            if action == "menu.separator" { String::new() } else { t!(label).to_string() }
        }).collect();
    let width = (u16::try_from(labels.iter().map(|l| l.chars().count()).max().unwrap_or(8)).unwrap_or(u16::MAX) + 4).min(area.width);
    let height = (u16::try_from(CONTEXT_ITEMS.len()).unwrap_or(u16::MAX) + 2).min(area.height);
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
    let width = (u16::try_from(widest).unwrap_or(u16::MAX) + 6).min(area.width);
    // Borders (2) + suggestion rows + hint (1).
    let height = (u16::try_from(rows).unwrap_or(u16::MAX) + 3).min(area.height);
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
        height: u16::try_from(p.suggestions.len()).unwrap_or(u16::MAX).min(chunks[0].height),
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
    let height = (u16::try_from(labels.len()).unwrap_or(u16::MAX) + 4).min(area.height);
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

fn draw_location_chooser(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(lc) = app.location_chooser.as_ref() else { return };
    let selected = lc.selected;
    // Show the file name and line first (survives truncation), then its directory.
    let labels: Vec<String> = lc
        .entries
        .iter()
        .map(|loc| {
            let name = loc
                .path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            match loc.path.parent() {
                Some(dir) if !dir.as_os_str().is_empty() => {
                    format!("{name}:{}  —  {}", loc.line, dir.display())
                }
                _ => format!("{name}:{}", loc.line),
            }
        })
        .collect();
    let hint = t!("ui.locations_hint");
    app.layout.chooser =
        draw_list_chooser(frame, area, &t!("ui.locations"), &hint, &labels, selected);
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
    let mut pos: u16 = 0;
    for m in menus() {
        offsets.push(pos);
        pos += u16::try_from(m.title().chars().count()).unwrap_or(u16::MAX) + 2;
    }
    offsets
}

fn draw_menu_bar(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans = Vec::new();
    for (i, m) in menus().iter().enumerate() {
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
    let w = items
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
        .max(14);
    u16::try_from(w).unwrap_or(u16::MAX)
}

/// Geometry of the dropdown for the menu at `index`. Shared by the renderer and
/// by mouse hit-testing (`App::on_mouse`) so clicks land on the right item.
#[must_use]
pub fn menu_dropdown_rect(frame_area: Rect, bar: Rect, index: usize) -> Rect {
    let def = &menus()[index];
    let x = bar.x + menu_offsets()[index];
    let width = dropdown_width(def.items);
    let height = u16::try_from(def.items.len()).unwrap_or(u16::MAX) + 2;
    let y = bar.y + 1;
    Rect {
        x: x.min(frame_area.width.saturating_sub(width)),
        y,
        width: width.min(frame_area.width),
        height: height.min(frame_area.height.saturating_sub(y)),
    }
}

/// First visible item index for a dropdown of `len` items in an inner viewport of
/// `inner_h` rows, keeping `selected` visible. Shared by rendering and mouse
/// hit-testing so a scrolled dropdown maps clicks to the right item.
#[must_use]
pub fn dropdown_scroll(selected: Option<usize>, inner_h: usize, len: usize) -> usize {
    if len <= inner_h || inner_h == 0 {
        return 0;
    }
    let max = len - inner_h;
    match selected {
        Some(s) if s >= inner_h => (s + 1 - inner_h).min(max),
        _ => 0,
    }
}

/// Render one dropdown (Clear + bordered list) at `area`, highlighting `selected`
/// and scrolling so it stays visible. A `●` scrollbar marks the right edge when
/// the items overflow.
fn render_dropdown(frame: &mut Frame, area: Rect, items: &[crate::menu::Item], selected: Option<usize>) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let inner_h = inner.height as usize;
    let offset = dropdown_scroll(selected, inner_h, items.len());
    let end = (offset + inner_h).min(items.len());
    let text_w = inner.width as usize;
    let rows: Vec<Line> = items[offset..end]
        .iter()
        .enumerate()
        .map(|(vis, it)| {
            if it.is_separator() {
                return Line::from(Span::styled("─".repeat(text_w), theme::dim()));
            }
            let label = it.label();
            let right = item_right(it);
            let pad = text_w.saturating_sub(label.chars().count() + right.chars().count() + 2);
            let style = if selected == Some(offset + vis) { theme::selected() } else { theme::base() };
            Line::from(vec![
                Span::styled(format!(" {label}"), style),
                Span::styled(" ".repeat(pad), style),
                Span::styled(format!("{right} "), if selected == Some(offset + vis) { style } else { theme::dim() }),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), inner);

    if items.len() > inner_h {
        // Draw the thumb over the right border column.
        let sb = Rect { x: area.x + area.width - 1, y: inner.y, width: 1, height: inner.height };
        draw_scrollbar(frame, sb, selected.unwrap_or(0), items.len().saturating_sub(1));
    }
}

fn draw_menu_dropdown(app: &mut App, frame: &mut Frame) {
    let Some(i) = app.menu.open else { return };
    let area = app.layout.menu_dropdown;
    render_dropdown(frame, area, menus()[i].items, app.menu.item);

    // An open submenu is drawn to the right of its parent item. It may be open
    // with nothing highlighted yet (`app.menu.sub == None`).
    if app.menu.submenu_open()
        && let Some(subitems) = app.menu.submenu_items() {
            let fa = frame.area();
            let sub_w = dropdown_width(subitems);
            let sub_h = u16::try_from(subitems.len()).unwrap_or(u16::MAX) + 2;
            let sub_x = (area.x + area.width).min(fa.width.saturating_sub(sub_w));
            let parent_row = u16::try_from(app.menu.item.unwrap_or(0)).unwrap_or(u16::MAX);
            let sub_y = (area.y + parent_row).min(fa.height.saturating_sub(sub_h));
            let sub_area = Rect {
                x: sub_x,
                y: sub_y,
                width: sub_w.min(fa.width),
                height: sub_h.min(fa.height.saturating_sub(sub_y)),
            };
            app.layout.submenu_dropdown = sub_area;
            render_dropdown(frame, sub_area, subitems, app.menu.sub);

            // A third-level submenu is drawn to the right of its parent row.
            if app.menu.subsubmenu_open()
                && let Some(ssitems) = app.menu.subsubmenu_items() {
                    let ss_w = dropdown_width(ssitems);
                    let ss_h = u16::try_from(ssitems.len()).unwrap_or(u16::MAX) + 2;
                    let ss_x = (sub_area.x + sub_area.width).min(fa.width.saturating_sub(ss_w));
                    let prow = u16::try_from(app.menu.sub.unwrap_or(0)).unwrap_or(u16::MAX);
                    let ss_y = (sub_area.y + prow).min(fa.height.saturating_sub(ss_h));
                    let ss_area = Rect {
                        x: ss_x,
                        y: ss_y,
                        width: ss_w.min(fa.width),
                        height: ss_h.min(fa.height.saturating_sub(ss_y)),
                    };
                    app.layout.subsubmenu_dropdown = ss_area;
                    render_dropdown(frame, ss_area, ssitems, app.menu.subsub);
                }
        }
}

/// The badge color for a git change in the file explorer.
fn git_change_color(change: crate::git::Change) -> Color {
    use crate::git::Change;
    match change {
        Change::Added | Change::Untracked => Color::Green,
        Change::Modified => Color::Yellow,
        Change::Deleted => Color::Red,
        Change::Renamed => Color::Cyan,
        Change::Conflicted => Color::Magenta,
    }
}

fn draw_explorer(app: &mut App, frame: &mut Frame, area: Rect) {
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

    let total = app.explorer.nodes.len();
    let allow_bars = app.settings.show_scrollbar && inner.width > 1 && inner.height > 1;
    let vbar = allow_bars && total > inner.height as usize;
    let text_w = if vbar { inner.width - 1 } else { inner.width } as usize;

    let top = app.explorer.top.min(total);
    // Build the full (unsliced) styled rows for the visible window.
    let win_h = inner.height as usize;
    let end = (top + win_h).min(total);
    let rows: Vec<Vec<Span<'static>>> = app.explorer.nodes[top..end]
        .iter()
        .map(|n| {
            let indent = "  ".repeat(n.depth);
            let glyph = if n.is_symlink {
                icon::LINK
            } else if n.is_dir {
                if n.expanded { icon::FOLDER_OPEN } else { icon::FOLDER }
            } else {
                theme::file_icon(&n.name)
            };
            let mut style = Style::default();
            let cut_pending = app.clip_cut && app.clip.contains(&n.path);
            if cut_pending {
                style = style.add_modifier(Modifier::DIM);
            }
            let mark = if app.explorer.marked.contains(&n.path) { "● " } else { "" };
            let mut spans =
                vec![Span::raw(indent), Span::styled(format!("{mark}{glyph} {}", n.name), style)];
            if !n.is_dir
                && let Some(change) = app.git_change_for(&n.path) {
                    spans.push(Span::styled(
                        format!("  {}", change.letter()),
                        Style::default().fg(git_change_color(change)),
                    ));
                }
            spans
        })
        .collect();

    let content_w = rows.iter().map(|s| span_line_width(s)).max().unwrap_or(0);
    let hbar = allow_bars && content_w > text_w;
    let body_h = if hbar { inner.height - 1 } else { inner.height } as usize;
    let hmax = content_w.saturating_sub(text_w);
    app.explorer_hmax = hmax;
    app.explorer_hscroll = app.explorer_hscroll.min(hmax);
    let off = app.explorer_hscroll;

    let visible = body_h.min(rows.len());
    let items: Vec<ListItem> = rows[..visible]
        .iter()
        .map(|spans| ListItem::new(Line::from(hslice_spans(spans, off, text_w))))
        .collect();
    let list_area = Rect {
        width: u16::try_from(text_w).unwrap_or(u16::MAX),
        height: u16::try_from(body_h).unwrap_or(u16::MAX),
        ..inner
    };
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    if app.explorer.selected >= top && app.explorer.selected < top + visible {
        state.select(Some(app.explorer.selected - top));
    }
    frame.render_stateful_widget(list, list_area, &mut state);

    if vbar {
        let sb = Rect { x: inner.x + inner.width - 1, y: inner.y, width: 1, height: u16::try_from(body_h).unwrap_or(u16::MAX) };
        draw_scrollbar(frame, sb, app.explorer.selected, total.saturating_sub(1));
    }
    app.layout.explorer_hscrollbar = if hbar {
        let hb = Rect { x: inner.x, y: inner.y + inner.height - 1, width: u16::try_from(text_w).unwrap_or(u16::MAX), height: 1 };
        draw_hscrollbar(frame, hb, off, hmax);
        hb
    } else {
        Rect::default()
    };
}

// Split the center column into the tab bar, an optional breadcrumb bar, and the
// editor cell. Returns (tabs, breadcrumb, editor).
fn center_split(area: Rect, breadcrumbs: bool) -> (Rect, Option<Rect>, Rect) {
    let dir = Direction::Vertical;
    if breadcrumbs {
        let c = Layout::default()
            .direction(dir)
            .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Min(1)])
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
fn draw_breadcrumb(app: &App, frame: &mut Frame, area: Rect) {
    let line = Line::from(Span::styled(format!(" {}", app.breadcrumb()), theme::dim()));
    frame.render_widget(Paragraph::new(line).style(theme::base()), area);
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

/// Render the editor region: a single pane, or two split panes with a divider.
fn draw_editor_region(app: &mut App, frame: &mut Frame, inner: Rect) {
    use crate::editor::SplitDir;
    app.layout.editor_region = inner;
    let panes = app.editor.split_layout(inner);
    if panes.is_empty() {
        let (editor_area, scrollbar_area) = if app.show_scrollbar {
            let s = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(inner);
            (s[0], s[1])
        } else {
            (inner, Rect { width: 0, height: 0, ..inner })
        };
        // Sticky scroll: reserve the top row for the enclosing scope's header.
        let header = if editor_area.height > 1 { app.sticky_header() } else { None };
        let (header_area, editor_area) = match &header {
            Some(_) => {
                let r = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Min(1)])
                    .split(editor_area);
                (Some(r[0]), r[1])
            }
            None => (None, editor_area),
        };
        app.layout.editor = editor_area;
        app.layout.scrollbar = scrollbar_area;
        draw_center(app, frame, editor_area, scrollbar_area);
        if let (Some(hrect), Some(text)) = (header_area, header) {
            let style = theme::region_title(theme::Region::Editor, true);
            frame.render_widget(Block::default().style(style), hrect);
            frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), hrect);
        }
        return;
    }

    // Draw split dividers (one per internal tree node).
    let dstyle = theme::region_title(theme::Region::Editor, true);
    for (dir, divider) in app.editor.split_dividers(inner) {
        if dir == SplitDir::Vertical {
            let col: Vec<Line> = (0..divider.height).map(|_| Line::from(Span::styled("│", dstyle))).collect();
            frame.render_widget(Paragraph::new(col), divider);
        } else {
            let row = "─".repeat(divider.width as usize);
            frame.render_widget(Paragraph::new(Line::from(Span::styled(row, dstyle))), divider);
        }
    }

    // Draw each pane; the focused leaf drives cursor/mouse mapping.
    let focused = app.editor.focused_leaf();
    let mut focused_rect = inner;
    for pane in panes {
        let text = draw_pane(app, frame, pane.rect, pane.tab);
        if pane.leaf == focused {
            focused_rect = text;
        }
    }
    app.layout.editor = focused_rect;
    app.layout.scrollbar = Rect::default();
    app.layout.editor_hscrollbar = Rect::default();
}

/// Render one split pane (tab `tab_index`) into `area` with its own vertical
/// scrollbar; returns the text rectangle (for mouse hit-testing).
fn draw_pane(app: &mut App, frame: &mut Frame, area: Rect, tab_index: usize) -> Rect {
    let (text, sb) = if app.show_scrollbar && area.width > 1 {
        let s = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        (s[0], s[1])
    } else {
        (area, Rect { width: 0, height: 0, ..area })
    };

    if app.editor.tabs.get(tab_index).is_some_and(super::editor::Tab::is_image) {
        if let Some(tab) = app.editor.tabs.get_mut(tab_index)
            && let Some(proto) = tab.image.as_mut() {
                frame.render_stateful_widget(StatefulImage::<StatefulProtocol>::new(), text, proto);
            }
        return text;
    }
    if let Some(tab) = app.editor.tabs.get(tab_index) {
        frame.render_widget(&tab.editor, text);
        if app.show_ruler {
            tint_ruler(frame, text, &tab.editor);
        }
        if sb.width > 0 {
            let total = tab.line_count().max(1);
            let pos = tab.cursor_1based().0.saturating_sub(1);
            draw_scrollbar(frame, sb, pos, total.saturating_sub(1));
        }
    }
    text
}

/// The text column the editor ruler guide marks.
pub const RULER_COLUMN: usize = 80;

/// Draw a faint vertical guide at [`RULER_COLUMN`] over an already-rendered
/// editor pane, accounting for the line-number gutter and horizontal scroll.
fn tint_ruler(frame: &mut Frame, text: Rect, editor: &super::editor::CodeEditor) {
    let off = editor.get_offset_x();
    if RULER_COLUMN < off {
        return; // scrolled past the guide
    }
    let gutter = u16::try_from(editor.gutter_width()).unwrap_or(u16::MAX);
    let Ok(rel) = u16::try_from(RULER_COLUMN - off) else { return };
    let x = text.x + gutter + rel;
    if x < text.x || x >= text.x + text.width {
        return;
    }
    let buf = frame.buffer_mut();
    for y in text.y..text.y + text.height {
        if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(x, y)) {
            if cell.symbol() == " " {
                cell.set_symbol("│");
            }
            cell.set_style(theme::dim());
        }
    }
}

fn draw_center(app: &mut App, frame: &mut Frame, text: Rect, scrollbar: Rect) {
    let is_image = app.editor.active_tab().is_some_and(super::editor::Tab::is_image);
    if is_image {
        if let Some(tab) = app.editor.active_tab_mut()
            && let Some(proto) = tab.image.as_mut() {
                frame.render_stateful_widget(StatefulImage::<StatefulProtocol>::new(), text, proto);
            }
        return;
    }
    let mut hbar_rect: Option<Rect> = None;
    if let Some(tab) = app.editor.active_tab() {
        let soft = tab.editor.soft_wrap_enabled();
        let gutter = tab.editor.gutter_width();
        let maxw = tab.editor.max_line_width();
        let off = tab.editor.get_offset_x();
        let text_visible = (text.width as usize).saturating_sub(gutter);
        // A horizontal scrollbar appears when not soft-wrapping and a line
        // overflows the visible text width (and the scrollbar is enabled).
        let hbar = app.settings.show_scrollbar && !soft && text.height > 1 && maxw > text_visible;
        let editor_area = if hbar { Rect { height: text.height - 1, ..text } } else { text };
        let vsb = if hbar {
            Rect { height: scrollbar.height.saturating_sub(1), ..scrollbar }
        } else {
            scrollbar
        };
        frame.render_widget(&tab.editor, editor_area);

        if vsb.width > 0 {
            let total = app.editor.active_line_count().max(1);
            let pos = app.editor.cursor_1based().0.saturating_sub(1);
            draw_scrollbar(frame, vsb, pos, total.saturating_sub(1));
        }
        if hbar {
            let gutter_w = u16::try_from(gutter).unwrap_or(u16::MAX);
            let hb = Rect {
                x: text.x + gutter_w,
                y: text.y + text.height - 1,
                width: text.width - gutter_w,
                height: 1,
            };
            draw_hscrollbar(frame, hb, off, maxw.saturating_sub(text_visible));
            hbar_rect = Some(hb);
            app.layout.editor = editor_area;
            app.editor_hmax = maxw.saturating_sub(text_visible);
        }
    }
    if hbar_rect.is_none() {
        app.editor_hmax = 0;
    }
    app.layout.editor_hscrollbar = hbar_rect.unwrap_or_default();
}

fn draw_test_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::test_runner::Status;
    let (pass, fail, ignore) = crate::test_runner::tally(&app.test_results);
    let block = Block::default()
        .style(theme::region_base(theme::Region::RightDock))
        .borders(Borders::TOP | Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(theme::region_title(theme::Region::RightDock, false))
        .title(format!(" {} {} {pass}/{fail}/{ignore} ", icon::CODE, t!("ui.tests")));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.layout.test_panel = inner;

    if app.test_results.is_empty() {
        let hint = Paragraph::new(t!("ui.tests_idle").to_string()).style(theme::dim()).wrap(Wrap { trim: true });
        frame.render_widget(hint, inner);
        return;
    }
    let view_h = inner.height as usize;
    let lines: Vec<Line> = app
        .test_results
        .iter()
        .enumerate()
        .take(view_h)
        .map(|(i, r)| {
            let (icon, style) = match r.status {
                Status::Pass => ("\u{2713} ", Style::default().fg(Color::Green)),
                Status::Fail => ("\u{2717} ", Style::default().fg(Color::Red)),
                Status::Ignore => ("\u{25cb} ", theme::dim()),
            };
            let row = Line::from(vec![Span::styled(icon, style), Span::raw(r.name.clone())]);
            if i == app.test_selected { row.style(theme::selected()) } else { row }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_debug_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .style(theme::region_base(theme::Region::RightDock))
        .borders(Borders::TOP | Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(theme::region_title(theme::Region::RightDock, false))
        .title(format!(" {} {} ", icon::CODE, t!("ui.debug")));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    let header = |lines: &mut Vec<Line>, text: String| {
        lines.push(Line::from(Span::styled(text, theme::title(true).add_modifier(Modifier::BOLD))));
    };
    if !app.dap.is_active() {
        let hint = Paragraph::new(t!("ui.debug_idle").to_string()).style(theme::dim()).wrap(Wrap { trim: true });
        frame.render_widget(hint, inner);
        return;
    }
    header(&mut lines, t!("ui.debug_call_stack").to_string());
    if app.dap_stack.is_empty() {
        lines.push(Line::from(Span::styled("  —", theme::dim())));
    }
    for f in &app.dap_stack {
        let loc = f.path.as_deref().and_then(|p| p.rsplit('/').next()).map_or(String::new(), |n| format!("  {n}:{}", f.line));
        lines.push(Line::from(vec![Span::raw(format!("  {}", f.name)), Span::styled(loc, theme::dim())]));
    }
    lines.push(Line::from(""));
    header(&mut lines, t!("ui.debug_variables").to_string());
    if app.dap_variables.is_empty() {
        lines.push(Line::from(Span::styled("  —", theme::dim())));
    }
    for v in &app.dap_variables {
        lines.push(Line::from(vec![
            Span::raw(format!("  {} = ", v.name)),
            Span::styled(v.value.clone(), theme::dim()),
        ]));
    }
    if !app.dap_watches.is_empty() {
        lines.push(Line::from(""));
        header(&mut lines, t!("ui.debug_watch").to_string());
        for (expr, result) in &app.dap_watches {
            lines.push(Line::from(vec![
                Span::raw(format!("  {expr} = ")),
                Span::styled(result.clone(), theme::dim()),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_outline_dock(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .style(theme::region_base(theme::Region::RightDock))
        .borders(Borders::TOP | Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(theme::region_title(theme::Region::RightDock, false))
        .title(format!(" {} {} ", icon::CODE, t!("ui.outline")));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.layout.outline_dock = inner;

    let Some(o) = app.outline_dock.as_mut() else {
        let hint = Paragraph::new(t!("status.outline_empty").to_string()).style(theme::dim()).wrap(Wrap { trim: true });
        frame.render_widget(hint, inner);
        return;
    };
    let view_h = inner.height as usize;
    o.ensure_visible(view_h);
    let total = o.len();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in o.scroll..(o.scroll + view_h).min(total) {
        let e = &o.entries[idx];
        let kind = if e.kind.is_empty() { String::new() } else { format!("{:<6} ", e.kind) };
        if idx == o.selected {
            lines.push(Line::from(Span::styled(format!("{kind}{}", e.name), theme::selected())));
        } else {
            lines.push(Line::from(vec![
                Span::styled(kind, theme::dim()),
                Span::raw(e.name.clone()),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_messages(app: &mut App, frame: &mut Frame, area: Rect) {
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

    let rows: Vec<Vec<Span<'static>>> = app
        .messages
        .items
        .iter()
        .map(|m| {
            let (sym, sym_style) = match m.level {
                Level::Info | Level::Advice => (icon::INFO, Style::default()),
                Level::Warn => (icon::BELL, Style::default()),
                Level::Error => (icon::CLOSE, Style::default()),
            };
            vec![
                Span::styled(format!("{sym} "), sym_style),
                Span::raw(m.text.clone()),
                Span::styled(format!("  {}", icon::CLOSE), theme::dim()),
            ]
        })
        .collect();
    let total = app.messages.items.len();
    let allow_bars = app.settings.show_scrollbar && inner.width > 1 && inner.height > 1;
    let vbar = allow_bars && total > inner.height as usize;
    let text_w = if vbar { inner.width - 1 } else { inner.width } as usize;
    let content_w = rows.iter().map(|s| span_line_width(s)).max().unwrap_or(0);
    let hbar = allow_bars && content_w > text_w;
    let body_h = if hbar { inner.height - 1 } else { inner.height };
    let hmax = content_w.saturating_sub(text_w);
    app.messages_hmax = hmax;
    app.messages_hscroll = app.messages_hscroll.min(hmax);
    let off = app.messages_hscroll;

    let items: Vec<ListItem> =
        rows.iter().map(|s| ListItem::new(Line::from(hslice_spans(s, off, text_w)))).collect();
    let list_area = Rect { width: u16::try_from(text_w).unwrap_or(u16::MAX), height: body_h, ..inner };
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(app.messages.selected));
    frame.render_stateful_widget(list, list_area, &mut state);
    if vbar {
        let sb = Rect { x: inner.x + inner.width - 1, y: inner.y, width: 1, height: body_h };
        draw_scrollbar(frame, sb, app.messages.selected, total.saturating_sub(1));
    }
    app.layout.messages_hscrollbar = if hbar {
        let hb = Rect { x: inner.x, y: inner.y + inner.height - 1, width: u16::try_from(text_w).unwrap_or(u16::MAX), height: 1 };
        draw_hscrollbar(frame, hb, off, hmax);
        hb
    } else {
        Rect::default()
    };
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
    let thumb_glyph = Span::styled("●", theme::title(true));
    let track_glyph = || Span::styled("│", theme::dim());
    // Proportional thumb position: round(pos.min(max) * (h-1) / max), done in
    // integer math to match the previous float rounding for in-range values.
    let span = h.saturating_sub(1);
    let thumb = (pos.min(max) * span + max / 2).checked_div(max).unwrap_or(0);
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
    let rel = usize::from(row.saturating_sub(area.y));
    let denom = usize::from((h - 1).max(1));
    // round(rel / denom * max) in integer math.
    let pos = (rel * max + denom / 2) / denom;
    pos.min(max)
}

/// Vix's one-row horizontal scrollbar, drawn into the one-row `area`: a dim `─`
/// track and a single `●` thumb positioned **proportionally** to `pos` within
/// `0..=max` (`max = content_width - viewport_width`). Mirrors [`draw_scrollbar`].
fn draw_hscrollbar(frame: &mut Frame, area: Rect, pos: usize, max: usize) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let w = area.width as usize;
    // Proportional thumb position via integer rounding (mirrors `draw_scrollbar`).
    let span = w.saturating_sub(1);
    let thumb = (pos.min(max) * span + max / 2).checked_div(max).unwrap_or(0);
    let spans: Vec<Span> = (0..w)
        .map(|c| {
            if c == thumb {
                Span::styled("●", theme::title(true))
            } else {
                Span::styled("─", theme::dim())
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Map a mouse `col` within a horizontal scrollbar `area` to a position in
/// `0..=max`, proportionally. Used for click and drag.
#[must_use]
pub fn scrollbar_pos_from_col(area: Rect, col: u16, max: usize) -> usize {
    if max == 0 || area.width == 0 {
        return 0;
    }
    let w = area.width;
    let rel = usize::from(col.saturating_sub(area.x));
    let denom = usize::from((w - 1).max(1));
    // round(rel / denom * max) in integer math.
    let pos = (rel * max + denom / 2) / denom;
    pos.min(max)
}

/// The longest line width (in chars) among `lines`.
fn max_line_width<'a>(lines: impl Iterator<Item = &'a str>) -> usize {
    lines.map(|l| l.chars().count()).max().unwrap_or(0)
}

/// Slice `line` to the horizontal window `[offset, offset + width)` by character.
fn hslice(line: &str, offset: usize, width: usize) -> String {
    line.chars().skip(offset).take(width).collect()
}

/// Total display width (chars) of a styled line's spans.
fn span_line_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.content.chars().count()).sum()
}

/// Slice a styled line's `spans` to the horizontal window `[offset, offset +
/// width)`, preserving each span's style. Used to horizontally scroll list rows.
fn hslice_spans(spans: &[Span], offset: usize, width: usize) -> Vec<Span<'static>> {
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut skip = offset;
    let mut remaining = width;
    for sp in spans {
        if remaining == 0 {
            break;
        }
        let chars: Vec<char> = sp.content.chars().collect();
        let len = chars.len();
        if skip >= len {
            skip -= len;
            continue;
        }
        let start = skip;
        skip = 0;
        let take = remaining.min(len - start);
        let text: String = chars[start..start + take].iter().collect();
        remaining -= take;
        out.push(Span::styled(text, sp.style));
    }
    out
}

fn draw_bottom_dock(app: &mut App, frame: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::BottomDock;
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::TOP)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(focused))
        .title(format!(" {} {} ", icon::INFO, t!("ui.bottom_dock")));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let total = app.bottom_dock.lines.len();
    let h = inner.height as usize;
    let allow_bars = app.settings.show_scrollbar && inner.width > 1 && inner.height > 1;
    let vbar = allow_bars && total > h;

    // The visible rows, and whether they overflow horizontally → a bottom hbar.
    let view_h = if allow_bars { inner.height.saturating_sub(1) } else { inner.height } as usize;
    let visible: Vec<String> = app.bottom_dock.visible(view_h).to_vec();
    let content_w = max_line_width(visible.iter().map(String::as_str));
    let text_w_full = if vbar { inner.width - 1 } else { inner.width } as usize;
    let hbar = allow_bars && content_w > text_w_full;

    let text_w = text_w_full;
    let body_h = if hbar { inner.height - 1 } else { inner.height };
    let text_area = Rect { width: u16::try_from(text_w).unwrap_or(u16::MAX), height: body_h, ..inner };

    let hmax = content_w.saturating_sub(text_w);
    app.bottom_hmax = hmax;
    app.bottom_hscroll = app.bottom_hscroll.min(hmax);
    let off = app.bottom_hscroll;

    let lines: Vec<Line> = if app.bottom_dock.is_empty() {
        vec![Line::from(Span::styled(t!("ui.bottom_dock_empty").to_string(), theme::dim()))]
    } else {
        visible.iter().map(|l| Line::from(hslice(l, off, text_w))).collect()
    };
    frame.render_widget(Paragraph::new(lines), text_area);

    if vbar {
        let sb = Rect { x: inner.x + inner.width - 1, y: inner.y, width: 1, height: body_h };
        draw_scrollbar(frame, sb, app.bottom_dock.scroll, total.saturating_sub(view_h));
    }
    app.layout.bottom_hscrollbar = if hbar {
        let hb = Rect { x: inner.x, y: inner.y + inner.height - 1, width: u16::try_from(text_w).unwrap_or(u16::MAX), height: 1 };
        draw_hscrollbar(frame, hb, off, hmax);
        hb
    } else {
        Rect::default()
    };
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
            crate::status_bar_panel::info_segment(Some(lang), t.editor.line_ending(), sel)
        })
        .unwrap_or_default();

    let git = crate::status_bar_panel::git_segment(app.git_branch.as_deref(), icon::BRANCH, app.git_dirty());
    let left = crate::status_bar_panel::left_segment(&mode, &path, &dirty_flag, &app.status);
    let right = crate::status_bar_panel::right_segment(&format!("{git}{info}"), line, col, icon::CALENDAR);

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
            Constraint::Length(u16::try_from(right.chars().count()).unwrap_or(u16::MAX) + 1),
        ])
        .split(inner);

    frame.render_widget(Paragraph::new(left).style(bg).alignment(Alignment::Left), cols[0]);
    frame.render_widget(Paragraph::new(right).style(bg).alignment(Alignment::Right), cols[1]);

    // Record the git/branch segment's rectangle (the leftmost part of the
    // right-aligned right segment, after its 1-cell padding) so a click on the
    // branch indicator opens the Git panel.
    let git_w = u16::try_from(git.chars().count()).unwrap_or(u16::MAX);
    app.layout.git_status_bar = if git_w > 0 {
        Rect { x: cols[1].x + 1, y: cols[1].y, width: git_w.min(cols[1].width), height: 1 }
    } else {
        Rect::default()
    };
}

fn draw_calendar(app: &mut App, frame: &mut Frame, area: Rect) {
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

    let help = Line::from(Span::styled(t!("ui.calendar_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(help), rows[2]);
}

fn draw_clock(app: &mut App, frame: &mut Frame, area: Rect) {
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

/// Previous-month arrow glyph, at column 0 of the calendar's month-header row.
pub const CAL_PREV: char = '\u{25c0}';
/// Next-month arrow glyph, at column 20 of the calendar's month-header row.
pub const CAL_NEXT: char = '\u{25b6}';

/// Columns each glyph cell occupies in the Nerd Font palette grid. The mouse
/// hit-test in [`crate::app::App::nerd_mouse`] divides by this, so the renderer
/// and the hit-test must agree on it.
pub const NERD_CELL_W: u16 = 4;

fn draw_nerd_palette(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::nerd_font_picker::{COLS, GLYPHS};
    let Some(p) = app.nerd_palette.as_ref() else {
        return;
    };
    let grid_w = u16::try_from(COLS).unwrap_or(u16::MAX) * NERD_CELL_W;
    let width = (grid_w + 2).min(area.width);
    let grid_rows = u16::try_from(p.rows()).unwrap_or(u16::MAX);
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
    use crate::ascii_character_picker::{self as ascii, LEN};
    if app.ascii_panel.is_none() {
        return;
    }
    let width = 26u16.min(area.width);
    // Borders (2) + header (1) + rows + hint (1); cap rows so the box fits.
    let max_rows = area.height.saturating_sub(4).max(1);
    let rows = u16::try_from(LEN).unwrap_or(u16::MAX).min(max_rows);
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
        let code = u8::try_from(idx).unwrap_or(u8::MAX);
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
        height: u16::try_from(view_h).unwrap_or(u16::MAX).min(chunks[1].height),
    };
}

// Pad or truncate `s` to exactly `w` display columns (by character count).
fn fit(s: &str, w: usize) -> String {
    let mut out: String = s.chars().take(w).collect();
    let len = out.chars().count();
    if len < w {
        out.push_str(&" ".repeat(w - len));
    }
    out
}

// Per-column display width: the widest cell (incl. header), clamped to [3, 24].
fn column_widths(grid: &crate::edit_table::Grid) -> Vec<usize> {
    (0..grid.col_count())
        .map(|c| {
            let mut w = 3;
            for r in 0..grid.row_count() {
                w = w.max(grid.cell(r, c).chars().count());
            }
            w.min(24)
        })
        .collect()
}

// First column to show so the selected column is visible within `avail` columns.
fn first_visible_col(grid: &crate::edit_table::Grid, widths: &[usize], avail: usize) -> usize {
    let sel = grid.col();
    let mut first = grid.col_scroll().min(sel);
    loop {
        let used: usize = (first..=sel).map(|c| widths.get(c).copied().unwrap_or(3) + 1).sum();
        if used <= avail || first >= sel {
            break;
        }
        first += 1;
    }
    first
}

// The column indices that fit in `avail` columns starting from the scroll offset.
fn visible_cols(grid: &crate::edit_table::Grid, widths: &[usize], avail: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut used = 0usize;
    for c in grid.col_scroll()..grid.col_count() {
        let need = widths.get(c).copied().unwrap_or(3) + 1;
        if used + need > avail && !out.is_empty() {
            break;
        }
        used += need;
        out.push(c);
    }
    out
}

// Build one rendered grid line for row `r` over the visible `cols`.
fn table_row_line(grid: &crate::edit_table::Grid, r: usize, cols: &[usize], widths: &[usize]) -> Line<'static> {
    let mut spans = Vec::with_capacity(cols.len() * 2);
    let editing = grid.is_editing() && r == grid.row();
    for &c in cols {
        let w = widths.get(c).copied().unwrap_or(3);
        let selected = r == grid.row() && c == grid.col();
        let raw = if editing && selected { grid.edit_buffer() } else { grid.cell(r, c) };
        let style = if selected {
            theme::selected()
        } else if r == 0 {
            theme::title(true)
        } else {
            theme::base()
        };
        spans.push(Span::styled(fit(raw, w), style));
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

// The bottom status/hint line: position, plus the find query, edit notice, or hint.
fn table_status_line(grid: &crate::edit_table::Grid) -> Line<'static> {
    let info = if grid.is_finding() {
        format!("/{}", grid.find_buffer())
    } else if grid.is_editing() {
        t!("ui.edit_table_editing").to_string()
    } else {
        t!("ui.edit_table_hint").to_string()
    };
    let pos = format!(
        " r{}/{} c{}/{}  ",
        grid.row() + 1,
        grid.row_count(),
        grid.col() + 1,
        grid.col_count(),
    );
    Line::from(vec![Span::styled(pos, theme::dim()), Span::styled(info, theme::dim())])
}

// Render the table editor overlay: pinned header, scrolling body with the
// selected cell highlighted, and a status/hint line.
fn draw_edit_table(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.edit_table.is_none() {
        return;
    }
    frame.render_widget(Clear, area);
    let dirty = if app.edit_table.as_ref().is_some_and(crate::edit_table::Grid::is_dirty) {
        " *"
    } else {
        ""
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {}{} ", icon::TABLE, t!("ui.edit_table"), dirty));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let grid = app.edit_table.as_ref().unwrap();
    let widths = column_widths(grid);
    let avail = usize::from(chunks[1].width);
    let first = first_visible_col(grid, &widths, avail);
    let body_h = usize::from(chunks[1].height);
    if let Some(g) = app.edit_table.as_mut() {
        g.set_col_scroll(first);
        g.ensure_row_visible(body_h);
    }

    let grid = app.edit_table.as_ref().unwrap();
    let cols = visible_cols(grid, &widths, avail);
    frame.render_widget(Paragraph::new(table_row_line(grid, 0, &cols, &widths)), chunks[0]);

    let start = grid.row_scroll();
    let mut lines = Vec::with_capacity(body_h);
    for r in start..(start + body_h).min(grid.row_count()) {
        lines.push(table_row_line(grid, r, &cols, &widths));
    }
    frame.render_widget(Paragraph::new(lines), chunks[1]);
    frame.render_widget(Paragraph::new(table_status_line(grid)), chunks[2]);

    app.layout.edit_table = chunks[1];
}

// One rendered outline line: indentation, a fold marker (▾/▸/·), and the text.
fn outline_line(tree: &crate::edit_outline::Tree, i: usize, selected: bool) -> Line<'static> {
    let marker = if tree.has_children(i) {
        if tree.is_collapsed(i) { "▸ " } else { "▾ " }
    } else {
        "· "
    };
    let text = format!("{}{marker}{}", "  ".repeat(tree.level(i)), tree.text(i));
    let style = if selected { theme::selected() } else { theme::base() };
    Line::from(Span::styled(text, style))
}

// Render the outline editor overlay: a scrolling tree of items with the selected
// item highlighted, and a status/hint line.
fn draw_edit_sql(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.edit_sql.is_none() {
        return;
    }
    frame.render_widget(Clear, area);
    let dirty = if app.edit_sql.as_ref().is_some_and(crate::edit_sql::Editor::is_dirty) { " *" } else { "" };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {}{} ", icon::CODE, t!("ui.edit_sql"), dirty));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let body_h = usize::from(chunks[0].height);
    if let Some(e) = app.edit_sql.as_mut() {
        e.ensure_visible(body_h);
    }
    let editor = app.edit_sql.as_ref().unwrap();
    let total = editor.len();
    let width = chunks[0].width as usize;
    let mut lines: Vec<Line> = Vec::with_capacity(body_h);
    for i in editor.scroll()..(editor.scroll() + body_h).min(total) {
        let kind = editor.kind(i);
        let preview = editor.preview(i);
        let text = format!(" {kind:8} {}", trunc(&preview, width.saturating_sub(11)));
        let line = if i == editor.sel() {
            Line::from(Span::styled(text, theme::selected()))
        } else {
            Line::from(vec![Span::styled(format!(" {kind:8} "), theme::dim()), Span::raw(trunc(&preview, width.saturating_sub(11)))])
        };
        lines.push(line);
    }
    if total == 0 {
        lines.push(Line::from(Span::styled(t!("ui.edit_sql_empty").to_string(), theme::dim())));
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(t!("ui.edit_sql_hint").to_string(), theme::dim()))),
        chunks[1],
    );

    app.layout.edit_sql = chunks[0];
}

fn draw_edit_outline(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.edit_outline.is_none() {
        return;
    }
    frame.render_widget(Clear, area);
    let dirty = if app.edit_outline.as_ref().is_some_and(crate::edit_outline::Tree::is_dirty) {
        " *"
    } else {
        ""
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {}{} ", icon::LIST, t!("ui.edit_outline"), dirty));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let body_h = usize::from(chunks[0].height);
    if let Some(t) = app.edit_outline.as_mut() {
        t.ensure_visible(body_h);
    }
    let tree = app.edit_outline.as_ref().unwrap();
    let vis = tree.visible();
    let start = tree.scroll();
    let mut lines = Vec::with_capacity(body_h);
    for &i in vis.iter().skip(start).take(body_h) {
        lines.push(outline_line(tree, i, i == tree.sel()));
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(t!("ui.edit_outline_hint").to_string(), theme::dim()))),
        chunks[1],
    );

    app.layout.edit_outline = chunks[0];
}

// One rendered row of the structured-value (JSON/YAML) tree.
fn value_line(tree: &crate::edit_value::Tree, i: usize, editing: bool) -> Line<'static> {
    let selected = i == tree.sel();
    let marker = if tree.is_container(i) {
        if tree.is_collapsed(i) { "▸ " } else { "▾ " }
    } else {
        "  "
    };
    let label = tree.label(i);
    let value = if editing && selected { tree.edit_buffer() } else { tree.value(i) };
    let head = if label.is_empty() {
        String::new()
    } else if tree.is_container(i) {
        format!("{label} ")
    } else {
        format!("{label}: ")
    };
    let text = format!("{}{marker}{head}{value}", "  ".repeat(tree.depth(i)));
    let style = if selected { theme::selected() } else { theme::base() };
    Line::from(Span::styled(text, style))
}

// Render the structured-value editor overlay (Edit JSON / Edit YAML).
fn draw_edit_value(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(format) = app.edit_value.as_ref().map(crate::edit_value::Tree::format) else {
        return;
    };
    frame.render_widget(Clear, area);
    let dirty = if app.edit_value.as_ref().is_some_and(crate::edit_value::Tree::is_dirty) {
        " *"
    } else {
        ""
    };
    let title_key = match format {
        crate::edit_value::Format::Json => "ui.edit_json",
        crate::edit_value::Format::Yaml => "ui.edit_yaml",
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {}{} ", icon::CODE, t!(title_key), dirty));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let body_h = usize::from(chunks[0].height);
    if let Some(t) = app.edit_value.as_mut() {
        t.ensure_visible(body_h);
    }
    let tree = app.edit_value.as_ref().unwrap();
    let editing = tree.is_editing();
    let start = tree.scroll();
    let mut lines = Vec::with_capacity(body_h);
    for i in start..(start + body_h).min(tree.row_count()) {
        lines.push(value_line(tree, i, editing));
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(t!("ui.edit_value_hint").to_string(), theme::dim()))),
        chunks[1],
    );
    app.layout.edit_value = chunks[0];
}

// One rendered hex-dump row: offset, hex byte pairs, and the ASCII gutter.
fn bytes_line(hex: &crate::edit_bytes::Hex, row: usize) -> Line<'static> {
    use crate::edit_bytes::COLS;
    let off = row * COLS;
    let mut spans = vec![Span::styled(format!("{off:08x}  "), theme::dim())];
    for col in 0..COLS {
        let idx = off + col;
        if idx < hex.len() {
            let style = if idx == hex.cursor() { theme::selected() } else { theme::base() };
            spans.push(Span::styled(format!("{:02x} ", hex.byte(idx)), style));
        } else {
            spans.push(Span::raw("   "));
        }
    }
    spans.push(Span::raw(" "));
    for col in 0..COLS {
        let idx = off + col;
        if idx < hex.len() {
            let b = hex.byte(idx);
            let ch = if (0x20..0x7f).contains(&b) { char::from(b) } else { '.' };
            let style = if idx == hex.cursor() { theme::selected() } else { theme::dim() };
            spans.push(Span::styled(ch.to_string(), style));
        }
    }
    Line::from(spans)
}

// Render the byte (hex) editor overlay.
fn draw_edit_bytes(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.edit_bytes.is_none() {
        return;
    }
    frame.render_widget(Clear, area);
    let dirty = if app.edit_bytes.as_ref().is_some_and(crate::edit_bytes::Hex::is_dirty) {
        " *"
    } else {
        ""
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {}{} ", icon::TABLE, t!("ui.edit_bytes"), dirty));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let body_h = usize::from(chunks[0].height);
    if let Some(h) = app.edit_bytes.as_mut() {
        h.ensure_visible(body_h);
    }
    let hex = app.edit_bytes.as_ref().unwrap();
    let start = hex.scroll();
    let mut lines = Vec::with_capacity(body_h);
    for row in start..(start + body_h).min(hex.rows()) {
        lines.push(bytes_line(hex, row));
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(t!("ui.edit_bytes_hint").to_string(), theme::dim()))),
        chunks[1],
    );
    app.layout.edit_bytes = chunks[0];
}

// Render the QR code overlay: the Unicode QR art, forced to black-on-white so it
// scans regardless of the active theme, centered with a hint line.
fn draw_qrcode(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(art) = app.qrcode.as_ref() else { return };
    let lines: Vec<&str> = art.lines().collect();
    let art_w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let width = (u16::try_from(art_w).unwrap_or(u16::MAX) + 2).min(area.width);
    let height = (u16::try_from(lines.len()).unwrap_or(u16::MAX) + 3).min(area.height);
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
        .title(format!(" {} {} ", icon::INFO, t!("ui.qrcode")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let qr_style = Style::default().fg(Color::Black).bg(Color::White);
    let body: Vec<Line> = lines
        .iter()
        .map(|l| Line::from(Span::styled((*l).to_string(), qr_style)))
        .collect();
    frame.render_widget(Paragraph::new(body), chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(t!("ui.qrcode_hint").to_string(), theme::dim()))),
        chunks[1],
    );
}

fn draw_x11_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let colors = crate::x11_color_picker::colors();
    let total = colors.len();
    if app.x11_panel.is_none() || total == 0 {
        return;
    }
    let width = 36u16.min(area.width);
    let max_rows = area.height.saturating_sub(4).max(1);
    let rows = u16::try_from(total).unwrap_or(u16::MAX).min(max_rows);
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
        height: u16::try_from(view_h).unwrap_or(u16::MAX).min(chunks[1].height),
    };
}

fn draw_media_type_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.media_type_panel.is_none() {
        return;
    }
    let table = crate::media_type::all();
    let width = 64u16.min(area.width);
    let height = (area.height.saturating_sub(4)).max(6).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let p = app.media_type_panel.as_ref().unwrap();
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::CODE, t!("ui.media_types")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    // Filter line: a typed query, or a dim prompt when empty.
    let filter = if p.query.is_empty() {
        Line::from(Span::styled(t!("ui.media_types_filter").to_string(), theme::dim()))
    } else {
        Line::from(vec![Span::styled("/ ", theme::dim()), Span::raw(p.query.clone())])
    };
    frame.render_widget(Paragraph::new(filter), chunks[0]);

    let view_h = chunks[1].height as usize;
    if let Some(p) = app.media_type_panel.as_mut() {
        p.ensure_visible(view_h);
    }
    let p = app.media_type_panel.as_ref().unwrap();
    let filtered = p.matches();
    let total = filtered.len();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for (row, &idx) in filtered.iter().enumerate().skip(p.scroll).take(view_h) {
        let m = &table[idx];
        let tag = if m.is_text() { "txt" } else { "bin" };
        let text =
            format!(" {:30} {:8} {tag}  {}", trunc(m.media_type, 30), trunc(m.extension, 8), m.description);
        let line = if row == p.selected {
            Line::from(Span::styled(text, theme::selected()))
        } else {
            Line::from(Span::raw(text))
        };
        lines.push(line);
    }
    let show_bar = total > view_h && chunks[1].width > 1;
    let row_area = if show_bar { Rect { width: chunks[1].width - 1, ..chunks[1] } } else { chunks[1] };
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect { x: chunks[1].x + chunks[1].width - 1, ..chunks[1] };
        draw_scrollbar(frame, sb_area, p.selected, total.saturating_sub(1));
    }

    let hint = Line::from(Span::styled(t!("ui.media_types_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[2]);

    app.layout.media_type_panel = Rect {
        x: chunks[1].x,
        y: chunks[1].y,
        width: row_area.width,
        height: u16::try_from(view_h).unwrap_or(u16::MAX).min(chunks[1].height),
    };
}

/// Truncate `s` to `max` columns, adding an ellipsis when clipped.
fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let keep = max.saturating_sub(1);
        format!("{}…", s.chars().take(keep).collect::<String>())
    }
}

fn draw_html_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let entities = crate::html_character_picker::entities();
    let total = entities.len();
    if app.html_panel.is_none() || total == 0 {
        return;
    }
    let width = 46u16.min(area.width);
    let max_rows = area.height.saturating_sub(4).max(1);
    let rows = u16::try_from(total).unwrap_or(u16::MAX).min(max_rows);
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
        height: u16::try_from(view_h).unwrap_or(u16::MAX).min(chunks[1].height),
    };
}

fn draw_outline(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.outline.is_none() {
        return;
    }
    let n = app.outline.as_ref().unwrap().len();
    let width = 48u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = u16::try_from(n).unwrap_or(u16::MAX).min(max_rows);
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
        height: u16::try_from(view_h).unwrap_or(u16::MAX).min(chunks[0].height),
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

    let hint = Line::from(Span::styled(t!("ui.dashboard_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
}

#[allow(clippy::too_many_lines)]
fn draw_ai_diff(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::ai_diff::Seg;
    let Some(review) = app.ai_diff_review() else { return };
    let width = (area.width * 8 / 10).clamp(30, area.width);
    let height = (area.height * 8 / 10).clamp(8, area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let title = format!(
        " {} {} ({}/{}) ",
        icon::INFO,
        t!("ui.ai_diff"),
        review.accepted_count(),
        review.change_count()
    );
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
    let body = chunks[0];
    let positions = review.change_positions();
    let selected_seg = positions.get(review.selected).copied();

    let mut lines: Vec<Line> = Vec::new();
    for (i, seg) in review.segs.iter().enumerate() {
        match seg {
            Seg::Equal(ls) => {
                for l in ls {
                    lines.push(Line::from(Span::styled(format!("  {}", l.trim_end_matches('\n')), theme::dim())));
                }
            }
            Seg::Change { old, new, accepted } => {
                let here = Some(i) == selected_seg;
                let marker = if here { "▸" } else { " " };
                for l in old {
                    let style = Style::default().fg(Color::Red).add_modifier(Modifier::DIM);
                    lines.push(Line::from(Span::styled(
                        format!("{marker} - {}", l.trim_end_matches('\n')),
                        style,
                    )));
                }
                for l in new {
                    let mut style = Style::default().fg(Color::Green);
                    if !accepted {
                        style = style.add_modifier(Modifier::DIM | Modifier::CROSSED_OUT);
                    } else if here {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    lines.push(Line::from(Span::styled(
                        format!("{marker} + {}", l.trim_end_matches('\n')),
                        style,
                    )));
                }
            }
        }
    }
    // Scroll so the selected hunk stays visible: anchor the view near it.
    let view_h = body.height as usize;
    let anchor = selected_seg
        .map_or(0, |sid| review.segs[..sid].iter().map(seg_line_count).sum::<usize>());
    let start = anchor.saturating_sub(view_h / 3).min(lines.len().saturating_sub(view_h));
    let window: Vec<Line> = lines.into_iter().skip(start).take(view_h).collect();
    frame.render_widget(Paragraph::new(window), body);

    let hint = Line::from(Span::styled(t!("ui.ai_diff_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[1]);
}

/// Number of rendered lines a diff segment occupies (old + new for a change).
fn seg_line_count(seg: &crate::ai_diff::Seg) -> usize {
    match seg {
        crate::ai_diff::Seg::Equal(ls) => ls.len(),
        crate::ai_diff::Seg::Change { old, new, .. } => old.len() + new.len(),
    }
}

/// Map a `vt100` color to a ratatui color.
fn vt_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

#[allow(clippy::too_many_lines)]
fn draw_terminal(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.terminal.is_none() {
        return;
    }
    let width = area.width.max(2);
    let height = area.height.max(2);
    let rect = Rect { x: area.x, y: area.y, width, height };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::CODE, t!("ui.terminal")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    // Match the PTY grid to the visible area so the shell wraps correctly.
    if let Some(term) = app.terminal.as_mut() {
        term.resize(inner.height, inner.width);
    }
    let Some(term) = app.terminal.as_ref() else { return };
    let parser = term.lock();
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    let mut lines: Vec<Line> = Vec::with_capacity(rows as usize);
    for row in 0..rows.min(inner.height) {
        let mut spans: Vec<Span> = Vec::with_capacity(cols as usize);
        for col in 0..cols.min(inner.width) {
            let (text, mut style) = match screen.cell(row, col) {
                Some(cell) => {
                    let contents = cell.contents();
                    let text = if contents.is_empty() { " ".to_string() } else { contents };
                    let mut s = Style::default().fg(vt_color(cell.fgcolor())).bg(vt_color(cell.bgcolor()));
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
    if !screen.hide_cursor() {
        let (crow, ccol) = screen.cursor_position();
        if crow < inner.height && ccol < inner.width {
            frame.set_cursor_position((inner.x + ccol, inner.y + crow));
        }
    }
}

fn draw_ai_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::ai_panel::Role;
    if app.ai_panel.is_none() {
        return;
    }
    let width = (area.width * 7 / 10).clamp(30, area.width);
    let height = (area.height * 7 / 10).clamp(8, area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let busy = app.ai_panel.as_ref().is_some_and(|p| p.busy);
    let title = if busy {
        format!(" {} {} ", icon::INFO, t!("ui.ai_thinking"))
    } else {
        format!(" {} {} ", icon::INFO, t!("menu.ai"))
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
        .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
        .split(inner);
    let body = chunks[0];
    let view_h = body.height as usize;
    let view_w = body.width as usize;
    let lines: Vec<Line> = {
        let p = app.ai_panel.as_mut().unwrap();
        let visible = p.visible(view_w, view_h);
        visible
            .into_iter()
            .map(|(role, text)| {
                let style = match role {
                    Role::User => theme::base().add_modifier(Modifier::BOLD),
                    Role::Assistant => theme::base(),
                    Role::Error => theme::dim().add_modifier(Modifier::ITALIC),
                };
                Line::from(Span::styled(text, style))
            })
            .collect()
    };
    frame.render_widget(Paragraph::new(lines), body);
    app.layout.ai_panel = body;

    // Input line: a leading prompt glyph then the in-progress text plus a caret.
    let input = app.ai_panel.as_ref().map(|p| p.input.clone()).unwrap_or_default();
    let input_line = Line::from(vec![
        Span::styled("› ", theme::title(true).add_modifier(Modifier::BOLD)),
        Span::raw(input),
        Span::styled("▏", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(input_line), chunks[1]);

    let hint = Line::from(Span::styled(t!("ui.ai_panel_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[2]);
}

fn draw_contacts(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(total) = app.contacts.as_ref().map(crate::contact_panel::Panel::len) else { return };
    let width = 40u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = u16::try_from(total.max(1)).unwrap_or(u16::MAX).min(max_rows);
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
    app.layout.contacts = Rect { x: chunks[0].x, y: chunks[0].y, width: list_area.width, height: u16::try_from(view_h).unwrap_or(u16::MAX).min(chunks[0].height) };
}

fn draw_vcard(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(total) = app.vcard.as_ref().map(crate::vcard_panel::Panel::len) else { return };
    let title = app.vcard.as_ref().map(crate::vcard_panel::Panel::title).unwrap_or_default();
    let width = 60u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = u16::try_from(total.max(1)).unwrap_or(u16::MAX).min(max_rows);
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
    app.layout.vcard = Rect { x: chunks[0].x, y: chunks[0].y, width: list_area.width, height: u16::try_from(view_h).unwrap_or(u16::MAX).min(chunks[0].height) };
}

fn draw_snippets(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.snippets.is_none() {
        return;
    }
    let width = 60u16.min(area.width).max(24);
    let height = area.height.saturating_sub(4).max(6).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let picker = app.snippets.as_ref().unwrap();
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.snippets")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let filter = if picker.query.is_empty() {
        Line::from(Span::styled(t!("ui.snippets_filter").to_string(), theme::dim()))
    } else {
        Line::from(vec![Span::styled("/ ", theme::dim()), Span::raw(picker.query.clone())])
    };
    frame.render_widget(Paragraph::new(filter), chunks[0]);

    let view_h = chunks[1].height as usize;
    let lib = &app.snippet_library;
    if let Some(p) = app.snippets.as_mut() {
        p.ensure_visible(view_h, lib);
    }
    let picker = app.snippets.as_ref().unwrap();
    let filtered = picker.matches(lib);
    let total = filtered.len();
    let colw = chunks[1].width as usize;
    let mut rows: Vec<Line> = Vec::with_capacity(view_h);
    for (row, &i) in filtered.iter().enumerate().skip(picker.scroll).take(view_h) {
        let s = &lib[i];
        let prefix = s.prefixes.first().map_or_else(String::new, |p| format!("[{p}] "));
        let text = format!(" {}{}  ", prefix, s.name);
        let scope = s.scope.label();
        let pad = colw.saturating_sub(text.chars().count() + scope.chars().count() + 1);
        let body_line = format!("{text}{}{scope} ", " ".repeat(pad));
        if row == picker.selected {
            rows.push(Line::from(Span::styled(body_line, theme::selected())));
        } else {
            rows.push(Line::from(vec![
                Span::raw(text),
                Span::styled(format!("{}{scope} ", " ".repeat(pad)), theme::dim()),
            ]));
        }
    }
    let show_bar = total > view_h && chunks[1].width > 1;
    let row_area = if show_bar { Rect { width: chunks[1].width - 1, ..chunks[1] } } else { chunks[1] };
    frame.render_widget(Paragraph::new(rows), row_area);
    if show_bar {
        let sb = Rect { x: chunks[1].x + chunks[1].width - 1, ..chunks[1] };
        draw_scrollbar(frame, sb, picker.selected, total.saturating_sub(1));
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(t!("ui.snippets_hint").to_string(), theme::dim()))),
        chunks[2],
    );
    app.layout.snippets = row_area;
}

fn draw_markdown_preview(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(panel) = app.markdown_preview.as_mut() else { return };
    // A large centered reading pane.
    let width = 80u16.min(area.width.saturating_sub(2)).max(20);
    let height = area.height.saturating_sub(2).max(6);
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
        .title(format!(" {} ", t!("ui.markdown_preview")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let view_h = inner.height as usize;
    let max_scroll = panel.lines.len().saturating_sub(view_h);
    if panel.scroll > max_scroll {
        panel.scroll = max_scroll;
    }
    let end = (panel.scroll + view_h).min(panel.lines.len());
    let lines: Vec<Line> = panel.lines[panel.scroll..end]
        .iter()
        .map(|l| {
            // Heading underline rules and the thematic break render dim.
            let dim = l.chars().all(|c| matches!(c, '=' | '-' | '─')) && !l.is_empty();
            if dim {
                Line::from(Span::styled(l.clone(), theme::dim()))
            } else {
                Line::from(l.clone())
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
    if panel.lines.len() > view_h {
        let sb = Rect { x: rect.x + rect.width - 1, y: inner.y, width: 1, height: inner.height };
        draw_scrollbar(frame, sb, panel.scroll, max_scroll);
    }
}

fn draw_text_info(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(n) = app.text_info.as_ref().map(crate::text_information_panel::Panel::len) else {
        return;
    };
    let width = 40u16.min(area.width).max(24);
    let height = (u16::try_from(n).unwrap_or(u16::MAX) + 3).min(area.height);
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
        .title(format!(" {} {} ", icon::INFO, t!("ui.text_info")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let p = app.text_info.as_ref().unwrap();
    let view_h = chunks[0].height as usize;
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for (idx, row) in p.rows.iter().take(view_h).enumerate() {
        let text = format!("  {:<12} {}", row.label, row.value);
        if idx == p.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(Span::raw(text)));
        }
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(t!("ui.system_info_hint").to_string(), theme::dim()))),
        chunks[1],
    );
    app.layout.text_info = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: chunks[0].width,
        height: u16::try_from(view_h).unwrap_or(u16::MAX).min(chunks[0].height),
    };
}

fn draw_file_info(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.file_info.is_none() {
        return;
    }
    let n = app.file_info.as_ref().unwrap().len();
    let width = 64u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = u16::try_from(n).unwrap_or(u16::MAX).min(max_rows);
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
        height: u16::try_from(view_h).unwrap_or(u16::MAX).min(chunks[0].height),
    };
}

fn draw_system_info(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.system_info.is_none() {
        return;
    }
    let n = app.system_info.as_ref().unwrap().len();
    let width = 60u16.min(area.width);
    let max_rows = area.height.saturating_sub(3).max(1);
    let rows = u16::try_from(n).unwrap_or(u16::MAX).min(max_rows);
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
        height: u16::try_from(view_h).unwrap_or(u16::MAX).min(chunks[0].height),
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

/// Render a left-to-right row of labeled buttons (each ` label `, one-cell gap),
/// returning each button's clickable rectangle (`Rect::default()` if it did not
/// fit).
fn button_row(frame: &mut Frame, row: Rect, buttons: &[(String, Style)]) -> Vec<Rect> {
    let mut rects = vec![Rect::default(); buttons.len()];
    let mut x = row.x;
    let right = row.x + row.width;
    for (i, (label, style)) in buttons.iter().enumerate() {
        let text = format!(" {label} ");
        let w = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
        if x >= right {
            break;
        }
        let w = w.min(right - x);
        let r = Rect { x, y: row.y, width: w, height: 1 };
        frame.render_widget(Paragraph::new(Line::from(Span::styled(text, *style))), r);
        rects[i] = r;
        x = x.saturating_add(w + 1);
    }
    rects
}

fn draw_search(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(s) = app.search.as_ref() else { return };
    let replacing = s.replacing;
    let height = if replacing { 6 } else { 4 };
    let width = area.width * 7 / 10;
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + 1,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let title = if s.interactive {
        format!(" {} {} ", icon::SEARCH, t!("ui.query_replace"))
    } else if replacing {
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
    // Forget any stale button rects (they are re-recorded below when shown).
    app.layout.search_case = Rect::default();
    app.layout.search_word = Rect::default();
    app.layout.search_regex = Rect::default();
    app.layout.search_once = Rect::default();
    app.layout.search_ask = Rect::default();
    app.layout.search_all = Rect::default();

    // Rows: Find field, toggle buttons, [Replace field, replace buttons,]? status.
    let constraints: Vec<Constraint> = if replacing {
        vec![Constraint::Length(1); 5]
    } else {
        vec![Constraint::Length(1); 3]
    };
    let rows = Layout::default().direction(Direction::Vertical).constraints(constraints).split(inner);

    let q_focus = !replacing || s.field == Field::Query;
    frame.render_widget(Paragraph::new(field_line(&t!("ui.field_find"), &s.query, q_focus)), rows[0]);

    // Case / Word / Regex toggle buttons (highlighted when on).
    let toggle_style = |on: bool| if on { theme::selected() } else { theme::dim() };
    let toggles = vec![
        (t!("ui.toggle_case").to_string(), toggle_style(s.case_sensitive)),
        (t!("ui.toggle_word").to_string(), toggle_style(s.whole_word)),
        (t!("ui.toggle_regex").to_string(), toggle_style(s.regex)),
    ];
    let trects = button_row(frame, rows[1], &toggles);
    app.layout.search_case = trects[0];
    app.layout.search_word = trects[1];
    app.layout.search_regex = trects[2];

    if replacing {
        frame.render_widget(
            Paragraph::new(field_line(&t!("ui.field_replace"), &s.replace, s.field == Field::Replace)),
            rows[2],
        );
        // Once / Ask / All replace buttons (reverse-video, like pressable buttons).
        let actions = vec![
            (t!("ui.btn_once").to_string(), theme::selected()),
            (t!("ui.btn_ask").to_string(), theme::selected()),
            (t!("ui.btn_all").to_string(), theme::selected()),
        ];
        let arects = button_row(frame, rows[3], &actions);
        app.layout.search_once = arects[0];
        app.layout.search_ask = arects[1];
        app.layout.search_all = arects[2];
    }

    let status = if !s.status.is_empty() {
        s.status.clone()
    } else if s.interactive {
        t!("ui.search_hint_interactive").to_string()
    } else if replacing {
        t!("ui.search_hint_replace").to_string()
    } else {
        t!("ui.search_hint").to_string()
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(status, theme::dim()))),
        rows[rows.len() - 1],
    );
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
    // The git-commit prompt accepts a multi-line message (Alt+Enter = newline).
    let multiline = matches!(p.kind, crate::app::PromptKind::GitCommit);
    let width = area.width * 6 / 10;
    let body_rows: u16 = if multiline {
        u16::try_from(p.input.split('\n').count()).unwrap_or(1).clamp(1, 12) + 1
    } else if toggles {
        2
    } else {
        1
    };
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + area.height / 3,
        width,
        height: body_rows + 2,
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

    if multiline {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);
        let parts: Vec<&str> = p.input.split('\n').collect();
        let last = parts.len().saturating_sub(1);
        let mut lines: Vec<Line> = Vec::with_capacity(parts.len());
        for (i, part) in parts.iter().enumerate() {
            let prefix = if i == 0 { "\u{276f} " } else { "  " };
            let mut spans = vec![Span::styled(prefix, theme::title(true)), Span::raw((*part).to_string())];
            if i == last {
                spans.push(Span::styled("\u{2588}", theme::dim()));
            }
            lines.push(Line::from(spans));
        }
        frame.render_widget(Paragraph::new(lines), rows[0]);
        let hint = Line::from(Span::styled(t!("ui.commit_hint").to_string(), theme::dim()));
        frame.render_widget(Paragraph::new(hint), rows[1]);
        return;
    }

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

fn draw_help(app: &App, frame: &mut Frame, area: Rect) {
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

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    // Filter line: a search glyph then the live query and a caret.
    let filter = app.help_filter.as_str();
    let search = Line::from(vec![
        Span::styled(format!("{} ", icon::SEARCH), theme::title(true)),
        Span::raw(filter.to_string()),
        Span::styled("\u{2588}", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(search), chunks[0]);

    let needle = filter.to_lowercase();
    let rows = crate::keyboard_shortcut_panel::ROWS;
    let key_w = rows.iter().map(|r| r.keys.len()).max().unwrap_or(0);
    let lines: Vec<Line> = rows
        .iter()
        .filter(|r| {
            needle.is_empty()
                || r.keys.to_lowercase().contains(&needle)
                || t!(r.desc).to_lowercase().contains(&needle)
        })
        .map(|r| {
            Line::from(vec![
                Span::styled(format!(" {:<key_w$} ", r.keys), theme::title(true)),
                Span::raw(format!("  {}", t!(r.desc))),
            ])
        })
        .collect();
    let body = if lines.is_empty() {
        vec![Line::from(Span::styled(t!("ui.no_matches").to_string(), theme::dim()))]
    } else {
        lines
    };
    frame.render_widget(Paragraph::new(body).wrap(Wrap { trim: false }), chunks[1]);
}
