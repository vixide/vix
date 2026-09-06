//! The structured-editing overlay family: the CSV/TSV table editor, the Org
//! Column View, the SQL snippet library, the prose outline editor, the
//! structured-value (JSON/YAML) editor, the byte (hex) editor, the HTML
//! character picker, and the document-outline panel.

#![warn(clippy::pedantic)]

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::{draw_scrollbar, trunc};
use crate::app::App;
use crate::theme::{self, icon};

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
        let used: usize = (first..=sel)
            .map(|c| widths.get(c).copied().unwrap_or(3) + 1)
            .sum();
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
fn table_row_line(
    grid: &crate::edit_table::Grid,
    r: usize,
    cols: &[usize],
    widths: &[usize],
) -> Line<'static> {
    let mut spans = Vec::with_capacity(cols.len() * 2);
    let editing = grid.is_editing() && r == grid.row();
    for &c in cols {
        let w = widths.get(c).copied().unwrap_or(3);
        let selected = r == grid.row() && c == grid.col();
        let raw = if editing && selected {
            grid.edit_buffer()
        } else {
            grid.cell(r, c)
        };
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
    Line::from(vec![
        Span::styled(pos, theme::dim()),
        Span::styled(info, theme::dim()),
    ])
}

// Render the table editor overlay: pinned header, scrolling body with the
// selected cell highlighted, and a status/hint line.
pub(super) fn draw_edit_table(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.edit_table.is_none() {
        return;
    }
    frame.render_widget(Clear, area);
    let dirty = if app
        .edit_table
        .as_ref()
        .is_some_and(crate::edit_table::Grid::is_dirty)
    {
        " *"
    } else {
        ""
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(
            " {} {}{} ",
            icon::TABLE,
            t!("ui.edit_table"),
            dirty
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
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
    frame.render_widget(
        Paragraph::new(table_row_line(grid, 0, &cols, &widths)),
        chunks[0],
    );

    let start = grid.row_scroll();
    let mut lines = Vec::with_capacity(body_h);
    for r in start..(start + body_h).min(grid.row_count()) {
        lines.push(table_row_line(grid, r, &cols, &widths));
    }
    frame.render_widget(Paragraph::new(lines), chunks[1]);
    frame.render_widget(Paragraph::new(table_status_line(grid)), chunks[2]);

    app.layout.edit_table = chunks[1];
}

// Per-column display width for the visible span, from `ColumnView::column_width`.
fn column_view_widths(view: &crate::column_view::ColumnView) -> Vec<usize> {
    (0..view.col_count())
        .map(|c| view.column_width(c).min(40))
        .collect()
}

// The column indices that fit in `avail` columns, keeping the selected column
// in view (no persistent scroll memory — recomputed fresh each frame from the
// current selection, since `ColumnView` does not track a horizontal offset).
fn column_view_visible_cols(
    view: &crate::column_view::ColumnView,
    widths: &[usize],
    avail: usize,
) -> Vec<usize> {
    let sel = view.col();
    let mut first = 0usize;
    loop {
        let used: usize = (first..=sel)
            .map(|c| widths.get(c).copied().unwrap_or(3) + 1)
            .sum();
        if used <= avail || first >= sel {
            break;
        }
        first += 1;
    }
    let mut out = Vec::new();
    let mut used = 0usize;
    for c in first..view.col_count() {
        let need = widths.get(c).copied().unwrap_or(3) + 1;
        if used + need > avail && !out.is_empty() {
            break;
        }
        used += need;
        out.push(c);
    }
    out
}

// Build one rendered Column View line: the header row (`r`/`is_header` unused
// for headers) or data row `r`, over the visible `cols`.
fn column_view_row_line(
    view: &crate::column_view::ColumnView,
    is_header: bool,
    r: usize,
    cols: &[usize],
    widths: &[usize],
) -> Line<'static> {
    let mut spans = Vec::with_capacity(cols.len() * 2);
    for &c in cols {
        let w = widths.get(c).copied().unwrap_or(3);
        let selected = !is_header && r == view.row() && c == view.col();
        let text = if is_header {
            let def = &view.columns()[c];
            def.title.clone().unwrap_or_else(|| def.property.clone())
        } else if selected && view.is_editing() {
            view.edit_buffer().to_string()
        } else {
            view.cell(r, c).to_string()
        };
        let style = if selected {
            theme::selected()
        } else if is_header {
            theme::title(true)
        } else {
            theme::base()
        };
        spans.push(Span::styled(fit(&text, w), style));
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

// The bottom status/hint line: field position, plus the allowed-value edit
// notice, the raw-value edit notice, or the key hint.
fn column_view_status_line(view: &crate::column_view::ColumnView) -> Line<'static> {
    let info = if view.is_editing() {
        t!("ui.column_view_editing").to_string()
    } else if view.is_editing_allowed() {
        format!(
            "{}: {}",
            t!("ui.column_view_editing_allowed"),
            view.edit_buffer()
        )
    } else {
        t!("ui.column_view_hint").to_string()
    };
    let pos = format!(
        " r{}/{} c{}/{}  ",
        view.row() + 1,
        view.row_count(),
        view.col() + 1,
        view.col_count(),
    );
    Line::from(vec![
        Span::styled(pos, theme::dim()),
        Span::styled(info, theme::dim()),
    ])
}

// Render the interactive Column View overlay: a pinned header row, a
// scrolling body with the selected field highlighted, and a status/hint line.
pub(super) fn draw_column_view(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.column_view.is_none() {
        return;
    }
    frame.render_widget(Clear, area);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::TABLE, t!("ui.column_view")));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let view = app.column_view.as_ref().unwrap();
    let widths = column_view_widths(view);
    let avail = usize::from(chunks[1].width);
    let cols = column_view_visible_cols(view, &widths, avail);
    let body_h = usize::from(chunks[1].height);
    if let Some(v) = app.column_view.as_mut() {
        v.ensure_row_visible(body_h);
    }

    let view = app.column_view.as_ref().unwrap();
    frame.render_widget(
        Paragraph::new(column_view_row_line(view, true, 0, &cols, &widths)),
        chunks[0],
    );

    let start = view.row_scroll();
    let mut lines = Vec::with_capacity(body_h);
    for r in start..(start + body_h).min(view.row_count()) {
        lines.push(column_view_row_line(view, false, r, &cols, &widths));
    }
    frame.render_widget(Paragraph::new(lines), chunks[1]);
    frame.render_widget(Paragraph::new(column_view_status_line(view)), chunks[2]);

    app.layout.column_view = chunks[1];
}

