//! Problem matchers (T632): turn a command's output into a list of
//! [`Problem`](crate::matcher::Problem)s — compiler errors, lint findings, failing tests — the way VS
//! Code's `problemMatcher`, Emacs's `compilation-mode` and Vim's quickfix list
//! do.
//!
//! A [`Scanner`](crate::matcher::Scanner) is fed one output line at a time, as the host streams it, and
//! keeps the small amount of state the multi-line formats need (a rustc header
//! waiting for its ` --> ` arrow, an eslint file-name line, a Python traceback's
//! innermost frame). [`ProblemList`](crate::matcher::ProblemList) holds a finished run's problems with a
//! wrap-around navigation cursor for next/previous error.
//!
//! Everything here is pure: no file is ever read. [`resolve_path`](crate::matcher::resolve_path) only joins
//! and folds path text; whether the file exists, and opening it, is the host's
//! job, done only when the user acts.

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use regex::{Captures, Regex};

/// The most problems one run keeps; output past it is no longer matched.
pub const MAX_PROBLEMS: usize = 5_000;

/// Bytes of a line considered for matching; the rest is ignored.
const MAX_LINE: usize = 4_096;

/// How serious a problem is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    /// An error (the default when a format names no severity).
    Error,
    /// A warning.
    Warning,
    /// Anything else a custom matcher reports (a note, a hint).
    Info,
}

impl Severity {
    /// Map free severity text (`error`, `Warning`, `fatal error`, `note`, …):
    /// text starting with `err`/`fatal` is an error, `warn` a warning, and
    /// anything else info.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let t = text.trim().to_ascii_lowercase();
        if t.starts_with("err") || t.starts_with("fatal") {
            Severity::Error
        } else if t.starts_with("warn") {
            Severity::Warning
        } else {
            Severity::Info
        }
    }

    /// The lowercase English tag (`error`, `warning`, `info`) used in
    /// tool-style rows such as the Problems panel's.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

/// A built-in matcher, one per tool family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Builtin {
    /// rustc and cargo: `error[E…]: msg` followed by ` --> file:line:col`, and
    /// panics.
    Rustc,
    /// gcc and clang: `file:line:col: error: msg`.
    Gcc,
    /// The TypeScript compiler: `file(line,col): error TS…: msg`.
    Tsc,
    /// eslint's stylish, compact and unix formats.
    Eslint,
    /// Python tracebacks and pytest's `path.py:N: Error` lines.
    Python,
    /// The Go toolchain: `file.go:line[:col]: msg`.
    Go,
}

impl Builtin {
    /// Every built-in matcher, in the order `auto` tries them.
    pub const ALL: [Builtin; 6] = [
        Builtin::Rustc,
        Builtin::Tsc,
        Builtin::Eslint,
        Builtin::Gcc,
        Builtin::Go,
        Builtin::Python,
    ];

    /// The matcher's name, as written in `tasks.toml` and shown in the
    /// Problems panel.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Builtin::Rustc => "rustc",
            Builtin::Gcc => "gcc",
            Builtin::Tsc => "tsc",
            Builtin::Eslint => "eslint",
            Builtin::Python => "python",
            Builtin::Go => "go",
        }
    }

    /// The built-in called `name` or one of its aliases (`cargo`, `clang`,
    /// `pytest`), ignoring ASCII case.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "rustc" | "cargo" | "rust" => Some(Builtin::Rustc),
            "gcc" | "clang" | "cc" => Some(Builtin::Gcc),
            "tsc" | "typescript" => Some(Builtin::Tsc),
            "eslint" => Some(Builtin::Eslint),
            "python" | "pytest" => Some(Builtin::Python),
            "go" => Some(Builtin::Go),
            _ => None,
        }
    }
}

/// Which matcher produced a problem.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// A built-in matcher.
    Builtin(Builtin),
    /// A task's own regular expression.
    Custom,
}

impl Source {
    /// The name shown with the problem (`rustc`, …, or `custom`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Source::Builtin(b) => b.name(),
            Source::Custom => "custom",
        }
    }
}

/// One problem found in command output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The file exactly as printed (relative to the run's cwd, or absolute).
    pub file: String,
    /// 1-based line.
    pub line: usize,
    /// 1-based column, when the format gives one.
    pub column: Option<usize>,
    /// How serious it is.
    pub severity: Severity,
    /// The one-line message.
    pub message: String,
    /// The matcher that found it.
    pub source: Source,
}

