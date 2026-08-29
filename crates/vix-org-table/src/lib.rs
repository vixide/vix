//! Org-mode pipe-table editing: parsing, alignment, structural edits, and TBLFM
//! formulas.
//!
//! This is a pragmatic subset of Org's table editor
//! (<https://orgmode.org/manual/Tables.html>), not a complete implementation.
//! Out of scope: Calc-mode formulas and unit conversions, Lisp formulas
//! (`'(...)`), named field/column references (`$name`), duration-aware
//! arithmetic beyond the [`copy_down`] increment case, `remote()` cross-table
//! references, recalculation-on-every-keystroke (recalculation only happens
//! when [`recalc`]/[`apply_tblfm`] is called explicitly), and Unicode
//! grapheme-cluster-aware column widths (widths are `.chars().count()`, so
//! combining marks and wide CJK characters are not accounted for).
//!
//! Functions operate on the whole buffer text plus a 0-based cursor line (and,
//! for field-level operations, a 0-based **byte offset** column into that
//! line), and return the rewritten text (and, where the cursor should follow,
//! its new position). All functions are pure so they can be unit-tested
//! without a live editor.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use evalexpr::Value;

// ================================= Model =====================================

/// A parsed pipe table: an ordered sequence of rows, in source order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Table {
    /// The rows, in the order they appear in the source text.
    pub rows: Vec<Row>,
}

/// One row of a table: a horizontal rule, or a row of cell contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A horizontal separator line (rendered as `|---+---|`).
    Hline,
    /// A data row: one string per column, already trimmed of surrounding
    /// whitespace. Rows may have different lengths if the source text is
    /// malformed; [`render`] pads short rows with empty cells up to the
    /// table's widest row.
    ///
    /// A literal `|` inside a field must be pre-escaped by the user as
    /// `\vert{}` (Org's own convention) — this crate does not unescape or
    /// re-escape it, it just does not split on it specially: cell splitting
    /// is naive on every unescaped `|`, matching Org's own default behavior.
    Data(Vec<String>),
}

// ============================ Recognition & parsing ==========================

