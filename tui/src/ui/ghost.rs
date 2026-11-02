//! Inline ghost text (T618): the suggestion drawn dimmed after the cursor.

use ratatui::prelude::*;
use unicode_width::UnicodeWidthChar as _;

use crate::app::App;

/// Draw the active buffer's ghost-text suggestion after its cursor, inside
/// `area` (the rectangle the editor widget was just rendered into). Draws the
/// suggestion's first line, ending in `…` when more lines follow, cut at the
/// area's right edge. Draws nothing without a current suggestion, when the
/// cursor is out of view, or in a soft-wrapped view.
pub(super) fn draw_ghost(app: &App, frame: &mut Frame, area: Rect) {
    let Some((line, more)) = app.ghost_overlay() else {
        return;
    };
    let Some(tab) = app.editor.active_tab() else {
        return;
    };
    if tab.editor.soft_wrap_enabled() {
        return;
    }
    let Some((x, y)) = tab.editor.get_visible_cursor(&area) else {
        return;
    };
    let style = Style::default().add_modifier(Modifier::DIM | Modifier::ITALIC);
    let text = if more {
        format!("{line}\u{2026}")
    } else {
        line.to_string()
    };
    let buf = frame.buffer_mut();
    let mut col = x;
    for ch in text.chars() {
        let width = u16::try_from(ch.width().unwrap_or(0)).unwrap_or(0);
        if width == 0 {
            continue;
        }
        if col + width > area.right() {
            break;
        }
        let ch = if ch == '\t' { ' ' } else { ch };
        let mut utf8 = [0u8; 4];
        buf[(col, y)]
            .set_symbol(ch.encode_utf8(&mut utf8))
            .set_style(style);
        col += width;
    }
}
