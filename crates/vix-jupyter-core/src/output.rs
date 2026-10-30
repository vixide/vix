//! What a run produced: the [`Output`] model and MIME selection.
//!
//! Each IOPub `stream`, `execute_result`, `display_data`,
//! `update_display_data` or `error` becomes one [`Output`]. A MIME bundle is
//! shown in the first type Vix can display ([`MimeBundle::choose`]):
//! `image/png`, `image/jpeg`, `text/markdown`, then `text/plain`. Anything
//! richer (`text/html`, `image/svg+xml`, widgets, …) falls back to
//! `text/plain`, and [`Choice::dropped_richer`] says so.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ansi::strip_ansi;
use crate::message::Content;

/// The MIME types Vix can show, best first.
pub const DISPLAYABLE: [&str; 4] = ["image/png", "image/jpeg", "text/markdown", "text/plain"];

/// Which output stream a `stream` message is from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamName {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// A value in several MIME types, keyed by MIME type.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MimeBundle(pub BTreeMap<String, Value>);

/// The displayed form of a [`Choice`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    /// Text (`text/plain` or `text/markdown`).
    Text(String),
    /// Base64 image data with whitespace removed (not yet decoded).
    Base64(String),
}

/// The MIME type [`MimeBundle::choose`] picked, and its data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// The chosen MIME type, one of [`DISPLAYABLE`].
    pub mime: &'static str,
    /// The chosen data.
    pub payload: Payload,
    /// Whether the bundle also had a type Vix cannot show (such as
    /// `text/html`) and the choice fell back to `text/plain`.
    pub dropped_richer: bool,
}

impl MimeBundle {
    /// The data for `mime` as text. Jupyter allows a string or a list of
    /// strings (joined).
    #[must_use]
    pub fn text(&self, mime: &str) -> Option<String> {
        match self.0.get(mime)? {
            Value::String(s) => Some(s.clone()),
            Value::Array(parts) => Some(parts.iter().filter_map(Value::as_str).collect()),
            _ => None,
        }
    }

    /// The best type Vix can display, or `None` when there is none.
    #[must_use]
    pub fn choose(&self) -> Option<Choice> {
        for mime in DISPLAYABLE {
            let Some(text) = self.text(mime) else {
                continue;
            };
            let payload = if mime.starts_with("image/") {
                Payload::Base64(text.chars().filter(|c| !c.is_ascii_whitespace()).collect())
            } else {
                Payload::Text(text)
            };
            let dropped_richer =
                mime == "text/plain" && self.0.keys().any(|k| !DISPLAYABLE.contains(&k.as_str()));
            return Some(Choice {
                mime,
                payload,
                dropped_richer,
            });
        }
        None
    }
}

/// One output of a run.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Output {
    /// Text on stdout or stderr.
    Stream {
        /// Which stream.
        name: StreamName,
        /// The text.
        text: String,
    },
    /// The cell's value (`execute_result`).
    Result {
        /// The execution count.
        execution_count: Option<u64>,
        /// The value.
        data: MimeBundle,
    },
    /// A `display()` call (`display_data`, or `update_display_data` when
    /// `update` is set).
    Display {
        /// The value.
        data: MimeBundle,
        /// `transient.display_id`, if any.
        display_id: Option<String>,
        /// Whether this updates an earlier display.
        update: bool,
    },
    /// An exception.
    Error {
        /// Exception name.
        ename: String,
        /// Exception value.
        evalue: String,
        /// Traceback lines, raw (ANSI colors included).
        traceback: Vec<String>,
    },
}

impl Output {
    /// The output carried by `content`, or `None` for a non-output message.
    #[must_use]
    pub fn from_content(content: Content) -> Option<Self> {
        Some(match content {
            Content::Stream(s) => Self::Stream {
                name: s.name,
                text: s.text,
            },
            Content::ExecuteResult(r) => Self::Result {
                execution_count: r.execution_count,
                data: r.data,
            },
            Content::DisplayData(d) => Self::Display {
                display_id: d.display_id().map(str::to_owned),
                data: d.data,
                update: false,
            },
            Content::UpdateDisplayData(d) => Self::Display {
                display_id: d.display_id().map(str::to_owned),
                data: d.data,
                update: true,
            },
            Content::Error(e) => Self::Error {
                ename: e.ename,
                evalue: e.evalue,
                traceback: e.traceback,
            },
            _ => return None,
        })
    }

    /// The output as plain text: stream text, a value's `text/markdown` or
    /// `text/plain`, or `ename: evalue` and the traceback, all without ANSI
    /// codes or other control characters.
    #[must_use]
    pub fn plain_text(&self) -> String {
        match self {
            Self::Stream { text, .. } => text.clone(),
            Self::Result { data, .. } | Self::Display { data, .. } => data
                .text("text/markdown")
                .or_else(|| data.text("text/plain"))
                .unwrap_or_default(),
            Self::Error {
                ename,
                evalue,
                traceback,
            } => {
                let mut out = strip_ansi(&format!("{ename}: {evalue}"));
                for line in traceback {
                    out.push('\n');
                    out.push_str(&strip_ansi(line));
                }
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bundle(v: &Value) -> MimeBundle {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }

    #[test]
    fn png_wins_and_whitespace_is_removed() {
        let b = bundle(
            &json!({"text/plain": "<Image>", "image/png": "iVBO\nRw0K", "text/html": "<b>"}),
        );
        let c = b.choose();
        assert_eq!(
            c,
            Some(Choice {
                mime: "image/png",
                payload: Payload::Base64("iVBORw0K".into()),
                dropped_richer: false,
            })
        );
    }

    #[test]
    fn html_falls_back_to_plain_with_a_note() {
        let b = bundle(
            &json!({"text/plain": "<IPython.core.display.HTML object>", "text/html": "<b>hi</b>"}),
        );
        let c = b.choose().unwrap_or_else(|| unreachable!());
        assert_eq!(c.mime, "text/plain");
        assert!(c.dropped_richer);
    }

    #[test]
    fn list_of_strings_is_joined() {
        let b = bundle(&json!({"text/plain": ["a\n", "b"]}));
        assert_eq!(b.text("text/plain").as_deref(), Some("a\nb"));
    }

    #[test]
    fn nothing_displayable() {
        assert_eq!(bundle(&json!({"application/json": {}})).choose(), None);
    }

    #[test]
    fn error_text_is_stripped() {
        let o = Output::Error {
            ename: "ZeroDivisionError".into(),
            evalue: "division by zero".into(),
            traceback: vec!["\u{1b}[0;31mZeroDivisionError\u{1b}[0m: division by zero".into()],
        };
        assert_eq!(
            o.plain_text(),
            "ZeroDivisionError: division by zero\nZeroDivisionError: division by zero"
        );
    }
}
