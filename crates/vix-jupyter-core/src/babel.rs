//! Org babel header arguments and how a run's outputs become a result.
//!
//! [`Header::parse`] reads the header arguments on a `#+begin_src` line
//! that a Jupyter run honors: `:results` (`value` or `output`, and
//! `replace`, `append` or `silent`), `:file` and `:eval`. Everything else
//! on the line (`:session`, `:exports`, `:var`, …) is ignored.
//! [`result_text`] and [`first_image`] pick, from a run's outputs, what
//! goes into the `#+RESULTS:` block.

use crate::output::{Output, Payload};

/// The keyword that opens a source block.
const BEGIN: &str = "#+begin_src";

/// Which outputs make up the result (`:results value` or `output`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Collect {
    /// `:results value` (the default): everything the run shows, as a
    /// notebook cell would — printed text, values, displays and errors.
    #[default]
    Value,
    /// `:results output`: only printed text (stdout and stderr) and errors.
    Output,
}

/// What happens to an existing `#+RESULTS:` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Handling {
    /// `replace` (the default): the new result replaces the old.
    #[default]
    Replace,
    /// `append`: the new result goes after the old.
    Append,
    /// `silent` (or `none`): nothing is written to the buffer.
    Silent,
}

/// Whether the block may run (`:eval`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Eval {
    /// Run it (no `:eval`, `yes`, or the export-only `no-export`,
    /// `never-export` and `query-export`, which do not concern an
    /// interactive run).
    #[default]
    Yes,
    /// `query`: ask before every run, whatever the trust settings say.
    Query,
    /// `no` or `never`: never run it. Holds the value as written.
    Never(String),
}

/// The header arguments a Jupyter run honors.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Header {
    /// `:results` collection.
    pub collect: Collect,
    /// `:results` handling.
    pub handling: Handling,
    /// `:file`: where an image result is saved, as written (relative to
    /// the Org file's folder).
    pub file: Option<String>,
    /// `:eval`.
    pub eval: Eval,
}