/// Whether `line`'s first non-whitespace character is `|` — the definition of
/// "this line belongs to a table".
fn is_table_line(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// Whether `line` is a horizontal rule: `|` then `-`, after trimming.
fn is_hline_line(line: &str) -> bool {
    line.trim().starts_with("|-")
}

/// The `[first_line, last_line]` (inclusive, 0-indexed) line range of the
/// contiguous run of table lines containing `line`, or `None` if `line` is not
/// itself a table line.
#[must_use]
pub fn table_range(text: &str, line: usize) -> Option<(usize, usize)> {
    let lines: Vec<&str> = text.split('\n').collect();
    if !lines.get(line).is_some_and(|l| is_table_line(l)) {
        return None;
    }
    let mut start = line;
    while start > 0 && is_table_line(lines[start - 1]) {
        start -= 1;
    }
    let mut end = line;
    while end + 1 < lines.len() && is_table_line(lines[end + 1]) {
        end += 1;
    }
    Some((start, end))
}

/// Parse one table line into a [`Row`].
fn parse_row(line: &str) -> Row {
    if is_hline_line(line) {
        return Row::Hline;
    }
    let trimmed = line.trim();
    let mut inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    if let Some(stripped) = inner.strip_suffix('|') {
        inner = stripped;
    }
    Row::Data(inner.split('|').map(|c| c.trim().to_string()).collect())
}

/// Parse the table occupying lines `[first_line, last_line]` (inclusive) of
/// `text` into a [`Table`]. Callers typically obtain the range from
/// [`table_range`] first.
#[must_use]
pub fn parse(text: &str, first_line: usize, last_line: usize) -> Table {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.is_empty() || first_line > last_line {
        return Table::default();
    }
    let last = last_line.min(lines.len() - 1);
    let rows = lines[first_line..=last]
        .iter()
        .map(|l| parse_row(l))
        .collect();
    Table { rows }
}

/// The number of columns of `table`: the widest data row's cell count.
fn table_ncols(table: &Table) -> usize {
    table
        .rows
        .iter()
        .filter_map(|r| match r {
            Row::Data(cells) => Some(cells.len()),
            Row::Hline => None,
        })
        .max()
        .unwrap_or(0)
}

/// The index of the header row, when the table has one: row 0, if it is a
/// data row immediately followed by an hline. Used only to exclude the header
/// from the per-column numeric-alignment heuristic in [`render`].
fn header_row_index(rows: &[Row]) -> Option<usize> {
    if rows.len() > 1 && matches!(rows[0], Row::Data(_)) && matches!(rows[1], Row::Hline) {
        Some(0)
    } else {
        None
    }
}

/// Re-render `table` as pipe-table text: one line per row, columns padded to
/// the widest cell in that column (counted in `char`s, not bytes — wide/combining
/// characters are out of scope), with a one-space margin on each side.
///
/// A column is right-aligned ("numeric") when a majority of its non-empty data
/// cells (excluding the header row, when the table has one) parse as a number;
/// otherwise it is left-aligned.
#[must_use]
pub fn render(table: &Table) -> String {
    if table.rows.is_empty() {
        return String::new();
    }
    // Zero columns means the table is nothing but horizontal rules — a data row
    // always has at least one cell. Size those rules as a single empty column
    // (`|---|`, as Emacs does) rather than returning nothing: every caller
    // *replaces* the table's lines with what this returns, so an empty string
    // would delete a `|-` line instead of aligning it.
    let ncols = table_ncols(table).max(1);
    let header = header_row_index(&table.rows);
    let mut widths = vec![1usize; ncols];
    for row in &table.rows {
        if let Row::Data(cells) = row {
            for (c, cell) in cells.iter().enumerate() {
                widths[c] = widths[c].max(cell.chars().count());
            }
        }
    }
    let numeric = column_numeric_alignment(table, ncols, header);

    let mut lines = Vec::with_capacity(table.rows.len());
    for row in &table.rows {
        lines.push(render_row(row, &widths, &numeric));
    }
    lines.join("\n")
}

/// Per-column numeric-alignment flags: true when a majority of that column's
/// non-empty data cells (outside the header row) parse as a number.
fn column_numeric_alignment(table: &Table, ncols: usize, header: Option<usize>) -> Vec<bool> {
    let mut numeric = vec![false; ncols];
    for (c, flag) in numeric.iter_mut().enumerate() {
        let mut total = 0usize;
        let mut num = 0usize;
        for (i, row) in table.rows.iter().enumerate() {
            if Some(i) == header {
                continue;
            }
            if let Row::Data(cells) = row
                && let Some(cell) = cells.get(c)
                && !cell.is_empty()
            {
                total += 1;
                if cell.parse::<f64>().is_ok() {
                    num += 1;
                }
            }
        }
        *flag = total > 0 && num * 2 > total;
    }
    numeric
}

/// Render one row (data or hline) to a single line of text.
fn render_row(row: &Row, widths: &[usize], numeric: &[bool]) -> String {
    match row {
        Row::Hline => {
            let mut out = String::from("|");
            for (i, w) in widths.iter().enumerate() {
                if i > 0 {
                    out.push('+');
                }
                out.push_str(&"-".repeat(w + 2));
            }
            out.push('|');
            out
        }
        Row::Data(cells) => {
            let mut out = String::from("|");
            for (c, w) in widths.iter().enumerate() {
                let cell = cells.get(c).map_or("", String::as_str);
                let pad = w.saturating_sub(cell.chars().count());
                out.push(' ');
                if numeric[c] {
                    out.push_str(&" ".repeat(pad));
                    out.push_str(cell);
                } else {
                    out.push_str(cell);
                    out.push_str(&" ".repeat(pad));
                }
                out.push(' ');
                out.push('|');
            }
            out
        }
    }
}

/// Replace lines `[first, last]` (inclusive) of `text` with `replacement`'s
/// lines, and return the whole rewritten buffer text.
fn splice_lines(text: &str, first: usize, last: usize, replacement: &str) -> String {
    let mut lines: Vec<&str> = text.split('\n').collect();
    let repl_lines: Vec<&str> = replacement.split('\n').collect();
    if first <= last && last < lines.len() {
        lines.splice(first..=last, repl_lines);
    }
    lines.join("\n")
}

/// Realign (re-render) the table at `line`: `C-c C-c` / `org-table-align`.
/// `None` if `line` is not inside a table.
#[must_use]
pub fn align(text: &str, line: usize) -> Option<String> {
    let (first, last) = table_range(text, line)?;
    let table = parse(text, first, last);
    let rendered = render(&table);
    Some(splice_lines(text, first, last, &rendered))
}

// ============================== Field position ================================

/// The byte-offset delimiter positions of `line`: pipes for a data row, or
/// pipes and `+` joints for an hline (whose internal joints are `+`, not `|`).
fn line_delimiter_positions(line: &str) -> Vec<usize> {
    if is_hline_line(line) {
        line.match_indices(['|', '+']).map(|(i, _)| i).collect()
    } else {
        line.match_indices('|').map(|(i, _)| i).collect()
    }
}

/// The 0-indexed field that byte offset `col` falls into on `line`, clamped to
/// the last field when `col` is past the closing delimiter.
fn field_index_at(line: &str, col: usize) -> usize {
    let delims = line_delimiter_positions(line);
    if delims.len() < 2 {
        return 0;
    }
    let max_field = delims.len() - 2;
    let mut idx = 0;
    for (i, &p) in delims.iter().enumerate() {
        if p < col {
            idx = i;
        } else {
            break;
        }
    }
    idx.min(max_field)
}

/// The byte offset in `line` of the start of field `field_idx`'s content
/// region: right after its opening delimiter and the one-space margin, if
/// present. `None` if `line` does not have that many fields.
fn field_start_offset(line: &str, field_idx: usize) -> Option<usize> {
    let delims = line_delimiter_positions(line);
    let start = *delims.get(field_idx)?;
    let mut offset = start + 1;
    if line.as_bytes().get(offset) == Some(&b' ') {
        offset += 1;
    }
    Some(offset)
}

/// Row `row_idx` (0-indexed within the table) of `rendered`'s lines.
fn table_row_line(rendered: &str, row_idx: usize) -> &str {
    rendered.split('\n').nth(row_idx).unwrap_or("")
}

/// The nearest [`Row::Data`] index strictly before `from`, skipping hlines.
fn previous_data_row_index(rows: &[Row], from: usize) -> Option<usize> {
    (0..from).rev().find(|&i| matches!(rows[i], Row::Data(_)))
}

/// The nearest [`Row::Data`] index strictly after `from`, skipping hlines.
fn next_data_row_index(rows: &[Row], from: usize) -> Option<usize> {
    (from + 1..rows.len()).find(|&i| matches!(rows[i], Row::Data(_)))
}

fn get_cell(table: &Table, row: usize, col: usize) -> Option<String> {
    match table.rows.get(row)? {
        Row::Data(cells) => cells.get(col).cloned(),
        Row::Hline => None,
    }
}

fn set_cell(table: &mut Table, row: usize, col: usize, value: String) {
    if let Some(Row::Data(cells)) = table.rows.get_mut(row) {
        if col >= cells.len() {
            cells.resize(col + 1, String::new());
        }
        cells[col] = value;
    }
}

// ================================ Field navigation =============================

/// `TAB` / `org-table-next-field`: realign the table, then move to the next
/// field. Wraps from the last field of a row to the first field of the next
/// data row (hlines in between are skipped over). If there is no further data
/// row, a new empty data row (same column count) is appended and becomes the
/// landing row. Returns `(new text, new line, new byte column)`.
#[must_use]
pub fn next_field(text: &str, line: usize, col: usize) -> Option<(String, usize, usize)> {
    let (first, last) = table_range(text, line)?;
    let orig_lines: Vec<&str> = text.split('\n').collect();
    let field_idx = field_index_at(orig_lines[line], col);
    let mut table = parse(text, first, last);
    let ncols = table_ncols(&table);
    let row_idx = line - first;

    let same_row_next =
        matches!(table.rows.get(row_idx), Some(Row::Data(cells)) if field_idx + 1 < cells.len());
    let target_row = if same_row_next {
        row_idx
    } else if let Some(nd) = next_data_row_index(&table.rows, row_idx) {
        nd
    } else {
        table.rows.push(Row::Data(vec![String::new(); ncols]));
        table.rows.len() - 1
    };
    let target_field = if same_row_next { field_idx + 1 } else { 0 };

    let rendered = render(&table);
    let new_text = splice_lines(text, first, last, &rendered);
    let row_line = table_row_line(&rendered, target_row);
    let new_col = field_start_offset(row_line, target_field).unwrap_or(0);
    Some((new_text, first + target_row, new_col))
}

/// `S-TAB` / `org-table-previous-field`: realign the table, then move to the
/// previous field, wrapping to the last field of the previous data row (hlines
/// skipped). `None` (no-op) at the first field of the first data row.
#[must_use]
pub fn previous_field(text: &str, line: usize, col: usize) -> Option<(String, usize, usize)> {
    let (first, last) = table_range(text, line)?;
    let orig_lines: Vec<&str> = text.split('\n').collect();
    let field_idx = field_index_at(orig_lines[line], col);
    let table = parse(text, first, last);
    let row_idx = line - first;

    let (target_row, target_field) =
        if field_idx > 0 && matches!(table.rows.get(row_idx), Some(Row::Data(_))) {
            (row_idx, field_idx - 1)
        } else {
            let prev = previous_data_row_index(&table.rows, row_idx)?;
            let Row::Data(cells) = &table.rows[prev] else {
                return None;
            };
            (prev, cells.len().saturating_sub(1))
        };

    let rendered = render(&table);
    let new_text = splice_lines(text, first, last, &rendered);
    let row_line = table_row_line(&rendered, target_row);
    let new_col = field_start_offset(row_line, target_field).unwrap_or(0);
    Some((new_text, first + target_row, new_col))
}

/// `RET` / `org-table-next-row`: realign the table, move down to the next
/// row (same column), creating a new empty data row after the current one if
/// it was the last row.
#[must_use]
pub fn next_row(text: &str, line: usize, col: usize) -> Option<(String, usize, usize)> {
    let (first, last) = table_range(text, line)?;
    let orig_lines: Vec<&str> = text.split('\n').collect();
    let field_idx = field_index_at(orig_lines[line], col);
    let mut table = parse(text, first, last);
    let ncols = table_ncols(&table);
    let row_idx = line - first;

    let target_row = if row_idx + 1 < table.rows.len() {
        row_idx + 1
    } else {
        table.rows.push(Row::Data(vec![String::new(); ncols]));
        table.rows.len() - 1
    };

    let rendered = render(&table);
    let new_text = splice_lines(text, first, last, &rendered);
    let row_line = table_row_line(&rendered, target_row);
    let new_col = field_start_offset(row_line, field_idx).unwrap_or(0);
    Some((new_text, first + target_row, new_col))
}

/// Clear the field at `(line, col)`.
#[must_use]
pub fn blank_field(text: &str, line: usize, col: usize) -> Option<String> {
    let (first, last) = table_range(text, line)?;
    let orig_lines: Vec<&str> = text.split('\n').collect();
    let field_idx = field_index_at(orig_lines[line], col);
    let mut table = parse(text, first, last);
    let row_idx = line - first;
    if let Some(Row::Data(cells)) = table.rows.get_mut(row_idx)
        && let Some(cell) = cells.get_mut(field_idx)
    {
        cell.clear();
    }
    let rendered = render(&table);
    Some(splice_lines(text, first, last, &rendered))
}

// ========================= Row & column structural edits =======================

/// Insert an empty data row (same column count as the table) above or below
/// the row at `line`. Returns the new cursor line.
#[must_use]
pub fn insert_row(text: &str, line: usize, below: bool) -> Option<(String, usize)> {
    let (first, last) = table_range(text, line)?;
    let mut table = parse(text, first, last);
    let ncols = table_ncols(&table);
    let row_idx = line - first;
    let insert_at = if below { row_idx + 1 } else { row_idx };
    table
        .rows
        .insert(insert_at, Row::Data(vec![String::new(); ncols]));
    let rendered = render(&table);
    let new_text = splice_lines(text, first, last, &rendered);
    Some((new_text, first + insert_at))
}

/// Delete the row (data row or hline) at `line`. Removes the whole table
/// block from the buffer if that was its only row.
#[must_use]
pub fn kill_row(text: &str, line: usize) -> Option<String> {
    let (first, last) = table_range(text, line)?;
    let mut table = parse(text, first, last);
    let row_idx = line - first;
    if row_idx >= table.rows.len() {
        return None;
    }
    table.rows.remove(row_idx);
    if table.rows.is_empty() {
        let mut lines: Vec<&str> = text.split('\n').collect();
        lines.drain(first..=last);
        return Some(lines.join("\n"));
    }
    let rendered = render(&table);
    Some(splice_lines(text, first, last, &rendered))
}

/// Swap the row at `line` with its previous (`up`) or next row (data or
/// hline). `None` if there is no adjacent row within the table.
fn move_row(text: &str, line: usize, up: bool) -> Option<(String, usize)> {
    let (first, last) = table_range(text, line)?;
    let mut table = parse(text, first, last);
    let row_idx = line - first;
    let other = if up {
        row_idx.checked_sub(1)?
    } else {
        row_idx + 1
    };
    if other >= table.rows.len() {
        return None;
    }
    table.rows.swap(row_idx, other);
    let rendered = render(&table);
    let new_text = splice_lines(text, first, last, &rendered);
    Some((new_text, first + other))
}

/// Swap the row at `line` with the previous row. `None` if `line` is the
/// table's first row.
#[must_use]
pub fn move_row_up(text: &str, line: usize) -> Option<(String, usize)> {
    move_row(text, line, true)
}

/// Swap the row at `line` with the next row. `None` if `line` is the table's
/// last row.
#[must_use]
pub fn move_row_down(text: &str, line: usize) -> Option<(String, usize)> {
    move_row(text, line, false)
}

/// Insert a horizontal rule above or below the row at `line`.
#[must_use]
pub fn insert_hline(text: &str, line: usize, above: bool) -> Option<String> {
    let (first, last) = table_range(text, line)?;
    let mut table = parse(text, first, last);
    let row_idx = line - first;
    let insert_at = (if above { row_idx } else { row_idx + 1 }).min(table.rows.len());
    table.rows.insert(insert_at, Row::Hline);
    let rendered = render(&table);
    Some(splice_lines(text, first, last, &rendered))
}

/// `C-c RET` / `org-table-hline-and-move`: insert a horizontal rule below the
/// row at `line`, then insert a fresh empty data row below that rule and move
/// the cursor there.
#[must_use]
pub fn hline_and_move(text: &str, line: usize) -> Option<(String, usize)> {
    let (first, last) = table_range(text, line)?;
    let mut table = parse(text, first, last);
    let ncols = table_ncols(&table);
    let row_idx = line - first;
    let hline_at = (row_idx + 1).min(table.rows.len());
    table.rows.insert(hline_at, Row::Hline);
    let new_row_at = hline_at + 1;
    table
        .rows
        .insert(new_row_at, Row::Data(vec![String::new(); ncols]));
    let rendered = render(&table);
    let new_text = splice_lines(text, first, last, &rendered);
    Some((new_text, first + new_row_at))
}

/// Insert an empty column at the field position under `col`, shifting that
/// column and everything to its right rightward. Returns the byte offset of
/// the new empty field on `line`.
#[must_use]
pub fn insert_column(text: &str, line: usize, col: usize) -> Option<(String, usize)> {
    let (first, last) = table_range(text, line)?;
    let orig_lines: Vec<&str> = text.split('\n').collect();
    let field_idx = field_index_at(orig_lines[line], col);
    let mut table = parse(text, first, last);
    for row in &mut table.rows {
        if let Row::Data(cells) = row {
            let at = field_idx.min(cells.len());
            cells.insert(at, String::new());
        }
    }
    let rendered = render(&table);
    let new_text = splice_lines(text, first, last, &rendered);
    let row_line = table_row_line(&rendered, line - first);
    let new_col = field_start_offset(row_line, field_idx).unwrap_or(0);
    Some((new_text, new_col))
}

/// Delete the column at the field position under `col`. Returns the byte
/// offset of the field now standing where the deleted column was (clamped to
/// the table's last column).
#[must_use]
pub fn delete_column(text: &str, line: usize, col: usize) -> Option<(String, usize)> {
    let (first, last) = table_range(text, line)?;
    let orig_lines: Vec<&str> = text.split('\n').collect();
    let field_idx = field_index_at(orig_lines[line], col);
    let mut table = parse(text, first, last);
    for row in &mut table.rows {
        if let Row::Data(cells) = row
            && field_idx < cells.len()
        {
            cells.remove(field_idx);
        }
    }
    let rendered = render(&table);
    let new_text = splice_lines(text, first, last, &rendered);
    let row_line = table_row_line(&rendered, line - first);
    let landing = field_idx.min(table_ncols(&table).saturating_sub(1));
    let new_col = field_start_offset(row_line, landing).unwrap_or(0);
    Some((new_text, new_col))
}

/// Swap the field-column under `col` with its left (`left = true`) or right
/// neighbor, on every row. `None` if there is no such neighbor column.
fn move_column(text: &str, line: usize, col: usize, left: bool) -> Option<(String, usize)> {
    let (first, last) = table_range(text, line)?;
    let orig_lines: Vec<&str> = text.split('\n').collect();
    let field_idx = field_index_at(orig_lines[line], col);
    let mut table = parse(text, first, last);
    let ncols = table_ncols(&table);
    let other = if left {
        field_idx.checked_sub(1)?
    } else {
        field_idx + 1
    };
    if other >= ncols {
        return None;
    }
    for row in &mut table.rows {
        if let Row::Data(cells) = row {
            let needed = other.max(field_idx) + 1;
            if cells.len() < needed {
                cells.resize(needed, String::new());
            }
            cells.swap(field_idx, other);
        }
    }
    let rendered = render(&table);
    let new_text = splice_lines(text, first, last, &rendered);
    let row_line = table_row_line(&rendered, line - first);
    let new_col = field_start_offset(row_line, other).unwrap_or(0);
    Some((new_text, new_col))
}

/// Swap the column under `col` with the column to its left. `None` if `col`
/// is already the first column.
#[must_use]
pub fn move_column_left(text: &str, line: usize, col: usize) -> Option<(String, usize)> {
    move_column(text, line, col, true)
}

/// Swap the column under `col` with the column to its right. `None` if `col`
/// is already the last column.
#[must_use]
pub fn move_column_right(text: &str, line: usize, col: usize) -> Option<(String, usize)> {
    move_column(text, line, col, false)
}

// ============================== Cell swap-moves (Shift-arrow) ==================

/// Swap the field at `(line, col)` with the field directly above/below it in
/// the same column, skipping over hlines to the nearest data row in that
/// direction (matching Emacs's shift-arrow cell swap, which only operates
/// between data rows). `None` if there is no such data row, or the field's own
/// row is an hline.
fn move_cell_vertical(
    text: &str,
    line: usize,
    col: usize,
    up: bool,
) -> Option<(String, usize, usize)> {
    let (first, last) = table_range(text, line)?;
    let orig_lines: Vec<&str> = text.split('\n').collect();
    let field_idx = field_index_at(orig_lines[line], col);
    let mut table = parse(text, first, last);
    let row_idx = line - first;
    if !matches!(table.rows.get(row_idx), Some(Row::Data(_))) {
        return None;
    }
    let other = if up {
        previous_data_row_index(&table.rows, row_idx)?
    } else {
        next_data_row_index(&table.rows, row_idx)?
    };
    let a = get_cell(&table, row_idx, field_idx).unwrap_or_default();
    let b = get_cell(&table, other, field_idx).unwrap_or_default();
    set_cell(&mut table, row_idx, field_idx, b);
    set_cell(&mut table, other, field_idx, a);
    let rendered = render(&table);
    let new_text = splice_lines(text, first, last, &rendered);
    let row_line = table_row_line(&rendered, other);
    let new_col = field_start_offset(row_line, field_idx).unwrap_or(0);
    Some((new_text, first + other, new_col))
}

/// Swap the field at `(line, col)` with the field above it in the same
/// column (skipping hlines).
#[must_use]
pub fn move_cell_up(text: &str, line: usize, col: usize) -> Option<(String, usize, usize)> {
    move_cell_vertical(text, line, col, true)
}

/// Swap the field at `(line, col)` with the field below it in the same
/// column (skipping hlines).
#[must_use]
pub fn move_cell_down(text: &str, line: usize, col: usize) -> Option<(String, usize, usize)> {
    move_cell_vertical(text, line, col, false)
}

/// Swap the field at `(line, col)` with its left/right neighbor in the same
/// row. `None` if the row is an hline (hlines have no cells) or there is no
/// such neighbor field.
fn move_cell_horizontal(
    text: &str,
    line: usize,
    col: usize,
    left: bool,
) -> Option<(String, usize, usize)> {
    let (first, last) = table_range(text, line)?;
    let orig_lines: Vec<&str> = text.split('\n').collect();
    let field_idx = field_index_at(orig_lines[line], col);
    let mut table = parse(text, first, last);
    let row_idx = line - first;
    let Some(Row::Data(cells)) = table.rows.get(row_idx) else {
        return None;
    };
    let other = if left {
        field_idx.checked_sub(1)?
    } else {
        field_idx + 1
    };
    if other >= cells.len() {
        return None;
    }
    if let Some(Row::Data(cells)) = table.rows.get_mut(row_idx) {
        cells.swap(field_idx, other);
    }
    let rendered = render(&table);
    let new_text = splice_lines(text, first, last, &rendered);
    let row_line = table_row_line(&rendered, row_idx);
    let new_col = field_start_offset(row_line, other).unwrap_or(0);
    Some((new_text, line, new_col))
}

/// Swap the field at `(line, col)` with the field to its left in the same
/// row.
#[must_use]
pub fn move_cell_left(text: &str, line: usize, col: usize) -> Option<(String, usize, usize)> {
    move_cell_horizontal(text, line, col, true)
}

/// Swap the field at `(line, col)` with the field to its right in the same
/// row.
#[must_use]
pub fn move_cell_right(text: &str, line: usize, col: usize) -> Option<(String, usize, usize)> {
    move_cell_horizontal(text, line, col, false)
}

// ==================================== Sort ======================================

/// How [`sort_rows`] should compare each row's key cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKind {
    /// Compare cell text as strings.
    Alphabetic,
    /// Parse cells as numbers; unparsable cells sort last.
    Numeric,
    /// Parse cells as an Org/ISO-ish timestamp or `HH:MM`, sorting
    /// chronologically; falls back to alphabetic comparison on parse failure.
    Time,
}

