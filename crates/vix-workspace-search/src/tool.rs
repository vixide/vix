//! A line-oriented search tool (`rg`, else `grep`) run where the files are
//! (T715).
//!
//! Under a remote authority, reading every indexed file to search it costs a
//! round trip per file. The App instead runs one search tool through the
//! authority's `command` and reads its output. This module is the pure part:
//! the argument lists and the output parsers. Paths in the output are relative
//! to the tool's working directory, in the authority's own path syntax; they
//! are never looked up on this machine.
//!
//! The tool is only a **prefilter**: the App re-checks every reported line
//! with its own Rust regex (the one the in-process search uses), so a tool may
//! report more lines than match, and the results keep the local semantics.
//!
//! Owning spec: `crates/vix-workspace-search/spec/index.md`.

use serde_json::Value;

/// What to ask a search tool for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolQuery {
    /// The pattern in Rust `regex` syntax, given to `rg` (whose default
    /// engine is the same crate).
    pub regex: String,
    /// Fixed strings for `grep -F` when `rg` is missing; `None` means only
    /// `rg` can run this query (a regex `grep`'s dialects don't share).
    pub fixed: Option<Vec<String>>,
    /// Whether `fixed` matches regardless of case (`grep -i`).
    pub ignore_case: bool,
    /// Skip files larger than this many bytes (`rg --max-filesize`; `grep`
    /// has no such option, so the App's own read limit doesn't apply there).
    pub max_filesize: Option<u64>,
}

impl ToolQuery {
    /// The query for a workspace search: `query` is literal unless `regex`,
    /// and matches regardless of case unless `case_sensitive` (the same
    /// pattern [`crate::WorkspaceSearch::pattern`] builds).
    #[must_use]
    pub fn search(query: &str, regex: bool, case_sensitive: bool) -> Self {
        // For a literal query the text is escaped so the same `regex` field
        // works for `rg` regardless of mode; `fixed` carries the unescaped
        // words for the `grep -F` fallback.
        let core = if regex {
            query.to_string()
        } else {
            regex::escape(query)
        };
        Self {
            regex: if case_sensitive {
                core
            } else {
                format!("(?i){core}")
            },
            fixed: (!regex).then(|| vec![query.to_string()]),
            ignore_case: !case_sensitive,
            max_filesize: None,
        }
    }

    /// Lines containing any of `words`, case-sensitively (the TODO scan's
    /// comment tags).
    #[must_use]
    pub fn any_of(words: &[&str]) -> Self {
        let alternatives: Vec<String> = words.iter().map(|w| regex::escape(w)).collect();
        Self {
            regex: alternatives.join("|"),
            fixed: Some(words.iter().map(|w| (*w).to_string()).collect()),
            ignore_case: false,
            max_filesize: None,
        }
    }

    /// A regex only `rg` can run (go-to-definition's keyword pattern).
    #[must_use]
    pub fn regex_only(regex: impl Into<String>) -> Self {
        Self {
            regex: regex.into(),
            fixed: None,
            ignore_case: false,
            max_filesize: None,
        }
    }

    /// The same query, skipping files larger than `bytes`.
    #[must_use]
    pub fn max_filesize(mut self, bytes: u64) -> Self {
        self.max_filesize = Some(bytes);
        self
    }
}

/// Which search tool runs a query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// ripgrep, with `--json` output.
    Rg,
    /// POSIX-ish `grep -rn` (GNU, BSD or busybox), fixed strings only.
    Grep,
}

impl Tool {
    /// The program name to resolve on the authority's `PATH`.
    #[must_use]
    pub fn program(self) -> &'static str {
        match self {
            Self::Rg => "rg",
            Self::Grep => "grep",
        }
    }

    /// The arguments that run `query` over the working directory, or `None`
    /// when this tool can't run it. `rg` keeps its own ignore rules (the App
    /// keeps only lines in files its index holds) but includes hidden files,
    /// so it never sees fewer files than the index.
    #[must_use]
    pub fn args(self, query: &ToolQuery) -> Option<Vec<String>> {
        match self {
            Self::Rg => Some(rg_args(query)),
            Self::Grep => grep_args(query),
        }
    }

    /// Parse one line of this tool's output. `known` says whether a path
    /// (relative to the working directory, `/`-separated as the tool printed
    /// it) is one the caller searches; `grep`'s `path:line:text` needs it to
    /// tell a `:` in a file name from the separator.
    #[must_use]
    pub fn parse(self, line: &str, known: &dyn Fn(&str) -> bool) -> Option<ToolLine> {
        match self {
            Self::Rg => parse_rg_json(line),
            Self::Grep => parse_grep(line, known),
        }
    }
}

