//! Test-at-point: locating the nearest enclosing/preceding test definition
//! and its run command, per language.
//!
//! A full parser-based implementation would use a real syntax tree per
//! language to resolve block nesting exactly; see the crate doc comment for
//! why this crate uses line-based regex heuristics instead. Every language
//! here follows the same shape: scan upward from the cursor line for the
//! nearest line matching that language's test-definition pattern. This is
//! intentionally *not* real block-nesting resolution (no attempt is made to
//! check that the matched definition actually still encloses the cursor
//! rather than merely preceding it) — documented per language below where it
//! matters most.
//!
//! Implemented: Rust, Python, Go, JavaScript/TypeScript, Ruby (RSpec and
//! Minitest, via [`TestFrameworkHint`](crate::test_at_point::TestFrameworkHint)), and Elixir. Not implemented (out of
//! scope, budget reasons): Java (branching on Maven vs. Gradle would need a
//! `TestFrameworkHint` variant and a third command shape), Erlang, and F# —
//! the dispatch in [`test_at_point`] is a plain per-extension `match`, so
//! adding a language later is a matter of adding one more arm and one more
//! private per-language function following the existing pattern; no public
//! "rules" API is needed to make that easy.

use regex::{Captures, Regex};
use std::sync::LazyLock;

/// `a + b`, or `i64::MAX` when the sum does not fit: a nesting depth that large
/// is past any real file, and stays "deeper than anything".
fn add_or_i64(a: i64, b: i64) -> i64 {
    a.checked_add(b).unwrap_or(i64::MAX)
}

/// `a - b`, or `i64::MIN` when the difference does not fit.
fn sub_or_i64(a: i64, b: i64) -> i64 {
    a.checked_sub(b).unwrap_or(i64::MIN)
}

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

/// The test found by [`test_at_point`]: its name and the shell command that
/// runs just that one test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestMatch {
    /// The test's name (or, for Elixir, a description string — see
    /// [`test_at_point`]'s Elixir case).
    pub name: String,
    /// The shell command that runs just this test.
    pub command: String,
}

/// Disambiguates a language's test framework where the run command differs
/// by framework and cannot be inferred from the file extension alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TestFrameworkHint {
    /// No framework hint available or needed (used for languages that don't
    /// currently branch on it).
    #[default]
    Unknown,
    /// Ruby, `RSpec` (`it "..."` / `describe "..."`).
    RubyRspec,
    /// Ruby, Minitest (`def test_...`).
    RubyMinitest,
    /// Java, Maven (reserved for future use — not currently dispatched to).
    JavaMaven,
    /// Java, Gradle (reserved for future use — not currently dispatched to).
    JavaGradle,
}

#[expect(
    clippy::unwrap_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
/// A bare `#[test]` attribute line (the anchor for a Rust test).
static RUST_TEST_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*#\[\s*test\s*\]\s*$").unwrap());
#[expect(
    clippy::unwrap_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
/// The function name on a `fn` line, allowing `pub`/`async`; the test name is the first of these after the `#[test]` attribute (other attributes may sit between).
static RUST_FN_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap()
});
#[expect(
    clippy::unwrap_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
/// An inline module opening on this line: `mod tests {`, `pub(crate) mod a {`.
static RUST_MOD_OPEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{").unwrap()
});
#[expect(
    clippy::unwrap_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
/// A `def test_...(` definition (pytest/unittest naming convention).
static PYTHON_TEST_DEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*def\s+(test_[A-Za-z0-9_]*)\s*\(").unwrap());
#[expect(
    clippy::unwrap_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
/// A top-level `func TestXxx(` (the `go test` naming convention).
static GO_TEST_FUNC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*func\s+(Test[A-Za-z0-9_]*)\s*\(").unwrap());
// The Rust `regex` crate has no backreferences, so each quote style needs
// its own alternative + capture group rather than one group plus `\1`.
#[expect(
    clippy::unwrap_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
/// A Jest/Mocha/Vitest `it`/`test`/`describe` call with a quoted title (three
/// quote styles, one capture group each).
static JS_TEST_CALL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\b(?:it|test|describe)\s*\(\s*(?:'([^']*)'|"([^"]*)"|`([^`]*)`)"#).unwrap()
});
#[expect(
    clippy::unwrap_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
/// An `RSpec` `it`/`describe` call with a quoted description (two quote styles; see the note above on why there is no backreference).
static RUBY_RSPEC_CALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\b(?:it|describe)\s+(?:'([^']*)'|"([^"]*)")"#).unwrap());
#[expect(
    clippy::unwrap_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