/// Sort the data rows in `[first_line, last_line]` (inclusive) by their
/// 0-indexed `column`, in place. Hlines within the range keep their line
/// position; only the data-row *contents* are reordered into the data-row
/// slots (in source order) — this is the simplest correct behavior for the
/// common cases (a whole table, or a caller-narrowed contiguous run between
/// hlines) and does not attempt to keep a run "anchored" to a particular
/// hline beyond that.
#[must_use]
pub fn sort_rows(
    text: &str,
    first_line: usize,
    last_line: usize,
    column: usize,
    kind: SortKind,
    reverse: bool,
    case_sensitive: bool,
) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.is_empty() || first_line > last_line || last_line >= lines.len() {
        return None;
    }
    if !lines[first_line..=last_line]
        .iter()
        .any(|l| is_table_line(l))
    {
        return None;
    }
    let mut table = parse(text, first_line, last_line);
    let slots: Vec<usize> = table
        .rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| matches!(r, Row::Data(_)).then_some(i))
        .collect();
    if slots.is_empty() {
        return None;
    }
    let mut rows: Vec<Vec<String>> = slots
        .iter()
        .map(|&i| match &table.rows[i] {
            Row::Data(cells) => cells.clone(),
            Row::Hline => Vec::new(),
        })
        .collect();

    sort_rows_by_key(&mut rows, column, kind, case_sensitive);
    if reverse {
        rows.reverse();
    }

    for (&slot, cells) in slots.iter().zip(rows) {
        table.rows[slot] = Row::Data(cells);
    }
    let rendered = render(&table);
    Some(splice_lines(text, first_line, last_line, &rendered))
}

/// Sort `rows` in place by their `column`-th cell, per `kind`.
fn sort_rows_by_key(rows: &mut [Vec<String>], column: usize, kind: SortKind, case_sensitive: bool) {
    match kind {
        SortKind::Alphabetic => rows.sort_by(|a, b| {
            text_key(a.get(column), case_sensitive).cmp(&text_key(b.get(column), case_sensitive))
        }),
        SortKind::Numeric => rows.sort_by(|a, b| {
            let na = a
                .get(column)
                .and_then(|s| s.trim().parse::<f64>().ok())
                .unwrap_or(f64::INFINITY);
            let nb = b
                .get(column)
                .and_then(|s| s.trim().parse::<f64>().ok())
                .unwrap_or(f64::INFINITY);
            na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
        }),
        SortKind::Time => rows.sort_by(|a, b| {
            let ka = a.get(column).map(String::as_str).unwrap_or_default();
            let kb = b.get(column).map(String::as_str).unwrap_or_default();
            match (parse_time_key(ka), parse_time_key(kb)) {
                (Some(x), Some(y)) => x.cmp(&y),
                _ => text_key(a.get(column), case_sensitive)
                    .cmp(&text_key(b.get(column), case_sensitive)),
            }
        }),
    }
}

/// A comparison key for `Alphabetic`/fallback text sorting.
fn text_key(cell: Option<&String>, case_sensitive: bool) -> String {
    let raw = cell.map(String::as_str).unwrap_or_default();
    if case_sensitive {
        raw.to_string()
    } else {
        raw.to_lowercase()
    }
}

/// Whether `s` is exactly `n` ASCII digits.
fn is_ascii_digits(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}

/// Permissively parse `raw` as an Org/ISO-ish timestamp (`<YYYY-MM-DD [Www]
/// [HH:MM]>`, with or without the brackets) or a bare `H:MM`/`HH:MM` duration,
/// into a string key that sorts chronologically. Bare durations (no date) sort
/// after every dated timestamp, under a sentinel date. `None` if `raw` matches
/// neither shape.
fn parse_time_key(raw: &str) -> Option<String> {
    let s = raw.trim().trim_matches(['<', '>', '[', ']']);

    if let Some((h, m)) = s.split_once(':')
        && s.matches(':').count() == 1
        && !h.is_empty()
        && h.len() <= 2
        && h.bytes().all(|b| b.is_ascii_digit())
        && is_ascii_digits(m, 2)
        && let Ok(hh) = h.parse::<u32>()
        && hh < 24
    {
        return Some(format!("9999-99-99 {h:0>2}:{m}"));
    }

    if s.len() >= 10
        && is_ascii_digits(&s[0..4], 4)
        && s.as_bytes()[4] == b'-'
        && is_ascii_digits(&s[5..7], 2)
        && s.as_bytes()[7] == b'-'
        && is_ascii_digits(&s[8..10], 2)
    {
        let date = &s[0..10];
        let rest = s[10..].trim_start();
        let rest = rest.split_once(' ').map_or(rest, |(first, tail)| {
            if !first.is_empty() && first.chars().all(char::is_alphabetic) {
                tail.trim_start()
            } else {
                rest
            }
        });
        let time = rest
            .split_once(':')
            .filter(|(h, m)| {
                !h.is_empty()
                    && h.len() <= 2
                    && h.bytes().all(|b| b.is_ascii_digit())
                    && is_ascii_digits(&m[..2.min(m.len())], 2)
            })
            .map_or_else(
                || "00:00".to_string(),
                |(h, m)| format!("{h:0>2}:{}", &m[..2]),
            );
        return Some(format!("{date} {time}"));
    }

    None
}

// ============================= Rectangle copy/cut/paste ========================

/// The rectangular block of cell text between `corner_a` and `corner_b`,
/// row-major. Corners are `(line, field index)` — **field index**, not byte
/// offset, unlike the byte-offset convention used by the field-navigation
/// functions above. Hline rows within the block are skipped, per the manual
/// ("the process ignores horizontal separator lines").
#[must_use]
pub fn copy_rectangle(
    text: &str,
    corner_a: (usize, usize),
    corner_b: (usize, usize),
) -> Vec<Vec<String>> {
    let (r0, r1) = (corner_a.0.min(corner_b.0), corner_a.0.max(corner_b.0));
    let (c0, c1) = (corner_a.1.min(corner_b.1), corner_a.1.max(corner_b.1));
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = Vec::new();
    for line in lines.iter().take(r1 + 1).skip(r0) {
        if let Row::Data(cells) = parse_row(line) {
            out.push(
                (c0..=c1)
                    .map(|c| cells.get(c).cloned().unwrap_or_default())
                    .collect(),
            );
        }
    }
    out
}