/// Why a `matcher = "…"` value was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatcherError {
    /// The value is neither a known name nor a valid regular expression.
    Regex(String),
    /// The regular expression lacks a required named group (`file`, `line`).
    MissingGroup(&'static str),
}

impl std::fmt::Display for MatcherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatcherError::Regex(e) => write!(f, "{e}"),
            MatcherError::MissingGroup(g) => write!(f, "missing named group (?P<{g}>…)"),
        }
    }
}

impl std::error::Error for MatcherError {}

/// What to match a run's output with.
#[derive(Clone, Debug, Default)]
pub enum MatcherSpec {
    /// Every built-in matcher (the default).
    #[default]
    Auto,
    /// No matching.
    Off,
    /// One built-in matcher.
    Builtin(Builtin),
    /// A custom regular expression with `file` and `line` named groups.
    Custom(Box<Regex>),
}

impl MatcherSpec {
    /// Parse a `tasks.toml` `matcher` value: empty or `auto`, `none`/`off`, a
    /// built-in name or alias, or else a custom regular expression with the
    /// named groups `file` and `line` (and optionally `column`, `severity`,
    /// `message`).
    ///
    /// # Errors
    ///
    /// [`MatcherError::Regex`] for an invalid expression, and
    /// [`MatcherError::MissingGroup`] when `file` or `line` is missing.
    pub fn parse(value: &str) -> Result<Self, MatcherError> {
        let v = value.trim();
        match v.to_ascii_lowercase().as_str() {
            "" | "auto" => return Ok(MatcherSpec::Auto),
            "none" | "off" => return Ok(MatcherSpec::Off),
            _ => {}
        }
        if let Some(b) = Builtin::from_name(v) {
            return Ok(MatcherSpec::Builtin(b));
        }
        let re = Regex::new(v).map_err(|e| MatcherError::Regex(e.to_string()))?;
        for group in ["file", "line"] {
            if !re.capture_names().any(|n| n == Some(group)) {
                return Err(MatcherError::MissingGroup(group));
            }
        }
        Ok(MatcherSpec::Custom(Box::new(re)))
    }

    /// Whether this spec runs `builtin`.
    fn runs(&self, builtin: Builtin) -> bool {
        match self {
            MatcherSpec::Auto => true,
            MatcherSpec::Builtin(b) => *b == builtin,
            MatcherSpec::Off | MatcherSpec::Custom(_) => false,
        }
    }
}

/// Compile one of this module's fixed patterns.
#[expect(
    clippy::expect_used,
    reason = "fixed regex literals, compiled once and covered by tests"
)]
fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("built-in matcher pattern")
}

static ANSI: LazyLock<Regex> = LazyLock::new(|| re(r"\x1b\[[0-9;?]*[ -/]*[@-~]"));

static RUST_HEADER: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(error|warning|note|help)(?:\[[A-Za-z0-9_]+\])?: (.+)$"));
static RUST_ARROW: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*--> (.+?):(\d+):(\d+)\s*$"));
static RUST_PANIC: LazyLock<Regex> =
    LazyLock::new(|| re(r"^thread '.*' panicked at (.+?):(\d+):(\d+):?\s*$"));
static RUST_PANIC_OLD: LazyLock<Regex> =
    LazyLock::new(|| re(r"^thread '.*' panicked at '(.*)', (.+?):(\d+):(\d+)\s*$"));

static GCC: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(\S.*?):(\d+):(?:(\d+):)? (fatal error|error|warning): (.+)$"));

static TSC_PAREN: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(\S.*?)\((\d+),(\d+)\): (error|warning) (TS\d+): (.+)$"));
static TSC_PRETTY: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(\S.*?):(\d+):(\d+) - (error|warning) (TS\d+): (.+)$"));

static ESLINT_ROW: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\s+(\d+):(\d+)\s+(error|warning)\s+(.+?)(?:\s{2,}(\S+))?\s*$"));
static ESLINT_COMPACT: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(\S.*?): line (\d+), col (\d+), (Error|Warning) - (.+)$"));
static ESLINT_UNIX: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(\S.*?):(\d+):(\d+): (.+) \[(Error|Warning)(?:/(\S+))?\]$"));

