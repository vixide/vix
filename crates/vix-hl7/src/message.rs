//! The lenient ER7 parser and the message tree it produces.

use crate::escape::unescape;
use crate::path::Path;

/// The five delimiter characters a message declares in its `MSH` segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delimiters {
    /// Field separator (`MSH-1`), normally `|`.
    pub field: char,
    /// Component separator, normally `^`.
    pub component: char,
    /// Repetition separator, normally `~`.
    pub repetition: char,
    /// Escape character, normally `\`.
    pub escape: char,
    /// Subcomponent separator, normally `&`.
    pub subcomponent: char,
}

impl Default for Delimiters {
    fn default() -> Self {
        Delimiters {
            field: '|',
            component: '^',
            repetition: '~',
            escape: '\\',
            subcomponent: '&',
        }
    }
}

impl Delimiters {
    /// Read the delimiters from a line that starts with `MSH`: the character
    /// after `MSH` is the field separator and the (up to four) characters
    /// before the next one are the encoding characters. Anything missing keeps
    /// its default, so a truncated header still parses.
    #[must_use]
    pub fn from_msh_line(line: &str) -> Self {
        let mut d = Delimiters::default();
        let Some(rest) = line.strip_prefix("MSH") else {
            return d;
        };
        let mut chars = rest.chars();
        let Some(field) = chars.next() else {
            return d;
        };
        d.field = field;
        let mut enc = chars.take_while(|&c| c != field);
        if let Some(c) = enc.next() {
            d.component = c;
        }
        if let Some(c) = enc.next() {
            d.repetition = c;
        }
        if let Some(c) = enc.next() {
            d.escape = c;
        }
        if let Some(c) = enc.next() {
            d.subcomponent = c;
        }
        d
    }
}

/// One component: its subcomponents (a lone value has exactly one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    /// Raw (still escaped) subcomponent texts; never empty.
    pub subcomponents: Vec<String>,
}

/// One repetition of a field: its components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repetition {
    /// The components; never empty.
    pub components: Vec<Component>,
}

/// One field of a segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// Character column of the field's first character within its source line.
    pub col: usize,
    /// The repetitions; never empty.
    pub repetitions: Vec<Repetition>,
}

/// One segment (a line of the message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// The segment id, e.g. `PID`.
    pub name: String,
    /// Zero-based line of the segment in the parsed text.
    pub line: usize,
    /// Fields in order: `fields[0]` is field 1 (for `MSH`, the separator).
    pub fields: Vec<Field>,
}

/// A parsed HL7 v2 message.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Message {
    /// The delimiters declared by (or defaulted for) the message.
    pub delimiters: Delimiters,
    /// The segments in order.
    pub segments: Vec<Segment>,
}

/// Split `s` on `sep`, returning each piece with its starting character
/// column (offset by `base`).
fn split_cols(s: &str, sep: char, base: usize) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut piece_col = base;
    for (col, (i, c)) in (base..).zip(s.char_indices()) {
        if c == sep {
            out.push((piece_col, &s[start..i]));
            start = i + c.len_utf8();
            piece_col = col + 1;
        }
    }
    out.push((piece_col, &s[start..]));
    out
}

fn parse_field(text: &str, col: usize, d: &Delimiters, literal: bool) -> Field {
    let repetitions = if literal {
        vec![vec![vec![text.to_string()]]]
    } else {
        text.split(d.repetition)
            .map(|rep| {
                rep.split(d.component)
                    .map(|comp| comp.split(d.subcomponent).map(str::to_string).collect())
                    .collect()
            })
            .collect()
    };
    Field {
        col,
        repetitions: repetitions
            .into_iter()
            .map(|r: Vec<Vec<String>>| Repetition {
                components: r
                    .into_iter()
                    .map(|subcomponents| Component { subcomponents })
                    .collect(),
            })
            .collect(),
    }
}