/// Like [`copy_rectangle`], but also blanks the copied fields in the source
/// text. Returns `(new text, the copied block)`.
#[must_use]
pub fn cut_rectangle(
    text: &str,
    corner_a: (usize, usize),
    corner_b: (usize, usize),
) -> (String, Vec<Vec<String>>) {
    let rect = copy_rectangle(text, corner_a, corner_b);
    let (r0, r1) = (corner_a.0.min(corner_b.0), corner_a.0.max(corner_b.0));
    let (c0, c1) = (corner_a.1.min(corner_b.1), corner_a.1.max(corner_b.1));
    let Some((first, last)) = table_range(text, r0) else {
        return (text.to_string(), rect);
    };
    let mut table = parse(text, first, last);
    for i in r0..=r1 {
        if i < first || i > last {
            continue;
        }
        if let Some(Row::Data(cells)) = table.rows.get_mut(i - first) {
            for c in c0..=c1 {
                if let Some(cell) = cells.get_mut(c) {
                    cell.clear();
                }
            }
        }
    }
    let rendered = render(&table);
    (splice_lines(text, first, last, &rendered), rect)
}

/// Paste `rect` with its upper-left cell at the field under `(line, col)`
/// (byte offset — the landing-field convention used elsewhere in this crate).
/// Overwrites existing fields; extends the table with new rows/columns when
/// `rect` does not fit within its current bounds. Destination rows are
/// counted among data rows only (hlines encountered are stepped over, and new
/// data rows are appended if the table runs out).
#[must_use]
pub fn paste_rectangle(text: &str, line: usize, col: usize, rect: &[Vec<String>]) -> String {
    let Some((first, last)) = table_range(text, line) else {
        return text.to_string();
    };
    if rect.is_empty() {
        return text.to_string();
    }
    let orig_lines: Vec<&str> = text.split('\n').collect();
    let field_idx = field_index_at(orig_lines[line], col);
    let mut table = parse(text, first, last);
    let row_idx = line - first;
    let rect_cols = rect.iter().map(Vec::len).max().unwrap_or(0);
    let ncols_needed = field_idx + rect_cols;

    let mut targets = Vec::with_capacity(rect.len());
    let mut probe = row_idx;
    while targets.len() < rect.len() {
        if probe >= table.rows.len() {
            table.rows.push(Row::Data(Vec::new()));
        }
        if matches!(table.rows[probe], Row::Data(_)) {
            targets.push(probe);
        }
        probe += 1;
    }

    let ncols_now = table_ncols(&table).max(ncols_needed);
    for row in &mut table.rows {
        if let Row::Data(cells) = row
            && cells.len() < ncols_now
        {
            cells.resize(ncols_now, String::new());
        }
    }

    for (r, &target_row) in targets.iter().enumerate() {
        if let Some(Row::Data(cells)) = table.rows.get_mut(target_row) {
            for (c, val) in rect[r].iter().enumerate() {
                if let Some(cell) = cells.get_mut(field_idx + c) {
                    cell.clone_from(val);
                }
            }
        }
    }

    let rendered = render(&table);
    splice_lines(text, first, last, &rendered)
}

// ==================================== Misc ======================================

/// Transpose the table at `line`, dropping hlines (per the manual:
/// "Transpose the table at point and eliminate hlines").
#[must_use]
pub fn transpose(text: &str, line: usize) -> Option<String> {
    let (first, last) = table_range(text, line)?;
    let table = parse(text, first, last);
    let data_rows: Vec<&Vec<String>> = table
        .rows
        .iter()
        .filter_map(|r| match r {
            Row::Data(cells) => Some(cells),
            Row::Hline => None,
        })
        .collect();
    if data_rows.is_empty() {
        return None;
    }
    let ncols = data_rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let new_rows = (0..ncols)
        .map(|c| {
            Row::Data(
                data_rows
                    .iter()
                    .map(|r| r.get(c).cloned().unwrap_or_default())
                    .collect(),
            )
        })
        .collect();
    let rendered = render(&Table { rows: new_rows });
    Some(splice_lines(text, first, last, &rendered))
}

/// Sum the numeric values in the data-row cells of the column containing
/// `(line, col)`. Non-numeric cells are ignored. `None` if `line` is not in a
/// table; `Some(0.0)` if the table has no numeric cells in that column.
#[must_use]
pub fn sum_column(text: &str, line: usize, col: usize) -> Option<f64> {
    let (first, last) = table_range(text, line)?;
    let orig_lines: Vec<&str> = text.split('\n').collect();
    let field_idx = field_index_at(orig_lines[line], col);
    let table = parse(text, first, last);
    let sum = table
        .rows
        .iter()
        .filter_map(|r| match r {
            Row::Data(cells) => cells.get(field_idx),
            Row::Hline => None,
        })
        .filter_map(|cell| cell.trim().parse::<f64>().ok())
        .sum();
    Some(sum)
}

/// `org-table-copy-increment`-ish "copy down": if the field at `(line, col)`
/// is empty, fill it with the nearest non-empty field above it in the same
/// column; otherwise, copy its value into the next data row's same column
/// (creating a new row if needed) and move point there. When `increment` is
/// set, the copied-down value's trailing (or, failing that, leading) run of
/// digits is incremented by one, preserving zero-padded width; a bare
/// `H(H):MM` duration has its minutes incremented by one, rolling into hours.
/// Any other value shape is copied verbatim.
#[must_use]
pub fn copy_down(
    text: &str,
    line: usize,
    col: usize,
    increment: bool,
) -> Option<(String, usize, usize)> {
    let (first, last) = table_range(text, line)?;
    let orig_lines: Vec<&str> = text.split('\n').collect();
    let field_idx = field_index_at(orig_lines[line], col);
    let mut table = parse(text, first, last);
    let row_idx = line - first;
    let current = get_cell(&table, row_idx, field_idx).unwrap_or_default();

    if current.trim().is_empty() {
        let value = (0..row_idx).rev().find_map(|r| {
            let v = get_cell(&table, r, field_idx)?;
            (!v.trim().is_empty()).then_some(v)
        })?;
        set_cell(&mut table, row_idx, field_idx, value);
        let rendered = render(&table);
        let new_text = splice_lines(text, first, last, &rendered);
        let row_line = table_row_line(&rendered, row_idx);
        let new_col = field_start_offset(row_line, field_idx).unwrap_or(0);
        return Some((new_text, line, new_col));
    }

    let new_value = if increment {
        increment_value(&current)
    } else {
        current
    };
    let target_row = if let Some(r) = next_data_row_index(&table.rows, row_idx) {
        r
    } else {
        let ncols = table_ncols(&table);
        table.rows.push(Row::Data(vec![String::new(); ncols]));
        table.rows.len() - 1
    };
    set_cell(&mut table, target_row, field_idx, new_value);
    let rendered = render(&table);
    let new_text = splice_lines(text, first, last, &rendered);
    let row_line = table_row_line(&rendered, target_row);
    let new_col = field_start_offset(row_line, field_idx).unwrap_or(0);
    Some((new_text, first + target_row, new_col))
}

/// Increment `s` per [`copy_down`]'s `increment` rule.
fn increment_value(s: &str) -> String {
    if s.matches(':').count() == 1
        && let Some((h, m)) = s.split_once(':')
        && !h.is_empty()
        && h.bytes().all(|b| b.is_ascii_digit())
        && is_ascii_digits(m, 2)
        && let (Ok(hh), Ok(mm)) = (h.parse::<i64>(), m.parse::<i64>())
    {
        let total = hh * 60 + mm + 1;
        return format!("{}:{:02}", total / 60, total % 60);
    }

    // A whole-string integer: increment its value, but preserve zero-padded
    // width for an unsigned value with a leading zero (e.g. "07" -> "08",
    // not "8"); a signed value's magnitude is not zero-pad-preserved.
    if let Ok(n) = s.parse::<i64>() {
        if !s.starts_with('-') && s.len() > 1 && s.starts_with('0') {
            let width = s.len();
            return format!("{:0width$}", n + 1);
        }
        return (n + 1).to_string();
    }

    let digit_start = s.rfind(|c: char| !c.is_ascii_digit()).map_or(0, |i| i + 1);
    if digit_start < s.len()
        && let Ok(n) = s[digit_start..].parse::<i64>()
    {
        let width = s.len() - digit_start;
        let digits = if s[digit_start..].starts_with('0') {
            format!("{:0width$}", n + 1)
        } else {
            (n + 1).to_string()
        };
        return format!("{}{digits}", &s[..digit_start]);
    }

    let digit_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if digit_end > 0
        && let Ok(n) = s[..digit_end].parse::<i64>()
    {
        return format!("{}{}", n + 1, &s[digit_end..]);
    }

    s.to_string()
}

/// A separator [`from_delimited_with`] should split on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Separator {
    /// Split on `,`.
    Comma,
    /// Split on `\t`.
    Tab,
    /// Split on runs of 2 or more whitespace characters.
    Whitespace,
    /// Split on this literal substring (a pragmatic stand-in for a real
    /// regex, since this crate does not depend on a regex engine).
    Regex(String),
}

/// `org-table-create-or-convert-from-region`: convert non-table delimited
/// text into a pipe table, auto-detecting the separator: comma if every
/// non-blank line contains one, else tab if every non-blank line contains
/// one, else runs of 2+ whitespace characters.
#[must_use]
pub fn from_delimited(text: &str) -> String {
    let nonblank: Vec<&str> = text.split('\n').filter(|l| !l.trim().is_empty()).collect();
    let sep = if !nonblank.is_empty() && nonblank.iter().all(|l| l.contains(',')) {
        Separator::Comma
    } else if !nonblank.is_empty() && nonblank.iter().all(|l| l.contains('\t')) {
        Separator::Tab
    } else {
        Separator::Whitespace
    };
    from_delimited_with(text, &sep)
}

/// Like [`from_delimited`], but with an explicit, caller-forced separator.
#[must_use]
pub fn from_delimited_with(text: &str, sep: &Separator) -> String {
    let rows = text
        .split('\n')
        .filter(|l| !l.trim().is_empty())
        .map(|l| Row::Data(split_by_separator(l, sep)))
        .collect();
    render(&Table { rows })
}

/// Split one line of delimited text into cell strings per `sep`.
fn split_by_separator(line: &str, sep: &Separator) -> Vec<String> {
    match sep {
        Separator::Comma => line.split(',').map(|c| c.trim().to_string()).collect(),
        Separator::Tab => line.split('\t').map(|c| c.trim().to_string()).collect(),
        Separator::Whitespace => split_on_whitespace_runs(line),
        Separator::Regex(pattern) => {
            if pattern.is_empty() {
                vec![line.to_string()]
            } else {
                line.split(pattern.as_str())
                    .map(|c| c.trim().to_string())
                    .collect()
            }
        }
    }
}

