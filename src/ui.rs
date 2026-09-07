//! All rendering. `draw` lays out the frame, records pane rectangles for mouse
//! hit-testing, and delegates to per-pane helpers.

#![warn(clippy::pedantic)]

mod ai_terminal;
mod boxes;
mod choosers;
mod db;
mod dialogs;
mod docks;
mod edit_surfaces;
mod explorer;
mod file_browser;
mod help;
mod hints;
mod info_panels;
mod lsp_popups;
mod menu_bar;
mod picker_panels;
mod search;
mod tabs;
mod tool_panels;

use ai_terminal::{draw_ai_diff, draw_ai_panel, draw_terminal};
use boxes::{draw_calendar, draw_clock, draw_dashboard};
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
use docks::{draw_debug_panel, draw_messages, draw_outline_dock, draw_test_panel};
use edit_surfaces::{
    draw_column_view, draw_edit_bytes, draw_edit_outline, draw_edit_sql, draw_edit_table,
    draw_edit_value, draw_html_panel, draw_outline,
};
use explorer::draw_explorer;
use file_browser::draw_file_browser;
use help::{draw_help, draw_keybinding_editor};
use hints::{draw_jump_labels, draw_which_key};
use info_panels::{
    draw_contacts, draw_file_info, draw_markdown_preview, draw_snippets, draw_system_info,
    draw_text_info, draw_vcard,
};
use lsp_popups::{draw_code_actions, draw_code_lens, draw_completion, draw_hover};
use menu_bar::{draw_menu_bar, draw_menu_dropdown, dropdown_width};
use picker_panels::{
    draw_ascii_panel, draw_media_type_panel, draw_nerd_palette, draw_qrcode, draw_x11_panel,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use search::{draw_palette, draw_prompt, draw_search, draw_workspace_search};
use tabs::{center_split, draw_breadcrumb, draw_tabs};
use tool_panels::{
    draw_calculator, draw_color_converter, draw_dialog, draw_pomodoro, draw_regex_tester,
    draw_unit_converter, draw_welcome,
};

use ratatui_image::StatefulImage;
use ratatui_image::protocol::StatefulProtocol;

use crate::app::{App, Focus};
use crate::menu::menus;
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

/// Render the editor region: a single pane, or two split panes with a divider.
/// Width (cells) of the code-overview minimap column.
const MINIMAP_WIDTH: u16 = 16;

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
