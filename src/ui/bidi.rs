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
    fn enabled_follows_setting_and_locale() {
        assert!(enabled("on", "en") && !enabled("off", "ar"));
        assert!(enabled("auto", "ar") && !enabled("auto", "en"));
    }
}
