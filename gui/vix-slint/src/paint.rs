//! The window's grid models: the planned background runs, text spans and
//! underline/strikethrough lines from `crate::render`, one Slint model of
//! each per grid row (the `rows` property of `ui/window.slint`).
//!
//! Slint is retained mode: every model row is a live element. So the models
//! are updated in place by diffing against what each row showed last frame —
//! an unchanged span keeps its `Text` element and its shaped text, and a
//! frame in which only the cursor line changed touches only that line.

use std::rc::Rc;

use crate::fonts::Families;
use crate::render::{BgRun, Palette, Rgba8, TextSpan, row_backgrounds, row_text};
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;
use slint::{Brush, Color, Model, ModelRc, VecModel};
use slint_interpreter::{Struct, Value};

/// An underline or strikethrough under/through a span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// First column.
    pub col: u16,
    /// Number of cells.
    pub len: u16,
    /// Line color.
    pub color: Rgba8,
    /// Through the middle (strikethrough) rather than along the bottom.
    pub strike: bool,
}

/// The underline/strikethrough lines for a row's spans.
#[must_use]
pub fn row_lines(spans: &[TextSpan]) -> Vec<Line> {
    let mut out = Vec::new();
    for s in spans {
        let len = u16::try_from(s.text.chars().count()).unwrap_or(u16::MAX);
        for (flag, strike) in [(Modifier::UNDERLINED, false), (Modifier::CROSSED_OUT, true)] {
            if s.style.modifier.contains(flag) {
                out.push(Line {
                    col: s.col,
                    len,
                    color: s.style.fg,
                    strike,
                });
            }
        }
    }
    out
}

/// A planned color as a Slint brush value.
#[must_use]
pub fn color(c: Rgba8) -> Color {
    Color::from_argb_u8(c.a, c.r, c.g, c.b)
}

fn brush(c: Rgba8) -> Value {
    Value::Brush(Brush::SolidColor(color(c)))
}

fn number(n: u16) -> Value {
    Value::Number(f64::from(n))
}

fn record<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Struct(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<Struct>(),
    )
}

/// Bring `model` (showing `last`) up to date with `new`, changing only the
/// entries that differ.
fn sync<T: PartialEq>(
    model: &VecModel<Value>,
    last: &mut Vec<T>,
    new: Vec<T>,
    value: impl Fn(&T) -> Value,
) {
    for (i, item) in new.iter().enumerate() {
        match last.get(i) {
            Some(old) if old == item => {}
            Some(_) => model.set_row_data(i, value(item)),
            None => model.push(value(item)),
        }
    }
    for i in (new.len()..last.len()).rev() {
        model.remove(i);
    }
    *last = new;
}

/// One grid row's models and what they show.
struct RowModels {
    bgs: Rc<VecModel<Value>>,
    texts: Rc<VecModel<Value>>,
    lines: Rc<VecModel<Value>>,
    shown_bgs: Vec<BgRun>,
    shown_texts: Vec<TextSpan>,
    shown_lines: Vec<Line>,
}

impl RowModels {
    fn new() -> Self {
        Self {
            bgs: Rc::new(VecModel::default()),
            texts: Rc::new(VecModel::default()),
            lines: Rc::new(VecModel::default()),
            shown_bgs: Vec::new(),
            shown_texts: Vec::new(),
            shown_lines: Vec::new(),
        }
    }

    /// The `GridRow` struct value pointing at this row's models.
    fn value(&self) -> Value {
        record([
            ("bgs", Value::Model(ModelRc::from(Rc::clone(&self.bgs)))),
            ("texts", Value::Model(ModelRc::from(Rc::clone(&self.texts)))),
            ("lines", Value::Model(ModelRc::from(Rc::clone(&self.lines)))),
        ])
    }
}

/// The grid's per-row models.
pub struct GridModel {
    rows: Rc<VecModel<Value>>,
    per_row: Vec<RowModels>,
}

impl Default for GridModel {
    fn default() -> Self {
        Self::new()
    }
}

