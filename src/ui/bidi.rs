//! Right-to-left reordering of the drawn frame (T564).
//!
//! Terminals paint cells left to right, so Arabic arrives in logical order and
//! reads backwards. After a frame is drawn this pass finds each stretch of text
//! that contains a right-to-left character and permutes its cells into UAX #9
//! visual order, in place. Stretches end at box-drawing characters and at runs
//! of two or more spaces, so borders, panes and column gaps stay put. A cell
//! keeps its style when it moves; a wide character keeps its trailing cell.

use ratatui::buffer::{Buffer, Cell};
use vix_locale_model::visual_order;

/// Whether the pass should run: `bidi = "on"`, or `"auto"` with a
/// right-to-left UI language.
pub(super) fn enabled(setting: &str, locale: &str) -> bool {
    match setting {
        "on" => true,
        "off" => false,
        _ => vix_locale_model::is_rtl(locale),
    }
}

/// Whether the layout is mirrored: `mirror_layout = "on"`, or `"auto"` with a
/// right-to-left UI language.
pub(super) fn mirrored(setting: &str, locale: &str) -> bool {
    enabled(setting, locale)
}

fn is_border(c: char) -> bool {
    ('\u{2500}'..='\u{259F}').contains(&c)
}

/// One screen unit: a cell plus the empty continuation cells of a wide glyph.
fn unit_char(cells: &[Cell]) -> char {
    cells[0].symbol().chars().next().unwrap_or(' ')
}

/// Reorder every right-to-left stretch in `buf`.
pub(super) fn reorder_buffer(buf: &mut Buffer) {
    let area = buf.area;
    for y in area.top()..area.bottom() {
        let cells: Vec<Cell> = (area.left()..area.right())
            .map(|x| buf[(x, y)].clone())
            .collect();
        let mut out = cells.clone();
        let mut changed = false;
        let mut x = 0;
        while x < cells.len() {
            if is_border(unit_char(&cells[x..])) {
                x += 1;
                continue;
            }
            // Collect units of one stretch.
            let start = x;
            let mut units: Vec<(usize, usize)> = Vec::new();
            let mut spaces = 0;
            while x < cells.len() {
                let c = unit_char(&cells[x..]);
                if is_border(c) {
                    break;
                }
                spaces = if c == ' ' { spaces + 1 } else { 0 };
                if spaces >= 2 {
                    break;
                }
                let mut end = x + 1;
                while end < cells.len() && cells[end].symbol().is_empty() {
                    end += 1;
                }
                units.push((x, end));
                x = end;
            }
            if units.is_empty() {
                x = start + 1;
                continue;
            }
            let chars: Vec<char> = units.iter().map(|&(s, _)| unit_char(&cells[s..])).collect();
            let order = visual_order(&chars);
            if order.iter().enumerate().all(|(i, &o)| i == o) {
                continue;
            }
            changed = true;
            let mut at = start;
            for &src in &order {
                let (s, e) = units[src];
                for cell in &cells[s..e] {
                    out[at] = cell.clone();
                    at += 1;
                }
            }
        }
        if changed {
            for (i, cell) in out.into_iter().enumerate() {
                buf[(area.left() + u16::try_from(i).unwrap_or(0), y)] = cell;
            }
        }
    }
}

/// Mirror each row's content inside the columns `x0..x1` of rows `y0..y1`: the
/// text block keeps its internal order but its distance from the left edge
/// becomes its distance from the right edge, so lines are right-aligned and tree
/// indentation grows from the right (T564). Rows that contain a border glyph
/// are left alone, as are rows that are blank. Cells keep their style.
pub(super) fn reflect_rows(buf: &mut Buffer, x0: u16, x1: u16, y0: u16, y1: u16) {
    let area = buf.area;
    let (x0, x1) = (x0.max(area.left()), x1.min(area.right()));
    let (y0, y1) = (y0.max(area.top()), y1.min(area.bottom()));
    if x1 <= x0 {
        return;
    }
    let width = usize::from(x1 - x0);
    for y in y0..y1 {
        let cells: Vec<Cell> = (x0..x1).map(|x| buf[(x, y)].clone()).collect();
        if cells
            .iter()
            .any(|c| c.symbol().chars().next().is_some_and(is_border))
        {
            continue;
        }
        let content = |c: &Cell| c.symbol() != " ";
        let (Some(start), Some(last)) = (
            cells.iter().position(content),
            cells.iter().rposition(content),
        ) else {
            continue;
        };
        let end = last + 1;
        let new_end = width - start;
        let new_start = new_end - (end - start);
        if new_start == start {
            continue;
        }
        let mut blank = cells
            .iter()
            .find(|c| c.symbol() == " ")
            .cloned()
            .unwrap_or_else(|| cells[start].clone());
        blank.set_symbol(" ");
        let mut out = vec![blank; width];
        out[new_start..new_end].clone_from_slice(&cells[start..end]);
        for (i, cell) in out.into_iter().enumerate() {
            buf[(x0 + u16::try_from(i).unwrap_or(0), y)] = cell;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    fn row(buf: &Buffer) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, 0)].symbol().to_string())
            .collect()
    }

    #[test]
    fn latin_row_is_untouched() {
        let mut b = Buffer::empty(Rect::new(0, 0, 10, 1));
        b.set_string(0, 0, "hello", ratatui::style::Style::default());
        reorder_buffer(&mut b);
        assert_eq!(row(&b).trim_end(), "hello");
    }

    #[test]
    fn arabic_stretch_reverses_but_borders_hold() {
        let mut b = Buffer::empty(Rect::new(0, 0, 12, 1));
        b.set_string(0, 0, "│ابج│", ratatui::style::Style::default());
        reorder_buffer(&mut b);
        assert_eq!(row(&b).trim_end(), "│جبا│");
    }

    #[test]
    fn reflect_right_aligns_and_mirrors_indent() {
        let mut b = Buffer::empty(Rect::new(0, 0, 12, 3));
        let st = ratatui::style::Style::default();
        b.set_string(0, 0, " ab", st); // one pad space -> one pad space on the right
        b.set_string(0, 1, "   cd", st); // deeper indent -> deeper right pad
        // A row with a border glyph is untouched.
        b.set_string(0, 2, "\u{2502}x", st);
        reflect_rows(&mut b, 0, 12, 0, 3);
        let line = |y: u16| -> String { (0..12).map(|x| b[(x, y)].symbol().to_string()).collect() };
        assert_eq!(line(0), "         ab ");
        assert_eq!(line(1), "       cd   ");
        assert!(line(2).starts_with("\u{2502}x"));
    }

    #[test]
    fn enabled_follows_setting_and_locale() {
        assert!(enabled("on", "en") && !enabled("off", "ar"));
        assert!(enabled("auto", "ar") && !enabled("auto", "en"));
    }
}