fn parse_segment(line: &str, line_no: usize, d: &Delimiters) -> Segment {
    let pieces = split_cols(line, d.field, 0);
    let name = pieces[0].1.trim().to_string();
    let is_msh = name == "MSH" && pieces.len() > 1;
    let mut fields = Vec::new();
    if is_msh {
        // MSH-1 is the separator itself and MSH-2 the encoding characters:
        // neither is split further.
        fields.push(parse_field(&d.field.to_string(), 3, d, true));
    }
    for (n, &(col, text)) in pieces.iter().enumerate().skip(1) {
        fields.push(parse_field(text, col, d, is_msh && n == 1));
    }
    Segment {
        name,
        line: line_no,
        fields,
    }
}

/// Split `text` into `(line number, line)` pairs on `\r\n`, `\n` or `\r`,
/// dropping the MLLP framing bytes if the caller pasted them.
fn lines(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut n = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push((n, std::mem::take(&mut cur)));
                n += 1;
            }
            '\u{0B}' | '\u{1C}' => {}
            other => cur.push(other),
        }
    }
    out.push((n, cur));
    out
}

impl Message {
    /// Parse `text` (segments separated by `\r`, `\n` or `\r\n`). Lenient: it
    /// never fails -- blank lines are skipped, a missing or truncated `MSH`
    /// falls back to the default delimiters, and ragged segments keep whatever
    /// fields they have.
    #[must_use]
    pub fn parse(text: &str) -> Message {
        let all = lines(text);
        let delimiters = all
            .iter()
            .find(|(_, l)| !l.trim().is_empty())
            .map(|(_, l)| Delimiters::from_msh_line(l.trim_start()))
            .unwrap_or_default();
        let segments = all
            .iter()
            .filter(|(_, l)| !l.trim().is_empty())
            .map(|(n, l)| parse_segment(l.trim_start(), *n, &delimiters))
            .collect();
        Message {
            delimiters,
            segments,
        }
    }

    /// Whether the first segment is an `MSH` header (what makes the text a
    /// plausible HL7 message rather than arbitrary prose).
    #[must_use]
    pub fn has_msh(&self) -> bool {
        self.segments.first().is_some_and(|s| s.name == "MSH")
    }

    /// The first segment named `name`.
    #[must_use]
    pub fn segment(&self, name: &str) -> Option<&Segment> {
        self.segments.iter().find(|s| s.name == name)
    }

    /// Re-encode as ER7 text with `\r` between segments (the wire form).
    #[must_use]
    pub fn to_er7(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.to_er7(&self.delimiters))
            .collect::<Vec<_>>()
            .join("\r")
    }

    /// The raw (still escaped) text addressed by `path`, or `None` if the
    /// segment, field, repetition, component or subcomponent is absent.
    #[must_use]
    pub fn get_raw(&self, path: &Path) -> Option<String> {
        let d = &self.delimiters;
        let field = self
            .segment(&path.segment)?
            .fields
            .get(path.field.checked_sub(1)?)?;
        let rep = field
            .repetitions
            .get(path.repetition.unwrap_or(1).checked_sub(1)?)?;
        let Some(c) = path.component else {
            return Some(rep.to_er7(d));
        };
        let comp = rep.components.get(c.checked_sub(1)?)?;
        let Some(s) = path.subcomponent else {
            return Some(comp.to_er7(d));
        };
        comp.subcomponents.get(s.checked_sub(1)?).cloned()
    }

    /// Like [`Message::get_raw`] but with escape sequences decoded.
    #[must_use]
    pub fn get(&self, path: &Path) -> Option<String> {
        self.get_raw(path).map(|r| unescape(&r, &self.delimiters))
    }
}

impl Segment {
    /// Re-encode this segment.
    #[must_use]
    pub fn to_er7(&self, d: &Delimiters) -> String {
        let is_msh = self.name == "MSH" && !self.fields.is_empty();
        let mut out = self.name.clone();
        for (i, f) in self.fields.iter().enumerate() {
            // MSH-1 is the separator, already emitted as the join below.
            if is_msh && i == 0 {
                continue;
            }
            out.push(d.field);
            out.push_str(&f.to_er7(d));
        }
        out
    }
}