/// A Minitest `def test_...` method.
static RUBY_MINITEST_DEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*def\s+(test_[A-Za-z0-9_]*)").unwrap());
#[expect(
    clippy::unwrap_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
/// An `ExUnit` `test "name" do` block.
static ELIXIR_TEST_CALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^\s*test\s+"([^"]+)"\s+do\b"#).unwrap());

/// Scan `lines` upward (from `min(cursor_line, lines.len() - 1)` down to
/// `0`, inclusive) for the nearest line matching `re`, returning its index
/// and captures. `None` for an empty `lines` or no match.
fn scan_upward<'a>(
    lines: &[&'a str],
    cursor_line: usize,
    re: &Regex,
) -> Option<(usize, Captures<'a>)> {
    let last = lines.len().checked_sub(1)?;
    let start = cursor_line.min(last);
    (0..=start)
        .rev()
        .find_map(|i| re.captures(lines.get(i)?).map(|c| (i, c)))
}

/// The first non-empty capture group among `indices`, as an owned `String`
/// — used where a regex has one capture group per alternative quote style
/// (single/double/backtick) instead of one shared group plus a
/// backreference, which the `regex` crate does not support.
fn first_group(caps: &Captures, indices: &[usize]) -> Option<String> {
    indices
        .iter()
        .find_map(|&i| caps.get(i))
        .map(|m| m.as_str().to_string())
}

/// The directory part of `path` (`.` for a bare file name or a root-level file);
/// Go tests run per package directory (`go test ./dir`).
fn dir_of(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some(("", _)) | None => ".",
        Some((dir, _)) => dir,
    }
}

/// Rust: find the nearest `#[test]` at or above the cursor, then the first `fn`
/// after it; run exactly that test with `cargo test -- --exact PATH`, `PATH`
/// being the test's full path within its crate (libtest's `--exact` matches
/// that, never the bare function name).
fn rust_test_at_point(path: &str, lines: &[&str], cursor_line: usize) -> Option<TestMatch> {
    let (attr_line, _) = scan_upward(lines, cursor_line, &RUST_TEST_ATTR)?;
    let name = lines
        .get(attr_line..)?
        .iter()
        .find_map(|line| RUST_FN_NAME.captures(line))?
        .get(1)?
        .as_str()
        .to_string();
    let mut segments = rust_file_module(path);
    segments.extend(rust_inline_modules(lines.get(..attr_line)?));
    segments.push(name.clone());
    Some(TestMatch {
        command: format!("cargo test -- --exact {}", segments.join("::")),
        name,
    })
}

/// The module path a Rust source file stands for, from its location:
/// `src/net/http.rs` and `src/net/http/mod.rs` are `net::http`; `lib.rs`,
/// `main.rs` and anything outside a `src/` directory (an integration test in
/// `tests/` is its own crate) add nothing.
fn rust_file_module(path: &str) -> Vec<String> {
    let Some(in_src) = path
        .strip_prefix("src/")
        .or_else(|| path.rsplit_once("/src/").map(|(_, rest)| rest))
    else {
        return Vec::new();
    };
    let mut parts: Vec<&str> = in_src.trim_end_matches(".rs").split('/').collect();
    if matches!(parts.last(), Some(&("mod" | "lib" | "main"))) {
        parts.pop();
    }
    // `src/bin/tool.rs` is a binary crate of its own.
    if parts.first() == Some(&"bin") {
        return Vec::new();
    }
    parts.into_iter().map(str::to_string).collect()
}

/// The inline `mod NAME {` blocks still open at the end of `lines`, outermost
/// first. Braces are counted per line, ignoring that a brace may sit in a
/// string or a comment: a heuristic, like the rest of this module.
fn rust_inline_modules(lines: &[&str]) -> Vec<String> {
    let mut open: Vec<(String, i64)> = Vec::new();
    let mut depth: i64 = 0;
    for line in lines {
        let before = depth;
        if let Some(caps) = RUST_MOD_OPEN.captures(line) {
            open.push((caps[1].to_string(), before));
        }
        // Braces on one line are counted in `usize`; a count past `i64` is not a line of code.
        let opens = i64::try_from(line.matches('{').count()).unwrap_or(i64::MAX);
        let closes = i64::try_from(line.matches('}').count()).unwrap_or(i64::MAX);
        depth = add_or_i64(depth, opens);
        depth = sub_or_i64(depth, closes);
        while open.last().is_some_and(|(_, at)| *at >= depth) {
            open.pop();
        }
    }
    open.into_iter().map(|(name, _)| name).collect()
}

