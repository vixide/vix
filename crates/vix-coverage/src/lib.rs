//! LCOV and Cobertura XML coverage report parsing (T210): per-file, per-line
//! hit data for the coverage gutter (Tools → Load Coverage File…). Vix
//! visualizes an existing report; it doesn't run coverage tools itself.
//!
//! [`parse`] sniffs the format and returns a [`Report`]; the host asks
//! [`Report::lines_for`] for the active buffer's path and paints a gutter mark
//! per [`Hit`] (it shares the gutter column with the git diff marks). Both
//! parsers are deliberately tolerant -- malformed records are skipped, never an
//! error -- and Cobertura is read with a regex scan rather than an XML
//! dependency. Spec: `crates/vix-coverage/spec/index.md`.
//!
//! # Examples
//! ```
//! use std::path::Path;
//! use vix_coverage::{parse, Hit};
//!
//! let report = parse("SF:src/lib.rs\nDA:1,3\nDA:2,0\nend_of_record\n");
//! let lines = report.lines_for(Path::new("src/lib.rs")).unwrap();
//! assert_eq!(lines[&1], Hit::Covered);
//! assert_eq!(lines[&2], Hit::Uncovered);
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

// A Cobertura class's opening tag or a line element, matched in document
// order -- see `parse_cobertura`. Alternative 1 captures `file`; alternative 2
// captures `num`, `hits` and `rest` (the remaining attributes, scanned later by
// `is_partial_branch`). `[^>]*` stops at the tag end, so `number` must come
// before `hits` within a `<line>`. Compiled once; the pattern is a fixed
// literal, so this can't fail at runtime.
#[expect(
    clippy::expect_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
static COBERTURA_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?x)
        <class\b[^>]*\bfilename="(?P<file>[^"]*)"
        |
        <line\b[^>]*\bnumber="(?P<num>\d+)"[^>]*\bhits="(?P<hits>\d+)"(?P<rest>[^>]*)
        "#,
    )
    .expect("COBERTURA_TOKEN is a fixed valid pattern")
});

/// A line's coverage state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// Executed at least once, and (when branch data is present) every
    /// branch on the line was taken.
    Covered,
    /// Never executed.
    Uncovered,
    /// Executed, but not every branch on the line was taken.
    Partial,
}

/// A parsed coverage report: the file paths it names, each mapped to its
/// per-line (1-based) hit state.
///
/// Lines the report does not mention (blank lines, comments, unreachable code)
/// are simply absent from the per-file map. Obtain one from [`parse`],
/// [`parse_lcov`] or [`parse_cobertura`]; `Default` is an empty report.
#[derive(Clone, Debug, Default)]
pub struct Report {
    files: HashMap<String, HashMap<usize, Hit>>,
}

impl Report {
    /// The per-line hit data recorded for `path` (matched against however the
    /// *report* names its files, e.g. `tui/src/main.rs`), or `None` if the
    /// report doesn't cover it.
    ///
    /// Reports name files inconsistently (relative to the project root,
    /// relative to some other directory, with a different separator, or —
    /// once a real path has been through `Path::canonicalize`, e.g. on
    /// Windows — different casing than the literal string that named the
    /// same file elsewhere), so this tries an exact match first, then falls
    /// back to the recorded path being a suffix of `path` or vice versa, all
    /// compared via `normalize`.
    ///
    /// When several recorded paths match by suffix (e.g. `a/lib.rs` and
    /// `b/lib.rs` for a query of `lib.rs`), the winner is whichever the map
    /// iterates first, which is unspecified; prefer reports with unambiguous
    /// paths.
    ///
    /// If neither finds a match, a last resort tries canonicalizing each
    /// recorded path and comparing that against `path` directly (assumed
    /// already canonical, as every real caller's is) — a real filesystem
    /// call, so only tried once the cheap string comparisons above have both
    /// failed. This catches path aliasing no amount of string normalization
    /// can bridge: found via a real Windows CI run (T547/T550), where a
    /// report built from `std::env::temp_dir()`'s own string recorded a
    /// legacy 8.3 short name (`RUNNER~1`) for a path segment `canonicalize`
    /// resolves to its long form (`runneradmin`) — genuinely different
    /// strings naming the identical directory. Silently falls through when
    /// a recorded path doesn't exist locally (a report from a different
    /// machine, or a synthetic path in a test fixture) rather than erroring.
    #[must_use]
    pub fn lines_for(&self, path: &Path) -> Option<&HashMap<usize, Hit>> {
        let wanted = normalize(&path.to_string_lossy());
        if let Some(lines) = self.files.iter().find_map(|(recorded, lines)| {
            let recorded = normalize(recorded);
            (wanted == recorded
                || wanted.ends_with(recorded.as_str())
                || recorded.ends_with(wanted.as_str()))
            .then_some(lines)
        }) {
            return Some(lines);
        }
        self.files.iter().find_map(|(recorded, lines)| {
            let canon = vix_authority::current()
                .canonicalize(Path::new(recorded))
                .ok()?;
            (canon == path).then_some(lines)
        })
    }

