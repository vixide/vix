//! The keyboard-shortcut browser (`F1`) and its editable sibling, the
//! keybinding editor (**Vix → Keybindings…**): both a filter/sort/scroll
//! table over every built-in binding, the editor additionally marking user
//! and script overrides and a rebind/reset target row.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::{centered, trunc};
use crate::app::App;
use crate::theme::{self, icon};

pub(super) fn draw_help(app: &mut App, frame: &mut Frame, area: Rect) {
    use crate::keyboard_shortcut_panel::Column;
    let Some(h) = app.help.as_ref() else {
        return;
    };
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
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);
    // Filter line: a search glyph then the live query and a caret.
    let search = Line::from(vec![
        Span::styled(format!("{} ", icon::SEARCH), theme::title(true)),
        Span::raw(h.query.clone()),
        Span::styled("\u{2588}", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(search), chunks[0]);

    // Two columns: action name, then key combo. The keys column is as wide as
    // the widest combo; the action column takes the rest.
    let keys_w = h
        .rows
        .iter()
        .map(|s| s.keys.chars().count())
        .max()
        .unwrap_or(8)
        .clamp(8, 28);
    let action_w = (chunks[1].width as usize).saturating_sub(keys_w + 3).max(8);

    // Header row with sort indicators; clicking a header sorts that column.
    let marker = |col: Column| match h.sort {
        Some((c, true)) if c == col => " ↑",
        Some((c, false)) if c == col => " ↓",
        _ => "",
    };
    let header = Line::from(vec![
        Span::styled(
            format!(
                " {:<action_w$}",
                format!("{}{}", t!("ui.help_col_action"), marker(Column::Action))
            ),
            theme::title(true),
        ),
        Span::styled(
            format!(
                "  {:<keys_w$}",
                format!("{}{}", t!("ui.help_col_keys"), marker(Column::Keys))
            ),
            theme::title(true),
        ),
    ]);
    frame.render_widget(Paragraph::new(header), chunks[1]);
    let header_w = u16::try_from(action_w + 1).unwrap_or(u16::MAX);
    app.layout.help_headers = [
        Rect {
            width: header_w.min(chunks[1].width),
            ..chunks[1]
        },
        Rect {
            x: chunks[1].x + header_w.min(chunks[1].width),
            width: chunks[1].width.saturating_sub(header_w),
            ..chunks[1]
        },
    ];

    let view_h = chunks[2].height as usize;
    if let Some(h) = app.help.as_mut() {
        h.clamp_scroll(view_h);
    }
    let h = app.help.as_ref().unwrap();
    let filtered = h.matches();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for &idx in filtered.iter().skip(h.scroll).take(view_h) {
        let s = &h.rows[idx];
        lines.push(Line::from(vec![
            Span::raw(format!(" {:<action_w$}", trunc(&s.action, action_w))),
            Span::styled(format!("  {}", s.keys), theme::title(true)),
        ]));
    }
    let body = if lines.is_empty() {
        vec![Line::from(Span::styled(
            t!("ui.no_matches").to_string(),
            theme::dim(),
        ))]
    } else {
        lines
    };
    frame.render_widget(Paragraph::new(body), chunks[2]);
    app.layout.help_body = chunks[2];

    let hint = Line::from(Span::styled(t!("ui.help_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[3]);
}

/// Vix → Keybindings…: the editable sibling of [`draw_help`] — same
/// filter/sort/scroll table shape, plus a highlighted [`Panel::selected`]
/// row (rebind/reset target) and a `[user]`/`[script: name]` tag on any
/// row whose source isn't the built-in keymap.
pub(super) fn draw_keybinding_editor(app: &mut App, frame: &mut Frame, area: Rect) {
    use vix_keybinding_editor_panel::Column;
    let Some(p) = app.keybinding_editor.as_ref() else {
        return;
    };
    let rect = centered(area, 60, 70);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} ", t!("ui.keybinding_editor_title")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);
    let search = Line::from(vec![
        Span::styled(format!("{} ", icon::SEARCH), theme::title(true)),
        Span::raw(p.query.clone()),
        Span::styled("\u{2588}", theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(search), chunks[0]);

    let keys_w = p
        .rows
        .iter()
        .map(|r| r.key_display.chars().count())
        .max()
        .unwrap_or(8)
        .clamp(8, 28);
    let action_w = (chunks[1].width as usize).saturating_sub(keys_w + 3).max(8);

    let marker = |col: Column| match p.sort {
        Some((c, true)) if c == col => " ↑",
        Some((c, false)) if c == col => " ↓",
        _ => "",
    };
    let header = Line::from(vec![
        Span::styled(
            format!(
                " {:<action_w$}",
                format!("{}{}", t!("ui.help_col_action"), marker(Column::Action))
            ),
            theme::title(true),
        ),
        Span::styled(
            format!(
                "  {:<keys_w$}",
                format!("{}{}", t!("ui.help_col_keys"), marker(Column::Keys))
            ),
            theme::title(true),
        ),
    ]);
    frame.render_widget(Paragraph::new(header), chunks[1]);
    let header_w = u16::try_from(action_w + 1).unwrap_or(u16::MAX);
    app.layout.keybinding_editor_headers = [
        Rect {
            width: header_w.min(chunks[1].width),
            ..chunks[1]
        },
        Rect {
            x: chunks[1].x + header_w.min(chunks[1].width),
            width: chunks[1].width.saturating_sub(header_w),
            ..chunks[1]
        },
    ];

    let view_h = chunks[2].height as usize;
    if let Some(p) = app.keybinding_editor.as_mut() {
        p.clamp_scroll(view_h);
    }
    let p = app.keybinding_editor.as_ref().unwrap();
    let filtered = p.matches();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for (row_pos, &idx) in filtered.iter().enumerate().skip(p.scroll).take(view_h) {
        lines.push(keybinding_editor_row_line(
            &p.rows[idx],
            row_pos == p.selected,
            action_w,
        ));
    }
    let body = if lines.is_empty() {
        vec![Line::from(Span::styled(
            t!("ui.no_matches").to_string(),
            theme::dim(),
        ))]
    } else {
        lines
    };
    frame.render_widget(Paragraph::new(body), chunks[2]);
    app.layout.keybinding_editor_body = chunks[2];

    let hint = Line::from(Span::styled(
        t!("ui.keybinding_editor_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[3]);
}

/// One row of [`draw_keybinding_editor`]'s table: the action title tagged
/// with its source (`[user]`/`[script: name]`, blank for a built-in), then
/// the key combo — reversed video when `selected`. Split out to keep
/// `draw_keybinding_editor` under clippy's line-count lint.
fn keybinding_editor_row_line(
    r: &vix_keybinding_editor_panel::Row,
    selected: bool,
    action_w: usize,
) -> Line<'static> {
    use vix_keybinding_editor_panel::Source;
    let tagged = match &r.source {
        Source::BuiltIn => r.action_title.clone(),
        Source::User => format!("{} {}", r.action_title, t!("ui.keybinding_source_user")),
        Source::Script(stem) => format!(
            "{} {}",
            r.action_title,
            t!("ui.keybinding_source_script", stem = stem)
        ),
    };
    let style = if selected {
        theme::selected()
    } else {
        theme::title(true)
    };
    Line::from(vec![
        Span::styled(format!(" {:<action_w$}", trunc(&tagged, action_w)), style),
        Span::styled(format!("  {}", r.key_display), style),
    ])
}
