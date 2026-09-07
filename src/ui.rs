//! All rendering. `draw` lays out the frame, records pane rectangles for mouse
//! hit-testing, and delegates to per-pane helpers.

#![warn(clippy::pedantic)]

mod ai_terminal;
mod choosers;
mod db;
mod dialogs;
mod edit_surfaces;
mod file_browser;
mod help;
mod info_panels;
mod menu_bar;
mod picker_panels;
mod search;

use ai_terminal::{draw_ai_diff, draw_ai_panel, draw_terminal};
use choosers::{
    draw_branch_chooser, draw_capture_chooser, draw_clipboard_chooser, draw_context_menu,
    draw_diff_view, draw_git_panel, draw_location_chooser, draw_macro_chooser, draw_recent_chooser,
    draw_refile_chooser, draw_script_chooser, draw_spell_suggest, draw_task_chooser,
    draw_workspace_chooser,
};
use db::draw_db;
use dialogs::{
    draw_confirm, draw_paste_conflict, draw_query_replace, draw_replace_confirm, draw_script_trust,
    draw_unsaved,
};
use edit_surfaces::{
    draw_column_view, draw_edit_bytes, draw_edit_outline, draw_edit_sql, draw_edit_table,
    draw_edit_value, draw_html_panel, draw_outline,
};
use file_browser::draw_file_browser;
use help::{draw_help, draw_keybinding_editor};
use info_panels::{
    draw_contacts, draw_file_info, draw_markdown_preview, draw_snippets, draw_system_info,
    draw_text_info, draw_vcard,
};
use menu_bar::{draw_menu_bar, draw_menu_dropdown, dropdown_width};
use picker_panels::{
    draw_ascii_panel, draw_media_type_panel, draw_nerd_palette, draw_qrcode, draw_x11_panel,
};
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap,
};
use search::{draw_palette, draw_prompt, draw_search, draw_workspace_search};

use ratatui_image::StatefulImage;
use ratatui_image::protocol::StatefulProtocol;