    /// How many distinct files the report names.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// Normalize a path string for tolerant comparison: forward slashes (a
/// report may name a file with either separator, regardless of platform) and
/// lowercase. Case-folding is deliberately unconditional, not
/// `cfg(windows)`-gated: `Path::canonicalize` on Windows can resolve a path
/// to different casing than a literal string built to name the same file
/// (e.g. `%TEMP%`'s own casing vs. the filesystem's canonical one for the
/// same case-insensitive path) — found via a real Windows CI run (T547) that
/// this exact mismatch left a loaded coverage report matching zero lines. It
/// can only ever cause a *false positive* between two paths differing solely
/// in case; on a case-sensitive filesystem (Linux) two real, distinct files
/// differing only in case are rare enough that the tolerance is worth it for
/// the Windows/macOS correctness it buys.
fn normalize(s: &str) -> String {
    s.replace('\\', "/").to_lowercase()
}

/// Parse a coverage report, auto-detecting LCOV vs. Cobertura XML from its
/// content (an LCOV `.info` file never starts with `<`).
///
/// # Examples
/// ```
/// let xml = r#"<class filename="a.rs"><line number="1" hits="0"/></class>"#;
/// assert_eq!(vix_coverage::parse(xml).file_count(), 1);
/// assert_eq!(vix_coverage::parse("SF:a.rs\nend_of_record\n").file_count(), 1);
/// ```
#[must_use]
pub fn parse(text: &str) -> Report {
    if text.trim_start().starts_with('<') {
        parse_cobertura(text)
    } else {
        parse_lcov(text)
    }
}

/// Parse an LCOV `.info` file's `SF:`/`DA:`/`BRDA:`/`end_of_record` records.
/// A line with any untaken branch (`BRDA:` `taken` of `-` or `0`) is
/// [`Hit::Partial`]; otherwise a nonzero `DA:` count is [`Hit::Covered`] and a
/// zero count is [`Hit::Uncovered`]. Malformed records are skipped.
///
/// Only complete records count: a file's data is committed at its
/// `end_of_record`, so a truncated trailing record is dropped. `DA:`'s optional
/// checksum field and `BRDA:` lines for lines with no `DA:` entry are ignored.
/// Repeated `SF:` entries for one file merge, later lines overwriting earlier.
#[must_use]
pub fn parse_lcov(text: &str) -> Report {
    let mut files = HashMap::new();
    let mut current: Option<String> = None;
    let mut counts: HashMap<usize, usize> = HashMap::new();
    let mut partial: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("SF:") {
            // A new record starts: drop any half-read state from a previous
            // record that never reached `end_of_record`.
            current = Some(rest.trim().to_string());
            counts.clear();
            partial.clear();
        } else if let Some(rest) = line.strip_prefix("DA:") {
            let fields: Vec<&str> = rest.split(',').collect();
            if let (Some(n), Some(c)) = (fields.first(), fields.get(1))
                && let (Ok(n), Ok(c)) = (n.trim().parse(), c.trim().parse())
            {
                counts.insert(n, c);
            }
        } else if let Some(rest) = line.strip_prefix("BRDA:") {
            let fields: Vec<&str> = rest.split(',').collect();
            if let (Some(n), Some(taken)) = (fields.first(), fields.get(3))
                && let Ok(n) = n.trim().parse::<usize>()
                && matches!(taken.trim(), "-" | "0")
            {
                partial.insert(n);
            }
        } else if line == "end_of_record"
            && let Some(f) = current.take()
        {
            let entry: &mut HashMap<usize, Hit> = files.entry(f).or_default();
            for (&n, &c) in &counts {
                // An unexecuted line is Uncovered even if it lists branches;
                // Partial only refines lines that did run.
                let hit = if c == 0 {
                    Hit::Uncovered
                } else if partial.contains(&n) {
                    Hit::Partial
                } else {
                    Hit::Covered
                };
                entry.insert(n, hit);
            }
        }
    }
    Report { files }
}