static PY_FRAME: LazyLock<Regex> =
    LazyLock::new(|| re(r#"^\s*File "(.+)", line (\d+)(?:, in .*)?\s*$"#));
static PY_EXCEPTION: LazyLock<Regex> =
    LazyLock::new(|| re(r"^([A-Za-z_][A-Za-z0-9_.]*)(?::\s?(.*))?$"));
static PYTEST_LOCATION: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*(\S[^:]*\.py):(\d+): (.+)$"));

static GO: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*(\S+\.go):(\d+)(?::(\d+))?: (.+)$"));

/// `text` without ANSI color/cursor escape sequences.
#[must_use]
pub fn strip_ansi(text: &str) -> Cow<'_, str> {
    if text.contains('\x1b') {
        ANSI.replace_all(text, "")
    } else {
        Cow::Borrowed(text)
    }
}

/// `line` cut to at most [`MAX_LINE`] bytes, on a character boundary.
fn clip(line: &str) -> &str {
    if line.len() <= MAX_LINE {
        return line;
    }
    let mut end = MAX_LINE;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    &line[..end]
}

/// Group `i` of `caps` parsed as a number.
fn num(caps: &Captures<'_>, i: usize) -> Option<usize> {
    caps.get(i)?.as_str().parse().ok()
}

/// Group `i` of `caps` as text (empty when absent).
fn text<'a>(caps: &Captures<'a>, i: usize) -> &'a str {
    caps.get(i).map_or("", |m| m.as_str())
}

/// A Python pseudo-file such as `<string>` or `<frozen importlib._bootstrap>`.
fn is_pseudo_file(file: &str) -> bool {
    file.starts_with('<') && file.ends_with('>')
}

/// Multi-line state carried between lines.
#[derive(Debug, Default)]
struct State {
    /// A rustc `error`/`warning` header waiting for its arrow.
    rust_header: Option<(Severity, String)>,
    /// Index of a just-emitted new-format panic whose message is the next line.
    panic_open: Option<usize>,
    /// The file named by the latest eslint stylish header line.
    eslint_file: Option<String>,
    /// The innermost frame of the Python traceback being read.
    py_frame: Option<(String, usize)>,
}

/// Feeds output lines through a [`MatcherSpec`] and collects the problems.
#[derive(Debug)]
pub struct Scanner {
    spec: MatcherSpec,
    state: State,
    problems: Vec<Problem>,
    seen: HashSet<(String, usize, Option<usize>, String)>,
}

impl Scanner {
    /// A scanner with no problems yet.
    #[must_use]
    pub fn new(spec: MatcherSpec) -> Self {
        Scanner {
            spec,
            state: State::default(),
            problems: Vec::new(),
            seen: HashSet::new(),
        }
    }

    /// The problems found so far, in output order.
    #[must_use]
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// The problems found, in output order.
    #[must_use]
    pub fn into_problems(self) -> Vec<Problem> {
        self.problems
    }

    /// Match one output line (ANSI escapes are stripped first).
    pub fn push_line(&mut self, raw: &str) {
        if self.problems.len() >= MAX_PROBLEMS || matches!(self.spec, MatcherSpec::Off) {
            return;
        }
        let stripped = strip_ansi(raw);
        let line = clip(stripped.trim_end_matches(['\r', '\n']));
        if let Some(i) = self.state.panic_open.take() {
            let message = line.trim();
            if !message.is_empty()
                && let Some(p) = self.problems.get_mut(i)
            {
                p.message = message.to_string();
                return;
            }
        }
        if let MatcherSpec::Custom(re) = &self.spec {
            let found = re
                .captures(line)
                .and_then(|caps| custom_problem(&caps, line));
            if let Some(p) = found {
                self.emit(p);
            }
            return;
        }
        let _ = self.try_python(line)
            || self.try_rustc(line)
            || self.try_tsc(line)
            || self.try_eslint(line)
            || self.try_gcc(line)
            || self.try_go(line)
            || self.try_pytest(line)
            || self.note_eslint_header(line);
    }

    /// Keep `p` unless the same location and message was already kept.
    fn emit(&mut self, p: Problem) {
        if self.problems.len() >= MAX_PROBLEMS || p.file.is_empty() || p.line == 0 {
            return;
        }
        let key = (p.file.clone(), p.line, p.column, p.message.clone());
        if self.seen.insert(key) {
            self.problems.push(p);
        }
    }