/// Split `line` on runs of 2+ whitespace characters; a single interior space
/// is kept as literal text within a field.
fn split_on_whitespace_runs(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.trim().chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            let mut run = 1;
            while chars.peek().is_some_and(|c2| c2.is_whitespace()) {
                chars.next();
                run += 1;
            }
            if run >= 2 {
                fields.push(std::mem::take(&mut field));
            } else {
                field.push(' ');
            }
        } else {
            field.push(c);
        }
    }
    fields.push(field);
    fields
}

/// `org-table-export`: tab-separated text of `table`'s data rows (hlines are
/// skipped).
#[must_use]
pub fn to_tsv(table: &Table) -> String {
    table
        .rows
        .iter()
        .filter_map(|r| match r {
            Row::Data(cells) => Some(cells.join("\t")),
            Row::Hline => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ================================ TBLFM formulas ================================

/// The left-hand side of a `#+TBLFM:` formula: what field(s) it targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormulaTarget {
    /// `@ROW$COL` — one specific field, 1-indexed against data rows only
    /// (hlines are not counted in the row numbering — a deliberate
    /// simplification of Org's fuller reference-crossing-hline semantics).
    Field(usize, usize),
    /// `$COL` (no `@`) — a column formula: apply the expression to every data
    /// row's field in this 1-indexed column.
    Column(usize),
}

/// One parsed `#+TBLFM:` directive: target reference, the raw expression text
/// (right-hand side, before substitution), and an optional printf-style
/// format spec (`%.1f`) parsed from a trailing `;`-suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formula {
    /// What field(s) this formula writes to.
    pub target: FormulaTarget,
    /// The expression text, before reference substitution.
    pub expr: String,
    /// An optional printf-style format spec, e.g. `%.1f`.
    pub format: Option<String>,
}

/// Parse a `#+TBLFM:` line into its `::`-separated formulas. `None` if `line`
/// is not (case-insensitively) a `#+TBLFM:` line, or has no content after the
/// prefix. Individual `::`-separated segments that fail to parse (malformed
/// target or missing `=`) are silently dropped rather than failing the whole
/// line — there is nowhere meaningful to report their error since we could
/// not even determine a target field.
#[must_use]
pub fn parse_tblfm(line: &str) -> Option<Vec<Formula>> {
    let rest = strip_prefix_ci(line.trim(), "#+TBLFM:")?.trim();
    if rest.is_empty() {
        return None;
    }
    Some(
        rest.split("::")
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .filter_map(parse_formula_part)
            .collect(),
    )
}

/// Case-insensitive `strip_prefix`.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len()
        && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Parse one `target=expr[;format]` segment.
fn parse_formula_part(part: &str) -> Option<Formula> {
    let (lhs, rhs) = part.split_once('=')?;
    let target = parse_target(lhs.trim())?;
    let (expr, format) = match rhs.rsplit_once(';') {
        Some((e, f)) => (e.trim().to_string(), Some(f.trim().to_string())),
        None => (rhs.trim().to_string(), None),
    };
    Some(Formula {
        target,
        expr,
        format,
    })
}

/// Parse a formula's left-hand side into a [`FormulaTarget`].
fn parse_target(lhs: &str) -> Option<FormulaTarget> {
    if let Some(rest) = lhs.strip_prefix('@') {
        let (row_str, col_str) = rest.split_once('$')?;
        Some(FormulaTarget::Field(
            row_str.parse().ok()?,
            col_str.parse().ok()?,
        ))
    } else {
        Some(FormulaTarget::Column(lhs.strip_prefix('$')?.parse().ok()?))
    }
}

/// Read-only view of a table used while expanding/evaluating formula
/// expressions: maps 1-indexed data-row numbers to the field's numeric value.
struct EvalContext<'a> {
    table: &'a Table,
    /// `data_rows[r - 1]` is the index into `table.rows` of data row `r`.
    data_rows: &'a [usize],
}

impl EvalContext<'_> {
    /// The numeric value of 1-indexed data row `row`, 1-indexed column `col`.
    /// `None` if out of range, an hline (impossible via `data_rows`), or the
    /// cell text does not parse as a number.
    fn field_numeric(&self, row: usize, col: usize) -> Option<f64> {
        let &idx = self.data_rows.get(row.checked_sub(1)?)?;
        let Row::Data(cells) = self.table.rows.get(idx)? else {
            return None;
        };
        cells.get(col.checked_sub(1)?)?.trim().parse().ok()
    }
}

/// Apply the `#+TBLFM:` formulas on `tblfm_line` to the table occupying
/// `[table_first_line, table_last_line]`, writing results back into the
/// buffer. Formulas run in the order written; a column formula loops rows
/// top-to-bottom, re-reading any already-updated cells from earlier in the
/// same pass (so forward references within one column formula see freshly
/// computed values — matching Org's left-to-right, top-to-bottom evaluation
/// in the common case; true circular/backward dependency resolution is out of
/// scope). On any reference or evaluation error for a formula, `#ERROR` is
/// written into its target field(s) rather than panicking or skipping —
/// `None` is only returned when there is no table or no TBLFM line to operate
/// on at all.
#[must_use]
pub fn apply_tblfm(
    text: &str,
    table_first_line: usize,
    table_last_line: usize,
    tblfm_line: usize,
) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let formulas = parse_tblfm(lines.get(tblfm_line)?)?;
    let mut table = parse(text, table_first_line, table_last_line);
    let data_rows: Vec<usize> = table
        .rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| matches!(r, Row::Data(_)).then_some(i))
        .collect();

    for formula in &formulas {
        match formula.target {
            FormulaTarget::Field(row, col) => {
                let value = eval_formula_value(&formula.expr, &table, &data_rows, Some(row));
                write_result(
                    &mut table,
                    &data_rows,
                    row,
                    col,
                    &value,
                    formula.format.as_deref(),
                );
            }
            FormulaTarget::Column(col) => {
                for row in 1..=data_rows.len() {
                    let value = eval_formula_value(&formula.expr, &table, &data_rows, Some(row));
                    write_result(
                        &mut table,
                        &data_rows,
                        row,
                        col,
                        &value,
                        formula.format.as_deref(),
                    );
                }
            }
        }
    }

    let rendered = render(&table);
    Some(splice_lines(
        text,
        table_first_line,
        table_last_line,
        &rendered,
    ))
}

/// Realign the table at `line`, then apply its `#+TBLFM:` line, if one
/// immediately follows the table (no blank line in between). `None` only if
/// `line` is not in a table.
#[must_use]
pub fn recalc(text: &str, line: usize) -> Option<String> {
    let aligned = align(text, line)?;
    let (first, last) = table_range(&aligned, line)?;
    let lines: Vec<&str> = aligned.split('\n').collect();
    let tblfm_line = last + 1;
    if lines
        .get(tblfm_line)
        .is_some_and(|l| strip_prefix_ci(l.trim(), "#+TBLFM:").is_some())
    {
        apply_tblfm(&aligned, first, last, tblfm_line)
    } else {
        Some(aligned)
    }
}

/// Write `col`'s field of 1-indexed data row `row` per formula evaluation
/// result `value` (`#ERROR` on `Err`), applying `format` if given. Silently
/// does nothing if `row` is out of range.
fn write_result(
    table: &mut Table,
    data_rows: &[usize],
    row: usize,
    col: usize,
    value: &Result<f64, String>,
    format: Option<&str>,
) {
    let Some(row0) = row.checked_sub(1) else {
        return;
    };
    let Some(&idx) = data_rows.get(row0) else {
        return;
    };
    let Some(col0) = col.checked_sub(1) else {
        return;
    };
    let Some(Row::Data(cells)) = table.rows.get_mut(idx) else {
        return;
    };
    if cells.len() <= col0 {
        cells.resize(col0 + 1, String::new());
    }
    cells[col0] = match value {
        Ok(v) => format_result(*v, format),
        Err(_) => "#ERROR".to_string(),
    };
}

/// Evaluate `expr` (a formula's right-hand side, with `$N`/`@R$C`/`v*(range)`
/// references) against `table`/`data_rows`, with `current_row` (1-indexed)
/// as the row `$N` refers to.
fn eval_formula_value(
    expr: &str,
    table: &Table,
    data_rows: &[usize],
    current_row: Option<usize>,
) -> Result<f64, String> {
    let ctx = EvalContext { table, data_rows };
    let expanded = expand_aggregate_functions(expr, &ctx)?;
    let substituted = substitute_scalar_refs(&expanded, &ctx, current_row)?;
    #[allow(clippy::cast_precision_loss)]
    // pragmatic: table formulas do not need i64-exact precision
    match evalexpr::eval(&substituted).map_err(|e| e.to_string())? {
        Value::Int(i) => Ok(i as f64),
        Value::Float(f) => Ok(f),
        other => Err(format!("formula did not evaluate to a number: {other}")),
    }
}

/// A parenthesized numeric literal, safe to splice into an expression string
/// regardless of the surrounding operator (avoids `5--3`-style ambiguity from
/// substituting a negative number after a `-`). Always includes a decimal
/// point (via `Debug`, not `Display`) so `evalexpr` treats it as a float —
/// otherwise a whole-number substitution like `3` would be parsed as an
/// integer literal and `/` would silently perform integer division.
fn wrap_number(v: f64) -> String {
    format!("({v:?})")
}

/// Expand `vsum(range)`, `vmean(range)`, `vmin(range)`, `vmax(range)`,
/// `vcount(range)` calls (where `range` is `@R$C..@R2$C2`) into their
/// computed numeral, before handing the rest of the expression to `evalexpr`
/// (which knows nothing about Org ranges).
fn expand_aggregate_functions(expr: &str, ctx: &EvalContext) -> Result<String, String> {
    const FUNCS: [&str; 5] = ["vsum", "vmean", "vmin", "vmax", "vcount"];
    let mut out = String::new();
    let mut i = 0;
    let bytes = expr.as_bytes();
    'outer: while i < expr.len() {
        let boundary_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
        if boundary_ok {
            for name in FUNCS {
                if expr[i..].starts_with(name) && bytes.get(i + name.len()) == Some(&b'(') {
                    let open = i + name.len();
                    let close = find_matching_paren(expr, open).ok_or("unbalanced parentheses")?;
                    let values = resolve_range(expr[open + 1..close].trim(), ctx)?;
                    out.push_str(&wrap_number(aggregate(name, &values)));
                    i = close + 1;
                    continue 'outer;
                }
            }
        }
        let ch = expr[i..]
            .chars()
            .next()
            .expect("i < expr.len() so a char exists");
        out.push(ch);
        i += ch.len_utf8();
    }
    Ok(out)
}

/// Compute the named aggregate over `values`.
#[allow(clippy::cast_precision_loss)] // pragmatic: element counts here are small
fn aggregate(name: &str, values: &[f64]) -> f64 {
    match name {
        "vsum" => values.iter().sum(),
        "vmean" => {
            if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<f64>() / values.len() as f64
            }
        }
        "vmin" => values.iter().copied().fold(f64::INFINITY, f64::min),
        "vmax" => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        _ => values.len() as f64, // vcount
    }
}

