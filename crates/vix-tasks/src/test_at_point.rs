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

static RUST_TEST_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*#\[\s*test\s*\]\s*$").unwrap());
static RUST_FN_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap()
});
static PYTHON_TEST_DEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*def\s+(test_[A-Za-z0-9_]*)\s*\(").unwrap());
static GO_TEST_FUNC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*func\s+(Test[A-Za-z0-9_]*)\s*\(").unwrap());
// The Rust `regex` crate has no backreferences, so each quote style needs
// its own alternative + capture group rather than one group plus `\1`.
static JS_TEST_CALL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\b(?:it|test|describe)\s*\(\s*(?:'([^']*)'|"([^"]*)"|`([^`]*)`)"#).unwrap()
});
static RUBY_RSPEC_CALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\b(?:it|describe)\s+(?:'([^']*)'|"([^"]*)")"#).unwrap());
static RUBY_MINITEST_DEF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*def\s+(test_[A-Za-z0-9_]*)").unwrap());
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
    if lines.is_empty() {
        return None;
    }
    let start = cursor_line.min(lines.len() - 1);
    (0..=start)
        .rev()
        .find_map(|i| re.captures(lines[i]).map(|c| (i, c)))
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

fn dir_of(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some(("", _)) | None => ".",
        Some((dir, _)) => dir,
    }
}

fn rust_test_at_point(lines: &[&str], cursor_line: usize) -> Option<TestMatch> {
    let (attr_line, _) = scan_upward(lines, cursor_line, &RUST_TEST_ATTR)?;
    let name = lines[attr_line..]
        .iter()
        .find_map(|line| RUST_FN_NAME.captures(line))?
        .get(1)?
        .as_str()
        .to_string();
    Some(TestMatch {
        command: format!("cargo test -- --exact {name}"),
        name,
    })
}

fn python_test_at_point(path: &str, lines: &[&str], cursor_line: usize) -> Option<TestMatch> {
    let (_, caps) = scan_upward(lines, cursor_line, &PYTHON_TEST_DEF)?;
    let name = caps[1].to_string();
    Some(TestMatch {
        command: format!("python -m pytest {path}::{name}"),
        name,
    })
}

fn go_test_at_point(path: &str, lines: &[&str], cursor_line: usize) -> Option<TestMatch> {
    let (_, caps) = scan_upward(lines, cursor_line, &GO_TEST_FUNC)?;
    let name = caps[1].to_string();
    let dir = dir_of(path);
    Some(TestMatch {
        command: format!("go test -run '^{name}$' ./{dir}"),
        name,
    })
}

fn js_test_at_point(path: &str, lines: &[&str], cursor_line: usize) -> Option<TestMatch> {
    let (_, caps) = scan_upward(lines, cursor_line, &JS_TEST_CALL)?;
    let name = first_group(&caps, &[1, 2, 3])?;
    Some(TestMatch {
        command: format!("npx jest {path} -t '{name}'"),
        name,
    })
}

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

fn elixir_test_at_point(path: &str, lines: &[&str], cursor_line: usize) -> Option<TestMatch> {
    let (line_idx, caps) = scan_upward(lines, cursor_line, &ELIXIR_TEST_CALL)?;
    let name = caps[1].to_string();
    let line_number = line_idx + 1;
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
///   it. `cargo test -- --exact NAME`.
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
        "rs" => rust_test_at_point(lines, cursor_line),
        "py" => python_test_at_point(project_relative_path, lines, cursor_line),
        "go" => go_test_at_point(project_relative_path, lines, cursor_line),
        "js" | "jsx" | "ts" | "tsx" => js_test_at_point(project_relative_path, lines, cursor_line),
        "rb" => ruby_test_at_point(project_relative_path, lines, cursor_line, hint),
        "ex" | "exs" => elixir_test_at_point(project_relative_path, lines, cursor_line),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
