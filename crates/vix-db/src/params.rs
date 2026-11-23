//! Named bind parameters for the query editor.
//!
//! A statement may carry `:name` placeholders; [`names`] finds them (skipping
//! string literals, comments, and `PostgreSQL` `::type` casts) and [`substitute`]
//! replaces each with a SQL literal once the workbench has prompted for values.
//! Pure and unit-tested; the workbench drives the prompt.
//!
//! The two functions share one set of skipping rules (single-quoted strings,
//! `--` and `/* */` comments, `::` casts) but are written as separate scans;
//! keep them in step. Values are always rendered through
//! [`crate::catalog::quote_literal`], so substitution is textual but safe:
//! anything that is not a plain number is quoted and escaped. Substituted
//! text is never recorded in persisted history (see `history_override` in
//! `lib.rs`). Owning spec: `crates/vix-db/spec/index.md`.

use super::catalog::quote_literal;

/// The char at `i`, or NUL past the end: the end-of-text sentinel (the scanners
/// stop at the end before reading it).
fn ch(chars: &[char], i: usize) -> char {
    chars.get(i).copied().unwrap_or('\0')
}

/// The chars between `from` and `to`; empty for a range that is not inside `chars`.
fn span(chars: &[char], from: usize, to: usize) -> &[char] {
    chars.get(from..to).unwrap_or(&[])
}

/// `from + step`; an index past the top of `usize` is past any text.
fn ahead(from: usize, step: usize) -> usize {
    add_or(from, step, usize::MAX)
}

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

/// Whether `c` may appear in a parameter name.
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The distinct `:name` placeholders in `sql`, in first-seen order. Colons in
/// string literals, comments, and `::` casts are ignored.
///
/// A bare `:` not followed by a name character is ignored. Double-quoted
/// identifiers are *not* skipped, so a `:` inside one would be read as a
/// placeholder.
///
/// # Examples
///
/// ```
/// use vix_db::params::names;
/// assert_eq!(
///     names("select * from t where a = :id and b = ':no' and c = :id and d = x::int"),
///     ["id"]
/// );
/// ```
#[must_use]
pub fn names(sql: &str) -> Vec<String> {
    let chars: Vec<char> = sql.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match ch(&chars, i) {
            '\'' => {
                // Skip a single-quoted string ('' is an escaped quote).
                i = ahead(i, 1);
                while i < chars.len() {
                    if ch(&chars, i) == '\'' {
                        if chars.get(ahead(i, 1)) == Some(&'\'') {
                            i = ahead(i, 2);
                            continue;
                        }
                        break;
                    }
                    i = ahead(i, 1);
                }
                i = ahead(i, 1);
            }
            '-' if chars.get(ahead(i, 1)) == Some(&'-') => {
                while i < chars.len() && ch(&chars, i) != '\n' {
                    i = ahead(i, 1);
                }
            }
            '/' if chars.get(ahead(i, 1)) == Some(&'*') => {
                i = ahead(i, 2);
                while i < chars.len()
                    && !(ch(&chars, i) == '*' && chars.get(ahead(i, 1)) == Some(&'/'))
                {
                    i = ahead(i, 1);
                }
                i = ahead(i, 2);
            }
            ':' if chars.get(ahead(i, 1)) == Some(&':') => i = ahead(i, 2), // `::type` cast
            ':' => {
                let start = ahead(i, 1);
                let mut j = start;
                while j < chars.len() && is_name_char(ch(&chars, j)) {
                    j = ahead(j, 1);
                }
                if j > start {
                    let name: String = span(&chars, start, j).iter().collect();
                    if !out.contains(&name) {
                        out.push(name);
                    }
                }
                i = j;
            }
            _ => i = ahead(i, 1),
        }
    }
    out
}

