//! Code references inside message text: `path:line`, `path:line:col` and
//! `path:line-line`, and the pure path logic that keeps one inside the
//! workspace root.
//!
//! Detection ([`find_refs`], [`parse_ref`]) is purely textual and
//! conservative (a clock time like `10:30` must not look like a file
//! position); [`resolve_in_root`] is the security boundary that stops a
//! received reference such as `../../etc/passwd:1` from opening files outside
//! the workspace. Owning spec: `crates/vix-im/spec/index.md`.
//!
//! ```
//! use vix_im::{find_refs, parse_ref};
//! let found = find_refs("see (src/a.rs:3-5), not 10:30");
//! assert_eq!(found.len(), 1);
//! assert_eq!(found[0].reference, parse_ref("src/a.rs:3-5").unwrap());
//! ```

use std::fmt;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

/// A reference to a place in a file. Lines and columns are 1-based.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CodeRef {
    /// The path as written in the message (relative to the workspace root,
    /// or absolute).
    pub path: String,
    /// The first line.
    pub line: u32,
    /// The column, for `path:line:col`.
    pub column: Option<u32>,
    /// The last line, for `path:line-line`.
    pub end_line: Option<u32>,
}

impl CodeRef {
    /// A reference to `path` at `line`.
    #[must_use]
    pub fn new(path: impl Into<String>, line: u32) -> CodeRef {
        CodeRef {
            path: path.into(),
            line,
            column: None,
            end_line: None,
        }
    }
}

impl fmt::Display for CodeRef {
    /// Written back in the form it is parsed from.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.path, self.line)?;
        if let Some(end) = self.end_line {
            write!(f, "-{end}")
        } else if let Some(col) = self.column {
            write!(f, ":{col}")
        } else {
            Ok(())
        }
    }
}

/// One reference found in a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRefMatch {
    /// The byte range of the reference in the body (without surrounding
    /// quotes, brackets or trailing punctuation).
    pub range: Range<usize>,
    /// What it refers to.
    pub reference: CodeRef,
}

/// Characters stripped from the front of a word before it is parsed.
const OPENERS: &[char] = &[
    '(', '[', '{', '<', '"', '\'', '`', '\u{201C}', '\u{2018}', '\u{300C}', '\u{300A}', '\u{FF08}',
];
/// Characters stripped from the end of a word (sentence punctuation and
/// closing brackets/quotes, including CJK, Arabic and Devanagari forms).
const CLOSERS: &[char] = &[
    ')', ']', '}', '>', '"', '\'', '`', ',', '.', ';', ':', '!', '?', '\u{201D}', '\u{2019}',
    '\u{300D}', '\u{300B}', '\u{FF09}', '\u{3002}', '\u{FF0C}', '\u{060C}', '\u{0964}',
];

/// Every code reference in `body`, in order.
///
/// A reference is one whitespace-separated word, less any opening quote or
/// bracket and trailing punctuation, of the form `PATH:LINE`,
/// `PATH:LINE:COL` or `PATH:LINE-END`. So that a time (`10:30`) or a ratio
/// is not taken for one, `PATH` must hold a letter and either a `/`, a `\`
/// or a `.`; a URL (anything with `://`) is never a reference. Lines and
/// columns start at 1, and an end line before its start line is rejected.
#[must_use]
pub fn find_refs(body: &str) -> Vec<CodeRefMatch> {
    let mut found = Vec::new();
    let mut start = None;
    for (i, c) in body
        .char_indices()
        .chain(std::iter::once((body.len(), ' ')))
    {
        match (c.is_whitespace(), start) {
            (true, Some(s)) => {
                if let Some(m) = word_ref(body, s, i) {
                    found.push(m);
                }
                start = None;
            }
            (false, None) => start = Some(i),
            _ => {}
        }
    }
    found
}

/// Parse one word `body[start..end]`.
fn word_ref(body: &str, start: usize, end: usize) -> Option<CodeRefMatch> {
    let word = &body[start..end];
    let trimmed_front = word.trim_start_matches(OPENERS);
    let from = start + (word.len() - trimmed_front.len());
    let core = trimmed_front.trim_end_matches(CLOSERS);
    let reference = parse_ref(core)?;
    Some(CodeRefMatch {
        range: from..from + core.len(),
        reference,
    })
}

/// Parse exactly one reference, with nothing around it. `None` for a URL or
/// anything that does not look like a path followed by a 1-based position.
#[must_use]
pub fn parse_ref(text: &str) -> Option<CodeRef> {
    if text.contains("://") {
        return None;
    }
    // The first `:` whose remainder is a valid position splits path from
    // position, so `C:\x.rs:3` and `a:b/c.rs:3` still find their path.
    text.match_indices(':').find_map(|(at, _)| {
        let path = &text[..at];
        let (line, column, end_line) = parse_position(&text[at + 1..])?;
        looks_like_path(path).then(|| CodeRef {
            path: path.to_string(),
            line,
            column,
            end_line,
        })
    })
}

/// The heuristic that keeps times and ratios out: needs a letter and one of
/// `/`, `\` or `.`.
fn looks_like_path(path: &str) -> bool {
    // `a.rs:1:2:3` is not `a.rs:1` at line 2, column 3.
    let ends_in_position = path
        .rsplit_once(':')
        .is_some_and(|(_, tail)| number(tail).is_some());
    !ends_in_position && path.chars().any(char::is_alphabetic) && path.contains(['/', '\\', '.'])
}

/// `(line, column, end_line)` as parsed from the text after the path.
type Position = (u32, Option<u32>, Option<u32>);