/// Python: nearest `def test_...` at or above the cursor.
fn python_test_at_point(path: &str, lines: &[&str], cursor_line: usize) -> Option<TestMatch> {
    let (_, caps) = scan_upward(lines, cursor_line, &PYTHON_TEST_DEF)?;
    let name = caps[1].to_string();
    Some(TestMatch {
        command: format!("python -m pytest {path}::{name}"),
        name,
    })
}

/// Go: nearest `func TestXxx` at or above the cursor.
fn go_test_at_point(path: &str, lines: &[&str], cursor_line: usize) -> Option<TestMatch> {
    let (_, caps) = scan_upward(lines, cursor_line, &GO_TEST_FUNC)?;
    let name = caps[1].to_string();
    let dir = dir_of(path);
    Some(TestMatch {
        command: format!("go test -run '^{name}$' ./{dir}"),
        name,
    })
}

/// JavaScript/TypeScript: nearest `it`/`test`/`describe` title at or above the
/// cursor, run through `npx jest -t`.
fn js_test_at_point(path: &str, lines: &[&str], cursor_line: usize) -> Option<TestMatch> {
    let (_, caps) = scan_upward(lines, cursor_line, &JS_TEST_CALL)?;
    let name = first_group(&caps, &[1, 2, 3])?;
    Some(TestMatch {
        command: format!("npx jest {path} -t '{name}'"),
        name,
    })
}

/// Ruby: Minitest `def test_` when the framework hint says so, otherwise `RSpec`
/// `it`/`describe` (see the in-function note on the default).
fn ruby_test_at_point(
    path: &str,
    lines: &[&str],
    cursor_line: usize,
    hint: TestFrameworkHint,
) -> Option<TestMatch> {
    // RSpec is the default assumption for `.rb` files with no explicit
    // hint (including `Unknown`), since it is the more common convention
    // for spec files.
    if hint == TestFrameworkHint::RubyMinitest {
        let (_, caps) = scan_upward(lines, cursor_line, &RUBY_MINITEST_DEF)?;
        let name = caps[1].to_string();
        Some(TestMatch {
            command: format!("bundle exec ruby -Itest {path} -n {name}"),
            name,
        })
    } else {
        let (_, caps) = scan_upward(lines, cursor_line, &RUBY_RSPEC_CALL)?;
        let name = first_group(&caps, &[1, 2])?;
        Some(TestMatch {
            command: format!("bundle exec rspec {path} -e '{name}'"),
            name,
        })
    }
}

/// Elixir: nearest `ExUnit` `test "..." do` at or above the cursor; the command
/// targets the file and the 1-based line of that test.
fn elixir_test_at_point(path: &str, lines: &[&str], cursor_line: usize) -> Option<TestMatch> {
    let (line_idx, caps) = scan_upward(lines, cursor_line, &ELIXIR_TEST_CALL)?;
    let name = caps[1].to_string();
    let line_number = add_or(line_idx, 1, usize::MAX);
    Some(TestMatch {
        command: format!("mix test {path}:{line_number}"),
        name,
    })
}

