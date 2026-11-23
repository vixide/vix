//! The status bar: mode indicator, file path/dirty flag, editor info
//! (language/line-ending/encoding/selection), git branch, and cursor
//! position.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use crate::geo::Geo;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::theme::{self, icon};

/// Draw the status bar: file path with dirty flag, cursor position and the
/// other indicators.
pub(super) fn draw_status_bar(app: &mut App, frame: &mut Frame, area: Rect) {
    let (line, col) = app.editor.cursor_1based();
    let path = app
        .editor
        .active_tab()
        .map_or_default(crate::editor::Tab::display_path);
    let dirty = app.editor.active_tab().is_some_and(|t| t.dirty);
    let dirty_flag = if dirty {
        format!(" {}", icon::FILE_DIRTY)
    } else {
        String::new()
    };

    let mode = app.mode_indicator().map_or_default(|m| format!("{m}   "));
    let info = editor_info(app);

    let git = crate::status_bar_panel::git_segment(
        app.git_branch.as_deref(),
        icon::BRANCH,
        app.git_dirty(),
    );
    // Language-server progress (T732) and Cowork participants, then git,
    // then IM's unread count and mention marker (T754).
    let before_git = format!(
        "{}{}{}{}",
        app.remote_segment(),
        app.lsp_progress_segment(),
        app.cowork_segment(),
        app.jupyter_segment()
    );
    let segments = format!(
        "{before_git}{git}{}{}{info}",
        app.project_env_segment(),
        app.im_status_segment()
    );
    let (left, right) = fit_segments(
        app,
        (&mode, &path, &dirty_flag),
        (&segments, &format!("{before_git}{git}")),
        (line, col),
        usize::from(area.width),
    );

    let bg = theme::region_base(theme::Region::StatusBar);
    // A top border separates the status bar from the body above it.
    let block = Block::default()
        .style(bg)
        .borders(Borders::TOP)
        .border_style(theme::region_title(theme::Region::StatusBar, false));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Mirrored layouts swap the two segments: the cursor/git segment sits at
    // the left edge and the status text hugs the right edge (T564).
    let mirrored = super::layout_mirrored(app);
    let right_w =
        Constraint::Length((u16::try_from(right.chars().count()).unwrap_or(u16::MAX)).plus(1));
    let right_col = if mirrored {
        let c = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([right_w, Constraint::Min(1)])
            .split(inner);
        frame.render_widget(
            Paragraph::new(left).style(bg).alignment(Alignment::Right),
            c[1],
        );
        frame.render_widget(
            Paragraph::new(right).style(bg).alignment(Alignment::Left),
            c[0],
        );
        c[0]
    } else {
        let c = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), right_w])
            .split(inner);
        frame.render_widget(
            Paragraph::new(left).style(bg).alignment(Alignment::Left),
            c[0],
        );
        frame.render_widget(
            Paragraph::new(right).style(bg).alignment(Alignment::Right),
            c[1],
        );
        c[1]
    };

    // Record the git/branch segment's rectangle (the leftmost part of the
    // right-aligned right segment, after its 1-cell padding) so a click on the
    // branch indicator opens the Git panel.
    // The progress/Cowork text before it shifts it right (T732).
    let git_w = u16::try_from(git.chars().count()).unwrap_or(u16::MAX);
    let lead = u16::try_from(before_git.chars().count()).unwrap_or(u16::MAX);
    app.layout.git_status_bar = if git_w > 0 {
        Rect {
            x: if mirrored {
                right_col.x.plus(lead)
            } else {
                right_col.x.plus(1).plus(lead)
            },
            y: right_col.y,
            width: git_w.min(right_col.width),
            height: 1,
        }
    } else {
        Rect::default()
    };
}

/// The left and right status segments fitted into `width` columns. When both
/// do not fit, the right one loses the segments after git (`short_segments`
/// instead of `segments`: project environment, IM, editor info), which keeps
/// the git segment's click rectangle where it was; the left one then gets
/// what is left through `fit_left_segment`, so the status message stays
/// readable.
fn fit_segments(
    app: &App,
    (mode, path, dirty_flag): (&str, &str, &str),
    (segments, short_segments): (&str, &str),
    (line, col): (usize, usize),
    width: usize,
) -> (String, String) {
    use crate::status_bar_panel::{fit_left_segment, left_segment, right_segment};
    let mut right = right_segment(segments, line, col, icon::CALENDAR);
    let wanted = left_segment(mode, path, dirty_flag, &app.status);
    if wanted.chars().count().plus(right.chars().count()).plus(1) > width {
        right = right_segment(short_segments, line, col, icon::CALENDAR);
    }
    let left_room = width.minus(right.chars().count().plus(1));
    let left = fit_left_segment(mode, path, dirty_flag, &app.status, left_room);
    (left, right)
}

/// Editor info (language, line ending, encoding, selection) for a text tab.
fn editor_info(app: &App) -> String {
    app.editor
        .active_tab()
        .filter(|t| !t.is_image())
        .map_or_default(|t| {
            let lang = match t.editor.language() {
                "unknown" | "" => "text",
                other => other,
            };
            let sel = t.editor.selection_span().map(|(s, e)| {
                let code = t.editor.code_ref();
                (
                    e.minus(s),
                    code.char_to_line(e).minus(code.char_to_line(s)).plus(1),
                )
            });
            crate::status_bar_panel::info_segment(
                Some(lang),
                t.editor.line_ending(),
                &t.encoding.to_string(),
                sel,
            )
        })
}
