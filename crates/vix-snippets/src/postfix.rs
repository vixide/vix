//! Postfix completion (T727): type an expression, a dot and a template key,
//! press Tab, and the expression is wrapped: `ready.if` becomes
//! `if ready { … }`, `items.for` becomes `for item in items { … }`.
//!
//! [`expand`] is pure: it reads the text before the cursor and returns what
//! to replace and the snippet body (tabstop syntax, see
//! [`vix_snippet_tool::parse`]) to put there. Templates are chosen by a
//! coarse [`Family`] derived from the file extension.
//!
//! The expression to the left of the dot is found by scanning backward over
//! identifier/path characters and balanced `()`/`[]` groups. It is inserted into
//! the template with snippet metacharacters escaped, so an expression containing
//! `$` can never be misread as a tabstop. Spec: `crates/vix-snippets/spec/index.md`.
//!
//! # Examples
//! ```
//! use vix_snippets::postfix::{expand, Family};
//! let e = expand("ready.if", Family::Rust).unwrap();
//! assert_eq!(e.replace_chars, "ready.if".len());
//! assert_eq!(e.body, "if ready {\n\t$0\n}");
//! ```

/// The language family a template set is written for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Rust.
    Rust,
    /// JavaScript and TypeScript.
    Script,
    /// Python.
    Python,
    /// Go.
    Go,
    /// C, C++, Java, C# and other brace languages with `if (…) {`.
    CLike,
}

impl Family {
    /// The family for a file extension (without the dot); `None` when Vix has
    /// no postfix templates for the language.
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "rs" => Some(Self::Rust),
            "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => Some(Self::Script),
            "py" => Some(Self::Python),
            "go" => Some(Self::Go),
            "c" | "h" | "cc" | "cpp" | "hpp" | "java" | "cs" | "php" => Some(Self::CLike),
            _ => None,
        }
    }
}

/// What a postfix expansion does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expansion {
    /// How many characters before the cursor to replace (the expression, the
    /// dot and the key).
    pub replace_chars: usize,
    /// The snippet body to insert (tabstops; `$` in the expression escaped).
    pub body: String,
}

/// The template keys offered for `family`, for display and tests.
#[must_use]
pub fn keys(family: Family) -> Vec<&'static str> {
    templates(family).iter().map(|(k, _)| *k).collect()
}

/// `(key, template)` pairs; `$EXPR` marks where the expression goes, `$ITEM`
/// the singular loop variable derived from it. The remaining `$N` markers are
/// ordinary snippet tabstops. `$EXPR`/`$ITEM` are substituted textually by
/// [`expand`] before the body is handed to the snippet parser.
fn templates(family: Family) -> &'static [(&'static str, &'static str)] {
    match family {
        Family::Rust => &[
            ("if", "if $EXPR {\n\t$0\n}"),
            ("else", "if !$EXPR {\n\t$0\n}"),
            ("not", "!$EXPR"),
            ("for", "for ${1:$ITEM} in $EXPR {\n\t$0\n}"),
            ("match", "match $EXPR {\n\t${1:_} => $0,\n}"),
            ("let", "let ${1:name} = $EXPR;$0"),
            ("ref", "&$EXPR"),
            ("mut", "&mut $EXPR"),
            ("some", "Some($EXPR)"),
            ("ok", "Ok($EXPR)"),
            ("dbg", "dbg!($EXPR)"),
            ("print", "println!(\"{}\", $EXPR);$0"),
            ("return", "return $EXPR;"),
        ],
        Family::Script => &[
            ("if", "if ($EXPR) {\n\t$0\n}"),
            ("else", "if (!$EXPR) {\n\t$0\n}"),
            ("not", "!$EXPR"),
            ("for", "for (const ${1:$ITEM} of $EXPR) {\n\t$0\n}"),
            ("const", "const ${1:name} = $EXPR;$0"),
            ("let", "let ${1:name} = $EXPR;$0"),
            ("log", "console.log($EXPR);$0"),
            ("return", "return $EXPR;"),
            ("await", "await $EXPR"),
        ],
        Family::Python => &[
            ("if", "if $EXPR:\n\t$0"),
            ("else", "if not $EXPR:\n\t$0"),
            ("not", "not $EXPR"),
            ("for", "for ${1:$ITEM} in $EXPR:\n\t$0"),
            ("print", "print($EXPR)"),
            ("len", "len($EXPR)"),
            ("return", "return $EXPR"),
            ("var", "${1:name} = $EXPR$0"),
        ],
        Family::Go => &[
            ("if", "if $EXPR {\n\t$0\n}"),
            ("else", "if !$EXPR {\n\t$0\n}"),
            ("not", "!$EXPR"),
            ("for", "for _, ${1:$ITEM} := range $EXPR {\n\t$0\n}"),
            ("var", "${1:name} := $EXPR$0"),
            ("print", "fmt.Println($EXPR)"),
            ("return", "return $EXPR"),
        ],
        Family::CLike => &[
            ("if", "if ($EXPR) {\n\t$0\n}"),
            ("else", "if (!$EXPR) {\n\t$0\n}"),
            ("not", "!$EXPR"),
            ("return", "return $EXPR;"),
        ],
    }
}

