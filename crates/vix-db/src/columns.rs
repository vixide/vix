//! Column facts for an editable table preview (T777): which columns are
//! generated (and so read-only), and which offer a fixed set of values to
//! pick from instead of free text.
//!
//! A column's allowed values come, in order of preference, from:
//!
//! 1. a `PostgreSQL` enum type (its `pg_enum` labels, in sort order);
//! 2. a `MySQL` `ENUM('a','b')` column type;
//! 3. a `CHECK (col IN ('a','b'))` constraint on that one column — parsed from
//!    `pg_get_constraintdef` on `PostgreSQL` (including the `col = ANY
//!    (ARRAY[…])` form it rewrites `IN` lists to), from
//!    `information_schema.check_constraints` on `MySQL` 8.0.16 and later, and
//!    from the table's `CREATE` statement on `SQLite`;
//! 4. a boolean column (`true`/`false` on `PostgreSQL`, `1`/`0` where
//!    booleans are integers: `SQLite` `BOOLEAN` and `MySQL` `tinyint(1)`).
//!
//! Everything here is pure: the `*_sql` builders make catalog queries and
//! [`from_rows`] turns their rows into [`ColumnInfo`]. A query that fails (an
//! old server without the catalog view, say) just means no facts, and the
//! column stays plain free text. Owning spec: `crates/vix-db/spec/index.md`.

use super::catalog::{quote_ident, quote_value};
use super::connect::Kind;

/// What the workbench knows about one column of the previewed table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColumnInfo {
    /// The column name, as the result grid's header shows it.
    pub name: String,
    /// The declared type, as the catalog reports it.
    pub data_type: String,
    /// Whether the server computes the column (a generated column, or a
    /// `PostgreSQL` `GENERATED ALWAYS` identity), so it cannot be written.
    pub generated: bool,
    /// The values to pick from in the cell editor; empty for free text.
    pub choices: Vec<String>,
}

/// The `regclass` literal naming `schema`.`table` on `PostgreSQL`, quoted so
/// any name (even one holding `"` or `'`) resolves to exactly that table.
fn pg_regclass(schema: &str, table: &str) -> String {
    let name = format!(
        "{}.{}",
        quote_ident(Kind::Postgres, schema),
        quote_ident(Kind::Postgres, table)
    );
    format!("{}::regclass", quote_value(Kind::Postgres, &name))
}

/// A query listing the columns of `schema`.`table` in declaration order as
/// `(name, type, generated, enum labels)` rows: `generated` is `1` or `0`,
/// and the labels are newline-separated (empty unless the column has a
/// `PostgreSQL` enum type).
///
/// # Examples
///
/// ```
/// use vix_db::{columns::columns_sql, connect::Kind};
/// assert!(columns_sql(Kind::Sqlite, "main", "t").contains("pragma_table_xinfo('t')"));
/// assert!(columns_sql(Kind::Postgres, "public", "t").contains("pg_enum"));
/// ```
#[must_use]
pub fn columns_sql(kind: Kind, schema: &str, table: &str) -> String {
    match kind {
        // hidden: 0 normal, 1 hidden (virtual tables), 2 virtual generated,
        // 3 stored generated.
        Kind::Sqlite => format!(
            "SELECT name, type, CASE WHEN hidden IN (2, 3) THEN 1 ELSE 0 END, '' \
             FROM pragma_table_xinfo({}) WHERE hidden <> 1 ORDER BY cid;",
            quote_value(kind, table)
        ),
        Kind::Postgres => format!(
            "SELECT a.attname::text, format_type(a.atttypid, a.atttypmod), \
             CASE WHEN a.attgenerated <> '' OR a.attidentity = 'a' THEN 1 ELSE 0 END, \
             COALESCE((SELECT string_agg(e.enumlabel::text, chr(10) ORDER BY e.enumsortorder) \
             FROM pg_enum e WHERE e.enumtypid = a.atttypid), '') \
             FROM pg_attribute a WHERE a.attrelid = {} AND a.attnum > 0 AND NOT a.attisdropped \
             ORDER BY a.attnum;",
            pg_regclass(schema, table)
        ),
        Kind::Mysql => format!(
            "SELECT column_name, column_type, \
             CASE WHEN extra LIKE '%VIRTUAL GENERATED%' OR extra LIKE '%STORED GENERATED%' \
             THEN 1 ELSE 0 END, '' \
             FROM information_schema.columns WHERE table_schema = {} AND table_name = {} \
             ORDER BY ordinal_position;",
            quote_value(kind, schema),
            quote_value(kind, table)
        ),
    }
}

