//! The code-overview minimap: a dim bar-chart summary of the active buffer.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::theme;

/// Draw the code-overview minimap for the active buffer in `area`. Each row maps
/// to a band of source lines, drawn as a dim bar whose length tracks the band's
/// longest (trimmed) line — a rough "shape of the code". Rows overlapping the
/// editor's current viewport (`editor_height` rows from the scroll offset) get a
/// highlighted background so the visible region stands out.
pub(super) fn draw_minimap(app: &mut App, frame: &mut Frame, area: Rect, editor_height: usize) {
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