/// Parse a Cobertura XML report's `<class filename="…">`/`<line number="…"
/// hits="…">` elements, tracking the enclosing class's `filename` as the
/// document is scanned in order. A `branch="true"` line whose
/// `condition-coverage="…% (a/b)"` has `a < b` is [`Hit::Partial`]. This is a
/// tolerant scan, not a validating XML parser -- it reads the two attributes
/// it needs and ignores everything else about the document's structure.
///
/// Within a `<line>` the `number` attribute must precede `hits`. Lines seen
/// before any `<class filename=…>` are dropped, as are non-numeric values.
#[must_use]
pub fn parse_cobertura(text: &str) -> Report {
    let mut files: HashMap<String, HashMap<usize, Hit>> = HashMap::new();
    // One pass, in document order: either a class's opening tag (updates
    // `current`) or a line element (recorded against `current`).
    let mut current: Option<String> = None;
    for cap in COBERTURA_TOKEN.captures_iter(text) {
        if let Some(f) = cap.name("file") {
            current = Some(f.as_str().to_string());
            continue;
        }
        let (Some(num), Some(hits)) = (cap.name("num"), cap.name("hits")) else {
            continue;
        };
        let (Ok(n), Ok(h)) = (
            num.as_str().parse::<usize>(),
            hits.as_str().parse::<usize>(),
        ) else {
            continue;
        };
        let Some(f) = &current else { continue };
        let rest = cap.name("rest").map_or("", |m| m.as_str());
        let hit = if h == 0 {
            Hit::Uncovered
        } else if is_partial_branch(rest) {
            Hit::Partial
        } else {
            Hit::Covered
        };
        files.entry(f.clone()).or_default().insert(n, hit);
    }
    Report { files }
}