/// A query whose rows' **last** column holds text containing the table's
/// `CHECK (…)` constraints: the whole `CREATE` statement on `SQLite`, one
/// constraint definition per row on the server engines.
///
/// # Examples
///
/// ```
/// use vix_db::{columns::checks_sql, connect::Kind};
/// assert!(checks_sql(Kind::Sqlite, "main", "t").contains("sqlite_master"));
/// assert!(checks_sql(Kind::Mysql, "app", "t").contains("check_constraints"));
/// ```
#[must_use]
pub fn checks_sql(kind: Kind, schema: &str, table: &str) -> String {
    match kind {
        Kind::Sqlite => format!(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = {};",
            quote_value(kind, table)
        ),
        Kind::Postgres => format!(
            "SELECT pg_get_constraintdef(c.oid) FROM pg_constraint c \
             WHERE c.conrelid = {} AND c.contype = 'c';",
            pg_regclass(schema, table)
        ),
        Kind::Mysql => format!(
            "SELECT CONCAT('CHECK (', cc.check_clause, ')') \
             FROM information_schema.check_constraints cc \
             JOIN information_schema.table_constraints tc \
             ON tc.constraint_schema = cc.constraint_schema \
             AND tc.constraint_name = cc.constraint_name \
             WHERE tc.table_schema = {} AND tc.table_name = {} AND tc.constraint_type = 'CHECK';",
            quote_value(kind, schema),
            quote_value(kind, table)
        ),
    }
}

/// Build the [`ColumnInfo`] list from the rows of [`columns_sql`] and
/// [`checks_sql`] (pass no check rows when that query failed).
///
/// # Examples
///
/// ```
/// use vix_db::{columns::from_rows, connect::Kind};
/// let cols = vec![
///     vec!["status".to_string(), "TEXT".into(), "0".into(), String::new()],
///     vec!["total".to_string(), "INTEGER".into(), "1".into(), String::new()],
/// ];
/// let checks = vec![vec![
///     "CREATE TABLE t (status TEXT CHECK (status IN ('open', 'done')), total INTEGER)".to_string(),
/// ]];
/// let info = from_rows(Kind::Sqlite, &cols, &checks);
/// assert_eq!(info[0].choices, ["open", "done"]);
/// assert!(info[1].generated);
/// ```
#[must_use]
pub fn from_rows(kind: Kind, columns: &[Vec<String>], checks: &[Vec<String>]) -> Vec<ColumnInfo> {
    let lists: Vec<(String, Vec<String>)> = checks
        .iter()
        .filter_map(|row| row.last())
        .flat_map(|text| check_bodies(text))
        .filter_map(|body| parse_in_check(&body))
        .collect();
    columns
        .iter()
        .filter(|row| !row.is_empty())
        .map(|row| {
            let field = |i: usize| row.get(i).map(String::as_str).unwrap_or_default();
            let (name, data_type) = (field(0).to_string(), field(1).to_string());
            let labels: Vec<String> = field(3)
                .split('\n')
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            let choices = if !labels.is_empty() {
                labels
            } else if let Some(values) = mysql_enum_values(&data_type) {
                values
            } else if let Some((_, values)) = lists.iter().find(|(c, _)| c == &name) {
                values.clone()
            } else {
                bool_choices(kind, &data_type)
            };
            ColumnInfo {
                generated: field(2) == "1",
                name,
                data_type,
                choices,
            }
        })
        .collect()
}

/// The pick list for a boolean column, or nothing for any other type.
fn bool_choices(kind: Kind, data_type: &str) -> Vec<String> {
    let t = data_type.trim().to_ascii_lowercase();
    let boolean = match kind {
        Kind::Postgres => t == "boolean" || t == "bool",
        Kind::Sqlite => t.contains("bool"),
        Kind::Mysql => t.starts_with("tinyint(1)") || t == "bool" || t == "boolean",
    };
    let values: &[&str] = match (boolean, kind) {
        (false, _) => &[],
        (true, Kind::Postgres) => &["true", "false"],
        (true, Kind::Sqlite | Kind::Mysql) => &["1", "0"],
    };
    values.iter().map(|v| (*v).to_string()).collect()
}

