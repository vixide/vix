//! Box-drawing and block-element characters, drawn as rectangles.
//!
//! BUI's embedded font (Inter) has none of the U+2500..U+259F characters Vix
//! draws its borders, separators and scroll bars with, and BUI does no
//! per-glyph font fallback, so they would show as missing-glyph boxes. Each is
//! instead built from filled rectangles that span the cell edge to edge, which
//! also joins neighbouring cells without gaps (something a font glyph seldom
//! does at fractional sizes).
//!
//! Rounded corners (`╭ ╮ ╯ ╰`) are drawn square, the diagonals (`╱ ╲ ╳`) are
//! not drawn, and dashed lines are drawn solid.

/// A filled rectangle inside one cell, in pixels from the cell's top-left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
    /// Fraction of the foreground color to paint (`░ ▒ ▓` shades).
    pub alpha: f32,
}

/// Line weights of U+2500..U+257F as four digits per character, in the order
/// left, right, up, down: 0 none, 1 light, 2 heavy, 3 double.
const BOX: [&str; 128] = [
    "1100", "2200", "0011", "0022", "1100", "2200", "0011", "0022", // 2500
    "1100", "2200", "0011", "0022", "0101", "0201", "0102", "0202", // 2508
    "1001", "2001", "1002", "2002", "0110", "0210", "0120", "0220", // 2510
    "1010", "2010", "1020", "2020", "0111", "0211", "0121", "0112", // 2518
    "0122", "0221", "0212", "0222", "1011", "2011", "1021", "1012", // 2520
    "1022", "2021", "2012", "2022", "1101", "2101", "1201", "2201", // 2528
    "1102", "2102", "1202", "2202", "1110", "2110", "1210", "2210", // 2530
    "1120", "2120", "1220", "2220", "1111", "2111", "1211", "2211", // 2538
    "1121", "1112", "1122", "2121", "1221", "2112", "1212", "2221", // 2540
    "2212", "2122", "1222", "2222", "1100", "2200", "0011", "0022", // 2548
    "3300", "0033", "0301", "0103", "0303", "3001", "1003", "3003", // 2550
    "0310", "0130", "0330", "3010", "1030", "3030", "0311", "0133", // 2558
    "0333", "3011", "1033", "3033", "3301", "1103", "3303", "3310", // 2560
    "1130", "3330", "3311", "1133", "3333", "0101", "1001", "1010", // 2568
    "0110", "0000", "0000", "0000", "1000", "0010", "0100", "0001", // 2570
    "2000", "0020", "0200", "0002", "1200", "0012", "2100", "0021", // 2578
];

/// The rectangles that draw `ch` in a `cw` x `ch_h` cell, or `None` when `ch`
/// is not a box-drawing or block character this module draws.
#[must_use]
pub fn pieces(ch: char, cw: f32, chh: f32) -> Option<Vec<Piece>> {
    match u32::from(ch) {
        c @ 0x2500..=0x257f => box_pieces(BOX[(c - 0x2500) as usize], cw, chh),
        c @ 0x2580..=0x259f => block_pieces(c, cw, chh),
        _ => None,
    }
}

/// A solid rectangle.
const fn solid(x: f32, y: f32, w: f32, h: f32) -> Piece {
    Piece {
        x,
        y,
        w,
        h,
        alpha: 1.0,
    }
}

/// Line pieces for one weights string (`None` for the undrawn diagonals).
fn box_pieces(weights: &str, cw: f32, chh: f32) -> Option<Vec<Piece>> {
    let mut w: Vec<u8> = weights.bytes().map(|b| b - b'0').collect();
    if w.iter().all(|&v| v == 0) {
        return None;
    }
    let (cx, cy) = (cw / 2.0, chh / 2.0);
    let mut out = Vec::new();
    // A line running through the cell is one piece, not two arms that overlap
    // (overlapping translucent-looking edges show as bright seams).
    for (pair, vertical) in [([0, 1], false), ([2, 3], true)] {
        let weight = w[pair[0]];
        if weight == 0 || weight != w[pair[1]] {
            continue;
        }
        for (thick, off) in strokes(weight) {
            let at = if vertical { cx } else { cy } + off - thick / 2.0;
            out.push(if vertical {
                solid(at.round(), 0.0, thick, chh)
            } else {
                solid(0.0, at.round(), cw, thick)
            });
        }
        w[pair[0]] = 0;
        w[pair[1]] = 0;
    }
    // Every other direction runs from the cell edge to just past the center,
    // so the arms of a corner or tee overlap into a clean square joint.
    for (dir, &weight) in w.iter().enumerate() {
        for (thick, off) in strokes(weight) {
            let reach = thick / 2.0 + 1.5;
            // Whole pixels, or a one-pixel rule is two faint half-covered rows.
            let line_y = (cy + off - thick / 2.0).round();
            let line_x = (cx + off - thick / 2.0).round();
            out.push(match dir {
                0 => solid(0.0, line_y, (cx + reach).round(), thick),
                1 => solid(
                    (cx - reach).round(),
                    line_y,
                    cw - (cx - reach).round(),
                    thick,
                ),
                2 => solid(line_x, 0.0, thick, (cy + reach).round()),
                _ => solid(
                    line_x,
                    (cy - reach).round(),
                    thick,
                    chh - (cy - reach).round(),
                ),
            });
        }
    }
    Some(out)
}