    fn try_python(&mut self, line: &str) -> bool {
        if !self.spec.runs(Builtin::Python) {
            return false;
        }
        if let Some(c) = PY_FRAME.captures(line) {
            let file = text(&c, 1);
            if !is_pseudo_file(file)
                && let Some(n) = num(&c, 2)
            {
                self.state.py_frame = Some((file.to_string(), n));
            }
            return true;
        }
        if self.state.py_frame.is_some() && !line.starts_with(char::is_whitespace) {
            if let Some(c) = PY_EXCEPTION.captures(line)
                && let Some((file, n)) = self.state.py_frame.take()
            {
                let name = text(&c, 1);
                let severity = if name.ends_with("Warning") {
                    Severity::Warning
                } else {
                    Severity::Error
                };
                self.emit(Problem {
                    file,
                    line: n,
                    column: None,
                    severity,
                    message: line.trim().to_string(),
                    source: Source::Builtin(Builtin::Python),
                });
                return true;
            }
            if line.starts_with("Traceback ") || line.starts_with("During handling") {
                return true;
            }
            // Any other unindented text ends the traceback without a problem.
            if !line.is_empty() {
                self.state.py_frame = None;
            }
        }
        false
    }

    fn try_rustc(&mut self, line: &str) -> bool {
        if !self.spec.runs(Builtin::Rustc) {
            return false;
        }
        if let Some(c) = RUST_HEADER.captures(line) {
            self.state.rust_header = match text(&c, 1) {
                "error" => Some((Severity::Error, text(&c, 2).to_string())),
                "warning" => Some((Severity::Warning, text(&c, 2).to_string())),
                // A note/help block's arrow is not a problem of its own.
                _ => None,
            };
            return true;
        }
        if let Some(c) = RUST_ARROW.captures(line) {
            if let Some((severity, message)) = self.state.rust_header.take()
                && let Some(n) = num(&c, 2)
            {
                self.emit(Problem {
                    file: text(&c, 1).to_string(),
                    line: n,
                    column: num(&c, 3),
                    severity,
                    message,
                    source: Source::Builtin(Builtin::Rustc),
                });
            }
            return true;
        }
        if let Some(c) = RUST_PANIC_OLD.captures(line) {
            if let Some(n) = num(&c, 3) {
                self.emit(Problem {
                    file: text(&c, 2).to_string(),
                    line: n,
                    column: num(&c, 4),
                    severity: Severity::Error,
                    message: text(&c, 1).to_string(),
                    source: Source::Builtin(Builtin::Rustc),
                });
            }
            return true;
        }
        if let Some(c) = RUST_PANIC.captures(line) {
            if let Some(n) = num(&c, 2) {
                let before = self.problems.len();
                self.emit(Problem {
                    file: text(&c, 1).to_string(),
                    line: n,
                    column: num(&c, 3),
                    severity: Severity::Error,
                    message: "panicked".to_string(),
                    source: Source::Builtin(Builtin::Rustc),
                });
                if self.problems.len() > before {
                    self.state.panic_open = Some(before);
                }
            }
            return true;
        }
        false
    }

    fn try_tsc(&mut self, line: &str) -> bool {
        if !self.spec.runs(Builtin::Tsc) {
            return false;
        }
        let Some(c) = TSC_PAREN
            .captures(line)
            .or_else(|| TSC_PRETTY.captures(line))
        else {
            return false;
        };
        if let Some(n) = num(&c, 2) {
            self.emit(Problem {
                file: text(&c, 1).to_string(),
                line: n,
                column: num(&c, 3),
                severity: Severity::from_text(text(&c, 4)),
                message: format!("{}: {}", text(&c, 5), text(&c, 6)),
                source: Source::Builtin(Builtin::Tsc),
            });
        }
        true
    }

