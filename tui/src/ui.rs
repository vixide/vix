//! All rendering. `draw` lays out the frame, records pane rectangles for mouse
//! hit-testing, and delegates to per-pane helpers.

#[cfg(feature = "ai")]
mod ai;
mod bidi;
mod bottom_dock;
mod boxes;
mod choice_list;
mod choosers;
#[cfg(feature = "sql")]
mod db;
mod dialogs;
mod docks;
mod edit_surfaces;
mod editor_region;
mod explorer;
mod fhir;
mod file_browser;
mod ghost;
mod git_graph;
mod help;
mod hints;
mod hl7;
mod hyperlink;
mod im;
mod info_panels;
mod large_file;
mod lsp_popups;
mod lsp_status;
mod menu_bar;
mod minimap;
mod oc;
mod picker_panels;
mod scroll_markers;
mod search;
mod status_bar;
mod tabs;
#[cfg(feature = "terminal")]
mod terminal;
mod tool_panels;
mod undo_tree;

#[cfg(feature = "ai")]
use ai::{draw_ai_diff, draw_ai_panel};
use bottom_dock::draw_bottom_dock;
use boxes::{draw_calendar, draw_clock, draw_dashboard};
use choice_list::draw_choice_list;
use choosers::{
    draw_branch_chooser, draw_capture_chooser, draw_clipboard_chooser, draw_context_menu,
    draw_diff_view, draw_encoding_chooser, draw_git_log, draw_git_panel, draw_git_rebase,
    draw_http_env_chooser, draw_location_chooser, draw_macro_chooser, draw_recent_chooser,
    draw_refile_chooser, draw_spell_suggest, draw_stash_chooser, draw_task_chooser,
    draw_workspace_chooser, draw_worktree_chooser,
};
#[cfg(feature = "script")]
use choosers::{draw_plugin_chooser, draw_script_chooser};
use dialogs::{
    draw_confirm, draw_dir_edit_confirm, draw_paste_conflict, draw_query_replace,
    draw_replace_confirm, draw_script_trust, draw_structural_replace, draw_unsaved,
    draw_wgrep_confirm,
};
use docks::{draw_debug_panel, draw_messages, draw_outline_dock, draw_test_panel};
use edit_surfaces::{
    draw_column_view, draw_conflict_list, draw_edit_bytes, draw_edit_outline, draw_edit_table,
    draw_edit_value, draw_html_panel, draw_outline,
};
use editor_region::draw_editor_region;
use explorer::draw_explorer;
use fhir::draw_fhir_bundle;
use file_browser::draw_file_browser;
use help::{draw_help, draw_keybinding_editor};
use hints::{draw_jump_labels, draw_which_key};
use info_panels::{
    draw_contacts, draw_file_info, draw_markdown_preview, draw_markdown_toc, draw_snippets,
    draw_system_info, draw_text_info, draw_vcard,
};
use lsp_popups::{draw_code_actions, draw_completion, draw_hover};
use lsp_status::draw_lsp_status;
use menu_bar::{draw_menu_bar, draw_menu_dropdown, dropdown_width};
use minimap::draw_minimap;
use picker_panels::{
    draw_ascii_panel, draw_media_type_panel, draw_nerd_palette, draw_qrcode, draw_theme_editor,
    draw_todo_tree, draw_x11_panel,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use search::{draw_palette, draw_prompt, draw_search, draw_workspace_search};
use status_bar::draw_status_bar;
pub use tabs::tabs_origin;
use tabs::{center_split, draw_breadcrumb, draw_tabs};
#[cfg(feature = "cowork")]
use terminal::draw_cowork_terminal;
#[cfg(feature = "terminal")]
use terminal::draw_terminal;
use tool_panels::{
    draw_calculator, draw_color_converter, draw_dialog, draw_pomodoro, draw_regex_tester,
    draw_unit_converter, draw_welcome,
};
use undo_tree::draw_undo_tree;

use crate::app::{App, AppFlags, Focus, OverlayId};
use crate::menu::MenuDef;
use crate::theme;

/// The body's column rectangles: file explorer, center editor, message drawer,
/// and outline sidebar. `None` for any dock that is hidden.
struct BodyColumns {
    explorer: Option<Rect>,
    center: Rect,
    messages: Option<Rect>,
    im: Option<Rect>,
    outline: Option<Rect>,
    debug: Option<Rect>,
    test: Option<Rect>,
    agent_plan: Option<Rect>,
}

impl BodyColumns {
    /// The side docks in draw order (see [`draw_side_docks`]).
    fn docks(&self) -> [Option<Rect>; 6] {
        [
            self.messages,
            self.im,
            self.outline,
            self.debug,
            self.test,
            self.agent_plan,
        ]
    }
}

/// `clamp` that never panics: where `min > max` (a terminal smaller than a
/// popup's preferred minimum) the result is `max`, so the popup just shrinks to
/// fit. `Ord::clamp` asserts `min <= max`, which a tiny window breaks (T575).
pub(crate) trait ClampFit: Ord + Sized {
    /// `self` at least `min`, then at most `max`.
    fn clamp_fit(self, min: Self, max: Self) -> Self {
        self.max(min).min(max)
    }
}

impl ClampFit for u16 {}

/// Split the body area into explorer | center | messages | outline | debug | test
/// columns based on which docks are shown. Dock widths come from settings,
/// clamped so the editor keeps room.
/// Whether the layout is mirrored for a right-to-left UI language (T564).
#[must_use]
pub fn layout_mirrored(app: &App) -> bool {
    bidi::mirrored(&app.settings.mirror_layout, &rust_i18n::locale())
}

/// Width (columns) of the IM dock panel (T752), clamped like the other docks.
const IM_DOCK_WIDTH: u16 = 40;

/// Width (columns) of the docked agent plan pane (T635).
const AGENT_PLAN_WIDTH: u16 = 36;

/// Split the body row into the explorer, centre and message-dock columns.
///
/// Column widths come from the settings, with the docks clamped so the centre
/// keeps at least 20 columns; a mirrored layout reverses the left-to-right order.
fn body_columns(app: &App, body: Rect) -> BodyColumns {
    use crate::app::Visible;
    let dock_max = body.width.saturating_sub(20).max(12);
    let st = &app.settings;
    // Left-to-right order; a mirrored layout reverses it.
    let mut order: Vec<(char, Constraint)> = Vec::new();
    if app.visible.contains(Visible::EXPLORER) {
        order.push((
            'e',
            Constraint::Length(st.explorer_width.clamp(12, dock_max)),
        ));
    }
    order.push(('c', Constraint::Min(20)));
    if app.visible.contains(Visible::MESSAGES) {
        order.push((
            'm',
            Constraint::Length(st.messages_width.clamp(12, dock_max)),
        ));
    }
    if app.im.open {
        order.push(('i', Constraint::Length(IM_DOCK_WIDTH.clamp(12, dock_max))));
    }
    if st.secondary_panels.show_outline_dock {
        order.push((
            'o',
            Constraint::Length(st.outline_width.clamp(12, dock_max)),
        ));
    }
    if app.visible.contains(Visible::DEBUG_PANEL) {
        order.push(('d', Constraint::Length(st.debug_width.clamp(12, dock_max))));
    }
    if app.visible.contains(Visible::TEST_PANEL) {
        order.push(('t', Constraint::Length(st.test_width.clamp(12, dock_max))));
    }
    if app.visible.contains(Visible::AGENT_PLAN) {
        order.push((
            'a',
            Constraint::Length(AGENT_PLAN_WIDTH.clamp(12, dock_max)),
        ));
    }
    if layout_mirrored(app) {
        order.reverse();
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(order.iter().map(|&(_, c)| c).collect::<Vec<_>>())
        .split(body);
    let at = |k: char| order.iter().position(|&(o, _)| o == k).map(|i| cols[i]);
    BodyColumns {
        explorer: at('e'),
        center: at('c').unwrap_or(body),
        messages: at('m'),
        im: at('i'),
        outline: at('o'),
        debug: at('d'),
        test: at('t'),
        agent_plan: at('a'),
    }
}

/// Render the whole frame: lay out panes, record their rectangles for mouse
/// hit-testing, draw each pane, then draw any active overlay on top.
pub fn draw(app: &mut App, frame: &mut Frame) {
    // Refresh misspelled-word underlines before painting. Frames also fire on
    // idle ticks and while a task is busy, so this only re-scans the buffer when
    // the active tab or its content changed (T597).
    if app.flags.contains(AppFlags::SPELLCHECK) {
        app.refresh_spellcheck_if_stale();
    }
    // A compared tab's gutter shows the comparison instead (T660).
    let comparing = app.refresh_compare();
    if comparing {
        // Both compared tabs already carry their compare marks.
    } else if app.coverage_gutter_active() {
        app.refresh_coverage_gutter_if_stale();
    } else if app.flags.contains(AppFlags::GIT_REPO) {
        app.refresh_git_gutter();
    }
    // Anchored IM threads (T753) have their own marks in the same sign
    // column; the editor draws a thread's ◆ over the diff/coverage bar on
    // that one line (see `Editor::set_comment_marks`).
    app.refresh_thread_marks_if_stale();
    app.layout.hyperlink_regions.clear();
    let area = frame.area();
    // Paint the whole frame in the theme's background so every pane (and the gaps
    // between them) shares one background — important for the light theme.
    frame.render_widget(Block::default().style(theme::base()), area);
    let mut vconstraints = vec![
        Constraint::Length(1), // menu bar
        Constraint::Min(1),    // body
    ];
    if app.visible.contains(crate::app::Visible::STATUS_BAR) {
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
    let (body, bottom_dock_rect) = if app.visible.contains(crate::app::Visible::BOTTOM_DOCK) {
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

    let cols = body_columns(app, body);
    let docks = cols.docks();
    let (explorer_rect, center_rect, messages_rect) = (cols.explorer, cols.center, cols.messages);

    // Center: tab bar, optional breadcrumb bar, then editor+scrollbar.
    let (tabs_rect, breadcrumb_rect, editor_cell) = center_split(
        center_rect,
        app.visible.contains(crate::app::Visible::BREADCRUMBS),
    );
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
    draw_side_docks(app, frame, docks);
    if let Some(r) = bottom_dock_rect {
        app.layout.bottom_dock = r;
        draw_bottom_dock(app, frame, r);
    }
    if let Some(r) = status_row {
        draw_status_bar(app, frame, r);
    }

    draw_overlays(app, frame, area, rows[0]);
    end_of_frame_passes(app, frame);
}

/// The passes over the finished frame, in order: right-to-left text, the
/// single-color repaint, OSC 8 hyperlinks, and last the downsample of every
/// color to what the terminal shows (T628).
fn end_of_frame_passes(app: &App, frame: &mut Frame) {
    right_to_left_passes(app, frame);
    single_color_pass(frame);
    hyperlink::apply(app, frame);
    crate::theme::color_depth::downsample_buffer(frame.buffer_mut(), app.terminal_caps.color_depth);
}

/// The docks beside the editor, each only when shown: messages, IM, outline,
/// debugger, tests, agent plan.
fn draw_side_docks(app: &mut App, frame: &mut Frame, rects: [Option<Rect>; 6]) {
    let [messages, im, outline, debug, test, agent_plan] = rects;
    if let Some(r) = messages {
        draw_messages(app, frame, r);
    }
    if let Some(r) = im {
        im::draw_im_dock(app, frame, r);
    }
    if let Some(r) = outline {
        draw_outline_dock(app, frame, r);
    }
    if let Some(r) = debug {
        draw_debug_panel(app, frame, r);
    }
    if let Some(r) = test {
        draw_test_panel(app, frame, r);
    }
    #[cfg(feature = "ai")]
    if let Some(r) = agent_plan {
        ai::draw_agent_plan_dock(app, frame, r);
    }
    #[cfg(not(feature = "ai"))]
    let _ = agent_plan;
}

/// A single-color theme repaints the whole finished frame in its one color,
/// including colors drawn outside the theme (gutters, spell check, the
/// terminal pane). See `crates/vix-theme/spec/index.md`.
fn single_color_pass(frame: &mut Frame) {
    if let Some((fg, bg)) = crate::theme_model::single_color::active() {
        crate::theme_model::single_color::flatten(frame.buffer_mut(), fg, bg);
    }
}

/// The end-of-frame right-to-left passes: reorder bidirectional text, then
/// right-align the mirrored surfaces (T564).
fn right_to_left_passes(app: &App, frame: &mut Frame) {
    if bidi::enabled(&app.settings.bidi, &rust_i18n::locale()) {
        bidi::reorder_buffer(frame.buffer_mut());
    }
    if layout_mirrored(app) {
        reflect_panels(app, frame.buffer_mut());
    }
}

/// Right-align the text of the surfaces whose mouse handling is row-based (the
/// explorer and the menu dropdowns), leaving borders and scroll columns in
/// place. Panels whose clicks depend on columns stay left-aligned (T564).
fn reflect_panels(app: &App, buf: &mut ratatui::buffer::Buffer) {
    use crate::app::Visible;
    let ex = app.layout.explorer;
    if app.visible.contains(Visible::EXPLORER) && ex.width > 4 {
        // Inside the border, short of the scroll bar column.
        bidi::reflect_rows(
            buf,
            ex.x + 1,
            ex.right().saturating_sub(2),
            ex.y + 1,
            ex.bottom(),
        );
    }
    if app.menu.open.is_some() {
        let mut rects = vec![app.layout.menu_dropdown];
        if app.menu.submenu_open() {
            rects.push(app.layout.submenu_dropdown);
        }
        if app.menu.subsubmenu_open() {
            rects.push(app.layout.subsubmenu_dropdown);
        }
        for d in rects.into_iter().filter(|d| d.width > 2 && d.height > 2) {
            bidi::reflect_rows(buf, d.x + 1, d.right() - 1, d.y + 1, d.bottom() - 1);
        }
    }
}

/// Draw every open overlay on top of the base frame, bottom of the stack
/// first. The order is the one table in `crate::app::overlays::OVERLAYS` --
/// the same table `App::route_overlay_key` walks backwards to deliver keys,
/// so the topmost layer drawn is the one that takes the key (T611).
fn draw_overlays(app: &mut App, frame: &mut Frame, area: Rect, menu_bar: Rect) {
    // Collected first: drawing takes `&mut App`, and a layer may close or open
    // others, which the old per-layer `is_some()` cascade also tolerated.
    let open: Vec<OverlayId> = app.open_overlays().map(|o| o.id).collect();
    for id in open {
        draw_overlay(app, frame, area, menu_bar, id);
    }
}

/// The menu bar's open dropdown: place it, then draw it.
fn draw_menu_overlay(app: &mut App, frame: &mut Frame, area: Rect, menu_bar: Rect) {
    if let Some(i) = app.menu.open {
        app.layout.menu_dropdown =
            menu_dropdown_rect_for(app.menu.defs(), area, menu_bar, i, layout_mirrored(app));
    }
    draw_menu_dropdown(app, frame);
}

/// Part 1 of the overlay draw dispatch (split for the per-function line limit).
fn draw_overlay(app: &mut App, frame: &mut Frame, area: Rect, menu_bar: Rect, id: OverlayId) {
    match id {
        OverlayId::Jump => draw_jump_labels(app, frame),
        OverlayId::Calendar => draw_calendar(app, frame, area),
        OverlayId::Clock => draw_clock(app, frame, area),
        OverlayId::Menu => draw_menu_overlay(app, frame, area, menu_bar),
        OverlayId::Search => draw_search(app, frame, area),
        OverlayId::Palette => draw_palette(app, frame, area),
        OverlayId::WorkspaceSearch => draw_workspace_search(app, frame, area),
        OverlayId::KeybindingEditor => draw_keybinding_editor(app, frame, area),
        OverlayId::GitRebase => draw_git_rebase(app, frame, area),
        OverlayId::Prompt => draw_prompt(app, frame, area),
        OverlayId::QueryReplace => draw_query_replace(app, frame, area),
        OverlayId::Confirm => draw_confirm(app, frame, area),
        OverlayId::ScriptTrust => draw_script_trust(app, frame, area),
        OverlayId::ReplaceConfirm => draw_replace_confirm(app, frame, area),
        OverlayId::Unsaved => draw_unsaved(app, frame, area),
        OverlayId::SpellSuggest => draw_spell_suggest(app, frame, area),
        OverlayId::ContextMenu => draw_context_menu(app, frame, area),
        OverlayId::GitPanel => draw_git_panel(app, frame, area),
        OverlayId::BranchChooser => draw_branch_chooser(app, frame, area),
        OverlayId::TaskChooser => draw_task_chooser(app, frame, area),
        OverlayId::StashChooser => draw_stash_chooser(app, frame, area),
        OverlayId::WorktreeChooser => draw_worktree_chooser(app, frame, area),
        #[cfg(feature = "script")]
        OverlayId::ScriptChooser => draw_script_chooser(app, frame, area),
        #[cfg(feature = "script")]
        OverlayId::PluginChooser => draw_plugin_chooser(app, frame, area),
        OverlayId::DiffView => draw_diff_view(app, frame, area),
        OverlayId::LocationChooser => draw_location_chooser(app, frame, area),
        OverlayId::CaptureChooser => draw_capture_chooser(app, frame, area),
        OverlayId::RefileChooser => draw_refile_chooser(app, frame, area),
        OverlayId::RecentChooser => draw_recent_chooser(app, frame, area),
        OverlayId::FileBrowser => draw_file_browser(app, frame, area),
        OverlayId::NerdPalette => draw_nerd_palette(app, frame, area),
        OverlayId::AsciiPanel => draw_ascii_panel(app, frame, area),
        OverlayId::EditTable => draw_edit_table(app, frame, area),
        OverlayId::ColumnView => draw_column_view(app, frame, area),
        OverlayId::EditOutline => draw_edit_outline(app, frame, area),
        OverlayId::EditValue => draw_edit_value(app, frame, area),
        OverlayId::EditBytes => draw_edit_bytes(app, frame, area),
        _ => draw_overlay_2(app, frame, area, id),
    }
}

/// Part 2 of the overlay draw dispatch (split for the per-function line limit).
fn draw_overlay_2(app: &mut App, frame: &mut Frame, area: Rect, id: OverlayId) {
    match id {
        OverlayId::LargeFile => large_file::draw_large_file(app, frame, area),
        OverlayId::Qrcode => draw_qrcode(app, frame, area),
        #[cfg(feature = "ai")]
        OverlayId::AiPanel => draw_ai_panel(app, frame, area),
        #[cfg(feature = "terminal")]
        OverlayId::Terminal => draw_terminal(app, frame, area),
        #[cfg(feature = "cowork")]
        OverlayId::CoworkTerminal => draw_cowork_terminal(app, frame, area),
        #[cfg(feature = "cowork")]
        OverlayId::CoworkConfirm => draw_confirm(app, frame, area),
        #[cfg(feature = "ai")]
        OverlayId::AiDiff => draw_ai_diff(app, frame, area),
        OverlayId::ThemeEditor => draw_theme_editor(app, frame, area),
        OverlayId::UndoTree => draw_undo_tree(app, frame, area),
        OverlayId::X11Panel => draw_x11_panel(app, frame, area),
        OverlayId::HtmlPanel => draw_html_panel(app, frame, area),
        OverlayId::WhichKey => draw_which_key(app, frame, area),
        OverlayId::WgrepConfirm => draw_wgrep_confirm(app, frame, area),
        OverlayId::DirEditConfirm => draw_dir_edit_confirm(app, frame, area),
        OverlayId::GitLog => draw_git_log(app, frame, area),
        OverlayId::GitGraph => git_graph::draw_git_graph(app, frame, area),
        OverlayId::Oc => oc::draw_oc(app, frame, area),
        OverlayId::ImDialog => im::draw_im_dialog(app, frame, area),
        OverlayId::TodoTree => draw_todo_tree(app, frame, area),
        OverlayId::FhirBundle => draw_fhir_bundle(app, frame, area),
        OverlayId::Hl7Panel => hl7::draw_hl7_panel(app, frame, area),
        OverlayId::HttpEnvChooser => draw_http_env_chooser(app, frame, area),
        OverlayId::ChoiceList => draw_choice_list(app, frame, area),
        OverlayId::StructuralReplace => draw_structural_replace(app, frame, area),
        #[cfg(feature = "sql")]
        OverlayId::EditSql => edit_surfaces::draw_edit_sql(app, frame, area),
        #[cfg(feature = "sql")]
        OverlayId::Db => db::draw_db(app, frame, area),
        OverlayId::MediaTypePanel => draw_media_type_panel(app, frame, area),
        OverlayId::EncodingChooser => draw_encoding_chooser(app, frame, area),
        OverlayId::MacroChooser => draw_macro_chooser(app, frame, area),
        OverlayId::ClipboardChooser => draw_clipboard_chooser(app, frame, area),
        OverlayId::WorkspaceChooser => draw_workspace_chooser(app, frame, area),
        OverlayId::SystemInfo => draw_system_info(app, frame, area),
        OverlayId::LspStatus => draw_lsp_status(app, frame, area),
        OverlayId::FileInfo => draw_file_info(app, frame, area),
        OverlayId::TextInfo => draw_text_info(app, frame, area),
        OverlayId::MarkdownPreview => draw_markdown_preview(app, frame, area),
        _ => draw_overlay_3(app, frame, area, id),
    }
}

/// Part 3 of the overlay draw dispatch (split for the per-function line limit).
fn draw_overlay_3(app: &mut App, frame: &mut Frame, area: Rect, id: OverlayId) {
    match id {
        OverlayId::MarkdownToc => draw_markdown_toc(app, frame, area),
        OverlayId::Snippets => draw_snippets(app, frame, area),
        OverlayId::Contacts => draw_contacts(app, frame, area),
        OverlayId::Vcard => draw_vcard(app, frame, area),
        OverlayId::Dashboard => draw_dashboard(app, frame, area),
        OverlayId::Outline => draw_outline(app, frame, area),
        OverlayId::ConflictList => draw_conflict_list(app, frame, area),
        OverlayId::Completion => draw_completion(app, frame),
        OverlayId::Hover => draw_hover(app, frame),
        OverlayId::Paste => draw_paste_conflict(app, frame, area),
        OverlayId::Help => draw_help(app, frame, area),
        OverlayId::Dialog => draw_dialog(app, frame, area),
        OverlayId::ColorConverter => draw_color_converter(app, frame, area),
        OverlayId::UnitConverter => draw_unit_converter(app, frame, area),
        OverlayId::Calculator => draw_calculator(app, frame, area),
        OverlayId::RegexTester => draw_regex_tester(app, frame, area),
        OverlayId::CodeActions => draw_code_actions(app, frame, area),
        OverlayId::Pomodoro => draw_pomodoro(app, frame, area),
        OverlayId::Welcome => draw_welcome(app, frame, area),
        _ => {}
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

/// Left edge (in columns) of each top-level menu title in the menu bar.
///
/// Each title is padded by two columns, matching [`menu_title_width`].
fn menu_offsets(defs: &[MenuDef]) -> Vec<u16> {
    let mut offsets = Vec::new();
    let mut pos: u16 = 0;
    for m in defs {
        offsets.push(pos);
        pos += u16::try_from(m.title().chars().count()).unwrap_or(u16::MAX) + 2;
    }
    offsets
}

/// Width in columns of the top-level menu title at `index` of `defs` (the
/// menu bar as shown, [`crate::menu::Menu::defs`]), padding included.
#[must_use]
pub fn menu_title_width(defs: &[MenuDef], index: usize) -> u16 {
    u16::try_from(defs[index].title().chars().count()).unwrap_or(u16::MAX) + 2
}

/// Screen column where the top-level menu title at `index` starts. Left to right
/// from the bar's left edge, or, mirrored, right to left from its right edge so
/// the first menu is rightmost (T564).
#[must_use]
pub fn menu_title_x(defs: &[MenuDef], bar: Rect, index: usize, mirrored: bool) -> u16 {
    let offset = menu_offsets(defs)[index];
    if mirrored {
        (bar.x + bar.width).saturating_sub(offset + menu_title_width(defs, index))
    } else {
        bar.x + offset
    }
}

/// Columns of the explorer and messages dock toggle icons, matching the layout
/// drawn by `draw_menu_bar` (right-aligned, or left-aligned when mirrored).
#[must_use]
pub fn dock_toggle_cols(menu: Rect) -> (u16, u16) {
    dock_toggle_cols_for(menu, false)
}

/// [`dock_toggle_cols`] for an explicit layout direction.
#[must_use]
pub fn dock_toggle_cols_for(menu: Rect, mirrored: bool) -> (u16, u16) {
    if mirrored {
        // From the left edge: space, BELL (messages), space, FOLDER (explorer).
        (menu.x + 3, menu.x + 1)
    } else {
        let right = menu.x + menu.width;
        // From the right edge: FOLDER (explorer), space, BELL (messages), space.
        (right.saturating_sub(4), right.saturating_sub(2))
    }
}

/// Geometry of the dropdown for the menu at `index`. Shared by the renderer and
/// by mouse hit-testing (`App::on_mouse`) so clicks land on the right item.
#[must_use]
pub fn menu_dropdown_rect(defs: &[MenuDef], frame_area: Rect, bar: Rect, index: usize) -> Rect {
    menu_dropdown_rect_for(defs, frame_area, bar, index, false)
}

/// [`menu_dropdown_rect`] for an explicit layout direction. Mirrored, the
/// dropdown's right edge lines up with its title's right edge.
#[must_use]
pub fn menu_dropdown_rect_for(
    defs: &[MenuDef],
    frame_area: Rect,
    bar: Rect,
    index: usize,
    mirrored: bool,
) -> Rect {
    let def = &defs[index];
    let width = dropdown_width(def.items);
    let x = if mirrored {
        (menu_title_x(defs, bar, index, true) + menu_title_width(defs, index)).saturating_sub(width)
    } else {
        menu_title_x(defs, bar, index, false)
    };
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

/// The text column the editor ruler guide marks.
pub const RULER_COLUMN: usize = 80;

/// Vix's one-character scrollbar, drawn into the vertical one-column `area`: a
/// dim track and a single `●` thumb positioned **proportionally** to `pos`
/// within `0..=max`. The thumb is always one cell tall (never proportional
/// height) and the track spans the whole `area` (no end-cap arrows). For
/// cursor/selection views pass `pos = selected`, `max = total - 1` (so the thumb
/// reaches the bottom only on the last item); for scroll views pass
/// `pos = scroll`, `max = total - viewport`.
fn draw_scrollbar(frame: &mut Frame, area: Rect, pos: usize, max: usize) {
    draw_scrollbar_marked(frame, area, pos, max, &[]);
}

/// [`draw_scrollbar`] with colored markers on the track (T578): `markers[r]` is
/// the color for track row `r` (from `scroll_markers::bucket`), drawn as a heavy
/// `┃`. The thumb is drawn on top of a marker; rows past `markers.len()` have none.
fn draw_scrollbar_marked(
    frame: &mut Frame,
    area: Rect,
    pos: usize,
    max: usize,
    markers: &[Option<Color>],
) {
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
        } else if let Some(Some(color)) = markers.get(r) {
            Span::styled("┃", Style::default().fg(*color))
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

/// A rectangle `pct_x` percent wide and `pct_y` percent tall, centred in `area`.
///
/// Both percentages must be at most 100 (the margin arithmetic subtracts them
/// from 100).
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
