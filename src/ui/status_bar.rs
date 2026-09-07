//! The status bar: mode indicator, file path/dirty flag, editor info
//! (language/line-ending/encoding/selection), git branch, and cursor
//! position.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::theme::{self, icon};

pub(super) fn draw_status_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    let (line, col) = app.editor.cursor_1based();
    let path = app
        .editor
        .active_tab()
        .map(crate::editor::Tab::display_path)
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