/// Replace each `:name` in `sql` with its value from `values` as a SQL literal.
/// Unmatched placeholders are left untouched; the same skipping rules as
/// [`names`] apply so colons in strings/comments/casts are preserved.
///
/// # Examples
///
/// ```
/// use vix_db::params::substitute;
/// let sql = substitute("where n = :n and s = :s", &[
///     ("n".into(), "5".into()),
///     ("s".into(), "O'Reilly".into()),
/// ]);
/// assert_eq!(sql, "where n = 5 and s = 'O''Reilly'");
/// ```
#[must_use]
pub fn substitute(sql: &str, values: &[(String, String)]) -> String {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    while i < chars.len() {
        match ch(&chars, i) {
            '\'' => {
                out.push('\'');
                i = ahead(i, 1);
                while i < chars.len() {
                    out.push(ch(&chars, i));
                    if ch(&chars, i) == '\'' {
                        if chars.get(ahead(i, 1)) == Some(&'\'') {
                            out.push('\'');
                            i = ahead(i, 2);
                            continue;
                        }
                        i = ahead(i, 1);
                        break;
                    }
                    i = ahead(i, 1);
                }
            }
            '-' if chars.get(ahead(i, 1)) == Some(&'-') => {
                while i < chars.len() && ch(&chars, i) != '\n' {
                    out.push(ch(&chars, i));
                    i = ahead(i, 1);
                }
            }
            '/' if chars.get(ahead(i, 1)) == Some(&'*') => {
                while i < chars.len()
                    && !(ch(&chars, i) == '*' && chars.get(ahead(i, 1)) == Some(&'/'))
                {
                    out.push(ch(&chars, i));
                    i = ahead(i, 1);
                }
            }
            ':' if chars.get(ahead(i, 1)) == Some(&':') => {
                out.push_str("::");
                i = ahead(i, 2);
            }
            ':' => {
                let start = ahead(i, 1);
                let mut j = start;
                while j < chars.len() && is_name_char(ch(&chars, j)) {
                    j = ahead(j, 1);
                }
                let name: String = span(&chars, start, j).iter().collect();
                match values.iter().find(|(n, _)| n == &name) {
                    Some((_, v)) if j > start => {
                        out.push_str(&quote_literal(v));
                        i = j;
                    }
                    _ => {
                        out.push(':');
                        i = ahead(i, 1);
                    }
                }
            }
            c => {
                out.push(c);
                i = ahead(i, 1);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_distinct_names_skipping_noise() {
        let sql = "SELECT * FROM t WHERE a = :id AND b = :name -- :nope\n OR c = :id";
        assert_eq!(names(sql), vec!["id", "name"], "deduped, comment ignored");
        assert!(
            names("SELECT ':x' AS lit").is_empty(),
            "colon in a string is not a param"
        );
        assert!(names("SELECT 1::int").is_empty(), "cast is not a param");
    }

    #[test]
    fn substitutes_values_as_literals() {
        let sql = "SELECT * FROM t WHERE id = :id AND name = :name";
        let out = substitute(
            sql,
            &[("id".into(), "42".into()), ("name".into(), "O'Hara".into())],
        );
        assert_eq!(out, "SELECT * FROM t WHERE id = 42 AND name = 'O''Hara'");
    }

    #[test]
    fn leaves_casts_strings_and_unknown_params_intact() {
        assert_eq!(substitute("SELECT 1::int", &[]), "SELECT 1::int");
        assert_eq!(
            substitute("SELECT ':x'", &[("x".into(), "y".into())]),
            "SELECT ':x'"
        );
        assert_eq!(substitute("WHERE a = :missing", &[]), "WHERE a = :missing");
    }

    #[test]
    fn names_skip_quoted_and_commented_text_and_stop_at_name_characters() {
        assert_eq!(names("SELECT 'it''s :no', :a /* :hidden */, :b_2 -- :gone"), ["a", "b_2"]);
        assert_eq!(names("SELECT '' || :x"), ["x"]);
        assert!(names("a ::: b : c").is_empty(), "a bare colon names nothing");
        assert!(names("/* unterminated :x").is_empty());
        assert!(names("'unterminated :x").is_empty());
        assert_eq!(names(":"), Vec::<String>::new());
    }

    #[test]
    fn substitution_leaves_strings_comments_casts_and_unknown_names_alone() {
        let values = [("id".to_string(), "7".to_string())];
        assert_eq!(
            substitute("SELECT 'it''s :id', :id::int /* :id */ -- :id\n, :other, :", &values),
            "SELECT 'it''s :id', 7::int /* :id */ -- :id\n, :other, :"
        );
        assert_eq!(substitute("a ::int b", &values), "a ::int b");
        assert_eq!(substitute("'open :id", &values), "'open :id");
        assert_eq!(substitute("/* open :id", &values), "/* open :id");
    }
}
