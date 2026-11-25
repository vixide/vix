//! Parse test-runner output into a pass/fail list with jump-to-failure
//! locations. Supports `cargo test`'s libtest format and the common
//! `name PASSED/FAILED/SKIPPED` shape (pytest `-v`, many others).
//!
//! Pure text → data, so it is unit-testable; the host runs the command (via the
//! async pipeline) and renders the [`TestResult`]s. Spec:
//! `crates/vix-test-runner/spec/index.md`.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

/// Outcome of a single test.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// The test passed.
    Pass,
    /// The test failed.
    Fail,
    /// The test was ignored / skipped.
    Ignore,
}

/// One parsed test result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestResult {
    /// Test name / path (e.g. `module::it_works`).
    pub name: String,
    /// Pass / fail / ignore.
    pub status: Status,
    /// Source location of a failure, if found: `(file, 1-based line)`.
    pub location: Option<(String, usize)>,
}

/// Counts of passed / failed / ignored in a result set, as
/// `(passed, failed, ignored)`.
///
/// # Examples
/// ```
/// use vix_test_runner::{parse, tally};
/// let r = parse("test a ... ok\ntest b ... FAILED\n");
/// assert_eq!(tally(&r), (1, 1, 0));
/// ```
#[must_use]
pub fn tally(results: &[TestResult]) -> (usize, usize, usize) {
    let count = |status: Status| results.iter().filter(|r| r.status == status).count();
    (
        count(Status::Pass),
        count(Status::Fail),
        count(Status::Ignore),
    )
}

/// Parse test-runner `output` lines into results, attaching failure locations
/// found in libtest failure blocks. Results are sorted by name (which groups
/// them by module prefix).
///
/// Lines are tried in order: a libtest `test NAME ... STATUS` line; the generic
/// `NAME PASSED|FAILED|SKIPPED` shape; a libtest `---- NAME stdout ----`
/// header, which selects which failure the next `panicked at FILE:LINE:COL`
/// line belongs to. Unrecognised lines are ignored, so noisy compiler output is
/// harmless and a run with no recognised lines yields an empty list.
///
/// # Examples
/// ```
/// use vix_test_runner::{parse, Status};
/// let out = "test m::bad ... FAILED\n\n---- m::bad stdout ----\n\
///            thread 'm::bad' panicked at src/lib.rs:7:5:\n";
/// let r = parse(out);
/// assert_eq!(r[0].status, Status::Fail);
/// assert_eq!(r[0].location, Some(("src/lib.rs".to_string(), 7)));
/// ```
#[must_use]
pub fn parse(output: &str) -> Vec<TestResult> {
    let mut rows = parse_rows(output);
    // Same name in two targets stays two rows: qualify each with its target.
    let names: Vec<String> = rows.iter().map(|r| r.result.name.clone()).collect();
    for row in &mut rows {
        if !row.target.is_empty() && names.iter().filter(|n| **n == row.result.name).count() > 1 {
            row.result.name = format!("{} [{}]", row.result.name, row.target);
        }
    }
    rows.into_iter().map(|r| r.result).collect()
}

/// A result plus the target (test binary) it came from.
struct Row {
    /// Cargo's `Running …` / `Doc-tests …` header in force; empty if none.
    target: String,
    /// The parsed result.
    result: TestResult,
}

/// [`parse`] before target qualification: results keyed by (target, name).
fn parse_rows(output: &str) -> Vec<Row> {
    let mut results: Vec<Row> = Vec::new();
    let mut current_failure: Option<String> = None;
    let mut target = String::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(t) = target_header(trimmed) {
            target = t;
            current_failure = None;
            continue;
        }
        // libtest: `test module::name ... ok | FAILED | ignored`
        if let Some(rest) = trimmed.strip_prefix("test ")
            && let Some((name, tail)) = rest.rsplit_once(" ... ")
        {
            let status = match tail.trim() {
                "ok" => Some(Status::Pass),
                t if t.starts_with("FAILED") => Some(Status::Fail),
                t if t.starts_with("ignored") => Some(Status::Ignore),
                _ => None,
            };
            if let Some(status) = status {
                upsert(&mut results, &target, name.trim(), status);
                continue;
            }
        }
        // Reached when the libtest branch did not match (or its tail was an
        // unknown status). Generic: `path::name PASSED | FAILED | SKIPPED` (pytest -v, etc.).
        if let Some((name, verdict)) = strip_progress(trimmed).rsplit_once(' ') {
            let status = match verdict {
                "PASSED" => Some(Status::Pass),
                "FAILED" => Some(Status::Fail),
                "SKIPPED" => Some(Status::Ignore),
                _ => None,
            };
            if let Some(status) = status
                && !name.is_empty()
            {
                upsert(&mut results, &target, name, status);
                continue;
            }
        }
        // libtest failure detail: `---- name stdout ----` then a panic location.
        if let Some(name) = trimmed
            .strip_prefix("---- ")
            .and_then(|s| s.strip_suffix(" stdout ----"))
        {
            current_failure = Some(name.trim().to_string());
            continue;
        }
        if let Some(loc) = panic_location(trimmed)
            && let Some(name) = current_failure.clone()
        {
            attach_location(&mut results, &target, &name, loc);
        }
    }

    results.sort_by(|a, b| (&a.result.name, &a.target).cmp(&(&b.result.name, &b.target)));
    // `upsert` already keeps names unique; this is a defensive no-op.
    results.dedup_by(|a, b| a.result.name == b.result.name && a.target == b.target);
    results
}