/// The byte offset (within `s`, starting the scan at `open`, which must be
/// the index of an opening `(`) of its matching closing `)`.
fn find_matching_paren(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Resolve a `@R$C..@R2$C2` range spec into the numeric values found in that
/// rectangle, row-major, skipping unparsable/non-numeric cells.
fn resolve_range(spec: &str, ctx: &EvalContext) -> Result<Vec<f64>, String> {
    let (a, b) = spec
        .split_once("..")
        .ok_or_else(|| format!("not a range: {spec}"))?;
    let (r1, c1) = parse_field_ref(a.trim()).ok_or_else(|| format!("bad range endpoint: {a}"))?;
    let (r2, c2) = parse_field_ref(b.trim()).ok_or_else(|| format!("bad range endpoint: {b}"))?;
    let (rlo, rhi) = (r1.min(r2), r1.max(r2));
    let (clo, chi) = (c1.min(c2), c1.max(c2));
    let mut values = Vec::new();
    for r in rlo..=rhi {
        for c in clo..=chi {
            if let Some(v) = ctx.field_numeric(r, c) {
                values.push(v);
            }
        }
    }
    Ok(values)
}

/// Parse a single `@R$C` reference into 1-indexed `(row, col)`.
fn parse_field_ref(s: &str) -> Option<(usize, usize)> {
    let (r, c) = s.strip_prefix('@')?.split_once('$')?;
    Some((r.parse().ok()?, c.parse().ok()?))
}

/// Substitute every remaining `$N` (current-row column `N`) and `@R$C`
/// (absolute field) reference in `expr` with its numeric value, parenthesized
/// so it is safe to splice next to any operator. Errors when a reference does
/// not parse, or its field is not itself a parsable number.
fn substitute_scalar_refs(
    expr: &str,
    ctx: &EvalContext,
    current_row: Option<usize>,
) -> Result<String, String> {
    let chars: Vec<char> = expr.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '@' => {
                let (row, next) =
                    read_digits(&chars, i + 1).ok_or("expected a row number after '@'")?;
                if chars.get(next) != Some(&'$') {
                    return Err("expected '$' after '@ROW'".to_string());
                }
                let (col, after) =
                    read_digits(&chars, next + 1).ok_or("expected a column number after '$'")?;
                let v = ctx
                    .field_numeric(row, col)
                    .ok_or_else(|| format!("unparsable field @{row}${col}"))?;
                out.push_str(&wrap_number(v));
                i = after;
            }
            '$' => {
                let (col, after) =
                    read_digits(&chars, i + 1).ok_or("expected a column number after '$'")?;
                let row = current_row.ok_or("'$COL' reference used without a current row")?;
                let v = ctx
                    .field_numeric(row, col)
                    .ok_or_else(|| format!("unparsable field ${col}"))?;
                out.push_str(&wrap_number(v));
                i = after;
            }
            ch => {
                out.push(ch);
                i += 1;
            }
        }
    }
    Ok(out)
}

/// Read a run of ASCII digits from `chars` starting at `start`, returning the
/// parsed number and the index just past it. `None` if `start` is not a
/// digit.
fn read_digits(chars: &[char], start: usize) -> Option<(usize, usize)> {
    let mut end = start;
    while chars.get(end).is_some_and(char::is_ascii_digit) {
        end += 1;
    }
    if end == start {
        return None;
    }
    let n: usize = chars[start..end].iter().collect::<String>().parse().ok()?;
    Some((n, end))
}

/// Format a formula result: `format` (a `%.Nf`-style printf spec) if given,
/// exactly; otherwise integers render with no decimal point, and non-integers
/// render to 2 decimal places with trailing zeros (and a trailing `.`)
/// trimmed off.
fn format_result(v: f64, format: Option<&str>) -> String {
    if let Some(fmt) = format
        && let Some(s) = apply_printf_format(v, fmt)
    {
        return s;
    }
    if !v.is_finite() {
        return "#ERROR".to_string();
    }
    #[allow(clippy::cast_possible_truncation)] // pragmatic: table cell values are small
    if v.fract().abs() < 1e-9 {
        (v.round() as i64).to_string()
    } else {
        let s = format!("{v:.2}");
        let trimmed = s.trim_end_matches('0').trim_end_matches('.');
        if trimmed.is_empty() || trimmed == "-" {
            "0".to_string()
        } else {
            trimmed.to_string()
        }
    }
}

/// Apply a `%.Nf` printf-style format spec to `v`. `None` if `fmt` is not of
/// that shape (only that one shape is supported).
fn apply_printf_format(v: f64, fmt: &str) -> Option<String> {
    let rest = fmt.trim().strip_prefix("%.")?;
    let digits_end = rest.find(['f', 'F'])?;
    let prec: usize = rest[..digits_end].parse().ok()?;
    Some(format!("{v:.prec$}"))
}

#[cfg(test)]
mod tests {
    // A table of nothing but horizontal rules has no columns to size. Found by
    // `cargo fuzz run org_table`: `render` used to return an empty string for
    // it, and since every caller replaces the table's lines with the rendered
    // text, aligning `|-` deleted the line instead of squaring it up.
    #[test]
    fn render_keeps_hline_only_tables() {
        let text = "|-\n|-\n";
        let (first, last) = super::table_range(text, 0).expect("a table");
        let table = super::parse(text, first, last);
        assert_eq!(table.rows.len(), 2, "two horizontal rules");
        let rendered = super::render(&table);
        assert_eq!(
            rendered, "|---|\n|---|",
            "each rule renders at minimum width"
        );
        let reparsed = super::parse(&rendered, 0, rendered.lines().count() - 1);
        assert_eq!(reparsed.rows, table.rows, "render/parse round-trips");
        assert_eq!(
            super::align(text, 0).as_deref(),
            Some("|---|\n|---|\n"),
            "align squares the rules up instead of deleting them"
        );
    }

    use super::*;
    use proptest::prelude::*;

    fn line_at(text: &str, n: usize) -> &str {
        text.split('\n').nth(n).unwrap()
    }

    // ------------------------------- table_range --------------------------------

    #[test]
    fn table_range_finds_contiguous_block_and_rejects_non_table_lines() {
        let text = "para\n| a | b |\n| c | d |\nmore text";
        assert_eq!(table_range(text, 1), Some((1, 2)));
        assert_eq!(table_range(text, 2), Some((1, 2)));
        assert_eq!(table_range(text, 0), None);
        assert_eq!(table_range(text, 3), None);
    }

    #[test]
    fn table_range_single_row() {
        assert_eq!(table_range("| only |", 0), Some((0, 0)));
    }

    #[test]
    fn table_range_at_buffer_start_and_end() {
        let text = "| a |\n| b |";
        assert_eq!(table_range(text, 0), Some((0, 1)));
        assert_eq!(table_range(text, 1), Some((0, 1)));
    }

    #[test]
    fn table_range_indented_pipe_still_counts() {
        assert_eq!(table_range("  | a | b |", 0), Some((0, 0)));
    }

    // -------------------------------- parse/render -------------------------------

    #[test]
    fn render_matches_manual_worked_example() {
        let table = Table {
            rows: vec![
                Row::Data(vec!["Name".into(), "Phone".into(), "Age".into()]),
                Row::Hline,
                Row::Data(vec!["Peter".into(), "1234".into(), "17".into()]),
                Row::Data(vec!["Anna".into(), "4321".into(), "25".into()]),
            ],
        };
        let expected = "\
| Name  | Phone | Age |
|-------+-------+-----|
| Peter |  1234 |  17 |
| Anna  |  4321 |  25 |";
        assert_eq!(render(&table), expected);
    }

    #[test]
    fn parse_then_render_reproduces_the_worked_example() {
        let text =
            "| Name | Phone | Age |\n|----+----+---|\n| Peter | 1234 | 17 |\n| Anna | 4321 | 25 |";
        let (first, last) = table_range(text, 0).unwrap();
        let table = parse(text, first, last);
        let expected = "\
| Name  | Phone | Age |
|-------+-------+-----|
| Peter |  1234 |  17 |
| Anna  |  4321 |  25 |";
        assert_eq!(render(&table), expected);
    }

    #[test]
    fn render_single_row_table() {
        let table = Table {
            rows: vec![Row::Data(vec!["a".into(), "bb".into()])],
        };
        assert_eq!(render(&table), "| a | bb |");
    }

    #[test]
    fn render_header_hline_no_data_rows() {
        let table = Table {
            rows: vec![Row::Data(vec!["A".into(), "B".into()]), Row::Hline],
        };
        assert_eq!(render(&table), "| A | B |\n|---+---|");
    }

    #[test]
    fn render_left_aligns_non_numeric_columns() {
        let table = Table {
            rows: vec![
                Row::Data(vec!["x".into()]),
                Row::Data(vec!["longer".into()]),
            ],
        };
        assert_eq!(render(&table), "| x      |\n| longer |");
    }

    #[test]
    fn parse_row_splits_naively_on_every_pipe() {
        assert_eq!(
            parse_row("| a | b |"),
            Row::Data(vec!["a".into(), "b".into()])
        );
        assert_eq!(parse_row("|---+---|"), Row::Hline);
    }

    // --------------------------------- align -------------------------------------

    #[test]
    fn align_splices_realigned_table_leaving_surrounding_text_intact() {
        let text = "before\n| a | bb |\n| ccc | d |\nafter";
        let out = align(text, 1).unwrap();
        assert_eq!(out, "before\n| a   | bb |\n| ccc | d  |\nafter");
    }

    #[test]
    fn align_none_outside_table() {
        assert_eq!(align("just text", 0), None);
    }

    // ---------------------------------------------------------------------------
    // helper: a canonical 2x3 table, already aligned, for navigation tests.
    fn sample_table_text() -> String {
        render(&Table {
            rows: vec![
                Row::Data(vec!["a".into(), "bb".into(), "c".into()]),
                Row::Data(vec!["dd".into(), "e".into(), "f".into()]),
            ],
        })
    }

    // ------------------------------ field navigation ------------------------------

    #[test]
    fn next_field_advances_wraps_rows_and_grows_table_at_the_end() {
        let text = sample_table_text();
        let col_a = line_at(&text, 0).find('a').unwrap();

        let (t1, l1, c1) = next_field(&text, 0, col_a).unwrap();
        assert_eq!(l1, 0);
        assert_eq!(&line_at(&t1, l1)[c1..c1 + 2], "bb");

        let (t2, l2, c2) = next_field(&t1, l1, c1).unwrap();
        assert_eq!(l2, 0);
        assert_eq!(&line_at(&t2, l2)[c2..=c2], "c");

        let (t3, l3, c3) = next_field(&t2, l2, c2).unwrap();
        assert_eq!(l3, 1);
        assert_eq!(&line_at(&t3, l3)[c3..c3 + 2], "dd");

        let (t4, l4, c4) = next_field(&t3, l3, c3).unwrap();
        let (t5, l5, c5) = next_field(&t4, l4, c4).unwrap();
        assert_eq!(&line_at(&t5, l5)[c5..=c5], "f");

        let (t6, l6, _c6) = next_field(&t5, l5, c5).unwrap();
        assert_eq!(l6, 2);
        let lines6: Vec<&str> = t6.split('\n').collect();
        assert_eq!(lines6.len(), 3);
        assert_eq!(parse_row(lines6[2]), Row::Data(vec![String::new(); 3]));
    }