// One rendered outline line: indentation, a fold marker (▾/▸/·), and the text.
fn outline_line(tree: &crate::edit_outline::Tree, i: usize, selected: bool) -> Line<'static> {
    let marker = if tree.has_children(i) {
        if tree.is_collapsed(i) { "▸ " } else { "▾ " }
    } else {
        "· "
    };
    let text = format!("{}{marker}{}", "  ".repeat(tree.level(i)), tree.text(i));
    let style = if selected {
        theme::selected()
    } else {
        theme::base()
    };
    Line::from(Span::styled(text, style))
}

// Render the outline editor overlay: a scrolling tree of items with the selected
// item highlighted, and a status/hint line.
pub(super) fn draw_edit_sql(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.edit_sql.is_none() {
        return;
    }
    frame.render_widget(Clear, area);
    let dirty = if app
        .edit_sql
        .as_ref()
        .is_some_and(crate::edit_sql::Editor::is_dirty)
    {
        " *"
    } else {
        ""
    };
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
            Line::from(vec![
                Span::styled(format!(" {kind:8} "), theme::dim()),
                Span::raw(trunc(&preview, width.saturating_sub(11))),
            ])
        };
        lines.push(line);
    }
    if total == 0 {
        lines.push(Line::from(Span::styled(
            t!("ui.edit_sql_empty").to_string(),
            theme::dim(),
        )));
    }
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            t!("ui.edit_sql_hint").to_string(),
            theme::dim(),
        ))),
        chunks[1],
    );

    app.layout.edit_sql = chunks[0];
}

