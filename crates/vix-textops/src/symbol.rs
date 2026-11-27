//! The offline declaration scan behind the `@` go-to-symbol palette mode, the
//! outline panel, the sticky-scroll header, the breadcrumb and the MCP
//! server's `document_symbols` tool (T861: moved here from `vix-palette`, which
//! wraps it under its published names, so the headless MCP server needs no palette crate).

use std::sync::LazyLock;

use crate::add_or;

/// The declaration-scanning pattern used by [`symbols`], compiled once. It
/// never varies with input, so callers on a per-frame path (sticky-scroll
/// header, breadcrumb) don't each pay to recompile it (T511).
#[expect(
    clippy::expect_used,
    reason = "fixed regex literal, compiled once and covered by tests"
)]
static SYMBOL_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    let kw = "fn|func|function|def|class|struct|enum|trait|impl|interface|type|mod|\
              namespace|package|macro_rules!";
    let pat =
        format!(r"(?:\b({kw})\s+([A-Za-z_][A-Za-z0-9_]*)|(#define)\s+([A-Za-z_][A-Za-z0-9_]*))");
    regex::Regex::new(&pat).expect("SYMBOL_RE pattern is a fixed, tested literal")
});

/// One declaration found in a buffer, for the `@` go-to-symbol mode and the
/// outline panel.
#[derive(Clone)]
pub struct Symbol {
    /// The structural keyword (`fn`, `struct`, `mod`, `impl`, …), for the outline
    /// type prefix. May be empty for `#define`-style matches.
    pub kind: String,
    /// The declared identifier (used for fuzzy matching).
    pub name: String,
    /// 1-based line of the declaration.
    pub line: usize,
    /// The trimmed source line, for display.
    pub text: String,
}

/// Scan `text` for declaration-style lines and return their symbols, in order.
///
/// A fast, offline, language-agnostic heuristic (the same family as
/// go-to-definition): a structural keyword followed by an identifier. Local
/// bindings (`let`/`var`/`const`) are intentionally excluded to keep the outline
/// to top-level structure.
#[must_use]
pub fn symbols(text: &str) -> Vec<Symbol> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| symbol_in_line(line, add_or(i, 1, usize::MAX)))
        .collect()
}

/// The declaration on one line, as [`symbols`] finds it, numbered `line`
/// (1-based): what [`symbols`] would return for that line of a buffer, so a
/// caller can look at only the lines it needs (T858: the sticky-scroll header
/// and the breadcrumb scan up from one line instead of the whole buffer).
/// `text` is the line without its line break.
#[must_use]
pub fn symbol_in_line(text: &str, line: usize) -> Option<Symbol> {
    let caps = SYMBOL_RE.captures(text)?;
    let name = caps.get(2).or_else(|| caps.get(4))?;
    let kind = caps
        .get(1)
        .or_else(|| caps.get(3))
        .map_or_else(String::new, |m| m.as_str().to_string());
    Some(Symbol {
        kind,
        name: name.as_str().to_string(),
        line,
        text: text.trim_start().chars().take(120).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_finds_declarations_in_order_with_1_based_lines() {
        let text = "use std::io;\n\npub fn main() {}\n  struct Point;\n#define MAX 3\nlet x = 1;\n";
        let found: Vec<_> = symbols(text)
            .into_iter()
            .map(|s| (s.kind, s.name, s.line, s.text))
            .collect();
        assert_eq!(
            found,
            [
                ("fn", "main", 3, "pub fn main() {}"),
                ("struct", "Point", 4, "struct Point;"),
                ("#define", "MAX", 5, "#define MAX 3"),
            ]
            .map(|(k, n, l, t)| (k.to_owned(), n.to_owned(), l, t.to_owned()))
        );
    }

    #[test]
    fn symbol_in_line_agrees_with_symbols() {
        let text = "mod a;\nclass B:\n  def c(self):\nplain line\n";
        for (i, line) in text.lines().enumerate() {
            let one = symbol_in_line(line, i + 1).map(|s| (s.name, s.line));
            let all = symbols(text)
                .into_iter()
                .find(|s| s.line == i + 1)
                .map(|s| (s.name, s.line));
            assert_eq!(one, all, "line {}", i + 1);
        }
        assert!(symbol_in_line("let x = 1;", 1).is_none());
    }

    #[test]
    fn the_display_text_is_capped_at_120_chars() {
        let long = format!("fn f() {{ {} }}", "x".repeat(300));
        let s = symbol_in_line(&long, 7).unwrap();
        assert_eq!(s.text.chars().count(), 120);
        assert_eq!(s.line, 7);
    }
}