    fn try_eslint(&mut self, line: &str) -> bool {
        if !self.spec.runs(Builtin::Eslint) {
            return false;
        }
        if let Some(c) = ESLINT_ROW.captures(line) {
            let Some(file) = self.state.eslint_file.clone() else {
                return false;
            };
            if let Some(n) = num(&c, 1) {
                let rule = text(&c, 5);
                let message = if rule.is_empty() {
                    text(&c, 4).to_string()
                } else {
                    format!("{} ({rule})", text(&c, 4))
                };
                self.emit(Problem {
                    file,
                    line: n,
                    column: num(&c, 2),
                    severity: Severity::from_text(text(&c, 3)),
                    message,
                    source: Source::Builtin(Builtin::Eslint),
                });
            }
            return true;
        }
        if let Some(c) = ESLINT_COMPACT.captures(line) {
            if let Some(n) = num(&c, 2) {
                self.emit(Problem {
                    file: text(&c, 1).to_string(),
                    line: n,
                    column: num(&c, 3),
                    severity: Severity::from_text(text(&c, 4)),
                    message: text(&c, 5).to_string(),
                    source: Source::Builtin(Builtin::Eslint),
                });
            }
            return true;
        }
        if let Some(c) = ESLINT_UNIX.captures(line) {
            if let Some(n) = num(&c, 2) {
                let rule = text(&c, 6);
                let message = if rule.is_empty() {
                    text(&c, 4).to_string()
                } else {
                    format!("{} ({rule})", text(&c, 4))
                };
                self.emit(Problem {
                    file: text(&c, 1).to_string(),
                    line: n,
                    column: num(&c, 3),
                    severity: Severity::from_text(text(&c, 5)),
                    message,
                    source: Source::Builtin(Builtin::Eslint),
                });
            }
            return true;
        }
        false
    }

    /// Remember an unindented, path-like line as the eslint stylish file
    /// header for the indented rows that follow; a `✖` summary ends the block.
    fn note_eslint_header(&mut self, line: &str) -> bool {
        if !self.spec.runs(Builtin::Eslint) {
            return false;
        }
        let t = line.trim_end();
        if t.is_empty() || t.starts_with(char::is_whitespace) {
            return false;
        }
        if t.starts_with('\u{2716}') || t.starts_with('\u{2714}') {
            self.state.eslint_file = None;
            return false;
        }
        let path_like = (t.contains('/') || t.contains('\\') || t.contains('.'))
            && !t.contains(": ")
            && !t.starts_with('$')
            && !t.starts_with('[');
        self.state.eslint_file = path_like.then(|| t.to_string());
        path_like
    }

    fn try_gcc(&mut self, line: &str) -> bool {
        if !self.spec.runs(Builtin::Gcc) {
            return false;
        }
        let Some(c) = GCC.captures(line) else {
            return false;
        };
        if let Some(n) = num(&c, 2) {
            self.emit(Problem {
                file: text(&c, 1).to_string(),
                line: n,
                column: num(&c, 3),
                severity: Severity::from_text(text(&c, 4)),
                message: text(&c, 5).to_string(),
                source: Source::Builtin(Builtin::Gcc),
            });
        }
        true
    }

    fn try_go(&mut self, line: &str) -> bool {
        if !self.spec.runs(Builtin::Go) {
            return false;
        }
        let Some(c) = GO.captures(line) else {
            return false;
        };
        if let Some(n) = num(&c, 2) {
            self.emit(Problem {
                file: text(&c, 1).to_string(),
                line: n,
                column: num(&c, 3),
                severity: Severity::Error,
                message: text(&c, 4).trim().to_string(),
                source: Source::Builtin(Builtin::Go),
            });
        }
        true
    }

    fn try_pytest(&mut self, line: &str) -> bool {
        if !self.spec.runs(Builtin::Python) {
            return false;
        }
        let Some(c) = PYTEST_LOCATION.captures(line) else {
            return false;
        };
        let message = text(&c, 3).trim();
        // `path.py:12: in test_x` is a traceback frame, not the failure.
        if message.starts_with("in ") {
            return true;
        }
        if let Some(n) = num(&c, 2) {
            let severity = if message
                .split(':')
                .next()
                .is_some_and(|s| s.ends_with("Warning"))
            {
                Severity::Warning
            } else {
                Severity::Error
            };
            self.emit(Problem {
                file: text(&c, 1).to_string(),
                line: n,
                column: None,
                severity,
                message: message.to_string(),
                source: Source::Builtin(Builtin::Python),
            });
        }
        true
    }
}