impl Field {
    /// Re-encode this field (all repetitions).
    #[must_use]
    pub fn to_er7(&self, d: &Delimiters) -> String {
        self.repetitions
            .iter()
            .map(|r| r.to_er7(d))
            .collect::<Vec<_>>()
            .join(&d.repetition.to_string())
    }
}

impl Repetition {
    /// Re-encode this repetition (all components).
    #[must_use]
    pub fn to_er7(&self, d: &Delimiters) -> String {
        self.components
            .iter()
            .map(|c| c.to_er7(d))
            .collect::<Vec<_>>()
            .join(&d.component.to_string())
    }
}

impl Component {
    /// Re-encode this component (all subcomponents).
    #[must_use]
    pub fn to_er7(&self, d: &Delimiters) -> String {
        self.subcomponents.join(&d.subcomponent.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) const SAMPLE: &str = "MSH|^~\\&|SEND|FAC|RECV|FAC|202401011200||ADT^A01|MSG0001|P|2.5\rPID|1||12345^^^HOSP^MR||DOE^JANE^Q||19800101|F\rPV1|1|I|ICU^101~ER^2";

    #[test]
    fn parses_delimiters_and_segments() {
        let m = Message::parse(SAMPLE);
        assert!(m.has_msh());
        assert_eq!(m.delimiters, Delimiters::default());
        let names: Vec<_> = m.segments.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["MSH", "PID", "PV1"]);
    }

    #[test]
    fn msh_numbering_counts_the_separator() {
        let m = Message::parse(SAMPLE);
        let get = |p: &str| m.get(&Path::parse(p).unwrap());
        assert_eq!(get("MSH-1").as_deref(), Some("|"));
        assert_eq!(get("MSH-2").as_deref(), Some("^~\\&"));
        assert_eq!(get("MSH-3").as_deref(), Some("SEND"));
        assert_eq!(get("MSH-9.2").as_deref(), Some("A01"));
        assert_eq!(get("MSH-10").as_deref(), Some("MSG0001"));
    }

    #[test]
    fn addresses_components_and_repetitions() {
        let m = Message::parse(SAMPLE);
        let get = |p: &str| m.get(&Path::parse(p).unwrap());
        assert_eq!(get("PID-5.1").as_deref(), Some("DOE"));
        assert_eq!(get("PID-5.2").as_deref(), Some("JANE"));
        assert_eq!(get("PID-3.4").as_deref(), Some("HOSP"));
        assert_eq!(get("PV1-3(2).1").as_deref(), Some("ER"));
        assert_eq!(get("PV1-3.1").as_deref(), Some("ICU"));
        assert_eq!(get("PID-99"), None);
        assert_eq!(get("ZZZ-1"), None);
    }

    #[test]
    fn round_trips_to_er7() {
        assert_eq!(Message::parse(SAMPLE).to_er7(), SAMPLE);
    }

    #[test]
    fn honours_custom_delimiters() {
        let m = Message::parse("MSH#*+!/#A#B\rPID#1#X*Y");
        assert_eq!(m.delimiters.field, '#');
        assert_eq!(m.delimiters.component, '*');
        let p = Path::parse("PID-2.2").unwrap();
        assert_eq!(m.get(&p).as_deref(), Some("Y"));
    }

    #[test]
    fn accepts_newlines_blank_lines_and_mllp_bytes() {
        let m = Message::parse("\u{0B}MSH|^~\\&|A\r\n\r\nPID|1\n\u{1C}\r");
        assert_eq!(m.segments.len(), 2);
        assert_eq!(m.segments[1].line, 2);
    }

    #[test]
    fn empty_and_headerless_input_do_not_panic() {
        assert_eq!(Message::parse("").segments.len(), 0);
        assert!(!Message::parse("hello world").has_msh());
        let m = Message::parse("MSH");
        assert_eq!(m.segments.len(), 1);
        let m = Message::parse("MSH|");
        assert_eq!(m.segments.len(), 1);
    }

    #[test]
    fn field_columns_point_at_the_field() {
        let m = Message::parse("PID|1||ABC");
        let f = &m.segments[0].fields;
        assert_eq!(f[0].col, 4);
        assert_eq!(f[2].col, 7);
    }
}
