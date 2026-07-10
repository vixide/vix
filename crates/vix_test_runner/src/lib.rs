//! Parse test-runner output into a pass/fail list with jump-to-failure
//! locations. Supports `cargo test`'s libtest format and the common
//! `name PASSED/FAILED/SKIPPED` shape (pytest `-v`, many others).
//!
//! Pure text → data, so it is unit-testable; the host runs the command (via the
//! async pipeline) and renders the [`TestResult`]s.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

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

/// Counts of passed / failed / ignored in a result set.
#[must_use]
pub fn tally(results: &[TestResult]) -> (usize, usize, usize) {
    let mut pass = 0;
    let mut fail = 0;
    let mut ignore = 0;
    for r in results {
        match r.status {
            Status::Pass => pass += 1,
            Status::Fail => fail += 1,
            Status::Ignore => ignore += 1,
        }
    }
    (pass, fail, ignore)
}

/// Parse test-runner `output` lines into results, attaching failure locations
/// found in libtest failure blocks. Results are sorted by name (which groups
/// them by module prefix).
#[must_use]
pub fn parse(output: &str) -> Vec<TestResult> {
    let mut results: Vec<TestResult> = Vec::new();
    let mut current_failure: Option<String> = None;

    for line in output.lines() {
        let trimmed = line.trim();
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
                upsert(&mut results, name.trim(), status);
                continue;
            }
        }
        // Generic: `path::name PASSED | FAILED | SKIPPED` (pytest -v, etc.).
        if let Some((name, verdict)) = trimmed.rsplit_once(' ') {
            let status = match verdict {
                "PASSED" => Some(Status::Pass),
                "FAILED" => Some(Status::Fail),
                "SKIPPED" => Some(Status::Ignore),
                _ => None,
            };
            if let Some(status) = status && !name.is_empty() {
                upsert(&mut results, name, status);
                continue;
            }
        }
        // libtest failure detail: `---- name stdout ----` then a panic location.
        if let Some(name) = trimmed.strip_prefix("---- ").and_then(|s| s.strip_suffix(" stdout ----")) {
            current_failure = Some(name.trim().to_string());
            continue;
        }
        if let Some(loc) = panic_location(trimmed)
            && let Some(name) = current_failure.clone()
        {
            attach_location(&mut results, &name, loc);
        }
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results.dedup_by(|a, b| a.name == b.name);
    results
}

/// Insert or update a result by name (a later status wins, e.g. the failure
/// summary after the run).
fn upsert(results: &mut Vec<TestResult>, name: &str, status: Status) {
    if let Some(r) = results.iter_mut().find(|r| r.name == name) {
        r.status = status;
    } else {
        results.push(TestResult { name: name.to_string(), status, location: None });
    }
}

/// Attach a failure location to the named test.
fn attach_location(results: &mut [TestResult], name: &str, loc: (String, usize)) {
    if let Some(r) = results.iter_mut().find(|r| r.name == name) {
        r.location = Some(loc);
    }
}

/// Extract a `<file>:<line>` from a `thread '…' panicked at <file>:<line>:<col>`
/// line (or a bare `<file>:<line>:<col>` prefix).
fn panic_location(line: &str) -> Option<(String, usize)> {
    let after = line.rfind("panicked at ").map_or(line, |i| &line[i + "panicked at ".len()..]);
    let mut parts = after.trim().splitn(3, ':');
    let file = parts.next()?.trim();
    let line_no: usize = parts.next()?.trim().parse().ok()?;
    if file.is_empty() || !file.contains(['/', '.']) {
        return None;
    }
    Some((file.to_string(), line_no))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(parse("compiling...\nwarning: unused\n").is_empty());
    }
}