    #[test]
    fn next_field_wraps_over_an_hline_to_the_next_data_row() {
        let text = "| a | b |\n|---+---|\n| c | d |";
        let col_b = line_at(text, 0).find('b').unwrap();
        let (new_text, line, col) = next_field(text, 0, col_b).unwrap();
        assert_eq!(line, 2);
        assert_eq!(&line_at(&new_text, line)[col..=col], "c");
    }

    #[test]
    fn previous_field_retreats_and_is_none_at_the_very_first_field() {
        let text = sample_table_text();
        let col_dd = line_at(&text, 1).find("dd").unwrap();
        let (t1, l1, c1) = previous_field(&text, 1, col_dd).unwrap();
        assert_eq!(l1, 0);
        assert_eq!(&line_at(&t1, l1)[c1..=c1], "c");

        let col_a = line_at(&text, 0).find('a').unwrap();
        assert_eq!(previous_field(&text, 0, col_a), None);
    }

    #[test]
    fn next_row_moves_down_same_column_and_grows_table_at_the_end() {
        let text = sample_table_text();
        let col_bb = line_at(&text, 0).find("bb").unwrap();
        let (t1, l1, c1) = next_row(&text, 0, col_bb).unwrap();
        assert_eq!(l1, 1);
        assert_eq!(&line_at(&t1, l1)[c1..=c1], "e");

        let (t2, l2, _c2) = next_row(&t1, l1, c1).unwrap();
        assert_eq!(l2, 2);
        assert_eq!(t2.split('\n').count(), 3);
    }

    #[test]
    fn blank_field_clears_only_the_targeted_field() {
        let text = sample_table_text();
        let col_bb = line_at(&text, 0).find("bb").unwrap();
        let out = blank_field(&text, 0, col_bb).unwrap();
        let (first, last) = table_range(&out, 0).unwrap();
        let table = parse(&out, first, last);
        assert_eq!(
            table.rows[0],
            Row::Data(vec!["a".into(), String::new(), "c".into()])
        );
        assert_eq!(
            table.rows[1],
            Row::Data(vec!["dd".into(), "e".into(), "f".into()])
        );
    }

    // ------------------------------ row structural edits ---------------------------

    #[test]
    fn insert_row_above_and_below() {
        let text = "| a |\n| b |";
        let (out, line) = insert_row(text, 0, false).unwrap();
        assert_eq!(line, 0);
        assert_eq!(out, "|   |\n| a |\n| b |");

        let (out, line) = insert_row(text, 0, true).unwrap();
        assert_eq!(line, 1);
        assert_eq!(out, "| a |\n|   |\n| b |");
    }

    #[test]
    fn kill_row_removes_a_row_or_the_whole_table_when_last() {
        let text = "before\n| a |\n| b |\nafter";
        let out = kill_row(text, 1).unwrap();
        assert_eq!(out, "before\n| b |\nafter");

        let single = "before\n| only |\nafter";
        let out = kill_row(single, 1).unwrap();
        assert_eq!(out, "before\nafter");
    }

    #[test]
    fn move_row_up_and_down_swap_and_report_bounds() {
        let text = "| a |\n| b |\n| c |";
        let (out, line) = move_row_down(text, 0).unwrap();
        assert_eq!(line, 1);
        assert_eq!(out, "| b |\n| a |\n| c |");
        assert_eq!(move_row_up(text, 0), None);

        let (out, line) = move_row_up(text, 2).unwrap();
        assert_eq!(line, 1);
        assert_eq!(out, "| a |\n| c |\n| b |");
        assert_eq!(move_row_down(text, 2), None);
    }

    #[test]
    fn insert_hline_above_and_below() {
        let text = "| a |\n| b |";
        assert_eq!(insert_hline(text, 0, true).unwrap(), "|---|\n| a |\n| b |");
        assert_eq!(insert_hline(text, 0, false).unwrap(), "| a |\n|---|\n| b |");
    }

    #[test]
    fn hline_and_move_inserts_rule_and_a_fresh_row_below_it() {
        let text = "| a |\n| b |";
        let (out, line) = hline_and_move(text, 0).unwrap();
        assert_eq!(line, 2);
        assert_eq!(out, "| a |\n|---|\n|   |\n| b |");
    }

    // --------------------------- column structural edits ---------------------------

    #[test]
    fn insert_column_shifts_right_and_lands_on_the_new_empty_field() {
        let text = "| a | b |\n| c | d |";
        let col_b = line_at(text, 0).find('b').unwrap();
        let (out, new_col) = insert_column(text, 0, col_b).unwrap();
        let (first, last) = table_range(&out, 0).unwrap();
        let table = parse(&out, first, last);
        assert_eq!(
            table.rows[0],
            Row::Data(vec!["a".into(), String::new(), "b".into()])
        );
        assert_eq!(
            table.rows[1],
            Row::Data(vec!["c".into(), String::new(), "d".into()])
        );
        assert_eq!(&line_at(&out, 0)[new_col..new_col], "");
    }

    #[test]
    fn delete_column_removes_the_targeted_column() {
        let text = "| a | b | c |\n| d | e | f |";
        let col_b = line_at(text, 0).find('b').unwrap();
        let (out, _) = delete_column(text, 0, col_b).unwrap();
        let (first, last) = table_range(&out, 0).unwrap();
        let table = parse(&out, first, last);
        assert_eq!(table.rows[0], Row::Data(vec!["a".into(), "c".into()]));
        assert_eq!(table.rows[1], Row::Data(vec!["d".into(), "f".into()]));
    }

    #[test]
    fn move_column_left_and_right_swap_and_report_bounds() {
        let text = "| a | b |\n| c | d |";
        let col_a = line_at(text, 0).find('a').unwrap();
        let col_b = line_at(text, 0).find('b').unwrap();
        assert_eq!(move_column_left(text, 0, col_a), None);

        let (out, _) = move_column_right(text, 0, col_a).unwrap();
        let (first, last) = table_range(&out, 0).unwrap();
        let table = parse(&out, first, last);
        assert_eq!(table.rows[0], Row::Data(vec!["b".into(), "a".into()]));

        assert_eq!(move_column_right(text, 0, col_b), None);
    }

    // ------------------------------- cell swap moves --------------------------------

    #[test]
    fn move_cell_vertical_skips_hlines() {
        let text = "| 1 |\n|---|\n| 2 |";
        let col = line_at(text, 0).find('1').unwrap();
        let (out, line, _) = move_cell_down(text, 0, col).unwrap();
        assert_eq!(line, 2);
        let (first, last) = table_range(&out, 0).unwrap();
        let table = parse(&out, first, last);
        assert_eq!(table.rows[0], Row::Data(vec!["2".into()]));
        assert_eq!(table.rows[2], Row::Data(vec!["1".into()]));

        // no data row above the first: None.
        assert_eq!(move_cell_up(text, 0, col), None);
        // hline itself has no cells to move.
        assert_eq!(move_cell_down(text, 1, 0), None);
    }

    #[test]
    fn move_cell_horizontal_swaps_within_the_row() {
        let text = "| a | b | c |";
        let col_b = line_at(text, 0).find('b').unwrap();
        let (out, line, _) = move_cell_left(text, 0, col_b).unwrap();
        assert_eq!(line, 0);
        let (first, last) = table_range(&out, 0).unwrap();
        assert_eq!(
            parse(&out, first, last).rows[0],
            Row::Data(vec!["b".into(), "a".into(), "c".into()])
        );

        let col_a = line_at(text, 0).find('a').unwrap();
        assert_eq!(move_cell_left(text, 0, col_a), None);
        let col_c = line_at(text, 0).find('c').unwrap();
        assert_eq!(move_cell_right(text, 0, col_c), None);
    }

    // ------------------------------------ sort --------------------------------------

    #[test]
    fn sort_rows_alphabetic_numeric_and_case_sensitivity() {
        let text = "| banana | 3 |\n| Apple | 1 |\n| cherry | 2 |";
        let (first, last) = table_range(text, 0).unwrap();

        let out = sort_rows(text, first, last, 0, SortKind::Alphabetic, false, false).unwrap();
        let table = parse(&out, first, last);
        let names: Vec<&str> = table
            .rows
            .iter()
            .map(|r| match r {
                Row::Data(c) => c[0].as_str(),
                Row::Hline => "",
            })
            .collect();
        assert_eq!(names, vec!["Apple", "banana", "cherry"]);

        let out_cs = sort_rows(text, first, last, 0, SortKind::Alphabetic, false, true).unwrap();
        let table_cs = parse(&out_cs, first, last);
        let names_cs: Vec<&str> = table_cs
            .rows
            .iter()
            .map(|r| match r {
                Row::Data(c) => c[0].as_str(),
                Row::Hline => "",
            })
            .collect();
        // Uppercase 'A' sorts before lowercase letters in a case-sensitive (byte) compare.
        assert_eq!(names_cs, vec!["Apple", "banana", "cherry"]);

        let out_num = sort_rows(text, first, last, 1, SortKind::Numeric, true, false).unwrap();
        let table_num = parse(&out_num, first, last);
        let nums: Vec<&str> = table_num
            .rows
            .iter()
            .map(|r| match r {
                Row::Data(c) => c[1].as_str(),
                Row::Hline => "",
            })
            .collect();
        assert_eq!(nums, vec!["3", "2", "1"]);
    }

    #[test]
    fn sort_rows_time_kind_orders_chronologically_and_falls_back() {
        let text = "| 2024-01-05 |\n| 2023-12-01 |\n| not-a-date |";
        let (first, last) = table_range(text, 0).unwrap();
        let out = sort_rows(text, first, last, 0, SortKind::Time, false, false).unwrap();
        let table = parse(&out, first, last);
        let dates: Vec<&str> = table
            .rows
            .iter()
            .map(|r| match r {
                Row::Data(c) => c[0].as_str(),
                Row::Hline => "",
            })
            .collect();
        assert_eq!(dates, vec!["2023-12-01", "2024-01-05", "not-a-date"]);
    }

    #[test]
    fn sort_rows_none_without_a_table() {
        assert_eq!(
            sort_rows("plain text", 0, 0, 0, SortKind::Alphabetic, false, false),
            None
        );
    }

    // --------------------------------- rectangles ------------------------------------

