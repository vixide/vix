//! Turn the cell grid into drawing operations, independent of BUI.
//!
//! The result is a flat list of filled rectangles and text items in logical
//! pixels, in paint order. `crate::view` replays it: [`Op::Rect`] and
//! [`Op::Text`] onto BUI's `Painter`, [`Op::Styled`] as elements above it;
//! keeping it a plain value is what lets the layout be tested without a
//! window.
//!
//! Every printable character on the canvas is placed at its own column, even
//! in a monospace font: the cell is a whole number of device pixels and the
//! font's advance is not, so a run drawn as one string would drift off the
//! grid, and BUI's canvas is cheapest per character anyway (see the spec).
//! Box-drawing and block characters are drawn as rectangles
//! (`crate::glyphs`), so they join up whatever the font.

use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use ratatui::style::Modifier;

use crate::font_choice::{self, FONT_SIZE};
use crate::fonts::CellFont;
use crate::glyphs;
use crate::render::{Palette, Rgba8, TextSpan, cell_colors, row_backgrounds, row_text};

/// Where BUI's kit puts the baseline below the top of a canvas text item, in
/// em (`kit_frame.c`, `BK_ITEM_TEXT`), whatever the font.
const CANVAS_BASELINE: f32 = 0.72;
/// BUI's fixed ascent for a line of element text, in em (`blenderui::text`,
/// Inter's metrics, used for every font).
const ELEMENT_ASCENT: f32 = 1984.0 / 2048.0;
/// BUI's fixed descent for a line of element text, in em.
const ELEMENT_DESCENT: f32 = 494.0 / 2048.0;
/// How far the embedded font's text sits below the cell top.
const EMBEDDED_DROP: f32 = 2.0;

/// The cell grid's geometry, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// Cell width.
    pub cell_w: f32,
    /// Cell height.
    pub cell_h: f32,
    /// How far a canvas text item sits below its cell's top.
    pub text_drop: f32,
    /// How far an element with a cell-high line sits below its cell's top,
    /// so its baseline matches the canvas's.
    pub element_drop: f32,
    /// A monospace font is drawing (not the embedded proportional one).
    pub mono: bool,
}

impl Metrics {
    /// The metrics of BUI's embedded font.
    #[must_use]
    pub fn embedded() -> Self {
        Self {
            cell_w: font_choice::CELL_WIDTH,
            cell_h: font_choice::cell_height(),
            text_drop: EMBEDDED_DROP,
            element_drop: 0.0,
            mono: false,
        }
    }

