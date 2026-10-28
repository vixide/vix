//! The segment-per-line, field-numbered view of a message.

use crate::escape::unescape;
use crate::message::{Field, Message};
use crate::path::Path;

/// What a [`Row`] stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// A segment header such as `PID`.
    Segment,
    /// A field such as `PID-5`.
    Field,
    /// A component (or subcomponent) of a structured field, such as `PID-5.1`.
    Component,
}

/// One displayable line of the structured view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// What the row stands for.
    pub kind: RowKind,
    /// The label: the segment id, or the field path such as `PID-5.1`.
    pub label: String,
    /// The address (its `field` is 0 for a segment row).
    pub path: Path,
    /// The decoded value (empty for a segment row).
    pub value: String,
    /// Zero-based line of the segment in the source text.
    pub line: usize,
    /// Character column of the field in its source line (0 for a segment).
    pub col: usize,
}

impl Row {
    /// The row as one line of text: nested rows are indented two spaces.
    #[must_use]
    pub fn display(&self) -> String {
        match self.kind {
            RowKind::Segment => self.label.clone(),
            RowKind::Field => format!("  {:<12} {}", self.label, self.value),
            RowKind::Component => format!("    {:<12} {}", self.label, self.value),
        }
    }
}

fn is_blank(f: &Field) -> bool {
    f.repetitions.iter().all(|r| {
        r.components
            .iter()
            .all(|c| c.subcomponents.iter().all(String::is_empty))
    })
}

/// The rows of `msg`: each segment header followed by its non-empty fields,
/// and, for a field with several components, repetitions or subcomponents, one
/// nested row per non-empty part.
#[must_use]
pub fn rows(msg: &Message) -> Vec<Row> {
    let d = &msg.delimiters;
    let mut out = Vec::new();
    for seg in &msg.segments {
        let header = Path::field(&seg.name, 0);
        out.push(Row {
            kind: RowKind::Segment,
            label: seg.name.clone(),
            path: header,
            value: String::new(),
            line: seg.line,
            col: 0,
        });
        for (i, f) in seg.fields.iter().enumerate() {
            if is_blank(f) {
                continue;
            }
            let path = Path::field(&seg.name, i + 1);
            out.push(Row {
                kind: RowKind::Field,
                label: path.to_string(),
                value: unescape(&f.to_er7(d), d),
                path: path.clone(),
                line: seg.line,
                col: f.col,
            });
            let many_reps = f.repetitions.len() > 1;
            for (ri, rep) in f.repetitions.iter().enumerate() {
                let structured = rep.components.len() > 1
                    || rep.components.iter().any(|c| c.subcomponents.len() > 1);
                if !many_reps && !structured {
                    continue;
                }
                for (ci, comp) in rep.components.iter().enumerate() {
                    let base = Path {
                        repetition: many_reps.then_some(ri + 1),
                        component: Some(ci + 1),
                        ..path.clone()
                    };
                    if comp.subcomponents.iter().all(String::is_empty) {
                        continue;
                    }
                    if comp.subcomponents.len() > 1 {
                        for (si, sub) in comp.subcomponents.iter().enumerate() {
                            if sub.is_empty() {
                                continue;
                            }
                            let p = Path {
                                subcomponent: Some(si + 1),
                                ..base.clone()
                            };
                            out.push(leaf(&p, sub, seg.line, f.col, msg));
                        }
                    } else {
                        let sub = comp.subcomponents.first().map_or("", String::as_str);
                        out.push(leaf(&base, sub, seg.line, f.col, msg));
                    }
                }
            }
        }
    }
    out
}

fn leaf(path: &Path, raw: &str, line: usize, col: usize, msg: &Message) -> Row {
    Row {
        kind: RowKind::Component,
        label: path.to_string(),
        path: path.clone(),
        value: unescape(raw, &msg.delimiters),
        line,
        col,
    }
}

/// The whole view as text, one [`Row::display`] per line.
#[must_use]
pub fn pretty(msg: &Message) -> String {
    rows(msg)
        .iter()
        .map(Row::display)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_fields_and_components() {
        let m = Message::parse("MSH|^~\\&|APP\rPID|1||X^Y&Z~W");
        let text = pretty(&m);
        assert!(text.contains("MSH-3"), "{text}");
        assert!(text.contains("PID-1"), "{text}");
        assert!(!text.contains("PID-2 "), "blank fields are skipped: {text}");
        assert!(text.contains("PID-3(1).1"), "{text}");
        assert!(text.contains("PID-3(1).2.2"), "{text}");
        assert!(text.contains("PID-3(2).1"), "{text}");
    }

    #[test]
    fn rows_carry_jump_targets() {
        let m = Message::parse("MSH|^~\\&|APP\rPID|1||ABC");
        let r = rows(&m);
        let pid3 = r.iter().find(|r| r.label == "PID-3").unwrap();
        assert_eq!((pid3.line, pid3.col), (1, 7));
        let seg = r
            .iter()
            .find(|r| r.kind == RowKind::Segment && r.label == "PID");
        assert_eq!(seg.unwrap().line, 1);
    }

    #[test]
    fn simple_fields_have_no_nested_rows() {
        let m = Message::parse("PID|1|abc");
        assert_eq!(rows(&m).len(), 3);
    }
}
