//! The structured-editing overlay family: the CSV/TSV table editor, the Org
//! Column View, the SQL snippet library, the prose outline editor, the
//! structured-value (JSON/YAML) editor, the byte (hex) editor, the HTML
//! character picker, and the document-outline panel.

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

use crate::geo::{Geo, chunk};
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::draw_scrollbar;
#[cfg(feature = "sql")]
use super::trunc;
use crate::app::App;
use crate::app::edit_surface::EditSlot;
use crate::theme::{self, icon};

/// Pad or truncate `s` to exactly `w` display columns (by character count).
fn fit(s: &str, w: usize) -> String {
    let mut out: String = s.chars().take(w).collect();
    let len = out.chars().count();
    if len < w {
        out.push_str(&" ".repeat(w.minus(len)));
    }
    out
}

/// Per-column display width: the widest cell (incl. header), clamped to [3, 24].
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

/// First column to show so the selected column is visible within `avail` columns.
fn first_visible_col(grid: &crate::edit_table::Grid, widths: &[usize], avail: usize) -> usize {
    let sel = grid.col();
    let mut first = grid.col_scroll().min(sel);
    loop {
        let used: usize = (first..=sel)
            .map(|c| widths.get(c).copied().unwrap_or(3).plus(1))
            .sum();
        if used <= avail || first >= sel {
            break;
        }
        first = first.plus(1);
    }
    first
}

/// The column indices that fit in `avail` columns starting from the scroll offset.
fn visible_cols(grid: &crate::edit_table::Grid, widths: &[usize], avail: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut used = 0usize;
    for c in grid.col_scroll()..grid.col_count() {
        let need = widths.get(c).copied().unwrap_or(3).plus(1);
        if used.plus(need) > avail && !out.is_empty() {
            break;
        }
        used = used.plus(need);
        out.push(c);
    }
    out
}

/// Build one rendered grid line for row `r` over the visible `cols`.
fn table_row_line(
    grid: &crate::edit_table::Grid,
    r: usize,
    cols: &[usize],
    widths: &[usize],
) -> Line<'static> {
    let mut spans = Vec::with_capacity(cols.len().times(2));
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

/// The bottom status/hint line: position, plus the find query, edit notice, or hint.
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
        grid.row().plus(1),
        grid.row_count(),
        grid.col().plus(1),
        grid.col_count(),
    );
    Line::from(vec![
        Span::styled(pos, theme::dim()),
        Span::styled(info, theme::dim()),
    ])
}

/// Render the table editor overlay: pinned header, scrolling body with the
/// selected cell highlighted, and a status/hint line.
pub(super) fn draw_edit_table(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.edit.edit_table().is_none() {
        return;
    }
    frame.render_widget(Clear, area);
    let dirty = if app
        .edit
        .edit_table()
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

    let Some(grid) = app.edit.edit_table() else {
        return;
    };
    let widths = column_widths(grid);
    let avail = usize::from(chunk(&chunks, 1).width);
    let first = first_visible_col(grid, &widths, avail);
    let body_h = usize::from(chunk(&chunks, 1).height);
    if let Some(g) = app.edit.edit_table_mut() {
        g.set_col_scroll(first);
        g.ensure_row_visible(body_h);
    }

    let Some(grid) = app.edit.edit_table() else {
        return;
    };
    let cols = visible_cols(grid, &widths, avail);
    frame.render_widget(
        Paragraph::new(table_row_line(grid, 0, &cols, &widths)),
        chunk(&chunks, 0),
    );

    let start = grid.row_scroll();
    let mut lines = Vec::with_capacity(body_h);
    for r in start..start.plus(body_h).min(grid.row_count()) {
        lines.push(table_row_line(grid, r, &cols, &widths));
    }
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 1));
    frame.render_widget(Paragraph::new(table_status_line(grid)), chunk(&chunks, 2));

    app.layout.edit_table = chunk(&chunks, 1);
}

/// Per-column display width for the visible span, from `ColumnView::column_width`.
fn column_view_widths(view: &crate::column_view::ColumnView) -> Vec<usize> {
    (0..view.col_count())
        .map(|c| view.column_width(c).min(40))
        .collect()
}