/// Whether a Cobertura `<line>`'s trailing attributes (everything after
/// `hits="…"`) describe a branch line where not every branch was taken:
/// `branch="true" condition-coverage="…% (a/b)"` with `a < b`.
fn is_partial_branch(attrs: &str) -> bool {
    if !attrs.contains("branch=\"true\"") {
        return false;
    }
    // Parse the `(a/b)` taken/total pair out of `condition-coverage`.
    let Some(start) = attrs.find('(') else {
        return false;
    };
    let Some(end) = attrs[start..].find(')') else {
        return false;
    };
    let Some((a, b)) = attrs[start + 1..start + end].split_once('/') else {
        return false;
    };
    let (Ok(a), Ok(b)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) else {
        return false;
    };
    a < b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcov_hit_and_missed_lines() {
        let report = parse_lcov("TN:\nSF:src/lib.rs\nDA:1,1\nDA:2,0\nDA:3,5\nend_of_record\n");
        let lines = report.lines_for(Path::new("src/lib.rs")).unwrap();
        assert_eq!(lines[&1], Hit::Covered);
        assert_eq!(lines[&2], Hit::Uncovered);
        assert_eq!(lines[&3], Hit::Covered);
    }

    #[test]
    fn lcov_untaken_branch_marks_the_line_partial() {
        let report = parse_lcov("SF:a.rs\nDA:5,3\nBRDA:5,0,0,3\nBRDA:5,0,1,-\nend_of_record\n");
        let lines = report.lines_for(Path::new("a.rs")).unwrap();
        assert_eq!(lines[&5], Hit::Partial);
    }

    #[test]
    fn lcov_two_files_stay_separate() {
        let report = parse_lcov("SF:a.rs\nDA:1,1\nend_of_record\nSF:b.rs\nDA:1,0\nend_of_record\n");
        assert_eq!(report.file_count(), 2);
        assert_eq!(
            report.lines_for(Path::new("a.rs")).unwrap()[&1],
            Hit::Covered
        );
        assert_eq!(
            report.lines_for(Path::new("b.rs")).unwrap()[&1],
            Hit::Uncovered
        );
    }

    #[test]
    fn cobertura_hit_and_missed_lines() {
        let xml = r#"<coverage><packages><package><classes>
            <class filename="src/main.rs">
                <lines>
                    <line number="1" hits="4" branch="false"/>
                    <line number="2" hits="0" branch="false"/>
                </lines>
            </class>
        </classes></package></packages></coverage>"#;
        let report = parse_cobertura(xml);
        let lines = report.lines_for(Path::new("src/main.rs")).unwrap();
        assert_eq!(lines[&1], Hit::Covered);
        assert_eq!(lines[&2], Hit::Uncovered);
    }

    #[test]
    fn cobertura_partially_taken_branch_is_partial() {
        let xml = r#"<class filename="x.py"><lines>
            <line number="10" hits="2" branch="true" condition-coverage="50% (1/2)"/>
            <line number="11" hits="2" branch="true" condition-coverage="100% (2/2)"/>
        </lines></class>"#;
        let report = parse_cobertura(xml);
        let lines = report.lines_for(Path::new("x.py")).unwrap();
        assert_eq!(lines[&10], Hit::Partial);
        assert_eq!(lines[&11], Hit::Covered);
    }

    #[test]
    fn cobertura_lines_attach_to_the_nearest_preceding_class() {
        let xml = r#"
            <class filename="a.rs"><lines><line number="1" hits="1"/></lines></class>
            <class filename="b.rs"><lines><line number="1" hits="0"/></lines></class>
        "#;
        let report = parse_cobertura(xml);
        assert_eq!(
            report.lines_for(Path::new("a.rs")).unwrap()[&1],
            Hit::Covered
        );
        assert_eq!(
            report.lines_for(Path::new("b.rs")).unwrap()[&1],
            Hit::Uncovered
        );
    }

    #[test]
    fn parse_auto_detects_xml_vs_lcov() {
        assert_eq!(
            parse("SF:a.rs\nDA:1,1\nend_of_record\n")
                .lines_for(Path::new("a.rs"))
                .unwrap()[&1],
            Hit::Covered
        );
        let xml = r#"<class filename="a.rs"><line number="1" hits="1"/></class>"#;
        assert_eq!(
            parse(xml).lines_for(Path::new("a.rs")).unwrap()[&1],
            Hit::Covered
        );
    }

    #[test]
    fn lines_for_matches_by_suffix_when_the_report_uses_a_different_root() {
        let report = parse_lcov("SF:/home/ci/checkout/src/lib.rs\nDA:1,1\nend_of_record\n");
        // The buffer's path is relative to a different (local) project root.
        assert!(report.lines_for(Path::new("src/lib.rs")).is_some());
    }

    #[test]
    fn lines_for_matches_a_windows_path_that_differs_only_in_case() {
        // T547: on Windows, `Path::canonicalize` can resolve a path to
        // different casing than a literal string built to name the same
        // file (e.g. `%TEMP%`'s own casing vs. the filesystem's canonical
        // one) -- confirmed by a real Windows CI run, where this exact
        // mismatch left a loaded report matching zero lines.
        let report = parse_lcov(
            "SF:C:\\Users\\RUNNERADMIN\\AppData\\Local\\Temp\\proj\\lib.rs\nDA:1,1\nend_of_record\n",
        );
        let canonicalized_form =
            Path::new(r"\\?\C:\Users\runneradmin\AppData\Local\Temp\proj\lib.rs");
        assert!(report.lines_for(canonicalized_form).is_some());
    }

    #[test]
    fn lines_for_canonicalizes_as_a_last_resort_when_string_matching_fails() {
        // The real bug behind the case test above: on Windows,
        // `std::env::temp_dir()`'s own string can carry a legacy 8.3 short
        // name (`RUNNER~1`) for a path segment `Path::canonicalize` resolves
        // to its long form (`runneradmin`) -- genuinely different strings,
        // not just different case, so no string-normalization step can ever
        // bridge them (confirmed via a real Windows CI run, T547/T550: the
        // case-only fix above did *not* resolve the actual failure). Proves
        // the canonicalize-based fallback tier itself works, using a path
        // alias (`dir/sibling/../lib.rs`) that's portable to reproduce on
        // any OS -- `..` resolution is exactly the kind of alias no amount
        // of string comparison, case-folding included, can match.
        let dir =
            std::env::temp_dir().join(format!("vix-coverage-canon-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sibling")).unwrap();
        let file = dir.join("lib.rs");
        std::fs::write(&file, "fn a() {}\n").unwrap();

        let recorded = dir.join("sibling").join("..").join("lib.rs");
        let report = parse_lcov(&format!(
            "SF:{}\nDA:1,1\nend_of_record\n",
            recorded.display()
        ));

        let canonical = file.canonicalize().unwrap();
        assert!(
            report.lines_for(&canonical).is_some(),
            "canonicalize-based fallback should resolve the `..` alias"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_records_are_skipped_not_panicking() {
        let report = parse_lcov("SF:a.rs\nDA:not,a,number\nend_of_record\n");
        assert!(report.lines_for(Path::new("a.rs")).unwrap().is_empty());
    }
}