/// Parse `LINE`, `LINE:COL` or `LINE-END`; an `END` before `LINE` is
/// rejected.
fn parse_position(text: &str) -> Option<Position> {
    if let Some((line, col)) = text.split_once(':') {
        Some((number(line)?, Some(number(col)?), None))
    } else if let Some((line, end)) = text.split_once('-') {
        let (line, end) = (number(line)?, number(end)?);
        (end >= line).then_some((line, None, Some(end)))
    } else {
        Some((number(text)?, None, None))
    }
}

/// A positive decimal number, digits only.
fn number(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|&n| n > 0)
}

/// Where `reference` points inside `root`, or `None` if it would leave it.
///
/// Pure path logic, with no filesystem access: `.` components are dropped,
/// `..` pops a component and may not climb above `root`, and an absolute
/// path is accepted only when, once normalized the same way, it lies under
/// `root`. The caller checks that the file exists, and should compare
/// canonical paths if symbolic links inside the workspace matter.
#[must_use]
pub fn resolve_in_root(root: &Path, reference: &CodeRef) -> Option<PathBuf> {
    let path = Path::new(&reference.path);
    // An empty path would resolve to the root itself, which is a directory.
    if reference.path.is_empty() {
        return None;
    }
    let root = normalize(root)?;
    if path.has_root() || path.is_absolute() {
        let full = normalize(path)?;
        // Component-wise `starts_with`, so `/work2` is not "under" `/work`.
        return (full.starts_with(&root) && full != root).then_some(full);
    }
    let mut parts: Vec<&std::ffi::OsStr> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                // `..` past the start (pop on empty) escapes the root: reject.
                parts.pop()?;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    let mut full = root;
    full.extend(parts);
    Some(full)
}

/// Lexically normalize an absolute or relative path; `None` if `..` climbs
/// above its start.
fn normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    let mut depth = 0usize;
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                // `depth` counts normal components pushed so far; a `..`
                // with none left would climb above the start.
                depth = depth.checked_sub(1)?;
                out.pop();
            }
            Component::Normal(part) => {
                depth += 1;
                out.push(part);
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(body: &str) -> Vec<String> {
        find_refs(body)
            .into_iter()
            .map(|m| {
                assert_eq!(&body[m.range.clone()], m.reference.to_string());
                m.reference.to_string()
            })
            .collect()
    }

    #[test]
    fn the_three_forms() {
        assert_eq!(
            refs("see src/app.rs:10, then (lib.rs:3:7) and `a/b.rs:4-9`."),
            ["src/app.rs:10", "lib.rs:3:7", "a/b.rs:4-9"]
        );
    }

    #[test]
    fn parsed_fields() {
        let m = &find_refs("x crates/a.rs:12:5")[0];
        assert_eq!(m.range, 2..18);
        assert_eq!(m.reference.path, "crates/a.rs");
        assert_eq!(m.reference.line, 12);
        assert_eq!(m.reference.column, Some(5));
        let r = parse_ref("a.rs:4-9").unwrap();
        assert_eq!((r.line, r.end_line, r.column), (4, Some(9), None));
    }

    #[test]
    fn not_references() {
        assert_eq!(
            refs("at 10:30 or 3:2 odds, https://x.org/a.rs:3"),
            Vec::<String>::new()
        );
        assert_eq!(
            refs("a.rs:0 a.rs:9-3 a.rs: a.rs:x a.rs:1:2:3 Makefile:3"),
            Vec::<String>::new()
        );
        assert_eq!(refs("a.rs:-3 a.rs:3- a.rs:+3"), Vec::<String>::new());
    }

    #[test]
    fn non_ascii_text_around_and_in_paths() {
        assert_eq!(
            refs("بنگرید به docs/راهنما.md:12 لطفاً"),
            ["docs/راهنما.md:12"]
        );
        assert_eq!(refs("देखें 「src/a.rs:2」।"), ["src/a.rs:2"]);
        assert_eq!(refs("见 （src/a.rs:2-3）。"), ["src/a.rs:2-3"]);
    }

    #[test]
    fn windows_drive_path_splits_on_the_right_colon() {
        let r = parse_ref(r"C:\w\a.rs:3").unwrap();
        assert_eq!(r.path, r"C:\w\a.rs");
        assert_eq!(r.line, 3);
    }

    #[test]
    fn resolve_relative() {
        let root = Path::new("/w/project");
        let ok = |p: &str| resolve_in_root(root, &CodeRef::new(p, 1));
        assert_eq!(ok("src/a.rs"), Some(PathBuf::from("/w/project/src/a.rs")));
        assert_eq!(ok("./src/../b.rs"), Some(PathBuf::from("/w/project/b.rs")));
        assert_eq!(ok("../other/a.rs"), None);
        assert_eq!(ok("src/../../a.rs"), None);
        assert_eq!(ok("."), None);
        assert_eq!(ok(""), None);
    }

    #[test]
    fn resolve_absolute() {
        let root = Path::new("/w/project");
        let ok = |p: &str| resolve_in_root(root, &CodeRef::new(p, 1));
        assert_eq!(
            ok("/w/project/src/a.rs"),
            Some(PathBuf::from("/w/project/src/a.rs"))
        );
        assert_eq!(ok("/w/project/../secret"), None);
        assert_eq!(ok("/w/projectx/a.rs"), None);
        assert_eq!(ok("/etc/passwd"), None);
        assert_eq!(ok("/w/project"), None);
    }
}