/// Characters that may appear in the expression to the left of the dot.
/// `-` and `>` are admitted only to support `->` paths; [`expression_before`]
/// rejects a bare `-` or `>` (subtraction / comparison).
fn is_expr_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '$' | '-' | '>')
}

/// The expression ending at the end of `s`: identifier/path characters plus
/// balanced `()` / `[]` groups (so `foo(a, b).bar` and `xs[0]` are one unit).
///
/// Returns the expression's length in chars (`0` if there is none). An
/// unbalanced closing bracket ends the scan.
fn expression_before(s: &[char]) -> usize {
    let mut i = s.len();
    while i > 0 {
        let c = s[i - 1];
        if matches!(c, ')' | ']') {
            let (open, close) = if c == ')' { ('(', ')') } else { ('[', ']') };
            let mut depth = 0usize;
            let mut j = i;
            let mut matched = false;
            while j > 0 {
                let cj = s[j - 1];
                if cj == close {
                    depth += 1;
                } else if cj == open {
                    // Safe: `depth` is already >= 1 here because the first char
                    // examined is always the `close` bracket checked above.
                    depth -= 1;
                    if depth == 0 {
                        matched = true;
                        j -= 1;
                        break;
                    }
                }
                j -= 1;
            }
            if !matched {
                break;
            }
            i = j;
        } else if is_expr_char(c) && !(c == '-' && s.get(i) != Some(&'>')) {
            // A `-` only belongs to a `->` arrow, never a subtraction.
            if c == '>' && s.get(i.wrapping_sub(2)) != Some(&'-') {
                break;
            }
            i -= 1;
        } else {
            break;
        }
    }
    s.len() - i
}

/// The loop variable for iterating `expr`: a trailing plural `s` dropped
/// (`items` -> `item`), else `item`.
fn item_name(expr: &str) -> String {
    let last = expr
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("");
    match last.strip_suffix('s') {
        Some(stem) if !stem.is_empty() && stem != last => stem.to_string(),
        _ => "item".to_string(),
    }
}

/// Escape snippet metacharacters in `s` (`\\`, `$`, and `}`, which would
/// otherwise close an enclosing placeholder).
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('$', "\\$")
        .replace('}', "\\}")
}

