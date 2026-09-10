//! Rendering for the small glyph/color/type picker panels: the Nerd
//! Font character picker, ASCII-art character picker, QR code generator,
//! X11 color picker, and media type catalog -- the drawing side of
//! `vix_app::picker_panels` (T141).
//!
//! Moved out of `ui.rs` verbatim (T142, slice 2). Two non-contiguous
//! source ranges bundled into one module for symmetry with the app-side
//! `picker_panels.rs` state module.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::{NERD_CELL_W, draw_scrollbar, trunc};
use crate::app::App;
use crate::theme::{self, icon};

pub(super) fn draw_nerd_palette(app: &mut App, frame: &mut Frame, area: Rect) {
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
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
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

    let hint = Line::from(Span::styled(
        t!("ui.nerd_palette_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[2]);

    // Record just the glyph rows for mouse hit-testing.
    app.layout.nerd_palette = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: grid_w.min(chunks[0].width),
        height: grid_rows.min(chunks[0].height),
    };
}

pub(super) fn draw_ascii_panel(app: &mut App, frame: &mut Frame, area: Rect) {
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
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let header = Line::from(Span::styled(
        t!("ui.ascii_header").to_string(),
        theme::dim(),
    ));
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
        let text = format!(
            "  {:>3}  {:>2}   {}",
            ascii::dec(code),
            ascii::hex(code),
            ascii::label(code)
        );
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
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[1].height),
    };
}

// Render the QR code overlay: the Unicode QR art, forced to black-on-white so it
// scans regardless of the active theme, centered with a hint line.
pub(super) fn draw_qrcode(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(art) = app.qrcode.as_ref() else {
        return;
    };
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
        Paragraph::new(Line::from(Span::styled(
            t!("ui.qrcode_hint").to_string(),
            theme::dim(),
        ))),
        chunks[1],
    );
}

pub(super) fn draw_x11_panel(app: &mut App, frame: &mut Frame, area: Rect) {
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
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
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
        Rect {
            width: chunks[1].width - 1,
            ..chunks[1]
        }
    } else {
        chunks[1]
    };
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect {
            x: chunks[1].x + chunks[1].width - 1,
            ..chunks[1]
        };
        draw_scrollbar(frame, sb_area, p.selected, total.saturating_sub(1));
    }

    let hint = Line::from(Span::styled(t!("ui.x11_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[2]);

    app.layout.x11_panel = Rect {
        x: chunks[1].x,
        y: chunks[1].y,
        width: row_area.width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[1].height),
    };
}

/// Draw the theme editor overlay (T202): one row per color slot, a swatch
/// plus its `#RRGGBB` value (or a placeholder for an unset slot, which falls
/// back to the primary editor color like everywhere else the theme model
/// reads a region color). Mirrors `draw_x11_panel`'s layout.
pub(super) fn draw_theme_editor(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(editor) = app.theme_editor.as_ref() else {
        return;
    };
    let total = editor.len();
    let width = 40u16.min(area.width);
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
        .title(format!(
            " {} {} ",
            icon::PALETTE,
            t!("ui.theme_editor_title")
        ));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view_h = chunks[0].height as usize;
    if let Some(p) = app.theme_editor.as_mut() {
        p.ensure_visible(view_h);
    }
    let editor = app.theme_editor.as_ref().unwrap();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for (i, slot) in vix_theme_editor_panel::Slot::ALL
        .iter()
        .enumerate()
        .skip(editor.scroll)
        .take(view_h)
    {
        let rgb = slot.get(&editor.theme);
        let (swatch, hex) = match rgb {
            Some([r, g, b]) => (
                Span::styled("██", Style::default().fg(Color::Rgb(r, g, b))),
                format!("#{r:02X}{g:02X}{b:02X}"),
            ),
            None => (Span::raw("··"), "—".to_string()),
        };
        let text = format!(" {:7} {}", hex, t!(slot.label_key()));
        let label = if i == editor.selected {
            Span::styled(text, theme::selected())
        } else {
            Span::raw(text)
        };
        lines.push(Line::from(vec![Span::raw(" "), swatch, label]));
    }
    let show_bar = total > view_h && chunks[0].width > 1;
    let row_area = if show_bar {
        Rect {
            width: chunks[0].width - 1,
            ..chunks[0]
        }
    } else {
        chunks[0]
    };
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect {
            x: chunks[0].x + chunks[0].width - 1,
            ..chunks[0]
        };
        draw_scrollbar(frame, sb_area, editor.selected, total.saturating_sub(1));
    }

    let hint = Line::from(Span::styled(
        t!("ui.theme_editor_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.theme_editor = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: row_area.width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[0].height),
    };
}

pub(super) fn draw_media_type_panel(app: &mut App, frame: &mut Frame, area: Rect) {
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
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // Filter line: a typed query, or a dim prompt when empty.
    let filter = if p.query.is_empty() {
        Line::from(Span::styled(
            t!("ui.media_types_filter").to_string(),
            theme::dim(),
        ))
    } else {
        Line::from(vec![
            Span::styled("/ ", theme::dim()),
            Span::raw(p.query.clone()),
        ])
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
        let text = format!(
            " {:30} {:8} {tag}  {}",
            trunc(m.media_type, 30),
            trunc(m.extension, 8),
            m.description
        );
        let line = if row == p.selected {
            Line::from(Span::styled(text, theme::selected()))
        } else {
            Line::from(Span::raw(text))
        };
        lines.push(line);
    }
    let show_bar = total > view_h && chunks[1].width > 1;
    let row_area = if show_bar {
        Rect {
            width: chunks[1].width - 1,
            ..chunks[1]
        }
    } else {
        chunks[1]
    };
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect {
            x: chunks[1].x + chunks[1].width - 1,
            ..chunks[1]
        };
        draw_scrollbar(frame, sb_area, p.selected, total.saturating_sub(1));
    }

    let hint = Line::from(Span::styled(
        t!("ui.media_types_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[2]);

    app.layout.media_type_panel = Rect {
        x: chunks[1].x,
        y: chunks[1].y,
        width: row_area.width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[1].height),
    };
}
