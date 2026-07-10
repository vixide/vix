//! Named bind parameters for the query editor.
//!
//! A statement may carry `:name` placeholders; [`names`] finds them (skipping
//! string literals, comments, and `PostgreSQL` `::type` casts) and [`substitute`]
//! replaces each with a SQL literal once the workbench has prompted for values.
//! Pure and unit-tested; the workbench drives the prompt.

use super::catalog::quote_literal;

/// Whether `c` may appear in a parameter name.
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The distinct `:name` placeholders in `sql`, in first-seen order. Colons in
/// string literals, comments, and `::` casts are ignored.
#[must_use]
pub fn names(sql: &str) -> Vec<String> {
    let chars: Vec<char> = sql.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\'' => {
                // Skip a single-quoted string ('' is an escaped quote).
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\'' {
                        if chars.get(i + 1) == Some(&'\'') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1;
            }
            '-' if chars.get(i + 1) == Some(&'-') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 2;
            }
            ':' if chars.get(i + 1) == Some(&':') => i += 2, // `::type` cast
            ':' => {
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && is_name_char(chars[j]) {
                    j += 1;
                }
                if j > start {
                    let name: String = chars[start..j].iter().collect();
                    if !out.contains(&name) {
                        out.push(name);
                    }
                }
                i = j;
            }
            _ => i += 1,
        }
    }
    out
}

/// Replace each `:name` in `sql` with its value from `values` as a SQL literal.
/// Unmatched placeholders are left untouched; the same skipping rules as
/// [`names`] apply so colons in strings/comments/casts are preserved.
#[must_use]
pub fn substitute(sql: &str, values: &[(String, String)]) -> String {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\'' => {
                out.push('\'');
                i += 1;
                while i < chars.len() {
                    out.push(chars[i]);
                    if chars[i] == '\'' {
                        if chars.get(i + 1) == Some(&'\'') {
                            out.push('\'');
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            '-' if chars.get(i + 1) == Some(&'-') => {
                while i < chars.len() && chars[i] != '\n' {
                    out.push(chars[i]);
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    out.push(chars[i]);
                    i += 1;
                }
            }
            ':' if chars.get(i + 1) == Some(&':') => {
                out.push_str("::");
                i += 2;
            }
            ':' => {
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && is_name_char(chars[j]) {
                    j += 1;
                }
                let name: String = chars[start..j].iter().collect();
                match values.iter().find(|(n, _)| n == &name) {
                    Some((_, v)) if j > start => {
                        out.push_str(&quote_literal(v));
                        i = j;
                    }
                    _ => {
                        out.push(':');
                        i += 1;
                    }
                }
            }
            c => {
                out.push(c);
                i += 1;
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
}