/// A problem from a custom matcher's captures (`file`, `line`, and optional
/// `column`, `severity`, `message`).
fn custom_problem(caps: &Captures<'_>, line: &str) -> Option<Problem> {
    let file = caps.name("file")?.as_str().trim();
    let line_no = caps.name("line")?.as_str().parse().ok()?;
    let column = caps.name("column").and_then(|m| m.as_str().parse().ok());
    let severity = caps
        .name("severity")
        .map_or(Severity::Error, |m| Severity::from_text(m.as_str()));
    let message = caps
        .name("message")
        .map_or_else(|| line.trim(), |m| m.as_str().trim());
    Some(Problem {
        file: file.to_string(),
        line: line_no,
        column,
        severity,
        message: message.to_string(),
        source: Source::Custom,
    })
}

/// Every problem `spec` finds in `output`, scanned line by line.
#[must_use]
pub fn scan(spec: MatcherSpec, output: &str) -> Vec<Problem> {
    let mut s = Scanner::new(spec);
    for line in output.lines() {
        s.push_line(line);
    }
    s.into_problems()
}

/// The `(file, line, column)` one output line names on its own, by any
/// built-in single-line pattern, the rustc ` --> ` arrow, or a Python
/// `File "…", line N` frame — the location a click on that line jumps to.
#[must_use]
pub fn locate(line: &str) -> Option<(String, usize, Option<usize>)> {
    let stripped = strip_ansi(line);
    let line = clip(stripped.trim_end());
    if let Some(c) = RUST_ARROW.captures(line) {
        return Some((text(&c, 1).to_string(), num(&c, 2)?, num(&c, 3)));
    }
    if let Some(c) = PY_FRAME.captures(line) {
        let file = text(&c, 1);
        return (!is_pseudo_file(file)).then_some((file.to_string(), num(&c, 2)?, None));
    }
    if let Some(c) = RUST_PANIC_OLD.captures(line) {
        return Some((text(&c, 2).to_string(), num(&c, 3)?, num(&c, 4)));
    }
    if let Some(c) = RUST_PANIC.captures(line) {
        return Some((text(&c, 1).to_string(), num(&c, 2)?, num(&c, 3)));
    }
    // Each of these has the file, line and column in groups 1, 2 and 3.
    let patterns = [
        &*TSC_PAREN,
        &*TSC_PRETTY,
        &*ESLINT_COMPACT,
        &*ESLINT_UNIX,
        &*GCC,
        &*GO,
    ];
    for pattern in patterns {
        if let Some(c) = pattern.captures(line) {
            return Some((text(&c, 1).to_string(), num(&c, 2)?, num(&c, 3)));
        }
    }
    let c = PYTEST_LOCATION.captures(line)?;
    Some((text(&c, 1).to_string(), num(&c, 2)?, None))
}

/// Resolve a problem's `file` against the `cwd` of the run that printed it:
/// an absolute path is kept, a relative one joined to `cwd`, and `.`/`..`
/// folded lexically (a `..` never climbs above the path's root). Nothing is
/// read from the filesystem.
#[must_use]
pub fn resolve_path(cwd: &Path, file: &str) -> PathBuf {
    let p = Path::new(file);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    };
    let mut out = PathBuf::new();
    for comp in joined.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !matches!(
                    out.components().next_back(),
                    None | Some(Component::RootDir | Component::Prefix(_))
                ) {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// One run's problems, the cwd they resolve against, and a next/previous
/// cursor that wraps around.
#[derive(Clone, Debug, Default)]
pub struct ProblemList {
    problems: Vec<Problem>,
    /// Each problem's file resolved against `cwd`, computed once.
    paths: Vec<PathBuf>,
    cwd: PathBuf,
    current: Option<usize>,
}

/// Where a [`ProblemList::step`] landed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// Index of the problem now current.
    pub index: usize,
    /// Whether the step wrapped past the end (or the start).
    pub wrapped: bool,
}

impl ProblemList {
    /// A list of `problems` printed by a run in `cwd`, with no current one.
    #[must_use]
    pub fn new(cwd: PathBuf, problems: Vec<Problem>) -> Self {
        let paths = problems
            .iter()
            .map(|p| resolve_path(&cwd, &p.file))
            .collect();
        ProblemList {
            problems,
            paths,
            cwd,
            current: None,
        }
    }