/// Find the test enclosing (or nearest preceding, scanning upward from)
/// `cursor_line` (0-indexed) in `lines`, for the language implied by
/// `file_extension` (no leading dot, e.g. `"rs"`, `"py"`, `"go"`), given the
/// file's `project_relative_path` (needed by several of the command
/// templates) and, where the language needs it, `hint`.
///
/// Per-language behavior:
/// - **Rust** (`rs`): nearest preceding `#[test]` attribute line (matched
///   only when it is the *entire* trimmed line — `#[test] fn foo() {}` on
///   one line is not recognized), then the next `fn NAME` line at or after
///   it. `cargo test -- --exact PATH`, `PATH` the test's full path in its
///   crate: the file's module (from `src/…`) and any enclosing inline
///   `mod NAME {` blocks, then `NAME`.
/// - **Python** (`py`): nearest enclosing/preceding `def test_NAME(`.
///   `python -m pytest FILE::NAME` (`NAME` includes the `test_` prefix, as
///   pytest node ids require).
/// - **Go** (`go`): nearest preceding `func TestXxx(`.
///   `go test -run '^NAME$' ./DIR`, `DIR` being `project_relative_path`'s
///   directory portion (`"."` if the file is at the project root).
/// - **JavaScript/TypeScript** (`js`, `jsx`, `ts`, `tsx`): nearest preceding
///   `it(...)`/`test(...)`/`describe(...)` call (single, double, or
///   backtick-quoted first argument). `npx jest FILE -t 'NAME'`. This does
///   **not** resolve real block nesting the way a parser would — it is
///   nearest-preceding-line only, so a cursor after a `describe` block has
///   closed but before the next one opens may still match the closed
///   block's innermost `it`.
/// - **Ruby** (`rb`): branches on `hint`. [`TestFrameworkHint::RubyMinitest`]
///   looks for `def test_NAME`, command
///   `bundle exec ruby -Itest FILE -n NAME`; any other hint (including
///   [`TestFrameworkHint::Unknown`]) assumes `RSpec` and looks for
///   `it "NAME"` / `describe "NAME"`, command
///   `bundle exec rspec FILE -e 'NAME'`.
/// - **Elixir** (`ex`, `exs`): nearest preceding `test "NAME" do`. Command
///   `mix test FILE:LINE`, using the matched line's 1-indexed line number
///   (`TestMatch::name` is still the test's description string, for
///   display).
///
/// Any other extension, or no match found by the language's rule, returns
/// `None`.
#[must_use]
pub fn test_at_point(
    file_extension: &str,
    project_relative_path: &str,
    lines: &[&str],
    cursor_line: usize,
    hint: TestFrameworkHint,
) -> Option<TestMatch> {
    match file_extension {
        "rs" => rust_test_at_point(project_relative_path, lines, cursor_line),
        "py" => python_test_at_point(project_relative_path, lines, cursor_line),
        "go" => go_test_at_point(project_relative_path, lines, cursor_line),
        "js" | "jsx" | "ts" | "tsx" => js_test_at_point(project_relative_path, lines, cursor_line),
        "rb" => ruby_test_at_point(project_relative_path, lines, cursor_line, hint),
        "ex" | "exs" => elixir_test_at_point(project_relative_path, lines, cursor_line),
        _ => None,
    }
}

/// Whether [`tests_in_file`] and [`test_at_point`] know a test syntax for
/// files with this extension — so a caller can skip reading a buffer's text
/// (on a 100 MB log, a 100 MB copy) when they would find nothing (T749).
#[must_use]
pub fn knows_tests_in(file_extension: &str) -> bool {
    matches!(
        file_extension,
        "rs" | "py" | "go" | "js" | "jsx" | "ts" | "tsx" | "rb" | "ex" | "exs"
    )
}