/// Insert or update a result by name (a later status wins, e.g. the failure
/// summary after the run).
fn upsert(results: &mut Vec<Row>, target: &str, name: &str, status: Status) {
    if let Some(r) = results
        .iter_mut()
        .find(|r| r.result.name == name && r.target == target)
    {
        r.result.status = status;
    } else {
        results.push(Row {
            target: target.to_string(),
            result: TestResult {
                name: name.to_string(),
                status,
                location: None,
            },
        });
    }
}

/// Attach a failure location to the named test.
fn attach_location(results: &mut [Row], target: &str, name: &str, loc: (String, usize)) {
    if let Some(r) = results
        .iter_mut()
        .find(|r| r.result.name == name && r.target == target)
    {
        r.result.location = Some(loc);
    }
}

/// The target named by a cargo `Running <what> (<path>)` or `Doc-tests <crate>`
/// header line, or `None` for any other line.
fn target_header(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("Running ")
        .or_else(|| line.strip_prefix("Doc-tests "))?;
    let name = rest.split(" (").next()?.trim();
    (!name.is_empty()).then(|| {
        if line.starts_with("Doc-tests") {
            format!("doc-tests {name}")
        } else {
            name.to_string()
        }
    })
}

/// Drop a trailing pytest progress marker such as ` [ 50%]` or ` [100%]`, so
/// `name PASSED [100%]` reads as `name PASSED`.
fn strip_progress(line: &str) -> &str {
    if let Some(i) = line.rfind(" [")
        && line.ends_with("%]")
        && let Some(pct) = line.get(i.saturating_add(2)..line.len().saturating_sub(2))
        && pct.trim().chars().all(|c| c.is_ascii_digit())
        && let Some(head) = line.get(..i)
    {
        return head.trim_end();
    }
    line
}

/// Extract a `<file>:<line>` from a `thread '…' panicked at <file>:<line>:<col>`
/// line (or a bare `<file>:<line>:<col>` prefix).
fn panic_location(line: &str) -> Option<(String, usize)> {
    let after = line
        .rsplit_once("panicked at ")
        .map_or(line, |(_, after)| after);
    let mut parts = after.trim().splitn(3, ':');
    let file = parts.next()?.trim();
    let line_no: usize = parts.next()?.trim().parse().ok()?;
    // Reject prose like `note: 3 failed` that merely contains a colon: a real
    // path has a separator or an extension.
    if file.is_empty() || !file.contains(['/', '.']) {
        return None;
    }
    Some((file.to_string(), line_no))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_libtest_tails_and_pytest_skips() {
        // A bench line is no test result.
        assert_eq!(
            parse("test bench_x ... bench:       12 ns/iter (+/- 1)"),
            []
        );
        let r = parse("tests/a.py::t SKIPPED");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].status, Status::Ignore);
    }

    #[test]
    fn panic_location_rejects_prose_with_a_number() {
        assert_eq!(panic_location("warning: 3: oops"), None);
        assert_eq!(panic_location(":3:4"), None);
        assert_eq!(
            panic_location("src/a.rs:3:4"),
            Some(("src/a.rs".to_string(), 3))
        );
    }

    #[test]
    fn parses_cargo_test_output() {
        let out = "\
running 3 tests
test mod_a::works ... ok
test mod_a::ignored_one ... ignored
test mod_b::breaks ... FAILED

failures:

---- mod_b::breaks stdout ----
thread 'mod_b::breaks' panicked at src/lib.rs:42:5:
assertion failed

test result: FAILED. 1 passed; 1 failed; 1 ignored;
";
        let results = parse(out);
        assert_eq!(results.len(), 3);
        let (p, f, i) = tally(&results);
        assert_eq!((p, f, i), (1, 1, 1));
        let broken = results.iter().find(|r| r.name == "mod_b::breaks").unwrap();
        assert_eq!(broken.status, Status::Fail);
        assert_eq!(broken.location, Some(("src/lib.rs".to_string(), 42)));
    }

    #[test]
    fn parses_pytest_style() {
        let out = "tests/test_x.py::test_ok PASSED\ntests/test_x.py::test_bad FAILED\n";
        let results = parse(out);
        assert_eq!(tally(&results), (1, 1, 0));
    }

    #[test]
    fn empty_output_has_no_results() {
        assert_eq!(
            parse("compiling...\nwarning: unused\n"),
            [] as [TestResult; 0]
        );
    }

    #[test]
    fn same_name_in_two_targets_stays_distinct() {
        let out = "Running unittests src/lib.rs (target/debug/deps/a-1)\n\
                   test tests::round_trip ... FAILED\n\
                   Running tests/it.rs (target/debug/deps/it-2)\n\
                   test tests::round_trip ... ok\n";
        let r = parse(out);
        assert_eq!(r.len(), 2);
        assert_eq!(tally(&r), (1, 1, 0));
        assert!(
            r.iter()
                .any(|t| t.status == Status::Fail && t.name.ends_with("[unittests src/lib.rs]"))
        );
    }

    #[test]
    fn pytest_progress_suffix_is_ignored() {
        let r = parse("tests/a.py::t PASSED [100%]\ntests/a.py::u FAILED [ 50%]\n");
        assert_eq!(tally(&r), (1, 1, 0));
        assert_eq!(r[0].name, "tests/a.py::t");
    }
}