impl GridModel {
    /// An empty grid.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows: Rc::new(VecModel::default()),
            per_row: Vec::new(),
        }
    }

    /// The value for the window's `rows` property; set it once.
    #[must_use]
    pub fn rows(&self) -> Value {
        Value::Model(ModelRc::from(Rc::clone(&self.rows)))
    }

    /// Update every row's models from `buffer`.
    pub fn update(&mut self, buffer: &Buffer, palette: Palette, fonts: &Families) {
        let height = usize::from(buffer.area.height);
        while self.per_row.len() < height {
            let row = RowModels::new();
            self.rows.push(row.value());
            self.per_row.push(row);
        }
        while self.per_row.len() > height {
            self.rows.remove(self.per_row.len() - 1);
            self.per_row.pop();
        }
        for (y, row) in (0..buffer.area.height).zip(&mut self.per_row) {
            sync(
                &row.bgs,
                &mut row.shown_bgs,
                row_backgrounds(buffer, y, palette),
                |b| {
                    record([
                        ("col", number(b.col)),
                        ("len", number(b.len)),
                        ("color", brush(b.color)),
                        ("strike", Value::Bool(false)),
                    ])
                },
            );
            let spans = row_text(buffer, y, palette);
            let lines = row_lines(&spans);
            sync(&row.texts, &mut row.shown_texts, spans, |s| {
                let m = s.style.modifier;
                record([
                    ("col", number(s.col)),
                    ("text", Value::String(s.text.as_str().into())),
                    ("color", brush(s.style.fg)),
                    ("family", Value::String(fonts.for_text(&s.text).into())),
                    ("bold", Value::Bool(m.contains(Modifier::BOLD))),
                    ("italic", Value::Bool(m.contains(Modifier::ITALIC))),
                ])
            });
            sync(&row.lines, &mut row.shown_lines, lines, |l| {
                record([
                    ("col", number(l.col)),
                    ("len", number(l.len)),
                    ("color", brush(l.color)),
                    ("strike", Value::Bool(l.strike)),
                ])
            });
        }
    }

    /// The number of text spans currently shown (for timing reports).
    #[cfg(any(test, feature = "screenshot"))]
    #[must_use]
    pub fn span_count(&self) -> usize {
        self.per_row.iter().map(|r| r.texts.row_count()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use ratatui::style::{Color as TuiColor, Style};

    const DARK: Palette = Palette::for_appearance(true);

    fn fonts() -> Families {
        Families {
            primary: "Menlo".into(),
            icons: None,
        }
    }

    fn texts(grid: &GridModel, y: usize) -> Vec<String> {
        grid.per_row[y]
            .texts
            .iter()
            .map(|v| match v {
                Value::Struct(s) => match s.get_field("text") {
                    Some(Value::String(t)) => t.to_string(),
                    _ => String::new(),
                },
                _ => String::new(),
            })
            .collect()
    }

    #[test]
    fn rows_follow_the_buffer_and_shrink_with_it() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 8, 3));
        buf.set_string(0, 0, "ab", Style::default());
        buf.set_string(0, 2, "xy", Style::default().fg(TuiColor::Red));
        let mut grid = GridModel::new();
        grid.update(&buf, DARK, &fonts());
        assert_eq!(grid.rows.row_count(), 3);
        assert_eq!(texts(&grid, 0), ["ab      "]);
        assert_eq!(texts(&grid, 1), Vec::<String>::new());
        assert_eq!(grid.span_count(), 2);

        // An edit replaces only the changed span; a smaller grid drops rows.
        buf.set_string(0, 0, "cd", Style::default());
        buf.resize(Rect::new(0, 0, 8, 1));
        grid.update(&buf, DARK, &fonts());
        assert_eq!(grid.rows.row_count(), 1);
        assert_eq!(texts(&grid, 0), ["cd      "]);
    }

    #[test]
    fn underline_and_strikethrough_become_lines() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 6, 1));
        buf.set_string(
            0,
            0,
            "ab",
            Style::default().add_modifier(Modifier::UNDERLINED | Modifier::CROSSED_OUT),
        );
        let lines = row_lines(&row_text(&buf, 0, DARK));
        assert_eq!(lines.len(), 2);
        assert_eq!((lines[0].col, lines[0].len, lines[0].strike), (0, 2, false));
        assert!(lines[1].strike);
    }
}