/// Every test defined in `lines`, in file order, for local "Run test" code
/// lenses (T726): `(line, match)` where `line` (0-based) is the line naming
/// the test (Rust: the `fn` line after `#[test]`; elsewhere the definition
/// line itself) and `match` is what [`test_at_point`] resolves there. Same
/// line-based heuristics and languages as [`test_at_point`]; any other
/// extension yields nothing.
#[must_use]
pub fn tests_in_file(
    file_extension: &str,
    project_relative_path: &str,
    lines: &[&str],
    hint: TestFrameworkHint,
) -> Vec<(usize, TestMatch)> {
    let def: &Regex = match file_extension {
        "rs" => &RUST_TEST_ATTR,
        "py" => &PYTHON_TEST_DEF,
        "go" => &GO_TEST_FUNC,
        "js" | "jsx" | "ts" | "tsx" => &JS_TEST_CALL,
        "rb" if hint == TestFrameworkHint::RubyMinitest => &RUBY_MINITEST_DEF,
        "rb" => &RUBY_RSPEC_CALL,
        "ex" | "exs" => &ELIXIR_TEST_CALL,
        _ => return Vec::new(),
    };
    let mut found = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !def.is_match(line) {
            continue;
        }
        let name_line = if file_extension == "rs" {
            let Some(off) = lines.iter().skip(i).position(|l| RUST_FN_NAME.is_match(l)) else {
                continue;
            };
            add_or(i, off, usize::MAX)
        } else {
            i
        };
        if let Some(m) = test_at_point(file_extension, project_relative_path, lines, i, hint) {
            found.push((name_line, m));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_in_file_lists_every_test_at_its_name_line() {
        let text = "fn helper() {}\n\n#[test]\nfn one() {}\n\n#[test]\n#[ignore]\nfn two() {}\n";
        let lines = split(text);
        let found = tests_in_file("rs", "src/lib.rs", &lines, TestFrameworkHint::Unknown);
        let names: Vec<(usize, &str)> = found.iter().map(|(l, m)| (*l, m.name.as_str())).collect();
        assert_eq!(names, vec![(3, "one"), (7, "two")]);
        assert_eq!(found[1].1.command, "cargo test -- --exact two");
        // lib.rs is the crate root: no module prefix.

        let py =
            split("def test_a():\n    pass\ndef helper():\n    pass\ndef test_b():\n    pass\n");
        let found = tests_in_file("py", "t.py", &py, TestFrameworkHint::Unknown);
        assert_eq!(found.iter().map(|f| f.0).collect::<Vec<_>>(), vec![0, 4]);

        assert_eq!(
            tests_in_file("txt", "a.txt", &py, TestFrameworkHint::Unknown),
            vec![]
        );
    }

    fn split(text: &str) -> Vec<&str> {
        text.lines().collect()
    }

    #[test]
    fn rust_finds_enclosing_test() {
        let text = "fn helper() {}\n\n#[test]\nfn it_works() {\n    assert!(true);\n}\n";
        let lines = split(text);
        let m = test_at_point("rs", "src/lib.rs", &lines, 4, TestFrameworkHint::Unknown).unwrap();
        assert_eq!(m.name, "it_works");
        assert_eq!(m.command, "cargo test -- --exact it_works");
    }

    #[test]
    fn rust_command_uses_the_full_test_path() {
        let text = "fn count() {}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn counts() {\n        assert!(true);\n    }\n}\n";
        let lines = split(text);
        let m = test_at_point("rs", "src/main.rs", &lines, 8, TestFrameworkHint::Unknown).unwrap();
        assert_eq!(m.name, "counts");
        assert_eq!(m.command, "cargo test -- --exact tests::counts");
        let m = test_at_point(
            "rs",
            "src/net/http.rs",
            &lines,
            8,
            TestFrameworkHint::Unknown,
        )
        .unwrap();
        assert_eq!(m.command, "cargo test -- --exact net::http::tests::counts");
        let m = test_at_point(
            "rs",
            "crates/x/src/a/mod.rs",
            &lines,
            8,
            TestFrameworkHint::Unknown,
        )
        .unwrap();
        assert_eq!(m.command, "cargo test -- --exact a::tests::counts");
    }

    #[test]
    fn rust_closed_and_nested_modules() {
        let text = "mod done {\n    fn x() {}\n}\npub(crate) mod outer {\n    mod inner {\n        #[test]\n        fn deep() {}\n    }\n}\n";
        let lines = split(text);
        let m = test_at_point("rs", "src/lib.rs", &lines, 6, TestFrameworkHint::Unknown).unwrap();
        assert_eq!(m.command, "cargo test -- --exact outer::inner::deep");
    }

    #[test]
    fn rust_file_module_cases() {
        assert_eq!(rust_file_module("src/lib.rs"), Vec::<String>::new());
        assert_eq!(rust_file_module("src/a/b.rs"), vec!["a", "b"]);
        assert_eq!(rust_file_module("tests/it.rs"), Vec::<String>::new());
        assert_eq!(rust_file_module("src/bin/tool.rs"), Vec::<String>::new());
        assert_eq!(rust_file_module("tui/src/app/git.rs"), vec!["app", "git"]);
    }

    #[test]
    fn rust_no_test_attr_above_is_none() {
        let lines = split("fn not_a_test() {}\n");
        assert!(test_at_point("rs", "src/lib.rs", &lines, 0, TestFrameworkHint::Unknown).is_none());
    }

    #[test]
    fn python_finds_enclosing_test() {
        let text = "def helper():\n    pass\n\ndef test_addition():\n    assert 1 + 1 == 2\n";
        let lines = split(text);
        let m = test_at_point(
            "py",
            "tests/test_math.py",
            &lines,
            4,
            TestFrameworkHint::Unknown,
        )
        .unwrap();
        assert_eq!(m.name, "test_addition");
        assert_eq!(
            m.command,
            "python -m pytest tests/test_math.py::test_addition"
        );
    }

    #[test]
    fn go_finds_enclosing_test_and_uses_dir() {
        let text = "package foo\n\nfunc TestAdd(t *testing.T) {\n    // ...\n}\n";
        let lines = split(text);
        let m = test_at_point(
            "go",
            "pkg/foo/add_test.go",
            &lines,
            3,
            TestFrameworkHint::Unknown,
        )
        .unwrap();
        assert_eq!(m.name, "TestAdd");
        assert_eq!(m.command, "go test -run '^TestAdd$' ./pkg/foo");
    }

    #[test]
    fn go_at_project_root_uses_dot() {
        let lines = split("func TestX(t *testing.T) {\n}\n");
        let m = test_at_point("go", "main_test.go", &lines, 0, TestFrameworkHint::Unknown).unwrap();
        assert_eq!(m.command, "go test -run '^TestX$' ./.");
    }

    #[test]
    fn js_finds_it_with_single_or_double_quotes() {
        let lines = split(
            "describe('suite', () => {\n  it(\"does the thing\", () => {\n    expect(1).toBe(1);\n  });\n});\n",
        );
        let m =
            test_at_point("ts", "src/x.test.ts", &lines, 2, TestFrameworkHint::Unknown).unwrap();
        assert_eq!(m.name, "does the thing");
        assert_eq!(m.command, "npx jest src/x.test.ts -t 'does the thing'");
    }

    #[test]
    fn ruby_rspec_default_hint() {
        let lines = split(
            "describe 'Widget' do\n  it 'does the thing' do\n    expect(1).to eq(1)\n  end\nend\n",
        );
        let m = test_at_point(
            "rb",
            "spec/widget_spec.rb",
            &lines,
            2,
            TestFrameworkHint::RubyRspec,
        )
        .unwrap();
        assert_eq!(m.name, "does the thing");
        assert_eq!(
            m.command,
            "bundle exec rspec spec/widget_spec.rb -e 'does the thing'"
        );
    }

    #[test]
    fn ruby_minitest_hint() {
        let lines = split(
            "class WidgetTest < Minitest::Test\n  def test_it_works\n    assert true\n  end\nend\n",
        );
        let m = test_at_point(
            "rb",
            "test/widget_test.rb",
            &lines,
            2,
            TestFrameworkHint::RubyMinitest,
        )
        .unwrap();
        assert_eq!(m.name, "test_it_works");
        assert_eq!(
            m.command,
            "bundle exec ruby -Itest test/widget_test.rb -n test_it_works"
        );
    }

    #[test]
    fn elixir_finds_test_and_uses_line_number() {
        let lines = split(
            "defmodule MathTest do\n  use ExUnit.Case\n\n  test \"adds numbers\" do\n    assert 1 + 1 == 2\n  end\nend\n",
        );
        let m = test_at_point(
            "ex",
            "test/math_test.exs",
            &lines,
            4,
            TestFrameworkHint::Unknown,
        )
        .unwrap();
        assert_eq!(m.name, "adds numbers");
        assert_eq!(m.command, "mix test test/math_test.exs:4");
    }

    #[test]
    fn unsupported_extension_is_none() {
        let lines = split("anything\n");
        assert!(test_at_point("cpp", "x.cpp", &lines, 0, TestFrameworkHint::Unknown).is_none());
    }

    #[test]
    fn empty_lines_is_none() {
        assert!(test_at_point("rs", "src/lib.rs", &[], 0, TestFrameworkHint::Unknown).is_none());
    }

    #[test]
    fn cursor_past_end_of_file_clamps_to_last_line() {
        let lines = split("#[test]\nfn foo() {}\n");
        let m = test_at_point("rs", "src/lib.rs", &lines, 999, TestFrameworkHint::Unknown).unwrap();
        assert_eq!(m.name, "foo");
    }

    #[test]
    fn cursor_on_blank_line_still_scans_upward() {
        let lines = split("#[test]\nfn foo() {}\n\n");
        let m = test_at_point("rs", "src/lib.rs", &lines, 2, TestFrameworkHint::Unknown).unwrap();
        assert_eq!(m.name, "foo");
    }

    #[test]
    fn knows_tests_in_matches_the_extensions_tests_in_file_reads() {
        let rust = ["#[test]", "fn it_works() {}"];
        for ext in [
            "rs", "py", "go", "js", "jsx", "ts", "tsx", "rb", "ex", "exs",
        ] {
            assert!(knows_tests_in(ext), "{ext}");
        }
        for ext in ["", "log", "txt", "md"] {
            assert!(!knows_tests_in(ext), "{ext}");
            assert_eq!(
                tests_in_file(ext, "a", &rust, TestFrameworkHint::Unknown),
                vec![]
            );
        }
    }
}
