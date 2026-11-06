//! Turn the cell grid into drawing operations, independent of BUI.
//!
//! The result is a flat list of filled rectangles and single-character text
//! items in logical pixels, in paint order. `crate::view` replays it onto
//! BUI's `Painter`; keeping it a plain value is what lets the layout be
//! tested without a window.
//!
//! BUI's font is proportional, so a run of text cannot be drawn as one string
//! without its letters drifting off the cell grid: every printable character
//! is placed at its own column instead. Box-drawing and block characters are
//! drawn as rectangles (`crate::glyphs`), because the font has none.

use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use ratatui::style::Modifier;

use crate::font_choice;
use crate::glyphs;
use crate::render::{Palette, Rgba8, cell_colors, row_backgrounds, row_text};

/// Cell size, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Cell width.
    pub cell_w: f32,
    /// Cell height.
    pub cell_h: f32,
}

/// One drawing operation.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// A filled rectangle.
    Rect {
        /// Left edge.
        x: f32,
        /// Top edge.
        y: f32,
        /// Width.
        w: f32,
        /// Height.
        h: f32,
        /// Fill color (its alpha already folds in a shade character's).
        color: Rgba8,
    },
    /// Text with its top-left at (`x`, `y`).
    Text {
        /// Left edge.
        x: f32,
        /// Top edge.
        y: f32,
        /// The character (or, for an emoji sequence, the cell's grapheme).
        text: String,
        /// Text color.
        color: Rgba8,
        /// Draw heavier (BUI has no bold face; see `crate::view`).
        bold: bool,
    },
}

/// `color` with its alpha scaled by `alpha` (0.0 to 1.0).
fn faded(color: Rgba8, alpha: f32) -> Rgba8 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a 0..=1 fraction of an 8-bit channel, clamped and rounded"
    )]
    let a = (f32::from(color.a) * alpha.clamp(0.0, 1.0)).round() as u8;
    Rgba8 { a, ..color }
}