/// The column indices that fit in `avail` columns, keeping the selected column
/// in view (no persistent scroll memory — recomputed fresh each frame from the
/// current selection, since `ColumnView` does not track a horizontal offset).
fn column_view_visible_cols(
    view: &crate::column_view::ColumnView,
    widths: &[usize],
    avail: usize,
) -> Vec<usize> {
    let sel = view.col();
    let mut first = 0usize;
    loop {
        let used: usize = (first..=sel)
            .map(|c| widths.get(c).copied().unwrap_or(3).plus(1))
            .sum();
        if used <= avail || first >= sel {
            break;
        }
        first = first.plus(1);
    }
    let mut out = Vec::new();
    let mut used = 0usize;
    for c in first..view.col_count() {
        let need = widths.get(c).copied().unwrap_or(3).plus(1);
        if used.plus(need) > avail && !out.is_empty() {
            break;
        }
        used = used.plus(need);
        out.push(c);
    }
    out
}

/// Build one rendered Column View line: the header row (`r`/`is_header` unused
/// for headers) or data row `r`, over the visible `cols`.
fn column_view_row_line(
    view: &crate::column_view::ColumnView,
    is_header: bool,
    r: usize,
    cols: &[usize],
    widths: &[usize],
) -> Line<'static> {
    let mut spans = Vec::with_capacity(cols.len().times(2));
    for &c in cols {
        let w = widths.get(c).copied().unwrap_or(3);
        let selected = !is_header && r == view.row() && c == view.col();
        let text = if is_header {
            view.columns().get(c).map_or_else(String::new, |def| {
                def.title.clone().unwrap_or_else(|| def.property.clone())
            })
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

/// The bottom status/hint line: field position, plus the allowed-value edit
/// notice, the raw-value edit notice, or the key hint.
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
        view.row().plus(1),
        view.row_count(),
        view.col().plus(1),
        view.col_count(),
    );
    Line::from(vec![
        Span::styled(pos, theme::dim()),
        Span::styled(info, theme::dim()),
    ])
}

/// Render the interactive Column View overlay: a pinned header row, a
/// scrolling body with the selected field highlighted, and a status/hint line.
pub(super) fn draw_column_view(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.edit.column_view().is_none() {
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

    let Some(view) = app.edit.column_view() else {
        return;
    };
    let widths = column_view_widths(view);
    let avail = usize::from(chunk(&chunks, 1).width);
    let cols = column_view_visible_cols(view, &widths, avail);
    let body_h = usize::from(chunk(&chunks, 1).height);
    if let Some(v) = app.edit.column_view_mut() {
        v.ensure_row_visible(body_h);
    }

    let Some(view) = app.edit.column_view() else {
        return;
    };
    frame.render_widget(
        Paragraph::new(column_view_row_line(view, true, 0, &cols, &widths)),
        chunk(&chunks, 0),
    );

    let start = view.row_scroll();
    let mut lines = Vec::with_capacity(body_h);
    for r in start..start.plus(body_h).min(view.row_count()) {
        lines.push(column_view_row_line(view, false, r, &cols, &widths));
    }
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 1));
    frame.render_widget(
        Paragraph::new(column_view_status_line(view)),
        chunk(&chunks, 2),
    );

    app.layout.column_view = chunk(&chunks, 1);
}

/// One rendered outline line: indentation, a fold marker (▾/▸/·), and the text.
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

/// Render the SQL snippet-library editor overlay: one row per statement (its
/// kind, then a truncated preview) with the selected row highlighted, an
/// empty-state line when there are none, and a hint line. Records the body rect
/// in `app.layout.edit_sql` for mouse hit-testing.
#[cfg(feature = "sql")]
pub(super) fn draw_edit_sql(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.edit.edit_sql().is_none() {
        return;
    }
    frame.render_widget(Clear, area);
    let dirty = if app
        .edit
        .edit_sql()
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

    let body_h = usize::from(chunk(&chunks, 0).height);
    if let Some(e) = app.edit.edit_sql_mut() {
        e.ensure_visible(body_h);
    }
    let Some(editor) = app.edit.edit_sql() else {
        return;
    };
    let total = editor.len();
    let width = usize::from(chunk(&chunks, 0).width);
    let mut lines: Vec<Line> = Vec::with_capacity(body_h);
    for i in editor.scroll()..editor.scroll().plus(body_h).min(total) {
        let kind = editor.kind(i);
        let preview = editor.preview(i);
        let text = format!(" {kind:8} {}", trunc(&preview, width.minus(11)));
        let line = if i == editor.sel() {
            Line::from(Span::styled(text, theme::selected()))
        } else {
            Line::from(vec![
                Span::styled(format!(" {kind:8} "), theme::dim()),
                Span::raw(trunc(&preview, width.minus(11))),
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
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            t!("ui.edit_sql_hint").to_string(),
            theme::dim(),
        ))),
        chunk(&chunks, 1),
    );

    app.layout.edit_sql = chunk(&chunks, 0);
}

