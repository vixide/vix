//! Each built-in problem matcher (T632) against captured tool output in the
//! repository's `test-data/problem-matchers/`.

use vix_tasks::matcher::{Builtin, MatcherSpec, Problem, Severity, scan};

/// The captured output `name` from `test-data/problem-matchers/`.
fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/problem-matchers")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `(file, line, column, severity)` of each problem, for compact assertions.
fn locations(problems: &[Problem]) -> Vec<(&str, usize, Option<usize>, Severity)> {
    problems
        .iter()
        .map(|p| (p.file.as_str(), p.line, p.column, p.severity))
        .collect()
}

/// Scan `name` with the one built-in `b` and with `auto`, which must agree.
fn both(name: &str, b: Builtin) -> Vec<Problem> {
    let text = fixture(name);
    let only = scan(MatcherSpec::Builtin(b), &text);
    let auto = scan(MatcherSpec::Auto, &text);
    assert_eq!(
        only,
        auto,
        "{name}: the {} matcher and auto differ",
        b.name()
    );
    for p in &only {
        assert_eq!(p.source.name(), b.name());
    }
    only
}

#[test]
fn rustc_multi_line_blocks() {
    let p = both("rustc.txt", Builtin::Rustc);
    assert_eq!(
        locations(&p),
        [
            ("src/lib.rs", 2, Some(9), Severity::Warning),
            ("src/main.rs", 4, Some(18), Severity::Error),
            ("src/deep/mod.rs", 120, Some(5), Severity::Error),
        ],
        "the note's arrow and the repeated warning are not problems"
    );
    assert_eq!(p[0].message, "unused variable: `x`");
    assert_eq!(p[1].message, "mismatched types");
    assert_eq!(p[2].message, "cannot find value `missing` in this scope");
}

#[test]
fn gcc_and_clang() {
    let p = both("gcc.txt", Builtin::Gcc);
    assert_eq!(
        locations(&p),
        [
            ("main.c", 5, Some(12), Severity::Warning),
            ("main.c", 7, Some(5), Severity::Error),
            ("include/util.h", 2, Some(10), Severity::Error),
            ("src/old.c", 14, None, Severity::Warning),
        ]
    );
    assert_eq!(p[2].message, "missing.h: No such file or directory");
}

#[test]
fn tsc_both_formats() {
    let p = both("tsc.txt", Builtin::Tsc);
    assert_eq!(
        locations(&p),
        [
            ("src/app.ts", 12, Some(7), Severity::Error),
            ("src/util/strings.ts", 3, Some(1), Severity::Error),
            ("src/view.tsx", 8, Some(15), Severity::Error),
        ]
    );
    assert_eq!(p[2].message, "TS2304: Cannot find name 'React'.");
}

#[test]
fn eslint_stylish_compact_and_unix() {
    let p = both("eslint.txt", Builtin::Eslint);
    assert_eq!(
        locations(&p),
        [
            ("/work/web/src/index.js", 1, Some(10), Severity::Error),
            ("/work/web/src/index.js", 14, Some(3), Severity::Warning),
            ("/work/web/src/lib/math.js", 7, Some(1), Severity::Error),
            ("src/a.js", 4, Some(2), Severity::Error),
            ("src/b.js", 9, Some(5), Severity::Warning),
        ]
    );
    assert_eq!(
        p[0].message,
        "'unused' is defined but never used (no-unused-vars)"
    );
    assert_eq!(p[2].message, "Parsing error: Unexpected token");
}

#[test]
fn python_traceback_is_one_problem_at_the_innermost_frame() {
    let p = both("python.txt", Builtin::Python);
    assert_eq!(locations(&p), [("app/calc.py", 5, None, Severity::Error)]);
    assert_eq!(p[0].message, "ZeroDivisionError: division by zero");
}

#[test]
fn pytest_failures_and_warnings() {
    let p = both("pytest.txt", Builtin::Python);
    assert_eq!(
        locations(&p),
        [
            ("tests/test_calc.py", 9, None, Severity::Error),
            ("tests/test_calc.py", 2, None, Severity::Warning),
        ]
    );
    assert_eq!(p[0].message, "AssertionError");
}

#[test]
fn go_build_and_test_failures() {
    let p = both("go.txt", Builtin::Go);
    assert_eq!(
        locations(&p),
        [
            ("./calc.go", 7, Some(2), Severity::Error),
            ("./calc.go", 12, Some(9), Severity::Error),
            ("calc_test.go", 15, None, Severity::Error),
        ],
        "the stack-trace line is not a problem"
    );
    assert_eq!(p[2].message, "Add(1, 1) = 3; want 2");
}

#[test]
fn every_fixture_together_under_auto() {
    let all: String = [
        "rustc.txt",
        "gcc.txt",
        "tsc.txt",
        "eslint.txt",
        "python.txt",
        "pytest.txt",
        "go.txt",
    ]
    .iter()
    .map(|n| fixture(n))
    .collect::<Vec<_>>()
    .join("\n");
    assert_eq!(
        scan(MatcherSpec::Auto, &all).len(),
        3 + 4 + 3 + 5 + 1 + 2 + 3
    );
}