    /// The problems, in output order.
    #[must_use]
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// The run's working directory.
    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// How many problems there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.problems.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.problems.is_empty()
    }

    /// The index of the current problem, once one has been visited.
    #[must_use]
    pub fn current(&self) -> Option<usize> {
        self.current
    }

    /// Problem `i`'s file resolved against the run's cwd ([`resolve_path`]).
    #[must_use]
    pub fn path(&self, i: usize) -> Option<&Path> {
        self.paths.get(i).map(PathBuf::as_path)
    }

    /// Move to the next (`forward`) or previous problem, wrapping around at
    /// either end. Starting fresh, forward visits the first problem and
    /// backward the last. `None` when the list is empty.
    pub fn step(&mut self, forward: bool) -> Option<Step> {
        let n = self.problems.len();
        if n == 0 {
            return None;
        }
        let (index, wrapped) = match (self.current, forward) {
            (None, true) => (0, false),
            (None, false) => (n - 1, false),
            (Some(i), true) if i + 1 >= n => (0, true),
            (Some(i), true) => (i + 1, false),
            (Some(0), false) => (n - 1, true),
            (Some(i), false) => (i.min(n) - 1, false),
        };
        self.current = Some(index);
        Some(Step { index, wrapped })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(spec: MatcherSpec, text: &str) -> Problem {
        let found = scan(spec, text);
        assert_eq!(found.len(), 1, "{found:#?}");
        found.into_iter().next().expect("one problem")
    }

    #[test]
    fn parse_spec_names_aliases_and_regex() {
        assert!(matches!(MatcherSpec::parse(""), Ok(MatcherSpec::Auto)));
        assert!(matches!(MatcherSpec::parse("AUTO"), Ok(MatcherSpec::Auto)));
        assert!(matches!(MatcherSpec::parse("none"), Ok(MatcherSpec::Off)));
        assert!(matches!(
            MatcherSpec::parse("cargo"),
            Ok(MatcherSpec::Builtin(Builtin::Rustc))
        ));
        assert!(matches!(
            MatcherSpec::parse("pytest"),
            Ok(MatcherSpec::Builtin(Builtin::Python))
        ));
        assert!(matches!(
            MatcherSpec::parse(r"^(?P<file>[^:]+):(?P<line>\d+)"),
            Ok(MatcherSpec::Custom(_))
        ));
        assert_eq!(
            MatcherSpec::parse(r"^(?P<file>[^:]+):\d+").err(),
            Some(MatcherError::MissingGroup("line"))
        );
        assert!(matches!(
            MatcherSpec::parse("(unclosed"),
            Err(MatcherError::Regex(_))
        ));
        for b in Builtin::ALL {
            assert_eq!(Builtin::from_name(b.name()), Some(b));
        }
    }

    #[test]
    fn custom_regex_uses_named_groups() {
        let spec = MatcherSpec::parse(
            r"^(?P<severity>\w+) at (?P<file>\S+) line (?P<line>\d+): (?P<message>.*)$",
        )
        .expect("valid");
        let p = one(spec, "warn at lib/a.pl line 7: odd number\nnothing here");
        assert_eq!(p.file, "lib/a.pl");
        assert_eq!(p.line, 7);
        assert_eq!(p.column, None);
        assert_eq!(p.severity, Severity::Warning);
        assert_eq!(p.message, "odd number");
        assert_eq!(p.source, Source::Custom);
    }

    #[test]
    fn off_matches_nothing_and_a_builtin_only_itself() {
        let gcc = "a.c:1:2: error: boom";
        assert_eq!(scan(MatcherSpec::Off, gcc), []);
        assert_eq!(scan(MatcherSpec::Builtin(Builtin::Go), gcc), []);
        assert_eq!(scan(MatcherSpec::Builtin(Builtin::Gcc), gcc).len(), 1);
    }

    #[test]
    fn ansi_colors_are_stripped_and_duplicates_dropped() {
        let text = "\x1b[1m\x1b[31ma.c:3:4: error: bad\x1b[0m\na.c:3:4: error: bad\n";
        let p = one(MatcherSpec::Auto, text);
        assert_eq!((p.file.as_str(), p.line, p.column), ("a.c", 3, Some(4)));
        assert_eq!(p.message, "bad");
    }

    #[test]
    fn rustc_note_arrows_and_orphan_arrows_are_ignored() {
        let text =
            "note: the lint level is defined here\n --> src/lib.rs:1:9\n  --> src/x.rs:2:3\n";
        assert_eq!(scan(MatcherSpec::Auto, text), []);
    }

    #[test]
    fn rustc_panic_takes_the_next_line_as_message() {
        let text = "thread 'main' panicked at src/main.rs:4:5:\nattempt to divide by zero\nnote: run with `RUST_BACKTRACE=1`";
        let p = one(MatcherSpec::Auto, text);
        assert_eq!(
            (p.file.as_str(), p.line, p.column),
            ("src/main.rs", 4, Some(5))
        );
        assert_eq!(p.message, "attempt to divide by zero");
        let old = "thread 'main' panicked at 'boom', src/main.rs:2:5";
        assert_eq!(one(MatcherSpec::Auto, old).message, "boom");
    }

    #[test]
    fn python_pseudo_frames_are_skipped() {
        let text = "Traceback (most recent call last):\n  File \"app.py\", line 3, in <module>\n  File \"<string>\", line 1, in <module>\nValueError: nope";
        let p = one(MatcherSpec::Auto, text);
        assert_eq!((p.file.as_str(), p.line), ("app.py", 3));
        assert_eq!(p.message, "ValueError: nope");
    }

    #[test]
    fn locate_finds_single_line_locations() {
        assert_eq!(
            locate("  --> src/main.rs:4:18"),
            Some(("src/main.rs".into(), 4, Some(18)))
        );
        assert_eq!(
            locate("  File \"pkg/a.py\", line 9, in f"),
            Some(("pkg/a.py".into(), 9, None))
        );
        assert_eq!(
            locate("src/a.ts(3,7): error TS2322: no"),
            Some(("src/a.ts".into(), 3, Some(7)))
        );
        assert_eq!(
            locate("    calc_test.go:12: got 3"),
            Some(("calc_test.go".into(), 12, None))
        );
        assert_eq!(locate("just some text"), None);
    }

    #[test]
    fn resolve_path_joins_and_folds() {
        let cwd = Path::new("/w/sub");
        assert_eq!(
            resolve_path(cwd, "src/a.rs"),
            PathBuf::from("/w/sub/src/a.rs")
        );
        assert_eq!(resolve_path(cwd, "./a.go"), PathBuf::from("/w/sub/a.go"));
        assert_eq!(resolve_path(cwd, "../b.c"), PathBuf::from("/w/b.c"));
        assert_eq!(resolve_path(cwd, "/abs/x.py"), PathBuf::from("/abs/x.py"));
        assert_eq!(resolve_path(cwd, "../../../../x"), PathBuf::from("/x"));
    }

    fn list(n: usize) -> ProblemList {
        let problems = (1..=n)
            .map(|i| Problem {
                file: "a.c".into(),
                line: i,
                column: None,
                severity: Severity::Error,
                message: String::new(),
                source: Source::Builtin(Builtin::Gcc),
            })
            .collect();
        ProblemList::new(PathBuf::from("/w"), problems)
    }

    #[test]
    fn step_wraps_around_both_ways() {
        let mut l = list(3);
        let fwd: Vec<_> = (0..4).filter_map(|_| l.step(true)).collect();
        assert_eq!(
            fwd.iter().map(|s| (s.index, s.wrapped)).collect::<Vec<_>>(),
            [(0, false), (1, false), (2, false), (0, true)]
        );
        assert_eq!(
            l.step(false),
            Some(Step {
                index: 2,
                wrapped: true
            })
        );
        assert_eq!(
            l.step(false),
            Some(Step {
                index: 1,
                wrapped: false
            })
        );
        let mut fresh = list(3);
        assert_eq!(
            fresh.step(false),
            Some(Step {
                index: 2,
                wrapped: false
            })
        );
        assert_eq!(ProblemList::default().step(true), None);
        assert_eq!(l.path(0), Some(Path::new("/w/a.c")));
    }

    #[test]
    fn problem_count_is_capped() {
        let mut s = Scanner::new(MatcherSpec::Auto);
        for i in 0..MAX_PROBLEMS + 10 {
            s.push_line(&format!("a.c:{}:1: error: e", i + 1));
        }
        assert_eq!(s.problems().len(), MAX_PROBLEMS);
    }
}