/// The values of a `MySQL` `enum('a','b')` column type, or `None` for any
/// other type.
///
/// # Examples
///
/// ```
/// use vix_db::columns::mysql_enum_values;
/// assert_eq!(mysql_enum_values("enum('s','it''s')").unwrap(), ["s", "it's"]);
/// assert_eq!(mysql_enum_values("varchar(10)"), None);
/// ```
#[must_use]
pub fn mysql_enum_values(column_type: &str) -> Option<Vec<String>> {
    let t = column_type.trim();
    let head = t.get(..5)?;
    if !head.eq_ignore_ascii_case("enum(") || !t.ends_with(')') {
        return None;
    }
    let tokens = tokenize(&t[4..])?;
    let values: Vec<String> = tokens
        .into_iter()
        .filter_map(|tok| match tok {
            Token::Str(s) => Some(s),
            _ => None,
        })
        .collect();
    (!values.is_empty()).then_some(values)
}

/// The text inside every `CHECK (…)` in `text`, outermost parentheses
/// removed. Quote-aware, so a parenthesis inside a string literal does not
/// end the body early; an unbalanced body is skipped.
///
/// # Examples
///
/// ```
/// use vix_db::columns::check_bodies;
/// let sql = "CREATE TABLE t (a TEXT CHECK (a IN ('x)', 'y')), CONSTRAINT c CHECK(b > 0))";
/// assert_eq!(check_bodies(sql), ["a IN ('x)', 'y')", "b > 0"]);
/// ```
#[must_use]
pub fn check_bodies(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut quote: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if matches!(c, '\'' | '"' | '`') {
            quote = Some(c);
            i += 1;
            continue;
        }
        let word_start = i == 0 || !is_word(chars[i - 1]);
        let is_check = word_start
            && chars.len() >= i + 5
            && chars[i..i + 5]
                .iter()
                .collect::<String>()
                .eq_ignore_ascii_case("check")
            && chars.get(i + 5).is_none_or(|&n| !is_word(n));
        if !is_check {
            i += 1;
            continue;
        }
        let mut j = i + 5;
        while chars.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        if chars.get(j) != Some(&'(') {
            i += 5;
            continue;
        }
        match matching_paren(&chars, j) {
            Some(end) => {
                out.push(
                    chars[j + 1..end]
                        .iter()
                        .collect::<String>()
                        .trim()
                        .to_string(),
                );
                i = end + 1;
            }
            None => i = j + 1,
        }
    }
    out
}