/// Expand a postfix template at the end of `before` (the text of the line up
/// to the cursor). `None` when the text does not end in `expr.key` for a key
/// of `family`, or when there is no expression before the dot.
///
/// `replace_chars` counts chars (expression + `.` + key) back from the cursor.
#[must_use]
pub fn expand(before: &str, family: Family) -> Option<Expansion> {
    let chars: Vec<char> = before.chars().collect();
    // The key: trailing identifier characters after the last dot.
    let key_len = chars
        .iter()
        .rev()
        .take_while(|c| c.is_alphanumeric() || **c == '_')
        .count();
    if key_len == 0 || key_len >= chars.len() || chars[chars.len() - key_len - 1] != '.' {
        return None;
    }
    let key: String = chars[chars.len() - key_len..].iter().collect();
    let template = templates(family)
        .iter()
        .find_map(|(k, t)| (*k == key).then_some(*t))?;
    let head = &chars[..chars.len() - key_len - 1];
    let expr_len = expression_before(head);
    if expr_len == 0 {
        return None;
    }
    let expr: String = head[head.len() - expr_len..].iter().collect();
    // Substitute `$ITEM` first: its text is derived from the expression, and
    // both are escaped so they cannot introduce tabstops of their own.
    let body = template
        .replace("$ITEM", &escape(&item_name(&expr)))
        .replace("$EXPR", &escape(&expr));
    Some(Expansion {
        replace_chars: expr_len + 1 + key_len,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(before: &str, f: Family) -> Option<(usize, String)> {
        expand(before, f).map(|e| (e.replace_chars, e.body))
    }

    #[test]
    fn wraps_a_simple_identifier() {
        assert_eq!(
            body("    ready.if", Family::Rust),
            Some((8, "if ready {\n\t$0\n}".to_string()))
        );
        assert_eq!(
            body("x = done.not", Family::Script),
            Some((8, "!done".to_string()))
        );
    }

    #[test]
    fn takes_paths_calls_and_indexing_as_one_expression() {
        let e = expand("let v = self.items.for", Family::Rust).unwrap();
        assert_eq!(e.replace_chars, "self.items.for".len());
        assert_eq!(e.body, "for ${1:item} in self.items {\n\t$0\n}");
        let e = expand("foo(a, b).some", Family::Rust).unwrap();
        assert_eq!(e.body, "Some(foo(a, b))");
        let e = expand("xs[0].dbg", Family::Rust).unwrap();
        assert_eq!(e.body, "dbg!(xs[0])");
    }

    #[test]
    fn loop_variable_is_singularized() {
        let e = expand("users.for", Family::Go).unwrap();
        assert_eq!(e.body, "for _, ${1:user} := range users {\n\t$0\n}");
        let e = expand("data.for", Family::Python).unwrap();
        assert_eq!(e.body, "for ${1:item} in data:\n\t$0");
    }

    #[test]
    fn dollar_in_the_expression_is_escaped_for_the_snippet_parser() {
        let e = expand("$el.log", Family::Script).unwrap();
        assert_eq!(e.body, "console.log(\\$el);$0");
        // And the snippet parser reads it back as a literal dollar.
        assert!(vix_snippet_tool::parse(&e.body).text.contains("$el"));
        // Backslashes and braces round-trip too.
        let e = expand("m[\"a\\\\}\"].log", Family::Script).unwrap();
        assert!(
            vix_snippet_tool::parse(&e.body)
                .text
                .contains("m[\"a\\\\}\"]"),
            "{}",
            e.body
        );
    }

    #[test]
    fn no_expansion_without_expression_or_known_key() {
        assert_eq!(expand(".if", Family::Rust), None);
        assert_eq!(expand("x.nonsense", Family::Rust), None);
        assert_eq!(expand("plain", Family::Rust), None);
        assert_eq!(expand("x.", Family::Rust), None);
        // Keys are per family.
        assert_eq!(expand("x.match", Family::Python), None);
    }

    #[test]
    fn family_from_extension() {
        assert_eq!(Family::from_extension("RS"), Some(Family::Rust));
        assert_eq!(Family::from_extension("tsx"), Some(Family::Script));
        assert_eq!(Family::from_extension("md"), None);
        assert!(keys(Family::Rust).contains(&"match"));
    }
}