/// The strokes of one line weight as (thickness, offset from the center
/// line): light and heavy are one, a double line is two light ones.
fn strokes(weight: u8) -> Vec<(f32, f32)> {
    match weight {
        1 => vec![(1.0, 0.0)],
        2 => vec![(2.0, 0.0)],
        3 => vec![(1.0, -1.5), (1.0, 1.5)],
        _ => Vec::new(),
    }
}

/// Pieces for the block elements U+2580..U+259F.
fn block_pieces(c: u32, cw: f32, chh: f32) -> Option<Vec<Piece>> {
    let (hw, hh) = (cw / 2.0, chh / 2.0);
    // Eighths as a count of 1..=8.
    let eighth = |n: u8| f32::from(n) / 8.0;
    let quad = |ul: bool, ur: bool, ll: bool, lr: bool| {
        let mut v = Vec::new();
        for (on, x, y) in [(ul, 0.0, 0.0), (ur, hw, 0.0), (ll, 0.0, hh), (lr, hw, hh)] {
            if on {
                v.push(solid(x, y, hw, hh));
            }
        }
        v
    };
    let shade = |a: f32| {
        vec![Piece {
            alpha: a,
            ..solid(0.0, 0.0, cw, chh)
        }]
    };
    Some(match c {
        0x2580 => vec![solid(0.0, 0.0, cw, hh)],
        n @ 0x2581..=0x2588 => {
            let k = u8::try_from(n - 0x2580).ok()?;
            let h = chh * eighth(k);
            vec![solid(0.0, chh - h, cw, h)]
        }
        n @ 0x2589..=0x258f => {
            let k = u8::try_from(0x2590 - n).ok()?;
            vec![solid(0.0, 0.0, cw * eighth(k), chh)]
        }
        0x2590 => vec![solid(hw, 0.0, hw, chh)],
        0x2591 => shade(0.25),
        0x2592 => shade(0.5),
        0x2593 => shade(0.75),
        0x2594 => vec![solid(0.0, 0.0, cw, chh / 8.0)],
        0x2595 => vec![solid(cw - cw / 8.0, 0.0, cw / 8.0, chh)],
        0x2596 => quad(false, false, true, false),
        0x2597 => quad(false, false, false, true),
        0x2598 => quad(true, false, false, false),
        0x2599 => quad(true, false, true, true),
        0x259a => quad(true, false, false, true),
        0x259b => quad(true, true, true, false),
        0x259c => quad(true, true, false, true),
        0x259d => quad(false, true, false, false),
        0x259e => quad(false, true, true, false),
        0x259f => quad(false, true, true, true),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CW: f32 = 8.0;
    const CH: f32 = 18.0;

    #[test]
    fn plain_text_is_not_a_glyph() {
        assert!(pieces('a', CW, CH).is_none());
        assert!(pieces('日', CW, CH).is_none());
    }

    #[test]
    fn horizontal_line_spans_the_cell_edge_to_edge() {
        let p = pieces('─', CW, CH).expect("drawn");
        assert_eq!(p.len(), 1, "one piece, not two overlapping arms");
        let left = p.iter().map(|r| r.x).fold(f32::MAX, f32::min);
        let right = p.iter().map(|r| r.x + r.w).fold(0.0, f32::max);
        assert!(left.abs() < f32::EPSILON);
        assert!((right - CW).abs() < f32::EPSILON);
    }

    #[test]
    fn corner_has_two_arms_a_tee_two_pieces_and_a_cross_two_lines() {
        assert_eq!(pieces('┌', CW, CH).expect("drawn").len(), 2);
        // The through line is one piece, plus the branch.
        assert_eq!(pieces('├', CW, CH).expect("drawn").len(), 2);
        assert_eq!(pieces('┼', CW, CH).expect("drawn").len(), 2);
    }

    #[test]
    fn double_lines_are_two_strokes() {
        assert_eq!(pieces('═', CW, CH).expect("drawn").len(), 2);
        assert_eq!(pieces('╬', CW, CH).expect("drawn").len(), 4);
    }

    #[test]
    fn heavy_is_thicker_than_light() {
        let light = pieces('─', CW, CH).expect("drawn")[0].h;
        let heavy = pieces('━', CW, CH).expect("drawn")[0].h;
        assert!(heavy > light);
    }

    #[test]
    fn rounded_corners_are_drawn_and_diagonals_are_not() {
        assert!(pieces('╭', CW, CH).is_some());
        assert!(pieces('╱', CW, CH).is_none());
    }

    #[test]
    fn blocks_cover_their_share_of_the_cell() {
        let full = pieces('█', CW, CH).expect("drawn");
        assert_eq!(full, vec![solid(0.0, 0.0, CW, CH)]);
        let lower_half = pieces('▄', CW, CH).expect("drawn");
        assert!((lower_half[0].h - CH / 2.0).abs() < f32::EPSILON);
        assert!((lower_half[0].y - CH / 2.0).abs() < f32::EPSILON);
        let left_quarter = pieces('▎', CW, CH).expect("drawn");
        assert!((left_quarter[0].w - CW / 4.0).abs() < f32::EPSILON);
        assert_eq!(pieces('▚', CW, CH).expect("drawn").len(), 2);
        assert!((pieces('▒', CW, CH).expect("drawn")[0].alpha - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn every_box_character_has_a_table_row() {
        // Only the three diagonals and nothing else are undrawn.
        let undrawn: Vec<u32> = (0x2500..=0x257f)
            .filter(|&c| pieces(char::from_u32(c).expect("char"), CW, CH).is_none())
            .collect();
        assert_eq!(undrawn, [0x2571, 0x2572, 0x2573]);
    }
}