/// Render the prose outline editor overlay.
pub(super) fn draw_edit_outline(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.edit.edit_outline().is_none() {
        return;
    }
    frame.render_widget(Clear, area);
    let dirty = if app
        .edit
        .edit_outline()
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

    let body_h = usize::from(chunk(&chunks, 0).height);
    if let Some(t) = app.edit.edit_outline_mut() {
        t.ensure_visible(body_h);
    }
    let Some(tree) = app.edit.edit_outline() else {
        return;
    };
    let vis = tree.visible();
    let start = tree.scroll();
    let mut lines = Vec::with_capacity(body_h);
    for &i in vis.iter().skip(start).take(body_h) {
        lines.push(outline_line(tree, i, i == tree.sel()));
    }
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            t!("ui.edit_outline_hint").to_string(),
            theme::dim(),
        ))),
        chunk(&chunks, 1),
    );

    app.layout.edit_outline = chunk(&chunks, 0);
}

/// One rendered row of the structured-value (JSON/YAML) tree.
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
    let Some(format) = app.edit.edit_value().map(crate::edit_value::Tree::format) else {
        return;
    };
    frame.render_widget(Clear, area);
    let dirty = if app
        .edit
        .edit_value()
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

    let body_h = usize::from(chunk(&chunks, 0).height);
    if let Some(t) = app.edit.edit_value_mut() {
        t.ensure_visible(body_h);
    }
    let Some(tree) = app.edit.edit_value() else {
        return;
    };
    let editing = tree.is_editing();
    let start = tree.scroll();
    let mut lines = Vec::with_capacity(body_h);
    for i in start..start.plus(body_h).min(tree.row_count()) {
        lines.push(value_line(tree, i, editing));
    }
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            t!("ui.edit_value_hint").to_string(),
            theme::dim(),
        ))),
        chunk(&chunks, 1),
    );
    app.layout.edit_value = chunk(&chunks, 0);
}

/// One rendered hex-dump row: offset, hex byte pairs, and the ASCII gutter.
fn bytes_line(hex: &crate::edit_bytes::Hex, row: usize) -> Line<'static> {
    use crate::edit_bytes::COLS;
    let off = row.times(COLS);
    let mut spans = vec![Span::styled(format!("{off:08x}  "), theme::dim())];
    for col in 0..COLS {
        let idx = off.plus(col);
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
        let idx = off.plus(col);
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
    if app.edit.edit_bytes().is_none() {
        return;
    }
    frame.render_widget(Clear, area);
    let dirty = if app
        .edit
        .edit_bytes()
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

    let body_h = usize::from(chunk(&chunks, 0).height);
    if let Some(h) = app.edit.edit_bytes_mut() {
        h.ensure_visible(body_h);
    }
    let Some(hex) = app.edit.edit_bytes() else {
        return;
    };
    let start = hex.scroll();
    let mut lines = Vec::with_capacity(body_h);
    for row in start..start.plus(body_h).min(hex.rows()) {
        lines.push(bytes_line(hex, row));
    }
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            t!("ui.edit_bytes_hint").to_string(),
            theme::dim(),
        ))),
        chunk(&chunks, 1),
    );
    app.layout.edit_bytes = chunk(&chunks, 0);
}