/// Whether `c` can be part of an SQL word (so `CHECK` inside `RECHECK` does
/// not count).
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The index of the `)` closing the `(` at `open`, skipping quoted text.
fn matching_paren(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    for (k, &c) in chars.iter().enumerate().skip(open) {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => quote = Some(c),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

/// One lexical token of a `CHECK` body.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    /// A bare word (identifier or keyword), as written.
    Word(String),
    /// A quoted identifier (`"x"`, `` `x` `` or `[x]`), unquoted.
    Quoted(String),
    /// A string literal, unescaped.
    Str(String),
    /// A number, as written (a leading `-` folded in).
    Num(String),
    /// Punctuation: `(`, `)`, `[`, `]`, `,`, `=`, or `::` as `:`.
    Sym(char),
}

/// Split `text` into [`Token`]s, or `None` on an unclosed quote or a
/// character no `IN` list contains. A `MySQL` charset introducer
/// (`_utf8mb4'a'`) is dropped in front of its string.
fn tokenize(text: &str) -> Option<Vec<Token>> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '\'' => {
                let (s, next) = quoted(&chars, i, '\'')?;
                out.push(Token::Str(s));
                i = next;
            }
            '"' | '`' => {
                let (s, next) = quoted(&chars, i, c)?;
                out.push(Token::Quoted(s));
                i = next;
            }
            '[' if chars.get(i + 1) != Some(&']')
                && !matches!(out.last(), Some(Token::Word(w)) if w.eq_ignore_ascii_case("array")) =>
            {
                // A bracketed identifier (`[x]`, SQLite style); `ARRAY[…]`
                // and a `type[]` suffix stay punctuation.
                let end = chars[i..].iter().position(|&c| c == ']')? + i;
                out.push(Token::Quoted(chars[i + 1..end].iter().collect()));
                i = end + 1;
            }
            '(' | ')' | '[' | ']' | ',' | '=' => {
                out.push(Token::Sym(c));
                i += 1;
            }
            ':' if chars.get(i + 1) == Some(&':') => {
                out.push(Token::Sym(':'));
                i += 2;
            }
            '-' if chars.get(i + 1).is_some_and(char::is_ascii_digit) => {
                let (n, next) = number(&chars, i + 1);
                out.push(Token::Num(format!("-{n}")));
                i = next;
            }
            c if c.is_ascii_digit() => {
                let (n, next) = number(&chars, i);
                out.push(Token::Num(n));
                i = next;
            }
            c if is_word(c) => {
                let start = i;
                while chars.get(i).is_some_and(|&c| is_word(c)) {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                // `_utf8mb4'a'`: a charset introducer glued to its string.
                if !(word.starts_with('_') && chars.get(i) == Some(&'\'')) {
                    out.push(Token::Word(word));
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// A quoted run starting at `start` (the opening `q`), with doubled quotes
/// unescaped; returns it and the index after the closing quote.
fn quoted(chars: &[char], start: usize, q: char) -> Option<(String, usize)> {
    let mut s = String::new();
    let mut i = start + 1;
    loop {
        let c = *chars.get(i)?;
        if c == q {
            if chars.get(i + 1) == Some(&q) {
                s.push(q);
                i += 2;
                continue;
            }
            return Some((s, i + 1));
        }
        s.push(c);
        i += 1;
    }
}

/// The digits (and a decimal point) from `start`, and the index after them.
fn number(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start;
    while chars
        .get(i)
        .is_some_and(|c| c.is_ascii_digit() || *c == '.')
    {
        i += 1;
    }
    (chars[start..i].iter().collect(), i)
}

/// Words that end a cast's type name (they belong to the expression).
const EXPR_WORDS: [&str; 6] = ["in", "any", "array", "and", "or", "not"];

/// Parse a `CHECK` body that limits one column to a list of values:
/// `col IN ('a', 'b')`, or `PostgreSQL`'s rewrite `(col)::text = ANY
/// ((ARRAY['a'::text, 'b'::text])::text[])`. Returns the column and the
/// values; any other expression (`NOT IN`, an `AND`, a range) is `None`.
///
/// # Examples
///
/// ```
/// use vix_db::columns::parse_in_check;
/// let pg = "((status)::text = ANY ((ARRAY['new'::character varying, 'done'::character varying])::text[]))";
/// assert_eq!(parse_in_check(pg).unwrap(), ("status".to_string(), vec!["new".into(), "done".into()]));
/// let mysql = "(`size` in (_utf8mb4'S',_utf8mb4'M'))";
/// assert_eq!(parse_in_check(mysql).unwrap().1, ["S", "M"]);
/// assert_eq!(parse_in_check("a NOT IN ('x')"), None);
/// ```
#[must_use]
pub fn parse_in_check(body: &str) -> Option<(String, Vec<String>)> {
    let tokens = strip_casts(tokenize(body)?);
    let mut it = tokens.into_iter().peekable();
    let column = match it.next()? {
        Token::Word(w) if !EXPR_WORDS.contains(&w.to_ascii_lowercase().as_str()) => w,
        Token::Quoted(q) => q,
        _ => return None,
    };
    let is_word = |t: &Token, w: &str| matches!(t, Token::Word(x) if x.eq_ignore_ascii_case(w));
    match it.next()? {
        t if is_word(&t, "in") => {}
        Token::Sym('=') => {
            if !is_word(&it.next()?, "any") {
                return None;
            }
            if !is_word(&it.next()?, "array") {
                return None;
            }
        }
        _ => return None,
    }
    let mut values = Vec::new();
    let mut want_value = true;
    for tok in it {
        match tok {
            Token::Sym('[' | ']') => {}
            Token::Str(s) | Token::Num(s) if want_value => {
                values.push(s);
                want_value = false;
            }
            Token::Sym(',') if !want_value => want_value = true,
            _ => return None,
        }
    }
    (!values.is_empty() && !want_value).then_some((column, values))
}

/// Drop parentheses and `::type` casts (including multi-word types such as
/// `character varying` and array suffixes `[]`), which only wrap the parts
/// [`parse_in_check`] looks at.
fn strip_casts(tokens: Vec<Token>) -> Vec<Token> {
    let mut out = Vec::new();
    let mut it = tokens
        .into_iter()
        .filter(|t| !matches!(t, Token::Sym('(' | ')')))
        .peekable();
    while let Some(tok) = it.next() {
        if tok != Token::Sym(':') {
            out.push(tok);
            continue;
        }
        while let Some(Token::Word(w)) = it.peek() {
            if EXPR_WORDS.contains(&w.to_ascii_lowercase().as_str()) {
                break;
            }
            it.next();
        }
        while matches!(it.peek(), Some(Token::Sym('['))) {
            it.next();
            if matches!(it.peek(), Some(Token::Sym(']'))) {
                it.next();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(cells: &[&str]) -> Vec<String> {
        cells.iter().map(|c| (*c).to_string()).collect()
    }

    #[test]
    fn sqlite_in_list_from_the_create_statement() {
        let sql = "CREATE TABLE t (id INTEGER PRIMARY KEY, \
                   size TEXT CHECK(size IN ('S','M','L')), \
                   qty INTEGER, CHECK (qty > 0), \
                   \"level\" INT CHECK (\"level\" IN (1, 2, -3)))";
        let bodies = check_bodies(sql);
        assert_eq!(bodies.len(), 3);
        let lists: Vec<_> = bodies.iter().filter_map(|b| parse_in_check(b)).collect();
        assert_eq!(
            lists,
            vec![
                ("size".to_string(), row(&["S", "M", "L"])),
                ("level".to_string(), row(&["1", "2", "-3"])),
            ]
        );
    }

    #[test]
    fn postgres_any_array_forms() {
        let body = check_bodies(
            "CHECK (((status)::text = ANY ((ARRAY['new'::character varying, 'it''s'::character varying])::text[])))",
        );
        assert_eq!(
            parse_in_check(&body[0]),
            Some(("status".to_string(), row(&["new", "it's"])))
        );
        assert_eq!(
            parse_in_check("(kind = ANY (ARRAY['a'::text, 'b'::text]))"),
            Some(("kind".to_string(), row(&["a", "b"])))
        );
        assert_eq!(
            parse_in_check("(n = ANY (ARRAY[1, 2]))"),
            Some(("n".to_string(), row(&["1", "2"])))
        );
    }

    #[test]
    fn mysql_check_clause_with_introducers() {
        let body = check_bodies("CHECK ((`size` in (_utf8mb4'S',_utf8mb4'M')))");
        assert_eq!(
            parse_in_check(&body[0]),
            Some(("size".to_string(), row(&["S", "M"])))
        );
    }

    #[test]
    fn other_checks_are_not_pick_lists() {
        for body in [
            "a NOT IN ('x')",
            "a IN ('x') AND b > 0",
            "a > 0",
            "a IN ()",
            "a IN ('x',)",
            "lower(a) IN ('x')",
            "a IN ('unclosed)",
        ] {
            assert_eq!(parse_in_check(body), None, "{body}");
        }
    }

    #[test]
    fn check_inside_a_word_or_string_is_ignored() {
        assert_eq!(
            check_bodies("CREATE TABLE recheck (a TEXT DEFAULT 'check (x)')"),
            [] as [String; 0]
        );
    }

    #[test]
    fn mysql_enum_types() {
        assert_eq!(
            mysql_enum_values("ENUM('a','b')").unwrap(),
            row(&["a", "b"])
        );
        assert_eq!(mysql_enum_values("set('a')"), None);
        assert_eq!(mysql_enum_values("enum"), None);
    }

    #[test]
    fn from_rows_prefers_enums_then_checks_then_booleans() {
        let cols = vec![
            row(&["mood", "mood", "0", "happy\nsad"]),
            row(&["flag", "boolean", "0", ""]),
            row(&["total", "integer", "1", ""]),
            row(&["note", "text", "0", ""]),
        ];
        let info = from_rows(Kind::Postgres, &cols, &[]);
        assert_eq!(info[0].choices, row(&["happy", "sad"]));
        assert_eq!(info[1].choices, row(&["true", "false"]));
        assert!(info[2].generated && !info[1].generated);
        assert_eq!(info[3].choices, [] as [String; 0]);

        let mysql = from_rows(
            Kind::Mysql,
            &[
                row(&["size", "enum('S','M')", "0", ""]),
                row(&["on", "tinyint(1)", "0", ""]),
                row(&["n", "int", "0", ""]),
            ],
            &[row(&["CHECK ((`n` in (1,2)))"])],
        );
        assert_eq!(mysql[0].choices, row(&["S", "M"]));
        assert_eq!(mysql[1].choices, row(&["1", "0"]));
        assert_eq!(mysql[2].choices, row(&["1", "2"]));

        let sqlite = from_rows(Kind::Sqlite, &[row(&["done", "BOOLEAN", "0", ""])], &[]);
        assert_eq!(sqlite[0].choices, row(&["1", "0"]));
    }

    #[test]
    fn catalog_queries_quote_names() {
        let pg = columns_sql(Kind::Postgres, "my\"s", "o't");
        assert!(pg.contains("'\"my\"\"s\".\"o''t\"'::regclass"), "{pg}");
        let my = checks_sql(Kind::Mysql, "db", "a\\'b");
        assert!(my.contains("tc.table_name = 'a\\\\''b'"), "{my}");
        let lite = columns_sql(Kind::Sqlite, "main", "o't");
        assert!(lite.contains("pragma_table_xinfo('o''t')"), "{lite}");
    }
}