    /// The metrics of `font` at `scale` device pixels per logical pixel: the
    /// cell and the baseline are whole device pixels, like a terminal's, so
    /// rectangles stay crisp and a column does not blur differently from the
    /// next one.
    #[must_use]
    pub fn monospace(font: CellFont, scale: f32) -> Self {
        let snap = |v: f32| (v * scale).round() / scale;
        let cell_h = snap(font_choice::cell_height());
        let baseline = snap((cell_h - font.ascent - font.descent) / 2.0 + font.ascent);
        let line = (ELEMENT_ASCENT + ELEMENT_DESCENT) * FONT_SIZE;
        Self {
            cell_w: snap(font.advance),
            cell_h,
            text_drop: baseline - CANVAS_BASELINE * FONT_SIZE,
            element_drop: baseline - ((cell_h - line) / 2.0 + ELEMENT_ASCENT * FONT_SIZE),
            mono: true,
        }
    }
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
    /// Canvas text in the default family, with its top-left at (`x`, `y`).
    Text {
        /// Left edge.
        x: f32,
        /// Top edge.
        y: f32,
        /// The character (or, for an emoji sequence, the cell's grapheme).
        text: String,
        /// Text color.
        color: Rgba8,
        /// Draw heavier (the canvas has no bold face; see `crate::view`).
        bold: bool,
    },
    /// Text in another face, which the canvas cannot draw: an element, in the
    /// cell whose top-left is (`x`, `y`).
    Styled {
        /// Left edge.
        x: f32,
        /// Top of the cell.
        y: f32,
        /// The text: a whole span, or one symbol.
        text: String,
        /// Text color.
        color: Rgba8,
        /// The bold face.
        bold: bool,
        /// The italic face.
        italic: bool,
        /// The fallback family (see `crate::fonts`), else the primary one.
        fallback: Option<u8>,
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

/// The operations that draw one styled `span` of the row at `top`.
fn push_span(
    ops: &mut Vec<Op>,
    span: &TextSpan,
    top: f32,
    m: Metrics,
    route: &mut dyn FnMut(char) -> Option<u8>,
) {
    let at_x = |col: u16| m.cell_w * f32::from(col);
    let mods = span.style.modifier;
    let color = span.style.fg;
    let bold = mods.contains(Modifier::BOLD);
    let italic = mods.contains(Modifier::ITALIC);
    let styled = m.mono && (bold || italic);
    let mut cells = 0_u16;
    for ch in span.text.chars() {
        let x = at_x(span.col + cells);
        if span.text.is_ascii() && styled {
            // One element for the span: the view keeps it on the grid.
            cells = u16::try_from(span.text.len()).unwrap_or(u16::MAX);
            ops.push(Op::Styled {
                x,
                y: top,
                text: span.text.trim_end().to_string(),
                color,
                bold,
                italic,
                fallback: None,
            });
            break;
        } else if span.text.is_ascii() {
            cells += 1;
            if ch != ' ' {
                let nudge = if m.mono { 0.0 } else { font_choice::nudge(ch) };
                ops.push(Op::Text {
                    x: x + nudge,
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
            let fallback = route(ch);
            if styled || fallback.is_some() {
                ops.push(Op::Styled {
                    x,
                    y: top,
                    // BUI shapes nothing: a selector has no glyph.
                    text: span.text.replace('\u{fe0f}', ""),
                    color,
                    bold,
                    italic,
                    fallback,
                });
            } else {
                ops.push(Op::Text {
                    x,
                    y: top,
                    text: span.text.replace('\u{fe0f}', ""),
                    color,
                    bold,
                });
            }
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

/// The operations that paint `buffer` (and the text `cursor`, if shown), in
/// paint order: backgrounds, then text and line decorations, then the cursor.
/// `route` names the fallback family for a character the primary font lacks.
#[must_use]
pub fn build(
    buffer: &Buffer,
    cursor: Option<Position>,
    p: Palette,
    m: Metrics,
    route: &mut dyn FnMut(char) -> Option<u8>,
) -> Vec<Op> {
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
            push_span(&mut ops, &span, top, m, route);
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
        text_drop: 2.0,
        element_drop: 0.0,
        mono: false,
    };

    const MONO: Metrics = Metrics { mono: true, ..M };

    fn palette() -> Palette {
        Palette::for_appearance(true)
    }

    /// `scene::build` with every character in the primary font.
    fn plain(b: &Buffer, cursor: Option<Position>, m: Metrics) -> Vec<Op> {
        build(b, cursor, palette(), m, &mut |_| None)
    }

    fn styled(ops: &[Op]) -> Vec<(f32, String, bool, bool, Option<u8>)> {
        ops.iter()
            .filter_map(|o| match o {
                Op::Styled {
                    x,
                    text,
                    bold,
                    italic,
                    fallback,
                    ..
                } => Some((*x, text.clone(), *bold, *italic, *fallback)),
                _ => None,
            })
            .collect()
    }

    fn buffer(w: u16, h: u16) -> Buffer {
        Buffer::empty(Rect::new(0, 0, w, h))
    }

    fn texts(ops: &[Op]) -> Vec<(f32, f32, String)> {
        ops.iter()
            .filter_map(|o| match o {
                Op::Text { x, y, text, .. } => Some((*x, *y, text.clone())),
                Op::Rect { .. } | Op::Styled { .. } => None,
            })
            .collect()
    }

    #[test]
    fn each_ascii_character_sits_on_its_own_column() {
        let mut b = buffer(10, 2);
        b.set_string(2, 1, "a b", Style::default());
        let t = texts(&plain(&b, None, M));
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
        let ops = plain(&b, None, M);
        assert_eq!(texts(&ops), Vec::new());
        // A corner is two pieces, the line one, the other corner two.
        assert_eq!(ops.len(), 5);
    }

    #[test]
    fn other_non_ascii_is_drawn_as_text_in_one_cell() {
        let mut b = buffer(4, 1);
        b.set_string(1, 0, "é", Style::default());
        assert_eq!(texts(&plain(&b, None, M)), [(8.0, 0.0, "é".to_string())]);
    }

    #[test]
    fn backgrounds_come_first_and_skip_the_default() {
        let mut b = buffer(6, 1);
        b.set_string(1, 0, "ab", Style::default().bg(Color::Rgb(1, 2, 3)));
        let ops = plain(&b, None, M);
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
        let ops = plain(&b, None, M);
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
        let ops = plain(&b, Some(Position::new(3, 1)), M);
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
        assert_eq!(plain(&b, Some(Position::new(9, 9)), M), Vec::<Op>::new());
    }

    #[test]
    fn a_monospace_font_is_not_nudged() {
        let mut b = buffer(3, 1);
        b.set_string(0, 0, "il", Style::default());
        assert_eq!(
            texts(&plain(&b, None, MONO)),
            [(0.0, 0.0, "i".to_string()), (8.0, 0.0, "l".to_string())]
        );
    }

    #[test]
    fn bold_and_italic_spans_become_one_styled_item_with_a_monospace_font() {
        let mut b = buffer(8, 1);
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let italic = Style::default().add_modifier(Modifier::ITALIC);
        b.set_string(1, 0, "ab", bold);
        b.set_string(3, 0, "cd", italic);
        let ops = plain(&b, None, MONO);
        assert_eq!(texts(&ops), Vec::new());
        assert_eq!(
            styled(&ops),
            [
                (8.0, "ab".to_string(), true, false, None),
                (24.0, "cd".to_string(), false, true, None),
            ]
        );
    }

    #[test]
    fn a_styled_span_keeps_its_full_width_for_decorations() {
        let mut b = buffer(5, 1);
        let style = Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        b.set_string(0, 0, "ab  ", style);
        let ops = plain(&b, None, MONO);
        assert_eq!(styled(&ops)[0].1, "ab");
        assert!(ops.iter().any(|o| matches!(o, Op::Rect { w, h, .. }
            if (*w - 32.0).abs() < f32::EPSILON && (*h - 1.0).abs() < f32::EPSILON)));
    }

    #[test]
    fn the_embedded_font_stays_on_the_canvas_when_bold() {
        let mut b = buffer(3, 1);
        b.set_string(
            0,
            0,
            "a",
            Style::default().add_modifier(Modifier::BOLD | Modifier::ITALIC),
        );
        let ops = plain(&b, None, M);
        assert_eq!(styled(&ops), Vec::new());
        assert!(matches!(ops[0], Op::Text { bold: true, .. }));
    }

    #[test]
    fn a_character_the_font_lacks_goes_to_its_fallback_family() {
        let mut b = buffer(4, 1);
        b.set_string(1, 0, "\u{f07b}", Style::default());
        let ops = build(&b, None, palette(), MONO, &mut |c| {
            (c == '\u{f07b}').then_some(2)
        });
        assert_eq!(texts(&ops), Vec::new());
        assert_eq!(
            styled(&ops),
            [(8.0, "\u{f07b}".to_string(), false, false, Some(2))]
        );
    }

    #[test]
    fn a_variation_selector_is_not_drawn() {
        let mut b = buffer(4, 1);
        b.set_string(0, 0, "\u{2764}\u{fe0f}", Style::default());
        let ops = build(&b, None, palette(), MONO, &mut |_| Some(0));
        assert_eq!(styled(&ops)[0].1, "\u{2764}");
    }

    #[test]
    fn monospace_metrics_snap_to_device_pixels_and_share_one_baseline() {
        let font = CellFont {
            advance: 8.43,
            ascent: 13.0,
            descent: 3.3,
        };
        let one = Metrics::monospace(font, 1.0);
        assert!((one.cell_w - 8.0).abs() < f32::EPSILON);
        let two = Metrics::monospace(font, 2.0);
        assert!((two.cell_w - 8.5).abs() < f32::EPSILON);
        assert!(two.mono);
        // Both paths land on the same baseline, 14 px below the cell top.
        let canvas = two.text_drop + CANVAS_BASELINE * FONT_SIZE;
        let line = (ELEMENT_ASCENT + ELEMENT_DESCENT) * FONT_SIZE;
        let element = two.element_drop + (two.cell_h - line) / 2.0 + ELEMENT_ASCENT * FONT_SIZE;
        assert!((canvas - element).abs() < 1e-4);
        assert!((canvas - 14.0).abs() < 1e-4);
    }

    #[test]
    fn shade_characters_fade_the_foreground() {
        let mut b = buffer(2, 1);
        b.set_string(0, 0, "▒", Style::default());
        let ops = plain(&b, None, M);
        assert!(matches!(&ops[0], Op::Rect { color, .. } if color.a == 128));
    }
}