/// Render the HTML character-entity picker overlay.
pub(super) fn draw_html_panel(app: &mut App, frame: &mut Frame, area: Rect) {
    let entities = crate::html_character_picker::entities();
    let total = entities.len();
    if app.html_panel.is_none() || total == 0 {
        return;
    }
    let width = 46u16.min(area.width);
    let max_rows = area.height.minus(4).max(1);
    let rows = u16::try_from(total).unwrap_or(u16::MAX).min(max_rows);
    let height = rows.plus(4).min(area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(3)),
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
    frame.render_widget(Paragraph::new(header), chunk(&chunks, 0));

    let view_h = usize::from(chunk(&chunks, 1).height);
    if let Some(p) = app.html_panel.as_mut() {
        p.ensure_visible(view_h);
    }
    let Some(p) = app.html_panel.as_ref() else {
        return;
    };
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for (i, e) in entities
        .get(p.scroll..p.scroll.plus(view_h).min(total))
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let text = format!("  {:2}  {:26}  {}", e.glyph, e.name, e.code);
        let idx = p.scroll.plus(i);
        if idx == p.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(Span::raw(text)));
        }
    }
    // Reserve a one-column gutter for the scrollbar when the table overflows.
    let show_bar = total > view_h && chunk(&chunks, 1).width > 1;
    let row_area = if show_bar {
        Rect {
            width: chunk(&chunks, 1).width.minus(1),
            ..chunk(&chunks, 1)
        }
    } else {
        chunk(&chunks, 1)
    };
    frame.render_widget(Paragraph::new(lines), row_area);
    if show_bar {
        let sb_area = Rect {
            x: chunk(&chunks, 1).x.plus(chunk(&chunks, 1).width).minus(1),
            ..chunk(&chunks, 1)
        };
        draw_scrollbar(frame, sb_area, p.selected, total.minus(1));
    }

    let hint = Line::from(Span::styled(t!("ui.html_hint").to_string(), theme::dim()));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 2));

    app.layout.html_panel = Rect {
        x: chunk(&chunks, 1).x,
        y: chunk(&chunks, 1).y,
        width: row_area.width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunk(&chunks, 1).height),
    };
}

