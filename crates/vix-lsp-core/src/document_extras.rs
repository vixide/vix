//! `textDocument/documentColor` and `textDocument/documentLink` (T621): the
//! reply parsers behind color swatches and openable links.

use crate::{Position, Range};
use serde_json::{Value, json};

/// One color a server found in a document.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct DocColor {
    /// Where the color is written.
    pub range: Range,
    /// The color as opaque 8-bit red, green and blue (alpha is ignored).
    pub rgb: (u8, u8, u8),
    /// The server's own red, green, blue and alpha, each clamped to 0.0-1.0,
    /// quoted back unrounded in a `textDocument/colorPresentation` request.
    pub rgba: [f64; 4],
}

/// One way a server offers to write a color (`ColorPresentation`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ColorPresentation {
    /// The text shown in the picker; also what replaces the color when the
    /// server gave no `textEdit`.
    pub label: String,
    /// The server's `textEdit`, if any.
    pub edit: Option<(Range, String)>,
    /// The server's `additionalTextEdits` (for example an import).
    pub additional: Vec<(Range, String)>,
}

impl ColorPresentation {
    /// The edits that apply this presentation to a color written at `color`:
    /// the `textEdit` (or, with none, the label over the color) first, then
    /// the additional edits.
    #[must_use]
    pub fn edits(&self, color: Range) -> Vec<(Range, String)> {
        let main = self
            .edit
            .clone()
            .unwrap_or_else(|| (color, self.label.clone()));
        std::iter::once(main)
            .chain(self.additional.iter().cloned())
            .collect()
    }
}

/// One link a server found in a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocLink {
    /// Where the link is written.
    pub range: Range,
    /// The URI it points at.
    pub target: String,
}

/// What opening a link target does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetKind {
    /// A local file, optionally at a 1-based line (`#L12` or `#12`).
    File {
        /// The file's path (still percent-encoded).
        path: String,
        /// The 1-based line, when the fragment names one.
        line: Option<u32>,
    },
    /// An `http`, `https` or `mailto` URL for the system opener.
    External(String),
    /// Any other scheme: refused.
    Unsupported,
}

/// Parse an LSP `Position` object; `None` if either field is missing, non-integer, or does not fit `u32`.
fn position(v: &Value) -> Option<Position> {
    Some(Position {
        line: u32::try_from(v.get("line")?.as_u64()?).ok()?,
        character: u32::try_from(v.get("character")?.as_u64()?).ok()?,
    })
}

/// Parse an LSP `Range` object from its `start` and `end` positions.
fn range(v: &Value) -> Option<Range> {
    Some(Range {
        start: position(v.get("start")?)?,
        end: position(v.get("end")?)?,
    })
}

/// A 0.0-1.0 channel as a byte, clamped and rounded.
fn channel(v: &Value, key: &str) -> Option<u8> {
    let scaled = (v.get(key)?.as_f64()?.clamp(0.0, 1.0) * 255.0).round();
    Some((0..=255u8).find(|b| f64::from(*b) >= scaled).unwrap_or(255))
}

/// A channel as the server wrote it, clamped to 0.0-1.0.
fn unit(v: &Value, key: &str) -> Option<f64> {
    Some(v.get(key)?.as_f64()?.clamp(0.0, 1.0))
}

