//! Table aliases in one SQL statement, for alias-aware completion (T780).
//!
//! [`aliases`] scans a statement for the table references after `FROM`,
//! `JOIN` and `UPDATE` and records each `alias → table` pair it finds:
//! `FROM orders o`, `FROM orders AS o`, `JOIN customers c`, comma joins
//! (`FROM a x, b y`), schema-qualified tables (`public.orders o`) and quoted
//! identifiers (`"Order Items" oi`, `` `orders` o ``). The completer then
//! resolves `o.` to the columns of `orders`.
//!
//! This is a small lexer plus a forgiving scan, not a parser: it never fails,
//! it ignores what it does not understand, and it only has to be right about
//! the shapes above. Clause keywords (`WHERE`, `ON`, `LEFT`, `JOIN`, `GROUP`,
//! …) are never taken for an alias unless written after `AS` or quoted.
//! Derived tables and table functions (`FROM (SELECT …) s`,
//! `FROM generate_series(1, 3) g`) are skipped, since they have no catalog
//! columns to offer. Owning spec: `crates/vix-db/spec/index.md`
//! ("Intelligent Autocomplete").

use super::highlight::KEYWORDS;

/// One `alias → table` binding found in a statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    /// The alias as written (unquoted).
    pub alias: String,
    /// The table's own name: the last part of a qualified name, unquoted.
    pub table: String,
    /// The schema qualifier, when the reference had one (`public` in
    /// `public.orders o`).
    pub schema: Option<String>,
}

/// Words that end a table reference and so can never be a bare alias, on top
/// of the highlighter's [`KEYWORDS`] (which already cover `where`, `on`,
/// `join`, `left`, `group`, `order`, `limit`, `union`, …).
const NOT_ALIASES: &[&str] = &[
    "natural",
    "lateral",
    "window",
    "fetch",
    "for",
    "straight_join",
    "tablesample",
    "qualify",
    "partition",
    "only",
    "use",
    "force",
    "ignore",
];

/// A lexical token. Strings, numbers, comments and operators carry nothing
/// the scan needs, so they collapse into [`Tok::Other`].
#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    /// An identifier or keyword; `true` when it was quoted.
    Word(String, bool),
    /// `.`
    Dot,
    /// `,`
    Comma,
    /// `(`
    Open,
    /// `)`
    Close,
    /// Anything else.
    Other,
}

/// Whether `c` can continue a bare identifier.
fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Read a quoted identifier closed by `close` starting after the opening
/// quote at `i`; a doubled closing quote is an escaped one. Returns the name
/// and the index after the closing quote (or the end of the text).
fn quoted(chars: &[char], mut i: usize, close: char) -> (String, usize) {
    let mut name = String::new();
    while i < chars.len() {
        if chars[i] == close {
            if close != ']' && chars.get(i + 1) == Some(&close) {
                name.push(close);
                i += 2;
                continue;
            }
            return (name, i + 1);
        }
        name.push(chars[i]);
        i += 1;
    }
    (name, i)
}

/// Split `sql` into [`Tok`]s, skipping whitespace, comments and literals.
fn lex(sql: &str) -> Vec<Tok> {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            _ if c.is_whitespace() => i += 1,
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
            '\'' => {
                // A string literal; `quoted` handles the doubled `''` escape.
                i = quoted(&chars, i + 1, '\'').1;
                out.push(Tok::Other);
            }
            '"' | '`' | '[' => {
                let close = match c {
                    '[' => ']',
                    other => other,
                };
                let (name, next) = quoted(&chars, i + 1, close);
                out.push(Tok::Word(name, true));
                i = next;
            }
            '.' => {
                out.push(Tok::Dot);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '(' => {
                out.push(Tok::Open);
                i += 1;
            }
            ')' => {
                out.push(Tok::Close);
                i += 1;
            }
            _ if is_ident_char(c) => {
                let start = i;
                while i < chars.len() && is_ident_char(chars[i]) {
                    i += 1;
                }
                if c.is_ascii_digit() {
                    out.push(Tok::Other);
                } else {
                    out.push(Tok::Word(chars[start..i].iter().collect(), false));
                }
            }
            _ => {
                out.push(Tok::Other);
                i += 1;
            }
        }
    }
    out
}

/// Every identifier in `sql`, bare or quoted (unquoted), skipping strings,
/// comments and numbers; keywords are included. Backs
/// [`crate::relations::mentions_table`].
pub(crate) fn identifiers(sql: &str) -> Vec<String> {
    lex(sql)
        .into_iter()
        .filter_map(|t| match t {
            Tok::Word(w, _) => Some(w),
            _ => None,
        })
        .collect()
}

/// Whether `tok` is the unquoted keyword `kw` (ASCII case-insensitive).
fn is_kw(tok: Option<&Tok>, kw: &str) -> bool {
    matches!(tok, Some(Tok::Word(w, false)) if w.eq_ignore_ascii_case(kw))
}

/// Whether `word`, written bare, is a keyword that cannot be an alias.
fn reserved(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    KEYWORDS.contains(&lower.as_str()) || NOT_ALIASES.contains(&lower.as_str())
}