/// Render the document-outline panel (a table-of-contents sidebar over the
/// current buffer's headings/symbols — distinct from the prose outline
/// editor's own `edit_outline` overlay above).
pub(super) fn draw_outline(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(n) = app.outline.as_ref().map(vix_outline_panel::Outline::len) else {
        return;
    };
    let width = 48u16.min(area.width);
    let max_rows = area.height.minus(3).max(1);
    let rows = u16::try_from(n).unwrap_or(u16::MAX).min(max_rows);
    let height = rows.plus(3).min(area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(4)),
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

    let view_h = usize::from(chunk(&chunks, 0).height);
    if let Some(o) = app.outline.as_mut() {
        o.ensure_visible(view_h);
    }
    let Some(o) = app.outline.as_ref() else {
        return;
    };
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in o.scroll..o.scroll.plus(view_h).min(o.len()) {
        let Some(e) = o.entries.get(idx) else {
            break;
        };
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
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));

    let hint = Line::from(Span::styled(
        t!("ui.outline_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));

    app.layout.outline = Rect {
        x: chunk(&chunks, 0).x,
        y: chunk(&chunks, 0).y,
        width: chunk(&chunks, 0).width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunk(&chunks, 0).height),
    };
}

/// The first line of `text` (with its ending stripped), truncated to `max`
/// characters — a one-line preview of a conflict side for
/// [`draw_conflict_list`]. Empty when `text` itself is empty (an empty
/// side is valid, e.g. a conflict where one branch deleted the lines).
fn preview_line(text: &str, max: usize) -> String {
    let first = text.lines().next().unwrap_or("");
    let truncated: String = first.chars().take(max).collect();
    if first.chars().count() > max {
        format!("{truncated}…")
    } else {
        truncated
    }
}

/// Render the Conflict overlay (T556, `git.conflict_list`): every merge
/// conflict in the active buffer, each row previewing both sides.
pub(super) fn draw_conflict_list(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(n) = app.conflict_list.as_ref().map(vix_conflict_tool::List::len) else {
        return;
    };
    let width = 64u16.min(area.width);
    let max_rows = area.height.minus(3).max(1);
    let rows = u16::try_from(n).unwrap_or(u16::MAX).min(max_rows);
    let height = rows.plus(3).min(area.height);
    let rect = Rect {
        x: area.x.plus(area.width.minus(width).quot(2)),
        y: area.y.plus(area.height.minus(height).quot(4)),
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .style(theme::base())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::title(true))
        .title(format!(" {} {} ", icon::BRANCH, t!("ui.conflict_list")));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view_h = usize::from(chunk(&chunks, 0).height);
    if let Some(l) = app.conflict_list.as_mut() {
        l.ensure_visible(view_h);
    }
    let Some(l) = app.conflict_list.as_ref() else {
        return;
    };
    // Reserve room for the row number and the " → " separator so both
    // previews get a fair, even share of the remaining width.
    let side_width = ((usize::from(chunk(&chunks, 0).width)).minus(9)).quot(2);
    let mut lines: Vec<Line> = Vec::with_capacity(view_h);
    for idx in l.scroll..l.scroll.plus(view_h).min(l.len()) {
        let Some(c) = l.entries.get(idx) else {
            break;
        };
        let text = format!(
            "{:>3}: {} → {}",
            idx.plus(1),
            preview_line(&c.ours, side_width),
            preview_line(&c.theirs, side_width)
        );
        if idx == l.selected {
            lines.push(Line::from(Span::styled(text, theme::selected())));
        } else {
            lines.push(Line::from(Span::raw(text)));
        }
    }
    frame.render_widget(Paragraph::new(lines), chunk(&chunks, 0));

    let hint = Line::from(Span::styled(
        t!("ui.conflict_list_hint").to_string(),
        theme::dim(),
    ));
    frame.render_widget(Paragraph::new(hint), chunk(&chunks, 1));

    app.layout.conflict_list = Rect {
        x: chunk(&chunks, 0).x,
        y: chunk(&chunks, 0).y,
        width: chunk(&chunks, 0).width,
        height: u16::try_from(view_h)
            .unwrap_or(u16::MAX)
            .min(chunk(&chunks, 0).height),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::cov4::{app_at, temp};
    use ratatui::{Terminal, backend::TestBackend};
    use vix_input::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ch(c: char) -> KeyEvent {
        key(KeyCode::Char(c))
    }

    /// Draw the whole app into a `w` x `h` terminal and return its text,
    /// one string per row.
    fn render(app: &mut App, w: u16, h: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| crate::ui::draw(app, f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect()
            })
            .collect()
    }

    /// The first characters of a (possibly clipped) hint line.
    fn head(text: impl std::fmt::Display) -> String {
        text.to_string().chars().take(10).collect()
    }

    fn any_row(rows: &[String], needle: &str) -> bool {
        rows.iter().any(|r| r.contains(needle))
    }

    #[test]
    fn fit_pads_and_cuts_to_the_width() {
        assert_eq!(fit("ab", 4), "ab  ");
        assert_eq!(fit("abcdef", 3), "abc");
    }

    #[test]
    fn preview_line_keeps_the_first_line_and_marks_a_cut() {
        assert_eq!(preview_line("short\nsecond", 10), "short");
        assert_eq!(preview_line("abcdefgh", 3), "abc…");
        assert_eq!(preview_line("", 3), "");
    }

    /// The table editor scrolls horizontally to keep the selected column in
    /// view, shows a dirty marker after an edit and the find query while
    /// typing it.
    #[test]
    fn table_editor_scrolls_columns_and_shows_edit_and_find_state() {
        let mut app = app_at(&temp("r3-ui-table"));
        let wide = "x".repeat(20);
        let header: Vec<String> = (0..14).map(|i| format!("{wide}{i}")).collect();
        let csv = format!("{}\n{}\n", header.join(","), header.join(","));
        app.edit
            .open_edit_table(crate::edit_table::Grid::from_text(&csv, false));
        let page = 5;
        {
            let grid = app.edit.edit_table_mut().unwrap();
            for _ in 0..10 {
                grid.handle_key(key(KeyCode::Right), page);
            }
        }
        let rows = render(&mut app, 70, 20);
        assert!(any_row(&rows, &t!("ui.edit_table")), "{rows:?}");
        assert!(!any_row(&rows, &format!("{wide}0 ")), "scrolled past col 0");
        assert!(app.edit.edit_table().unwrap().col_scroll() > 0);
        // Edit a cell: the title gains the dirty marker and the hint changes.
        {
            let grid = app.edit.edit_table_mut().unwrap();
            grid.handle_key(key(KeyCode::Enter), page);
            grid.handle_key(ch('Z'), page);
        }
        let rows = render(&mut app, 70, 20);
        assert!(any_row(&rows, &t!("ui.edit_table_editing")));
        {
            let grid = app.edit.edit_table_mut().unwrap();
            grid.handle_key(key(KeyCode::Enter), page);
            assert!(grid.is_dirty());
            grid.handle_key(ch('/'), page);
            grid.handle_key(ch('q'), page);
        }
        let rows = render(&mut app, 70, 20);
        assert!(any_row(&rows, " *"), "{rows:?}");
        assert!(any_row(&rows, "/q"), "{rows:?}");
    }

    /// An empty table closes without drawing; the other surfaces draw nothing
    /// when closed.
    #[test]
    fn closed_surfaces_draw_nothing() {
        let mut app = app_at(&temp("r3-ui-closed"));
        let before = render(&mut app, 60, 12);
        for f in [
            draw_edit_table,
            draw_column_view,
            draw_edit_outline,
            draw_edit_value,
            draw_edit_bytes,
            draw_outline,
            draw_conflict_list,
            draw_html_panel,
        ] {
            let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    f(&mut app, frame, area);
                })
                .unwrap();
            let buf = terminal.backend().buffer();
            assert!(
                (0..12).all(|y| (0..60).all(|x| buf[(x, y)].symbol() == " ")),
                "a closed surface leaves the frame blank"
            );
        }
        assert_eq!(render(&mut app, 60, 12), before);
    }

    /// The Org Column View shows its header, hint and the edit notices.
    #[test]
    fn column_view_shows_hint_editing_and_allowed_value_notices() {
        let mut app = app_at(&temp("r3-ui-cv"));
        let text = "#+COLUMNS: %25ITEM %TODO %EFFORT\n* TODO Task\n:PROPERTIES:\n:EFFORT: 1h\n:END:\n* Other\n";
        app.editor.new_tab_with_content(text);
        app.editor.active_tab_mut().unwrap().path = Some(app.root.join("x.org"));
        app.edit
            .open_column_view(crate::column_view::ColumnView::open(
                text,
                1,
                (2025, 1, 2),
                Some("x.org".into()),
            ));
        let rows = render(&mut app, 80, 20);
        assert!(any_row(&rows, &head(t!("ui.column_view_hint"))), "{rows:?}");
        assert!(any_row(&rows, "ITEM"));
        // Move to the EFFORT column and start a raw-value edit.
        let mut body = text.to_string();
        {
            let view = app.edit.column_view_mut().unwrap();
            view.handle_key(key(KeyCode::Right), &mut body);
            view.handle_key(key(KeyCode::Right), &mut body);
            view.handle_key(ch('e'), &mut body);
            assert!(view.is_editing());
        }
        let rows = render(&mut app, 160, 20);
        assert!(
            any_row(&rows, &head(t!("ui.column_view_editing"))),
            "{rows:?}"
        );
        // The allowed-values editor on the TODO column.
        {
            let view = app.edit.column_view_mut().unwrap();
            view.handle_key(key(KeyCode::Esc), &mut body);
            view.handle_key(key(KeyCode::Left), &mut body);
            view.handle_key(ch('a'), &mut body);
            assert!(view.is_editing_allowed());
        }
        let rows = render(&mut app, 160, 20);
        assert!(
            any_row(&rows, &head(t!("ui.column_view_editing_allowed"))),
            "{rows:?}"
        );
    }

    /// The SQL statement editor lists statements and a placeholder when empty.
    #[cfg(feature = "sql")]
    #[test]
    fn sql_editor_lists_statements_dirty_marker_and_empty_state() {
        let mut app = app_at(&temp("r3-ui-sql"));
        app.edit.open_edit_sql(crate::edit_sql::Editor::from_text(
            "select 1;\nselect 2;\ninsert into t values (1);\n",
        ));
        let rows = render(&mut app, 70, 20);
        assert!(any_row(&rows, &head(t!("ui.edit_sql_hint"))), "{rows:?}");
        {
            let ed = app.edit.edit_sql_mut().unwrap();
            ed.handle_key(ch('d'), 4);
            assert!(ed.is_dirty());
        }
        let rows = render(&mut app, 70, 20);
        assert!(any_row(&rows, " *"), "{rows:?}");
        for _ in 0..3 {
            app.edit.edit_sql_mut().unwrap().handle_key(ch('d'), 4);
        }
        let rows = render(&mut app, 70, 20);
        assert!(any_row(&rows, &t!("ui.edit_sql_empty")), "{rows:?}");
    }

    /// The outline, JSON/YAML and byte editors mark unsaved edits in the
    /// title; the bytes view scrolls with its cursor.
    #[test]
    fn outline_value_and_byte_editors_mark_unsaved_edits() {
        let mut app = app_at(&temp("r3-ui-edits"));
        app.edit
            .open_edit_outline(crate::edit_outline::Tree::from_text("one\n  child\ntwo\n"));
        let rows = render(&mut app, 70, 20);
        assert!(any_row(&rows, &t!("ui.edit_outline")), "{rows:?}");
        {
            let t = app.edit.edit_outline_mut().unwrap();
            t.handle_key(key(KeyCode::Down), 4);
            t.handle_key(key(KeyCode::Down), 4);
            t.handle_key(key(KeyCode::Tab), 4);
            assert!(t.is_dirty());
        }
        assert!(any_row(&render(&mut app, 70, 20), " *"));
        app.edit.close_edit_outline();

        let tree = crate::edit_value::Tree::from_text(
            "{\"a\": 1, \"b\": [1, 2]}",
            crate::edit_value::Format::Json,
        )
        .unwrap();
        app.edit.open_edit_value(tree);
        let rows = render(&mut app, 70, 20);
        assert!(!any_row(&rows, " *"));
        {
            let t = app.edit.edit_value_mut().unwrap();
            t.handle_key(key(KeyCode::Down), 8);
            t.handle_key(key(KeyCode::Enter), 8);
            t.handle_key(key(KeyCode::Backspace), 8);
            t.handle_key(ch('9'), 8);
            t.handle_key(key(KeyCode::Enter), 8);
            assert!(t.is_dirty());
        }
        assert!(any_row(&render(&mut app, 70, 20), " *"));
        app.edit.close_edit_value();

        app.edit.open_edit_bytes(crate::edit_bytes::Hex::from_bytes(
            (0..=255u8).cycle().take(2000).collect(),
        ));
        {
            let h = app.edit.edit_bytes_mut().unwrap();
            h.handle_key(key(KeyCode::PageDown), 4);
            h.handle_key(ch('f'), 4);
            assert!(h.is_dirty());
        }
        let rows = render(&mut app, 80, 20);
        assert!(any_row(&rows, " *"), "{rows:?}");
        assert!(any_row(&rows, &head(t!("ui.edit_bytes_hint"))));
    }

    /// The HTML entity picker, document outline and conflict list draw their
    /// rows, a scrollbar only when they overflow, and the selected row.
    #[test]
    fn picker_outline_and_conflict_overlays_draw_their_rows() {
        let mut app = app_at(&temp("r3-ui-lists"));
        app.open_html_panel();
        let short = render(&mut app, 80, 12);
        assert!(any_row(&short, &t!("ui.html_header")), "{short:?}");
        let tall = render(&mut app, 80, 400);
        assert!(any_row(&tall, &head(t!("ui.html_hint"))));
        app.html_panel = None;

        app.outline = Some(vix_outline_panel::Outline::new(vec![
            crate::outline_panel::Entry {
                kind: "fn".into(),
                name: "alpha".into(),
                line: 1,
            },
            crate::outline_panel::Entry {
                kind: String::new(),
                name: "Heading".into(),
                line: 5,
            },
        ]));
        let rows = render(&mut app, 80, 20);
        assert!(
            any_row(&rows, "alpha") && any_row(&rows, "Heading"),
            "{rows:?}"
        );
        app.outline = None;

        let long = "y".repeat(80);
        app.conflict_list = Some(vix_conflict_tool::List::new(vec![
            vix_conflict_tool::Conflict {
                start: 0,
                end: 5,
                ours: format!("{long}\nmore\n"),
                theirs: "theirs\n".into(),
            },
            vix_conflict_tool::Conflict {
                start: 6,
                end: 9,
                ours: String::new(),
                theirs: "gone\n".into(),
            },
        ]));
        let rows = render(&mut app, 100, 20);
        assert!(any_row(&rows, "…"), "{rows:?}");
        assert!(any_row(&rows, "gone"));
    }
}