/// The operations that paint `buffer` (and the text `cursor`, if shown), in
/// paint order: backgrounds, then text and line decorations, then the cursor.
#[must_use]
pub fn build(buffer: &Buffer, cursor: Option<Position>, p: Palette, m: Metrics) -> Vec<Op> {
    let mut ops = Vec::new();
    let at_x = |col: u16| m.cell_w * f32::from(col);
    for y in 0..buffer.area.height {
        let top = m.cell_h * f32::from(y);
        for run in row_backgrounds(buffer, y, p) {
            ops.push(Op::Rect {
                x: at_x(run.col),
                y: top,
                w: m.cell_w * f32::from(run.len),
                h: m.cell_h,
                color: run.color,
            });
        }
        for span in row_text(buffer, y, p) {
            let mods = span.style.modifier;
            let color = span.style.fg;
            let bold = mods.contains(Modifier::BOLD);
            let mut cells = 0_u16;
            for ch in span.text.chars() {
                let x = at_x(span.col + cells);
                if span.text.is_ascii() {
                    cells += 1;
                    if ch != ' ' {
                        ops.push(Op::Text {
                            x: x + font_choice::nudge(ch),
                            y: top,
                            text: ch.to_string(),
                            color,
                            bold,
                        });
                    }
                } else if let Some(pieces) = glyphs::pieces(ch, m.cell_w, m.cell_h) {
                    cells += 1;
                    for piece in pieces {
                        ops.push(Op::Rect {
                            x: x + piece.x,
                            y: top + piece.y,
                            w: piece.w,
                            h: piece.h,
                            color: faded(color, piece.alpha),
                        });
                    }
                } else {
                    // A non-ASCII span is one cell (see `row_text`): draw its
                    // whole symbol there, whatever it is made of.
                    cells = 1;
                    ops.push(Op::Text {
                        x,
                        y: top,
                        text: span.text.clone(),
                        color,
                        bold,
                    });
                    break;
                }
            }
            let x = at_x(span.col);
            let w = m.cell_w * f32::from(cells.max(1));
            if mods.contains(Modifier::UNDERLINED) {
                ops.push(Op::Rect {
                    x,
                    y: top + m.cell_h - 2.0,
                    w,
                    h: 1.0,
                    color,
                });
            }
            if mods.contains(Modifier::CROSSED_OUT) {
                ops.push(Op::Rect {
                    x,
                    y: top + (m.cell_h / 2.0).floor(),
                    w,
                    h: 1.0,
                    color,
                });
            }
        }
    }
    if let Some(pos) = cursor
        && pos.x < buffer.area.width
        && pos.y < buffer.area.height
    {
        let (fg, _) = cell_colors(&buffer[(pos.x, pos.y)], p);
        ops.push(Op::Rect {
            x: at_x(pos.x),
            y: m.cell_h * f32::from(pos.y),
            w: 2.0,
            h: m.cell_h,
            color: fg,
        });
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use ratatui::style::{Color, Style};

    const M: Metrics = Metrics {
        cell_w: 8.0,
        cell_h: 18.0,
    };

    fn palette() -> Palette {
        Palette::for_appearance(true)
    }

    fn buffer(w: u16, h: u16) -> Buffer {
        Buffer::empty(Rect::new(0, 0, w, h))
    }

    fn texts(ops: &[Op]) -> Vec<(f32, f32, String)> {
        ops.iter()
            .filter_map(|o| match o {
                Op::Text { x, y, text, .. } => Some((*x, *y, text.clone())),
                Op::Rect { .. } => None,
            })
            .collect()
    }

    #[test]
    fn each_ascii_character_sits_on_its_own_column() {
        let mut b = buffer(10, 2);
        b.set_string(2, 1, "a b", Style::default());
        let t = texts(&build(&b, None, palette(), M));
        // The space is skipped; letters keep their columns around it.
        assert_eq!(
            t,
            [(16.0, 18.0, "a".to_string()), (32.0, 18.0, "b".to_string())]
        );
    }

    #[test]
    fn box_drawing_becomes_rectangles_not_text() {
        let mut b = buffer(4, 1);
        b.set_string(0, 0, "┌─┐", Style::default());
        let ops = build(&b, None, palette(), M);
        assert_eq!(texts(&ops), Vec::new());
        // A corner is two pieces, the line one, the other corner two.
        assert_eq!(ops.len(), 5);
    }

    #[test]
    fn other_non_ascii_is_drawn_as_text_in_one_cell() {
        let mut b = buffer(4, 1);
        b.set_string(1, 0, "é", Style::default());
        assert_eq!(
            texts(&build(&b, None, palette(), M)),
            [(8.0, 0.0, "é".to_string())]
        );
    }

    #[test]
    fn backgrounds_come_first_and_skip_the_default() {
        let mut b = buffer(6, 1);
        b.set_string(1, 0, "ab", Style::default().bg(Color::Rgb(1, 2, 3)));
        let ops = build(&b, None, palette(), M);
        assert_eq!(
            ops[0],
            Op::Rect {
                x: 8.0,
                y: 0.0,
                w: 16.0,
                h: 18.0,
                color: Rgba8::rgb(1, 2, 3)
            }
        );
        assert_eq!(texts(&ops).len(), 2);
    }

    #[test]
    fn underline_and_strikethrough_are_rules_across_the_span() {
        let mut b = buffer(5, 1);
        b.set_string(
            0,
            0,
            "abc",
            Style::default().add_modifier(Modifier::UNDERLINED | Modifier::CROSSED_OUT),
        );
        let ops = build(&b, None, palette(), M);
        let rules: Vec<_> = ops
            .iter()
            .filter(|o| matches!(o, Op::Rect { h, .. } if (*h - 1.0).abs() < f32::EPSILON))
            .collect();
        assert_eq!(rules.len(), 2);
        assert!(matches!(rules[0], Op::Rect { w, .. } if (*w - 24.0).abs() < f32::EPSILON));
    }

    #[test]
    fn cursor_is_a_bar_last_and_clipped_to_the_grid() {
        let b = buffer(5, 2);
        let ops = build(&b, Some(Position::new(3, 1)), palette(), M);
        assert_eq!(
            ops.last(),
            Some(&Op::Rect {
                x: 24.0,
                y: 18.0,
                w: 2.0,
                h: 18.0,
                color: palette().fg
            })
        );
        assert_eq!(
            build(&b, Some(Position::new(9, 9)), palette(), M),
            Vec::<Op>::new()
        );
    }

    #[test]
    fn shade_characters_fade_the_foreground() {
        let mut b = buffer(2, 1);
        b.set_string(0, 0, "▒", Style::default());
        let ops = build(&b, None, palette(), M);
        assert!(matches!(&ops[0], Op::Rect { color, .. } if color.a == 128));
    }
}