/// The index after the parenthesized group opening at `i` (a `Tok::Open`),
/// or the end of `toks` when it never closes.
fn skip_group(toks: &[Tok], mut i: usize) -> usize {
    let mut depth = 0usize;
    while i < toks.len() {
        match toks[i] {
            Tok::Open => depth += 1,
            Tok::Close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    i
}

/// Every `alias → table` binding in `sql`, in the order written. A table
/// written without an alias contributes nothing (its own name already works
/// as a qualifier).
///
/// # Examples
///
/// ```
/// use vix_db::alias::aliases;
/// let found = aliases("SELECT * FROM orders o JOIN public.customers AS c ON c.id = o.cid");
/// let pairs: Vec<(&str, &str)> = found
///     .iter()
///     .map(|a| (a.alias.as_str(), a.table.as_str()))
///     .collect();
/// assert_eq!(pairs, [("o", "orders"), ("c", "customers")]);
/// assert_eq!(found[1].schema.as_deref(), Some("public"));
/// assert!(aliases("SELECT * FROM t WHERE x = 1").is_empty());
/// ```
#[must_use]
pub fn aliases(sql: &str) -> Vec<Alias> {
    let mut out = Vec::new();
    scan(&lex(sql), &mut out);
    out
}

/// Push every binding in `toks` to `out` (see [`aliases`]).
fn scan(toks: &[Tok], out: &mut Vec<Alias>) {
    let mut i = 0;
    while i < toks.len() {
        let comma_list = if is_kw(toks.get(i), "from") {
            true
        } else if is_kw(toks.get(i), "join")
            || is_kw(toks.get(i), "straight_join")
            || is_kw(toks.get(i), "update")
        {
            false
        } else {
            i += 1;
            continue;
        };
        i += 1;
        loop {
            i = table_ref(toks, i, out);
            if comma_list && toks.get(i) == Some(&Tok::Comma) {
                i += 1;
            } else {
                break;
            }
        }
    }
}

/// Skip the parenthesized group opening at `i`, first scanning its inside for
/// bindings of its own (a subquery's `FROM b x`); returns the index after it.
fn nested(toks: &[Tok], i: usize, out: &mut Vec<Alias>) -> usize {
    let end = skip_group(toks, i);
    // `end` is past the closing `)` unless the group never closes.
    let inner_end = if toks.get(end.wrapping_sub(1)) == Some(&Tok::Close) {
        end - 1
    } else {
        end
    };
    if i < inner_end {
        scan(&toks[i + 1..inner_end], out);
    }
    end
}

/// Scan one table reference starting at `i`, pushing its alias (if any) to
/// `out`; returns the index of the first token after it.
fn table_ref(toks: &[Tok], mut i: usize, out: &mut Vec<Alias>) -> usize {
    // `FROM ONLY t` (PostgreSQL) and `JOIN LATERAL (…)` prefix the reference.
    while is_kw(toks.get(i), "only") || is_kw(toks.get(i), "lateral") {
        i += 1;
    }
    let mut parts: Vec<String> = Vec::new();
    let mut derived = false;
    if toks.get(i) == Some(&Tok::Open) {
        // A derived table: nothing in the catalog to resolve it to.
        i = nested(toks, i, out);
        derived = true;
    } else {
        while let Some(Tok::Word(w, q)) = toks.get(i) {
            if !q && reserved(w) {
                break;
            }
            parts.push(w.clone());
            i += 1;
            if toks.get(i) == Some(&Tok::Dot) {
                i += 1;
            } else {
                break;
            }
        }
        if parts.is_empty() {
            return i;
        }
        if toks.get(i) == Some(&Tok::Open) {
            // A table function such as `generate_series(1, 3)`.
            i = nested(toks, i, out);
            derived = true;
        }
    }
    let explicit = is_kw(toks.get(i), "as");
    if explicit {
        i += 1;
    }
    let alias = match toks.get(i) {
        Some(Tok::Word(w, q)) if explicit || *q || !reserved(w) => {
            i += 1;
            Some(w.clone())
        }
        _ => None,
    };
    // A column-alias list after the alias: `AS o (a, b)`.
    if alias.is_some() && toks.get(i) == Some(&Tok::Open) {
        i = skip_group(toks, i);
    }
    if let (Some(alias), false, Some(table)) = (alias, derived, parts.pop()) {
        out.push(Alias {
            alias,
            table,
            schema: parts.pop(),
        });
    }
    i
}

/// The table `qualifier` names in `sql`: the aliased table when `qualifier`
/// is one of its aliases (ASCII case-insensitive, the last binding winning),
/// otherwise `None`.
///
/// # Examples
///
/// ```
/// use vix_db::alias::resolve;
/// assert_eq!(resolve("SELECT o. FROM orders o", "O").as_deref(), Some("orders"));
/// assert_eq!(resolve("SELECT o. FROM orders", "o"), None);
/// ```
#[must_use]
pub fn resolve(sql: &str, qualifier: &str) -> Option<String> {
    aliases(sql)
        .into_iter()
        .rev()
        .find(|a| a.alias.eq_ignore_ascii_case(qualifier))
        .map(|a| a.table)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(alias, table)` pairs for compact assertions.
    fn pairs(sql: &str) -> Vec<(String, String)> {
        aliases(sql)
            .into_iter()
            .map(|a| (a.alias, a.table))
            .collect()
    }

    fn p(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, t)| ((*a).to_string(), (*t).to_string()))
            .collect()
    }

    #[test]
    fn bare_and_as_aliases() {
        assert_eq!(pairs("select * from orders o"), p(&[("o", "orders")]));
        assert_eq!(pairs("SELECT * FROM orders AS o"), p(&[("o", "orders")]));
        assert_eq!(
            pairs("SELECT * FROM orders o JOIN customers c ON c.id = o.cid"),
            p(&[("o", "orders"), ("c", "customers")])
        );
        assert_eq!(
            pairs("SELECT * FROM orders o LEFT JOIN customers AS c USING (id)"),
            p(&[("o", "orders"), ("c", "customers")])
        );
    }

    #[test]
    fn comma_joins() {
        assert_eq!(
            pairs("SELECT * FROM a x, b y, c WHERE x.id = y.id"),
            p(&[("x", "a"), ("y", "b")])
        );
        assert_eq!(
            pairs("SELECT * FROM (SELECT 1) s, orders o"),
            p(&[("o", "orders")]),
            "a derived table is skipped but the list goes on"
        );
    }

    #[test]
    fn keywords_are_not_aliases() {
        for sql in [
            "SELECT * FROM t WHERE a = 1",
            "SELECT * FROM t JOIN u ON t.id = u.id",
            "SELECT * FROM t LEFT JOIN u USING (id)",
            "SELECT * FROM t INNER JOIN u ON 1 = 1",
            "SELECT * FROM t CROSS JOIN u",
            "SELECT * FROM t NATURAL JOIN u",
            "SELECT * FROM t GROUP BY a ORDER BY a LIMIT 3",
            "SELECT * FROM t UNION SELECT * FROM u",
            "UPDATE t SET a = 1",
            "DELETE FROM t RETURNING *",
            "SELECT * FROM t;",
            "SELECT * FROM",
        ] {
            assert!(aliases(sql).is_empty(), "{sql}: {:?}", aliases(sql));
        }
    }

    #[test]
    fn as_allows_a_keyword_like_alias_and_quotes_allow_anything() {
        assert_eq!(pairs("SELECT * FROM t AS key"), p(&[("key", "t")]));
        assert_eq!(pairs("SELECT * FROM t \"where\""), p(&[("where", "t")]));
    }

    #[test]
    fn qualified_and_quoted_names() {
        let a = aliases("SELECT * FROM public.orders o");
        assert_eq!(a[0].table, "orders");
        assert_eq!(a[0].schema.as_deref(), Some("public"));
        assert_eq!(
            pairs("SELECT * FROM \"Order Items\" oi JOIN `shop`.`orders` AS \"O\""),
            p(&[("oi", "Order Items"), ("O", "orders")])
        );
        assert_eq!(
            pairs("SELECT * FROM \"we\"\"ird\" w"),
            p(&[("w", "we\"ird")])
        );
        assert_eq!(pairs("SELECT * FROM [dbo].[t] x"), p(&[("x", "t")]));
    }

    #[test]
    fn update_and_table_functions() {
        assert_eq!(
            pairs("UPDATE orders o SET o.total = 0"),
            p(&[("o", "orders")])
        );
        assert_eq!(pairs("SELECT * FROM generate_series(1, 3) g"), p(&[]));
        assert_eq!(pairs("SELECT * FROM ONLY orders o"), p(&[("o", "orders")]));
        assert_eq!(
            pairs("SELECT * FROM orders AS o (a, b) JOIN u x ON true"),
            p(&[("o", "orders"), ("x", "u")])
        );
    }

    #[test]
    fn strings_and_comments_are_ignored() {
        assert_eq!(
            pairs("SELECT 'FROM orders o' -- FROM t x\n/* FROM t y */"),
            p(&[])
        );
        assert_eq!(
            pairs("SELECT * FROM -- note\n orders /* c */ o"),
            p(&[("o", "orders")])
        );
    }

    #[test]
    fn multiline_and_subquery_scopes() {
        let sql = "SELECT o.id\nFROM orders o\nWHERE o.cid IN (SELECT c.id FROM customers c)";
        assert_eq!(pairs(sql), p(&[("o", "orders"), ("c", "customers")]));
    }

    #[test]
    fn resolve_is_case_insensitive_and_last_wins() {
        let sql = "SELECT * FROM a x JOIN (SELECT * FROM b x) y ON true";
        assert_eq!(resolve(sql, "X").as_deref(), Some("b"));
        assert_eq!(resolve(sql, "y"), None, "derived table");
        assert_eq!(resolve("", "x"), None);
    }

    #[test]
    fn hostile_input_never_panics() {
        for sql in [
            "\"",
            "`",
            "[",
            "'",
            "/*",
            "--",
            "FROM (",
            "FROM a.",
            "FROM a AS",
            "))((",
        ] {
            let _ = aliases(sql);
        }
    }
}