/// Split the arguments into `(key, values)` pairs: each `:key` and the
/// words after it up to the next `:key`. A `"…"` value may hold spaces.
fn pairs(args: &str) -> Vec<(String, Vec<String>)> {
    let mut words = Vec::new();
    let mut rest = args.trim();
    while !rest.is_empty() {
        if let Some(quoted) = rest.strip_prefix('"')
            && let Some(close) = quoted.find('"')
        {
            words.push(quoted[..close].to_owned());
            rest = quoted[close + 1..].trim_start();
            continue;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        words.push(rest[..end].to_owned());
        rest = rest[end..].trim_start();
    }
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for word in words {
        if word.starts_with(':') && word.len() > 1 {
            out.push((word.to_ascii_lowercase(), Vec::new()));
        } else if let Some((_, values)) = out.last_mut() {
            values.push(word);
        }
    }
    out
}

impl Header {
    /// The header arguments on a `#+begin_src <language> …` line. A line
    /// that is not a `#+begin_src` line gives the defaults.
    #[must_use]
    pub fn parse(begin_line: &str) -> Self {
        let mut header = Self::default();
        let trimmed = begin_line.trim_start();
        let Some(rest) = trimmed
            .get(..BEGIN.len())
            .filter(|p| p.eq_ignore_ascii_case(BEGIN))
            .map(|_| &trimmed[BEGIN.len()..])
            .filter(|r| r.is_empty() || r.starts_with(char::is_whitespace))
        else {
            return header;
        };
        // Skip the language, if there is one.
        let rest = rest.trim_start();
        let args = if rest.starts_with(':') {
            rest
        } else {
            rest.find(char::is_whitespace).map_or("", |i| &rest[i..])
        };
        for (key, values) in pairs(args) {
            match key.as_str() {
                ":results" => header.read_results(&values),
                ":file" => header.file = values.first().cloned().filter(|f| !f.is_empty()),
                ":eval" => {
                    header.eval = match values.first().map(|v| v.to_ascii_lowercase()) {
                        Some(v) if v == "no" || v == "never" => Eval::Never(v),
                        Some(v) if v == "query" => Eval::Query,
                        _ => Eval::Yes,
                    };
                }
                _ => {}
            }
        }
        header
    }

    /// Apply the values of a `:results` header: `value`/`output` choose what is collected, `replace`/`append`/`silent`/`none` how the block is updated. Later words win; format words (`raw`, `drawer`, `table`, ...) are ignored.
    fn read_results(&mut self, values: &[String]) {
        for value in values {
            match value.to_ascii_lowercase().as_str() {
                "value" => self.collect = Collect::Value,
                "output" => self.collect = Collect::Output,
                "replace" => self.handling = Handling::Replace,
                "append" => self.handling = Handling::Append,
                "silent" | "none" => self.handling = Handling::Silent,
                // Formats (`raw`, `drawer`, `table`, …) are not honored.
                _ => {}
            }
        }
    }
}

/// The result text of `outputs` for `collect`, in arrival order, without
/// ANSI codes. An image contributes its `text/plain` form (the text used
/// when the image itself cannot be saved; see [`first_image`]).
#[must_use]
pub fn result_text(outputs: &[Output], collect: Collect) -> String {
    let mut out = String::new();
    for output in outputs {
        let text = match output {
            Output::Stream { .. } | Output::Error { .. } => output.plain_text(),
            Output::Result { data, .. } | Output::Display { data, .. } => {
                if collect == Collect::Output {
                    continue;
                }
                match data.choose().map(|c| c.payload) {
                    Some(Payload::Text(text)) => text,
                    Some(Payload::Base64(_)) => match data.text("text/plain") {
                        Some(text) => text,
                        None => continue,
                    },
                    None => continue,
                }
            }
        };
        let text = crate::ansi::strip_ansi(&text);
        if text.is_empty() {
            continue;
        }
        out.push_str(&text);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// The first image among `outputs`: its MIME type and base64 data.
#[must_use]
pub fn first_image(outputs: &[Output]) -> Option<(&'static str, String)> {
    outputs.iter().find_map(|output| match output {
        Output::Result { data, .. } | Output::Display { data, .. } => {
            let choice = data.choose()?;
            match choice.payload {
                Payload::Base64(b64) => Some((choice.mime, b64)),
                Payload::Text(_) => None,
            }
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{MimeBundle, StreamName};
    use serde_json::Value;

    fn h(line: &str) -> Header {
        Header::parse(line)
    }

    #[test]
    fn defaults() {
        assert_eq!(h("#+begin_src python"), Header::default());
        assert_eq!(h("#+BEGIN_SRC jupyter-python"), Header::default());
        assert_eq!(h("not a block"), Header::default());
    }

    #[test]
    fn results_file_and_eval() {
        let header =
            h("  #+begin_src python :session a :results output append :file out.png :eval never");
        assert_eq!(header.collect, Collect::Output);
        assert_eq!(header.handling, Handling::Append);
        assert_eq!(header.file.as_deref(), Some("out.png"));
        assert_eq!(header.eval, Eval::Never("never".into()));
        assert_eq!(
            h("#+begin_src R :results silent").handling,
            Handling::Silent
        );
        assert_eq!(h("#+begin_src R :eval query").eval, Eval::Query);
        assert_eq!(h("#+begin_src R :eval no").eval, Eval::Never("no".into()));
        assert_eq!(h("#+begin_src R :eval no-export").eval, Eval::Yes);
        assert_eq!(
            h(r#"#+begin_src R :file "my plot.png" :results value"#)
                .file
                .as_deref(),
            Some("my plot.png")
        );
    }

    #[test]
    fn last_word_wins_and_unknown_words_are_ignored() {
        let header = h("#+begin_src python :results value raw output replace");
        assert_eq!(header.collect, Collect::Output);
        assert_eq!(header.handling, Handling::Replace);
    }

    fn bundle(pairs: &[(&str, &str)]) -> MimeBundle {
        MimeBundle(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), Value::String((*v).to_owned())))
                .collect(),
        )
    }

    fn outputs() -> Vec<Output> {
        vec![
            Output::Stream {
                name: StreamName::Stdout,
                text: "printed".into(),
            },
            Output::Result {
                execution_count: Some(1),
                data: bundle(&[("text/plain", "42")]),
            },
            Output::Display {
                data: bundle(&[("image/png", "iVBO Rw=="), ("text/plain", "<Figure>")]),
                display_id: None,
                update: false,
            },
            Output::Error {
                ename: "E".into(),
                evalue: "\u{1b}[31mbad\u{1b}[0m".into(),
                traceback: Vec::new(),
            },
        ]
    }

    #[test]
    fn value_and_output_text() {
        assert_eq!(
            result_text(&outputs(), Collect::Value),
            "printed\n42\n<Figure>\nE: bad\n"
        );
        assert_eq!(
            result_text(&outputs(), Collect::Output),
            "printed\nE: bad\n"
        );
        assert_eq!(result_text(&[], Collect::Value), "");
    }

    #[test]
    fn the_first_image() {
        assert_eq!(
            first_image(&outputs()),
            Some(("image/png", "iVBORw==".to_owned()))
        );
        assert_eq!(first_image(&outputs()[..2]), None);
    }
}