/// Parse a `textDocument/documentColor` result (`ColorInformation[]`).
/// Malformed entries are skipped.
#[must_use]
pub fn parse_colors(result: &Value) -> Vec<DocColor> {
    result
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    let color = c.get("color")?;
                    Some(DocColor {
                        range: range(c.get("range")?)?,
                        rgb: (
                            channel(color, "red")?,
                            channel(color, "green")?,
                            channel(color, "blue")?,
                        ),
                        rgba: [
                            unit(color, "red")?,
                            unit(color, "green")?,
                            unit(color, "blue")?,
                            unit(color, "alpha").unwrap_or(1.0),
                        ],
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The params of `textDocument/colorPresentation` for `color` in document `uri`.
#[must_use]
pub fn color_presentation_params(uri: &str, color: &DocColor) -> Value {
    let [red, green, blue, alpha] = color.rgba;
    json!({
        "textDocument": { "uri": uri },
        "color": { "red": red, "green": green, "blue": blue, "alpha": alpha },
        "range": {
            "start": { "line": color.range.start.line, "character": color.range.start.character },
            "end": { "line": color.range.end.line, "character": color.range.end.character },
        },
    })
}

/// Parse one `TextEdit` object.
fn text_edit(v: &Value) -> Option<(Range, String)> {
    Some((
        range(v.get("range")?)?,
        v.get("newText")?.as_str()?.to_string(),
    ))
}

/// Parse a `textDocument/colorPresentation` result (`ColorPresentation[]`).
/// Entries without a `label` are skipped; malformed edits are dropped.
#[must_use]
pub fn parse_color_presentations(result: &Value) -> Vec<ColorPresentation> {
    result
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let label = p.get("label")?.as_str()?.to_string();
                    let additional = p
                        .get("additionalTextEdits")
                        .and_then(Value::as_array)
                        .map(|e| e.iter().filter_map(text_edit).collect())
                        .unwrap_or_default();
                    Some(ColorPresentation {
                        label,
                        edit: p.get("textEdit").and_then(text_edit),
                        additional,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse a `textDocument/documentLink` result (`DocumentLink[]`), dropping
/// links with no `target` (`documentLink/resolve` is not used).
#[must_use]
pub fn parse_links(result: &Value) -> Vec<DocLink> {
    result
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| {
                    let target = l.get("target")?.as_str()?;
                    if target.is_empty() {
                        return None;
                    }
                    Some(DocLink {
                        range: range(l.get("range")?)?,
                        target: target.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Classify a link `target`.
#[must_use]
pub fn target_kind(target: &str) -> TargetKind {
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
    {
        return TargetKind::External(target.to_string());
    }
    let Some(rest) = target.strip_prefix("file://") else {
        return TargetKind::Unsupported;
    };
    let (path, frag) = rest.split_once('#').unwrap_or((rest, ""));
    let line = frag
        .strip_prefix('L')
        .unwrap_or(frag)
        .split([',', ':', 'C'])
        .next()
        .and_then(|n| n.parse::<u32>().ok());
    TargetKind::File {
        path: path.to_string(),
        line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn presentations_parse_and_resolve_to_edits() {
        let r = json!([
            {"label": "#ff0000"},
            {"label": "rgb(255, 0, 0)",
             "textEdit": {"range": {"start": {"line": 0, "character": 1},
                                    "end": {"line": 0, "character": 4}}, "newText": "red"},
             "additionalTextEdits": [{"range": {"start": {"line": 0, "character": 0},
                                                "end": {"line": 0, "character": 0}},
                                       "newText": "use x;\n"}]},
            {"nolabel": true}
        ]);
        let ps = parse_color_presentations(&r);
        assert_eq!(ps.len(), 2);
        let at = Range {
            start: Position {
                line: 3,
                character: 2,
            },
            end: Position {
                line: 3,
                character: 9,
            },
        };
        let plain = ps[0].edits(at);
        assert_eq!(plain, vec![(at, "#ff0000".to_string())]);
        let full = ps[1].edits(at);
        assert_eq!(full.len(), 2);
        assert_eq!(full[0].1, "red");
        assert_eq!(full[0].0.start.character, 1);
        assert_eq!(full[1].1, "use x;\n");
        assert_eq!(parse_color_presentations(&Value::Null), vec![]);
    }

    #[test]
    fn presentation_params_quote_the_unrounded_color() {
        let c = parse_colors(&json!([
            {"range": {"start": {"line": 1, "character": 2}, "end": {"line": 1, "character": 9}},
             "color": {"red": 0.5, "green": 0.25, "blue": 1.0, "alpha": 0.5}}
        ]));
        let p = color_presentation_params("file:///a.css", &c[0]);
        assert_eq!(p["textDocument"]["uri"], "file:///a.css");
        assert_eq!(p["color"]["red"], 0.5);
        assert_eq!(p["color"]["alpha"], 0.5);
        assert_eq!(p["range"]["end"]["character"], 9);
    }

    #[test]
    fn colors_parse_and_clamp() {
        let r = json!([
            {"range": {"start": {"line": 1, "character": 2}, "end": {"line": 1, "character": 9}},
             "color": {"red": 1.0, "green": 0.5, "blue": 2.0, "alpha": 1.0}},
            {"range": "bad", "color": {}}
        ]);
        let c = parse_colors(&r);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].rgb, (255, 128, 255));
        assert_eq!(c[0].range.start.character, 2);
        assert_eq!(parse_colors(&Value::Null), vec![]);
    }

    #[test]
    fn links_need_a_target() {
        let r = json!([
            {"range": {"start": {"line": 0, "character": 1}, "end": {"line": 0, "character": 5}},
             "target": "https://example.com"},
            {"range": {"start": {"line": 0, "character": 1}, "end": {"line": 0, "character": 5}}}
        ]);
        let l = parse_links(&r);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].target, "https://example.com");
    }

    #[test]
    fn target_kinds() {
        let file = |line| TargetKind::File {
            path: "/a/b.rs".into(),
            line,
        };
        assert_eq!(target_kind("file:///a/b.rs#L12"), file(Some(12)));
        assert_eq!(target_kind("file:///a/b.rs#7"), file(Some(7)));
        assert_eq!(target_kind("file:///a/b.rs"), file(None));
        assert!(matches!(
            target_kind("HTTPS://x.y"),
            TargetKind::External(_)
        ));
        assert_eq!(target_kind("command:evil"), TargetKind::Unsupported);
    }
}