/// One line a search tool reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolLine {
    /// The file, relative to the tool's working directory, without a
    /// leading `./`.
    pub rel: String,
    /// 1-based line number.
    pub line: usize,
    /// The line's text without its line ending.
    pub text: String,
}

/// `rg` arguments for `query`: JSON output, no user config file (it could
/// change the output), no error messages, hidden files included. `-e` plus
/// `--` keep a pattern or path starting with `-` from being read as an option.
fn rg_args(query: &ToolQuery) -> Vec<String> {
    let mut args: Vec<String> = ["--json", "--no-config", "--no-messages", "--hidden"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    if let Some(bytes) = query.max_filesize {
        args.push("--max-filesize".into());
        args.push(bytes.to_string());
    }
    args.extend(["-e".into(), query.regex.clone(), "--".into(), ".".into()]);
    args
}

/// `grep` arguments for `query`, or `None` when it has no fixed strings or
/// would need a case fold `grep -i` does by locale (non-ASCII).
fn grep_args(query: &ToolQuery) -> Option<Vec<String>> {
    // A regex query has no `fixed` words: grep's regex dialects differ from
    // Rust's, so only `rg` may run those.
    let words = query.fixed.as_ref()?;
    if words.is_empty() || (query.ignore_case && words.iter().any(|w| !w.is_ascii())) {
        return None;
    }
    let mut args: Vec<String> = ["-r", "-n", "-I", "-H", "-F"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    if query.ignore_case {
        args.push("-i".into());
    }
    // grep has no ignore-file support, so the common heavyweight directories
    // are excluded by name; the App re-filters against its own file index.
    for dir in [".git", "node_modules", "target"] {
        args.push(format!("--exclude-dir={dir}"));
    }
    for w in words {
        args.push("-e".into());
        args.push(w.clone());
    }
    args.extend(["--".into(), ".".into()]);
    Some(args)
}

/// Drop a leading `./` (or `.\` on Windows).
fn strip_dot(path: &str) -> &str {
    path.strip_prefix("./")
        .or_else(|| path.strip_prefix(".\\"))
        .unwrap_or(path)
}

/// Drop one trailing `\n` or `\r\n`, as `str::lines` does.
fn strip_eol(text: &str) -> &str {
    let text = text.strip_suffix('\n').unwrap_or(text);
    text.strip_suffix('\r').unwrap_or(text)
}

/// One `rg --json` line: a `match` message becomes a [`ToolLine`]; every
/// other message (`begin`, `end`, `summary`), and a path or line that isn't
/// UTF-8 (`rg` sends those as base64 `bytes`), is `None`.
#[must_use]
pub fn parse_rg_json(line: &str) -> Option<ToolLine> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "match" {
        return None;
    }
    let data = v.get("data")?;
    let path = data.get("path")?.get("text")?.as_str()?;
    let text = data.get("lines")?.get("text")?.as_str()?;
    let number = usize::try_from(data.get("line_number")?.as_u64()?).ok()?;
    Some(ToolLine {
        rel: strip_dot(path).to_string(),
        line: number,
        text: strip_eol(text).to_string(),
    })
}

/// One `grep -rnH` line, `path:line:text`. The path may itself hold
/// `:digits:`, so each candidate split is tried left to right and the first
/// whose path `known` accepts wins.
#[must_use]
pub fn parse_grep(line: &str, known: &dyn Fn(&str) -> bool) -> Option<ToolLine> {
    let line = strip_eol(line);
    for (i, _) in line.match_indices(':') {
        let rest = &line[i + 1..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || rest.as_bytes().get(digits) != Some(&b':') {
            continue;
        }
        let rel = strip_dot(&line[..i]);
        if !known(rel) {
            continue;
        }
        let number = rest[..digits].parse().ok()?;
        return Some(ToolLine {
            rel: rel.to_string(),
            line: number,
            text: rest[digits + 1..].to_string(),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_queries_match_the_panel_pattern() {
        let q = ToolQuery::search("a.b", false, false);
        assert_eq!(q.regex, r"(?i)a\.b");
        assert_eq!(q.fixed, Some(vec!["a.b".to_string()]));
        assert!(q.ignore_case);
        let q = ToolQuery::search("a.b", true, true);
        assert_eq!(q.regex, "a.b");
        assert_eq!(q.fixed, None);
        assert_eq!(Tool::Grep.args(&q), None, "grep can't run a Rust regex");
    }

    #[test]
    fn rg_runs_every_query_and_grep_only_fixed_ascii_folds() {
        let q = ToolQuery::any_of(&["TODO", "FIXME"]).max_filesize(10);
        assert_eq!(q.regex, "TODO|FIXME");
        assert_eq!(
            Tool::Rg.args(&q).unwrap(),
            [
                "--json",
                "--no-config",
                "--no-messages",
                "--hidden",
                "--max-filesize",
                "10",
                "-e",
                "TODO|FIXME",
                "--",
                "."
            ]
        );
        let grep = Tool::Grep.args(&q).unwrap();
        assert_eq!(&grep[..5], ["-r", "-n", "-I", "-H", "-F"]);
        assert!(!grep.contains(&"-i".to_string()));
        assert!(grep.ends_with(&[
            "-e".into(),
            "TODO".into(),
            "-e".into(),
            "FIXME".into(),
            "--".into(),
            ".".into()
        ]));
        assert!(
            Tool::Grep
                .args(&ToolQuery::search("ab", false, false))
                .unwrap()
                .contains(&"-i".to_string())
        );
        assert_eq!(
            Tool::Grep.args(&ToolQuery::search("éa", false, false)),
            None
        );
        assert!(
            Tool::Grep
                .args(&ToolQuery::search("éa", false, true))
                .is_some()
        );
        assert_eq!(Tool::Rg.program(), "rg");
        assert_eq!(Tool::Grep.program(), "grep");
    }

    #[test]
    fn rg_json_matches_parse_and_other_messages_do_not() {
        let m = r#"{"type":"match","data":{"path":{"text":"./src/a b.rs"},"lines":{"text":"let x = 1;\r\n"},"line_number":7,"absolute_offset":0,"submatches":[]}}"#;
        assert_eq!(
            parse_rg_json(m),
            Some(ToolLine {
                rel: "src/a b.rs".into(),
                line: 7,
                text: "let x = 1;".into()
            })
        );
        assert_eq!(
            parse_rg_json(r#"{"type":"begin","data":{"path":{"text":"a"}}}"#),
            None
        );
        let bytes = r#"{"type":"match","data":{"path":{"bytes":"/w=="},"lines":{"text":"x\n"},"line_number":1}}"#;
        assert_eq!(parse_rg_json(bytes), None);
        assert_eq!(parse_rg_json("not json"), None);
    }

    #[test]
    fn grep_lines_split_on_the_first_known_path() {
        let known = |p: &str| p == "a:1:b.txt" || p == "src/x.rs";
        assert_eq!(
            parse_grep("./src/x.rs:12:fn main() { a:3:b }\n", &known),
            Some(ToolLine {
                rel: "src/x.rs".into(),
                line: 12,
                text: "fn main() { a:3:b }".into()
            })
        );
        assert_eq!(
            Tool::Grep.parse("./a:1:b.txt:4:hit", &known),
            Some(ToolLine {
                rel: "a:1:b.txt".into(),
                line: 4,
                text: "hit".into()
            })
        );
        assert_eq!(parse_grep("./unknown.rs:1:x", &known), None);
        assert_eq!(parse_grep("Binary file x matches", &known), None);
    }
}