use crate::app::{App, Focus};
use crate::calendar;
use crate::clock;
use crate::menu::menus;
use crate::messages::Level;
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
        constraints.push(Constraint::Length(
            app.settings.explorer_width.clamp(12, dock_max),
        ));
    }
    constraints.push(Constraint::Min(20));
    if app.show_messages {
        constraints.push(Constraint::Length(
            app.settings.messages_width.clamp(12, dock_max),
        ));
    }
    if app.settings.show_outline_dock {
        constraints.push(Constraint::Length(
            app.settings.outline_width.clamp(12, dock_max),
        ));
    }
    if app.show_debug_panel {
        constraints.push(Constraint::Length(
            app.settings.debug_width.clamp(12, dock_max),
        ));
    }
    if app.show_test_panel {
        constraints.push(Constraint::Length(
            app.settings.test_width.clamp(12, dock_max),
        ));
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(body);
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
        .border_style(theme::region_title(
            theme::Region::Editor,
            app.focus == Focus::Editor,
        ));
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
    if app.jump.is_some() {
        draw_jump_labels(app, frame);
    }
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
    if app.keybinding_editor.is_some() {
        draw_keybinding_editor(app, frame, area);
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
    if app.script_trust.is_some() {
        draw_script_trust(app, frame, area);
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
    draw_chooser_overlays(app, frame, area);
    if app.nerd_palette.is_some() {
        draw_nerd_palette(app, frame, area);
    }
    if app.ascii_panel.is_some() {
        draw_ascii_panel(app, frame, area);
    }
    if app.edit_table.is_some() {
        draw_edit_table(app, frame, area);
    }
    if app.column_view.is_some() {
        draw_column_view(app, frame, area);
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
    // Which-key popup for a pending key prefix (drawn under other modals).
    draw_which_key(app, frame, area);
    if app.edit_sql.is_some() {
        draw_edit_sql(app, frame, area);
    }
    if app.db.is_some() {
        draw_db(app, frame, area);
    }
    if app.media_type_panel.is_some() {
        draw_media_type_panel(app, frame, area);
    }
    if app.macro_chooser.is_some() {
        draw_macro_chooser(app, frame, area);
    }
    if app.clipboard_chooser.is_some() {
        draw_clipboard_chooser(app, frame, area);
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
    if app.help.is_some() {
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

/// The chooser-family overlays (branch/task/diff/location/recent/file
/// browser). Split out of [`draw_overlays`] to keep it within the line limit.
fn draw_chooser_overlays(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.branch_chooser.is_some() {
        draw_branch_chooser(app, frame, area);
    }
    if app.task_chooser.is_some() {
        draw_task_chooser(app, frame, area);
    }
    if app.script_chooser.is_some() {
        draw_script_chooser(app, frame, area);
    }
    if app.diff_view.is_some() {
        draw_diff_view(app, frame, area);
    }
    if app.location_chooser.is_some() {
        draw_location_chooser(app, frame, area);
    }
    if app.capture_chooser.is_some() {
        draw_capture_chooser(app, frame, area);
    }
    if app.refile_chooser.is_some() {
        draw_refile_chooser(app, frame, area);
    }
    if app.recent_chooser.is_some() {
        draw_recent_chooser(app, frame, area);
    }
    if app.file_browser.is_some() {
        draw_file_browser(app, frame, area);
    }
}

fn draw_pomodoro(app: &mut App, frame: &mut Frame, area: Rect) {
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

/// The active editor cursor's screen position within the editor text area, as
/// `(x, y)`, accounting for vertical scroll. The x is approximate (it does not
/// subtract the line-number gutter), which is fine for anchoring a popup.
fn cursor_screen_yx(app: &App) -> Option<(u16, u16)> {
    let area = app.layout.editor;
    let t = app.editor.active_tab()?;
    let (line, col) = app.editor.cursor_1based();
    let off_y = t.editor.get_offset_y();
    let y = area.y + u16::try_from(line.saturating_sub(1).saturating_sub(off_y)).unwrap_or(0);
    let x = area.x
        + u16::try_from(col.saturating_sub(1))
            .unwrap_or(0)
            .min(area.width);
    Some((
        x.min(area.x + area.width.saturating_sub(1)),
        y.min(area.y + area.height.saturating_sub(1)),
    ))
}

/// Draw the LSP completion popup as a small list anchored at the cursor.
fn draw_completion(app: &App, frame: &mut Frame) {
    let Some(popup) = app.completion.as_ref() else {
        return;
    };
    if popup.items.is_empty() {
        return;
    }
    let area = app.layout.editor;
    if area.width < 16 || area.height < 4 {
        return;
    }
    let max_rows = 10.min(popup.items.len());
    // Scroll so the highlighted row stays visible.
    let start = if popup.selected >= max_rows {
        popup.selected + 1 - max_rows
    } else {
        0
    };
    let end = (start + max_rows).min(popup.items.len());

    // Width from the widest visible label (+detail), capped to the area.
    let widest = popup.items[start..end]
        .iter()
        .map(|it| {
            it.label.chars().count() + it.detail.as_ref().map_or(0, |d| d.chars().count() + 3)
        })
        .max()
        .unwrap_or(10);
    let width =
        (u16::try_from(widest).unwrap_or(u16::MAX) + 2).clamp(16, area.width.saturating_sub(2));
    let height = u16::try_from(max_rows).unwrap_or(u16::MAX) + 2;

    let (cx, cy) = cursor_screen_yx(app).unwrap_or((area.x + 2, area.y));
    // Below the cursor if it fits, else above.
    let y = if cy + 1 + height <= area.y + area.height {
        cy + 1
    } else {
        cy.saturating_sub(height)
    };
    let x = cx.min(area.x + area.width.saturating_sub(width));
    let rect = Rect {
        x,
        y,
        width,
        height,
    };

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
    let rect = Rect {
        x,
        y,
        width,
        height,
    };

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

fn draw_color_converter(app: &mut App, frame: &mut Frame, area: Rect) {
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

fn draw_code_actions(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(menu) = app.code_actions.as_ref() else {
        return;
    };
    let titles: Vec<&str> = menu.actions.iter().map(|(t, _)| t.as_str()).collect();
    draw_chooser(
        frame,
        area,
        &t!("menu.item.lsp.code_action"),
        &titles,
        menu.selected,
    );
}

fn draw_code_lens(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(menu) = app.code_lens.as_ref() else {
        return;
    };
    let titles: Vec<&str> = menu.lenses.iter().map(|(_, t, _, _)| t.as_str()).collect();
    draw_chooser(
        frame,
        area,
        &t!("menu.item.lsp.code_lens"),
        &titles,
        menu.selected,
    );
}

/// A centered single-column chooser: a bordered list of `titles` with `selected`
/// highlighted. Shared by the code-action and code-lens menus.
fn draw_chooser(frame: &mut Frame, area: Rect, title: &str, titles: &[&str], selected: usize) {
    let longest = titles.iter().map(|t| t.chars().count()).max().unwrap_or(20);
    let width = u16::try_from(longest)
        .unwrap_or(u16::MAX)
        .saturating_add(4)
        .clamp(24, area.width);
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
            let style = if i == selected {
                theme::selected()
            } else {
                theme::base()
            };
            Line::from(Span::styled(format!(" {t} "), style))
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), inner);
}

fn draw_regex_tester(app: &mut App, frame: &mut Frame, area: Rect) {
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

fn draw_calculator(app: &mut App, frame: &mut Frame, area: Rect) {
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

fn draw_unit_converter(app: &mut App, frame: &mut Frame, area: Rect) {
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

/// Unix seconds as a local `YYYY-MM-DD HH:MM` label; empty when unknown.
fn unix_secs_label(secs: Option<i64>) -> String {
    secs.and_then(|s| jiff::Timestamp::from_second(s).ok())
        .map(|ts| {
            ts.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
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

/// Columns (within the menu-bar rect) of the left- and right-dock toggle icons,
/// matching the right-aligned layout drawn by `draw_menu_bar`.
#[must_use]
pub fn dock_toggle_cols(menu: Rect) -> (u16, u16) {
    let right = menu.x + menu.width;
    // Layout from the right edge: FOLDER, space, BELL, space.
    (right.saturating_sub(4), right.saturating_sub(2))
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

/// Build the styled spans for explorer rows `top..end`: indent, type glyph,
/// optional cut-dimming and mark dot, and a trailing git-change letter. Split
/// out of [`draw_explorer`] to keep it within the line limit.
fn explorer_rows(app: &App, top: usize, end: usize) -> Vec<Vec<Span<'static>>> {
    app.explorer.nodes[top..end]
        .iter()
        .map(|n| {
            let indent = "  ".repeat(n.depth);
            let glyph = if n.is_symlink {
                icon::LINK
            } else if n.is_dir {
                if n.expanded {
                    icon::FOLDER_OPEN
                } else {
                    icon::FOLDER
                }
            } else {
                theme::file_icon(&n.name)
            };
            let mut style = Style::default();
            let cut_pending = app.clip_cut && app.clip.contains(&n.path);
            if cut_pending {
                style = style.add_modifier(Modifier::DIM);
            }
            let mark = if app.explorer.marked.contains(&n.path) {
                "● "
            } else {
                ""
            };
            let mut spans = vec![
                Span::raw(indent),
                Span::styled(format!("{mark}{glyph} {}", n.name), style),
            ];
            if !n.is_dir
                && let Some(change) = app.git_change_for(&n.path)
            {
                spans.push(Span::styled(
                    format!("  {}", change.letter()),
                    Style::default().fg(git_change_color(change)),
                ));
            }
            spans
        })
        .collect()
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
            format!(
                " {} {}  {} ",
                icon::FOLDER,
                t!("ui.explorer"),
                t!("ui.explorer_filtered")
            )
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
    let rows = explorer_rows(app, top, end);

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
        let sb = Rect {
            x: inner.x + inner.width - 1,
            y: inner.y,
            width: 1,
            height: u16::try_from(body_h).unwrap_or(u16::MAX),
        };
        draw_scrollbar(frame, sb, app.explorer.selected, total.saturating_sub(1));
    }
    app.layout.explorer_hscrollbar = if hbar {
        let hb = Rect {
            x: inner.x,
            y: inner.y + inner.height - 1,
            width: u16::try_from(text_w).unwrap_or(u16::MAX),
            height: 1,
        };
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
/// Width (cells) of the code-overview minimap column.
const MINIMAP_WIDTH: u16 = 16;

/// Draw the which-key popup (candidate keys for a pending prefix) anchored at the
/// bottom of `area`. No-op when no prefix is pending or there are no candidates.
fn draw_which_key(app: &App, frame: &mut Frame, area: Rect) {
    let Some((title, rows)) = app.which_key() else {
        return;
    };
    if rows.is_empty() {
        return;
    }
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, a)| {
            Line::from(vec![
                Span::styled(format!(" {k:<4}"), theme::selected()),
                Span::styled(format!(" {a} "), theme::dim()),
            ])
        })
        .collect();
    let width = rows
        .iter()
        .map(|(k, a)| k.len() + a.len() + 7)
        .max()
        .unwrap_or(20)
        .clamp(20, area.width.max(20) as usize);
    let width = u16::try_from(width).unwrap_or(20).min(area.width);
    let height = (u16::try_from(rows.len()).unwrap_or(1) + 2).min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width),
        y: area.y + area.height.saturating_sub(height),
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(format!(" {title} "));
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

/// Draw the jump-to-line labels at the left edge of each labeled visible row.
fn draw_jump_labels(app: &App, frame: &mut Frame) {
    let Some(jm) = app.jump.as_ref() else { return };
    let ed = app.layout.editor;
    let top = app.editor.top_visible_line();
    let style = theme::selected();
    for (label, line) in &jm.labels {
        // The label's row within the editor viewport (non-wrapped mapping).
        let Some(row_off) = line.checked_sub(top) else {
            continue;
        };
        let y = ed.y + u16::try_from(row_off).unwrap_or(u16::MAX);
        if y >= ed.y + ed.height {
            continue;
        }
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(label.clone(), style))),
            Rect {
                x: ed.x,
                y,
                width: u16::try_from(label.len()).unwrap_or(2).min(ed.width),
                height: 1,
            },
        );
    }
}

/// Draw the code-overview minimap for the active buffer in `area`. Each row maps
/// to a band of source lines, drawn as a dim bar whose length tracks the band's
/// longest (trimmed) line — a rough "shape of the code". Rows overlapping the
/// editor's current viewport (`editor_height` rows from the scroll offset) get a
/// highlighted background so the visible region stands out.
fn draw_minimap(app: &mut App, frame: &mut Frame, area: Rect, editor_height: usize) {
    let Some(tab) = app.editor.active_tab() else {
        return;
    };
    if tab.is_image() {
        return;
    }
    let lines: Vec<String> = tab.text().lines().map(str::to_string).collect();
    let total = lines.len().max(1);
    let rows = area.height as usize;
    let top = app.editor.top_visible_line();
    let view_end = (top + editor_height).min(total);
    let width = area.width as usize;
    let dim = theme::dim();
    let view_bg = theme::region_title(theme::Region::Editor, true);
    let mut text = Vec::with_capacity(rows);
    for r in 0..rows {
        // The band of source lines this minimap row represents.
        let start = r * total / rows;
        let end = ((r + 1) * total / rows).max(start + 1).min(total);
        let longest = lines[start..end]
            .iter()
            .map(|l| l.trim_end().chars().count())
            .max()
            .unwrap_or(0);
        // Scale the longest line (~120 cols) to the minimap width.
        let bar = (longest * width / 120).clamp(usize::from(longest > 0), width);
        let in_view = start < view_end && end > top;
        let style = if in_view { view_bg } else { dim };
        let glyph = if in_view { '\u{2593}' } else { '\u{2592}' }; // ▓ vs ▒
        let mut s: String = std::iter::repeat_n(glyph, bar).collect();
        for _ in bar..width {
            s.push(' ');
        }
        text.push(Line::from(Span::styled(s, style)));
    }
    frame.render_widget(Paragraph::new(text), area);
}

fn draw_editor_region(app: &mut App, frame: &mut Frame, inner: Rect) {
    use crate::editor::SplitDir;
    app.layout.editor_region = inner;
    let panes = app.editor.split_layout(inner);
    if panes.is_empty() {
        // Carve a minimap column from the right edge (when enabled and there's
        // room); the remainder splits into editor text + scrollbar as before.
        let (inner, minimap_area) = if app.settings.show_minimap && inner.width > 40 {
            let s = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(1), Constraint::Length(MINIMAP_WIDTH)])
                .split(inner);
            (s[0], s[1])
        } else {
            (
                inner,
                Rect {
                    width: 0,
                    height: 0,
                    ..inner
                },
            )
        };
        app.layout.minimap = minimap_area;
        let (editor_area, scrollbar_area) = if app.show_scrollbar {
            let s = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(inner);
            (s[0], s[1])
        } else {
            (
                inner,
                Rect {
                    width: 0,
                    height: 0,
                    ..inner
                },
            )
        };
        // Sticky scroll: reserve the top row for the enclosing scope's header.
        let header = if editor_area.height > 1 {
            app.sticky_header()
        } else {
            None
        };
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
        if minimap_area.width > 0 {
            draw_minimap(app, frame, minimap_area, editor_area.height as usize);
        }
        return;
    }

    // Draw split dividers (one per internal tree node).
    let dstyle = theme::region_title(theme::Region::Editor, true);
    for (dir, divider) in app.editor.split_dividers(inner) {
        if dir == SplitDir::Vertical {
            let col: Vec<Line> = (0..divider.height)
                .map(|_| Line::from(Span::styled("│", dstyle)))
                .collect();
            frame.render_widget(Paragraph::new(col), divider);
        } else {
            let row = "─".repeat(divider.width as usize);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(row, dstyle))),
                divider,
            );
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
        (
            area,
            Rect {
                width: 0,
                height: 0,
                ..area
            },
        )
    };

    if app
        .editor
        .tabs
        .get(tab_index)
        .is_some_and(super::editor::Tab::is_image)
    {
        if let Some(tab) = app.editor.tabs.get_mut(tab_index)
            && let Some(proto) = tab.image.as_mut()
        {
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
    let Ok(rel) = u16::try_from(RULER_COLUMN - off) else {
        return;
    };
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
    let is_image = app
        .editor
        .active_tab()
        .is_some_and(super::editor::Tab::is_image);
    if is_image {
        if let Some(tab) = app.editor.active_tab_mut()
            && let Some(proto) = tab.image.as_mut()
        {
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
        let editor_area = if hbar {
            Rect {
                height: text.height - 1,
                ..text
            }
        } else {
            text
        };
        let vsb = if hbar {
            Rect {
                height: scrollbar.height.saturating_sub(1),
                ..scrollbar
            }
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
        .title(format!(
            " {} {} {pass}/{fail}/{ignore} ",
            icon::CODE,
            t!("ui.tests")
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.layout.test_panel = inner;

    if app.test_results.is_empty() {
        let hint = Paragraph::new(t!("ui.tests_idle").to_string())
            .style(theme::dim())
            .wrap(Wrap { trim: true });
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
            if i == app.test_selected {
                row.style(theme::selected())
            } else {
                row
            }
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
        lines.push(Line::from(Span::styled(
            text,
            theme::title(true).add_modifier(Modifier::BOLD),
        )));
    };
    if !app.dap.is_active() {
        let hint = Paragraph::new(t!("ui.debug_idle").to_string())
            .style(theme::dim())
            .wrap(Wrap { trim: true });
        frame.render_widget(hint, inner);
        return;
    }
    header(&mut lines, t!("ui.debug_call_stack").to_string());
    if app.dap_stack.is_empty() {
        lines.push(Line::from(Span::styled("  —", theme::dim())));
    }
    for f in &app.dap_stack {
        let loc = f
            .path
            .as_deref()
            .and_then(|p| p.rsplit('/').next())
            .map_or(String::new(), |n| format!("  {n}:{}", f.line));
        lines.push(Line::from(vec![
            Span::raw(format!("  {}", f.name)),
            Span::styled(loc, theme::dim()),
        ]));
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
        let hint = Paragraph::new(t!("status.outline_empty").to_string())
            .style(theme::dim())
            .wrap(Wrap { trim: true });
        frame.render_widget(hint, inner);
        return;
    };
    let view_h = inner.height as usize;
    o.ensure_visible(view_h);
    let total = o.len();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in o.scroll..(o.scroll + view_h).min(total) {
        let e = &o.entries[idx];
        let kind = if e.kind.is_empty() {
            String::new()
        } else {
            format!("{:<6} ", e.kind)
        };
        if idx == o.selected {
            lines.push(Line::from(Span::styled(
                format!("{kind}{}", e.name),
                theme::selected(),
            )));
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

    let items: Vec<ListItem> = rows
        .iter()
        .map(|s| ListItem::new(Line::from(hslice_spans(s, off, text_w))))
        .collect();
    let list_area = Rect {
        width: u16::try_from(text_w).unwrap_or(u16::MAX),
        height: body_h,
        ..inner
    };
    let list = List::new(items).highlight_style(theme::selected());
    let mut state = ListState::default();
    state.select(Some(app.messages.selected));
    frame.render_stateful_widget(list, list_area, &mut state);
    if vbar {
        let sb = Rect {
            x: inner.x + inner.width - 1,
            y: inner.y,
            width: 1,
            height: body_h,
        };
        draw_scrollbar(frame, sb, app.messages.selected, total.saturating_sub(1));
    }
    app.layout.messages_hscrollbar = if hbar {
        let hb = Rect {
            x: inner.x,
            y: inner.y + inner.height - 1,
            width: u16::try_from(text_w).unwrap_or(u16::MAX),
            height: 1,
        };
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
    let thumb = (pos.min(max) * span + max / 2)
        .checked_div(max)
        .unwrap_or(0);
    let mut lines: Vec<Line> = Vec::with_capacity(h);
    for r in 0..h {
        lines.push(Line::from(if r == thumb {
            thumb_glyph.clone()
        } else {
            track_glyph()
        }));
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
    let thumb = (pos.min(max) * span + max / 2)
        .checked_div(max)
        .unwrap_or(0);
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
    let view_h = if allow_bars {
        inner.height.saturating_sub(1)
    } else {
        inner.height
    } as usize;
    let visible: Vec<String> = app.bottom_dock.visible(view_h).to_vec();
    let content_w = max_line_width(visible.iter().map(String::as_str));
    let text_w_full = if vbar { inner.width - 1 } else { inner.width } as usize;
    let hbar = allow_bars && content_w > text_w_full;

    let text_w = text_w_full;
    let body_h = if hbar { inner.height - 1 } else { inner.height };
    let text_area = Rect {
        width: u16::try_from(text_w).unwrap_or(u16::MAX),
        height: body_h,
        ..inner
    };

    let hmax = content_w.saturating_sub(text_w);
    app.bottom_hmax = hmax;
    app.bottom_hscroll = app.bottom_hscroll.min(hmax);
    let off = app.bottom_hscroll;

    let lines: Vec<Line> = if app.bottom_dock.is_empty() {
        vec![Line::from(Span::styled(
            t!("ui.bottom_dock_empty").to_string(),
            theme::dim(),
        ))]
    } else {
        visible
            .iter()
            .map(|l| Line::from(hslice(l, off, text_w)))
            .collect()
    };
    frame.render_widget(Paragraph::new(lines), text_area);

    if vbar {
        let sb = Rect {
            x: inner.x + inner.width - 1,
            y: inner.y,
            width: 1,
            height: body_h,
        };
        draw_scrollbar(
            frame,
            sb,
            app.bottom_dock.scroll,
            total.saturating_sub(view_h),
        );
    }
    app.layout.bottom_hscrollbar = if hbar {
        let hb = Rect {
            x: inner.x,
            y: inner.y + inner.height - 1,
            width: u16::try_from(text_w).unwrap_or(u16::MAX),
            height: 1,
        };
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

    let git = crate::status_bar_panel::git_segment(
        app.git_branch.as_deref(),
        icon::BRANCH,
        app.git_dirty(),
    );
    let left = crate::status_bar_panel::left_segment(&mode, &path, &dirty_flag, &app.status);
    let right =
        crate::status_bar_panel::right_segment(&format!("{git}{info}"), line, col, icon::CALENDAR);

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

    frame.render_widget(
        Paragraph::new(left).style(bg).alignment(Alignment::Left),
        cols[0],
    );
    frame.render_widget(
        Paragraph::new(right).style(bg).alignment(Alignment::Right),
        cols[1],
    );

    // Record the git/branch segment's rectangle (the leftmost part of the
    // right-aligned right segment, after its 1-cell padding) so a click on the
    // branch indicator opens the Git panel.
    let git_w = u16::try_from(git.chars().count()).unwrap_or(u16::MAX);
    app.layout.git_status_bar = if git_w > 0 {
        Rect {
            x: cols[1].x + 1,
            y: cols[1].y,
            width: git_w.min(cols[1].width),
            height: 1,
        }
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

    let help = Line::from(Span::styled(
        t!("ui.calendar_hint").to_string(),
        theme::dim(),
    ));
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
/// hit-test in `App::nerd_mouse` (private) divides by this, so the renderer
/// and the hit-test must agree on it.
pub const NERD_CELL_W: u16 = 4;

/// Truncate `s` to `max` columns, adding an ellipsis when clipped.
fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let keep = max.saturating_sub(1);
        format!("{}…", s.chars().take(keep).collect::<String>())
    }
}

fn draw_dashboard(app: &App, frame: &mut Frame, area: Rect) {
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