/// Render the prose outline editor overlay.
pub(super) fn draw_edit_outline(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.edit_outline.is_none() {
        return;
    }
    frame.render_widget(Clear, area);
    let dirty = if app
        .edit_outline
        .as_ref()
        .is_some_and(crate::edit_outline::Tree::is_dirty)
    {
        " *"
    } else {
        ""
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(
            " {} {}{} ",
            icon::LIST,
            t!("ui.edit_outline"),
            dirty
        ));
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
        Paragraph::new(Line::from(Span::styled(
            t!("ui.edit_outline_hint").to_string(),
            theme::dim(),
        ))),
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
    let value = if editing && selected {
        tree.edit_buffer()
    } else {
        tree.value(i)
    };
    let head = if label.is_empty() {
        String::new()
    } else if tree.is_container(i) {
        format!("{label} ")
    } else {
        format!("{label}: ")
    };
    let text = format!("{}{marker}{head}{value}", "  ".repeat(tree.depth(i)));
    let style = if selected {
        theme::selected()
    } else {
        theme::base()
    };
    Line::from(Span::styled(text, style))
}

/// Render the structured-value editor overlay (Edit JSON / Edit YAML).
pub(super) fn draw_edit_value(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(format) = app.edit_value.as_ref().map(crate::edit_value::Tree::format) else {
        return;
    };
    frame.render_widget(Clear, area);
    let dirty = if app
        .edit_value
        .as_ref()
        .is_some_and(crate::edit_value::Tree::is_dirty)
    {
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
        Paragraph::new(Line::from(Span::styled(
            t!("ui.edit_value_hint").to_string(),
            theme::dim(),
        ))),
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
            let style = if idx == hex.cursor() {
                theme::selected()
            } else {
                theme::base()
            };
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
            let ch = if (0x20..0x7f).contains(&b) {
                char::from(b)
            } else {
                '.'
            };
            let style = if idx == hex.cursor() {
                theme::selected()
            } else {
                theme::dim()
            };
            spans.push(Span::styled(ch.to_string(), style));
        }
    }
    Line::from(spans)
}

/// Render the byte (hex) editor overlay.
pub(super) fn draw_edit_bytes(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.edit_bytes.is_none() {
        return;
    }
    frame.render_widget(Clear, area);
    let dirty = if app
        .edit_bytes
        .as_ref()
        .is_some_and(crate::edit_bytes::Hex::is_dirty)
    {
        " *"
    } else {
        ""
    };
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(
            " {} {}{} ",
            icon::TABLE,
            t!("ui.edit_bytes"),
            dirty
        ));
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
        Paragraph::new(Line::from(Span::styled(
            t!("ui.edit_bytes_hint").to_string(),
            theme::dim(),
        ))),
        chunks[1],
    );
    app.layout.edit_bytes = chunks[0];
}

/// Render the HTML character-entity picker overlay.
pub(super) fn draw_html_panel(app: &mut App, frame: &mut Frame, area: Rect) {
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
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let header = Line::from(Span::styled(t!("ui.html_header").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(header), chunks[0]);

    let view_h = chunks[1].height as usize;
    if let Some(p) = app.html_panel.as_mut() {
        p.ensure_visible(view_h);
    }
    let p = app.html_panel.as_ref().unwrap();
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for (i, e) in entities[p.scroll..(p.scroll + view_h).min(total)]
        .iter()
        .enumerate()
    {
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

    let hint = Line::from(Span::styled(t!("ui.html_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunks[2]);

    app.layout.html_panel = Rect {
        x: chunks[1].x,
        y: chunks[1].y,
        width: row_area.width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[1].height),
    };
}

/// Render the document-outline panel (a table-of-contents sidebar over the
/// current buffer's headings/symbols — distinct from the prose outline
/// editor's own `edit_outline` overlay above).
pub(super) fn draw_outline(app: &mut App, frame: &mut Frame, area: Rect) {
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

    let hint = Line::from(Span::styled(
        t!("ui.outline_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunks[1]);

    app.layout.outline = Rect {
        x: chunks[0].x,
        y: chunks[0].y,
        width: chunks[0].width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunks[0].height),
    };
}