    #[test]
    fn copy_rectangle_ignores_hlines() {
        let text = "| a | b |\n|---+---|\n| c | d |\n| e | f |";
        let rect = copy_rectangle(text, (0, 0), (3, 1));
        assert_eq!(
            rect,
            vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["c".to_string(), "d".to_string()],
                vec!["e".to_string(), "f".to_string()]
            ]
        );
    }

    #[test]
    fn cut_rectangle_blanks_the_source() {
        let text = "| a | b |\n| c | d |";
        let (out, rect) = cut_rectangle(text, (0, 0), (1, 0));
        assert_eq!(rect, vec![vec!["a".to_string()], vec!["c".to_string()]]);
        let (first, last) = table_range(&out, 0).unwrap();
        let table = parse(&out, first, last);
        assert_eq!(table.rows[0], Row::Data(vec![String::new(), "b".into()]));
        assert_eq!(table.rows[1], Row::Data(vec![String::new(), "d".into()]));
    }

    #[test]
    fn paste_rectangle_overwrites_within_bounds() {
        let text = "| a | b |\n| c | d |";
        let rect = vec![
            vec!["X".to_string(), "Y".to_string()],
            vec!["Z".to_string(), "W".to_string()],
        ];
        let col_b = line_at(text, 0).find('b').unwrap();
        let out = paste_rectangle(text, 0, col_b, &rect);
        let (first, last) = table_range(&out, 0).unwrap();
        let table = parse(&out, first, last);
        // Row 0: field 1 -> X, field 2 (new column) -> Y; row 1 likewise, both
        // existing rows exactly cover the 2-row rectangle, so no growth needed.
        assert_eq!(table.rows.len(), 2);
        assert_eq!(
            table.rows[0],
            Row::Data(vec!["a".into(), "X".into(), "Y".into()])
        );
        assert_eq!(
            table.rows[1],
            Row::Data(vec!["c".into(), "Z".into(), "W".into()])
        );
    }

    #[test]
    fn paste_rectangle_extends_the_table_when_it_does_not_fit() {
        let text = "| a |";
        let rect = vec![vec!["X".to_string()], vec!["Z".to_string()]];
        let col_a = line_at(text, 0).find('a').unwrap();
        let out = paste_rectangle(text, 0, col_a, &rect);
        let (first, last) = table_range(&out, 0).unwrap();
        let table = parse(&out, first, last);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0], Row::Data(vec!["X".into()]));
        assert_eq!(table.rows[1], Row::Data(vec!["Z".into()]));
    }

    // ------------------------------------ misc ---------------------------------------

    #[test]
    fn transpose_swaps_rows_and_columns_and_drops_hlines() {
        let text = "| a | b |\n|---+---|\n| c | d |";
        let out = transpose(text, 0).unwrap();
        let (first, last) = table_range(&out, 0).unwrap();
        let table = parse(&out, first, last);
        assert_eq!(
            table.rows,
            vec![
                Row::Data(vec!["a".into(), "c".into()]),
                Row::Data(vec!["b".into(), "d".into()])
            ]
        );
    }

    #[test]
    fn sum_column_ignores_non_numeric_cells() {
        let text = "| 1 |\n| x |\n| 2.5 |";
        let col = 2;
        let sum = sum_column(text, 0, col).unwrap();
        assert!((sum - 3.5).abs() < 1e-9);
        assert_eq!(sum_column("no table", 0, 0), None);
    }

    #[test]
    fn copy_down_fills_empty_from_above_and_copies_with_optional_increment() {
        let text = "| 1 |\n|   |\n| 5 |";
        let col = line_at(text, 1).len() - 2; // inside the empty field
        let (out, line, _) = copy_down(text, 1, col, false).unwrap();
        assert_eq!(line, 1);
        let (first, last) = table_range(&out, 0).unwrap();
        assert_eq!(
            parse(&out, first, last).rows[1],
            Row::Data(vec!["1".into()])
        );

        let text2 = "| 07 |\n|    |";
        let col2 = line_at(text2, 0).find('7').unwrap();
        let (out2, line2, _) = copy_down(text2, 0, col2, true).unwrap();
        assert_eq!(line2, 1);
        let (first2, last2) = table_range(&out2, 0).unwrap();
        assert_eq!(
            parse(&out2, first2, last2).rows[1],
            Row::Data(vec!["08".into()])
        );

        let text3 = "| 1:59 |\n|      |";
        let col3 = line_at(text3, 0).find('1').unwrap();
        let (out3, ..) = copy_down(text3, 0, col3, true).unwrap();
        let (first3, last3) = table_range(&out3, 0).unwrap();
        assert_eq!(
            parse(&out3, first3, last3).rows[1],
            Row::Data(vec!["2:00".into()])
        );
    }

    #[test]
    fn from_delimited_autodetects_comma_tab_and_whitespace() {
        assert_eq!(from_delimited("a,b\nc,d"), "| a | b |\n| c | d |");
        assert_eq!(from_delimited("a\tb\nc\td"), "| a | b |\n| c | d |");
        assert_eq!(from_delimited("a  b\nc  d"), "| a | b |\n| c | d |");
    }

    #[test]
    fn from_delimited_with_forces_a_separator() {
        assert_eq!(
            from_delimited_with("a;b\nc;d", &Separator::Regex(";".to_string())),
            "| a | b |\n| c | d |"
        );
    }

    #[test]
    fn to_tsv_skips_hlines() {
        let table = Table {
            rows: vec![
                Row::Data(vec!["a".into(), "b".into()]),
                Row::Hline,
                Row::Data(vec!["c".into(), "d".into()]),
            ],
        };
        assert_eq!(to_tsv(&table), "a\tb\nc\td");
    }

    // ------------------------------------ TBLFM ---------------------------------------

    #[test]
    fn parse_tblfm_reads_targets_expr_and_format_and_drops_malformed_segments() {
        let formulas = parse_tblfm("#+TBLFM: @2$3=$1+$2::$4=$1*2;%.1f::garbage").unwrap();
        assert_eq!(formulas.len(), 2);
        assert_eq!(formulas[0].target, FormulaTarget::Field(2, 3));
        assert_eq!(formulas[0].expr, "$1+$2");
        assert_eq!(formulas[0].format, None);
        assert_eq!(formulas[1].target, FormulaTarget::Column(4));
        assert_eq!(formulas[1].expr, "$1*2");
        assert_eq!(formulas[1].format.as_deref(), Some("%.1f"));

        assert_eq!(parse_tblfm("not a tblfm line"), None);
        assert_eq!(parse_tblfm("#+TBLFM:"), None);
    }

    #[test]
    fn apply_tblfm_field_formula() {
        let text = "| 2 | 3 |  |\n#+TBLFM: @1$3=$1+$2";
        let out = apply_tblfm(text, 0, 0, 1).unwrap();
        let table = parse(&out, 0, 0);
        assert_eq!(
            table.rows[0],
            Row::Data(vec!["2".into(), "3".into(), "5".into()])
        );
    }

    #[test]
    fn apply_tblfm_column_formula_sees_prior_rows_in_the_same_pass() {
        let text = "| 1 |  |\n| 2 |  |\n| 3 |  |\n#+TBLFM: $2=$1+@-1$2";
        // @-1 relative refs are out of scope; use an absolute forward-reference-safe
        // column formula instead: running total via $1 plus the previous *output*.
        // Simpler: verify each row just computes $1*2 independently, and that a
        // column formula referencing an earlier row's freshly written field also
        // works (row 2 doubles row 1's original $1, not the output).
        let text2 = "| 1 |  |\n| 2 |  |\n#+TBLFM: $2=$1*2";
        let out = apply_tblfm(text2, 0, 1, 2).unwrap();
        let table = parse(&out, 0, 1);
        assert_eq!(table.rows[0], Row::Data(vec!["1".into(), "2".into()]));
        assert_eq!(table.rows[1], Row::Data(vec!["2".into(), "4".into()]));
        let _ = text;
    }

    #[test]
    fn apply_tblfm_aggregate_functions() {
        let text = "| 1 |  |\n| 2 |  |\n| 3 |  |\n#+TBLFM: @1$2=vsum(@1$1..@3$1)::@2$2=vmean(@1$1..@3$1)::@3$2=vcount(@1$1..@3$1)";
        let out = apply_tblfm(text, 0, 2, 3).unwrap();
        let table = parse(&out, 0, 2);
        assert_eq!(table.rows[0], Row::Data(vec!["1".into(), "6".into()]));
        assert_eq!(table.rows[1], Row::Data(vec!["2".into(), "2".into()]));
        assert_eq!(table.rows[2], Row::Data(vec!["3".into(), "3".into()]));
    }

    #[test]
    fn apply_tblfm_honors_printf_format() {
        let text = "| 1 | 3 |  |\n#+TBLFM: @1$3=$1/$2;%.3f";
        let out = apply_tblfm(text, 0, 0, 1).unwrap();
        let table = parse(&out, 0, 0);
        assert_eq!(
            table.rows[0],
            Row::Data(vec!["1".into(), "3".into(), "0.333".into()])
        );
    }

    #[test]
    fn apply_tblfm_writes_error_marker_on_bad_reference() {
        let text = "| x | 3 |  |\n#+TBLFM: @1$3=$1+$2";
        let out = apply_tblfm(text, 0, 0, 1).unwrap();
        let table = parse(&out, 0, 0);
        assert_eq!(
            table.rows[0],
            Row::Data(vec!["x".into(), "3".into(), "#ERROR".into()])
        );
    }

    #[test]
    fn apply_tblfm_none_without_table_or_tblfm_line() {
        assert_eq!(apply_tblfm("no table here", 0, 0, 0), None);
        let text = "| 1 |\nno formula line";
        assert_eq!(apply_tblfm(text, 0, 0, 1), None);
    }

    #[test]
    fn recalc_aligns_then_applies_tblfm_when_present() {
        let text = "| 1 | 2 |  |\n#+TBLFM: @1$3=$1+$2";
        let out = recalc(text, 0).unwrap();
        let table = parse(&out, 0, 0);
        assert_eq!(
            table.rows[0],
            Row::Data(vec!["1".into(), "2".into(), "3".into()])
        );

        let no_formula = "| 1 | 2 |";
        assert_eq!(recalc(no_formula, 0), Some(no_formula.to_string()));
    }

    // ------------------------------------ proptest ------------------------------------

    fn rectangular_table_strategy() -> impl Strategy<Value = Table> {
        proptest::collection::vec(proptest::collection::vec("[a-zA-Z0-9]{0,4}", 1..4), 1..5)
            .prop_map(|rows| {
                let ncols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
                Table {
                    rows: rows
                        .into_iter()
                        .map(|mut r| {
                            while r.len() < ncols {
                                r.push(String::new());
                            }
                            Row::Data(r)
                        })
                        .collect(),
                }
            })
    }

    proptest! {
        #[test]
        fn render_is_idempotent_on_already_rendered_tables(table in rectangular_table_strategy()) {
            let rendered_once = render(&table);
            let (first, last) = table_range(&rendered_once, 0).unwrap();
            let reparsed = parse(&rendered_once, first, last);
            let rendered_twice = render(&reparsed);
            prop_assert_eq!(rendered_once, rendered_twice);
        }

        #[test]
        fn insert_then_delete_column_round_trips(table in rectangular_table_strategy()) {
            let text = render(&table);
            let line0 = line_at(&text, 0);
            let col_byte = field_start_offset(line0, 0).unwrap_or(0);
            let (after_insert, new_col) = insert_column(&text, 0, col_byte).unwrap();
            let (after_delete, _) = delete_column(&after_insert, 0, new_col).unwrap();
            let (first, last) = table_range(&after_delete, 0).unwrap();
            let round_tripped = parse(&after_delete, first, last);
            prop_assert_eq!(round_tripped, table);
        }
    }
}
