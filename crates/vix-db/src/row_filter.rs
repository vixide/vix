//! Server-side row search on a table preview (T778): a column, an
//! operator and a value become a quoted `WHERE` clause, and the preview is
//! re-run on the server with it, so the search covers the whole table rather
//! than the rows already loaded (which `/` filters client-side).
//!
//! [`where_clause`] is the pure builder. Identifiers go through
//! [`quote_ident`] and values through [`quote_value`], so a typed value can
//! never leave its literal; the one exception is [`Op::Raw`], whose text is
//! the whole condition, passed through as written (the workbench still
//! refuses one that writes). Owning spec: `crates/vix-db/spec/index.md`.

use super::catalog::{quote_ident, quote_value};
use super::connect::Kind;

/// A row-search operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// The value appears anywhere in the column, ignoring case.
    Contains,
    /// `=`.
    Equals,
    /// `<>`.
    NotEqual,
    /// `<`.
    Less,
    /// `<=`.
    LessOrEqual,
    /// `>`.
    Greater,
    /// `>=`.
    GreaterOrEqual,
    /// `BETWEEN low AND high`, typed as `low..high`.
    Between,
    /// `IN (a, b, …)`, typed as a comma-separated list.
    In,
    /// `IS NULL` (no value).
    IsNull,
    /// `IS NOT NULL` (no value).
    IsNotNull,
    /// The value is the whole condition, as written.
    Raw,
}

/// Every operator, in the order the filter row cycles through them.
pub const OPS: [Op; 12] = [
    Op::Contains,
    Op::Equals,
    Op::NotEqual,
    Op::Less,
    Op::LessOrEqual,
    Op::Greater,
    Op::GreaterOrEqual,
    Op::Between,
    Op::In,
    Op::IsNull,
    Op::IsNotNull,
    Op::Raw,
];

impl Op {
    /// The operator's label on the filter row: SQL symbols and keywords as
    /// written, the rest translated.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Op::Contains => t!("ui.db_filter_op_contains").to_string(),
            Op::Equals => "=".into(),
            Op::NotEqual => "<>".into(),
            Op::Less => "<".into(),
            Op::LessOrEqual => "<=".into(),
            Op::Greater => ">".into(),
            Op::GreaterOrEqual => ">=".into(),
            Op::Between => t!("ui.db_filter_op_between").to_string(),
            Op::In => "IN (a, b, …)".into(),
            Op::IsNull => "IS NULL".into(),
            Op::IsNotNull => "IS NOT NULL".into(),
            Op::Raw => t!("ui.db_filter_op_raw").to_string(),
        }
    }

    /// Whether the operator takes a value.
    #[must_use]
    pub fn needs_value(self) -> bool {
        !matches!(self, Op::IsNull | Op::IsNotNull)
    }
}

/// Why a row filter could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FilterError {
    /// The operator needs a value and none was typed.
    MissingValue,
    /// A [`Op::Between`] value is not `low..high`.
    BadRange,
    /// An [`Op::In`] list has no values.
    EmptyList,
    /// A quoted item in an [`Op::In`] list is not closed.
    UnclosedQuote,
}

impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = match self {
            FilterError::MissingValue => "msg.db_filter_missing_value",
            FilterError::BadRange => "msg.db_filter_bad_range",
            FilterError::EmptyList => "msg.db_filter_empty_list",
            FilterError::UnclosedQuote => "msg.db_filter_unclosed_quote",
        };
        f.write_str(&t!(key))
    }
}

impl std::error::Error for FilterError {}

/// The `WHERE` condition (without the keyword) for `column op value` on
/// `kind`.
///
/// `Contains` is `ILIKE` on `PostgreSQL` and `LOWER(…) LIKE LOWER(…)` on
/// `MySQL` and `SQLite`, with the column cast to text so it works on any
/// type, and `%`, `_` and the escape character `!` in the value escaped.
///
/// # Errors
///
/// A [`FilterError`] when the value does not fit the operator.
///
/// # Examples
///
/// ```
/// use vix_db::{connect::Kind, row_filter::{where_clause, Op}};
/// assert_eq!(
///     where_clause(Kind::Postgres, "name", Op::Contains, "50%").unwrap(),
///     "CAST(\"name\" AS TEXT) ILIKE '%50!%%' ESCAPE '!'"
/// );
/// assert_eq!(
///     where_clause(Kind::Mysql, "id", Op::In, "1, 2").unwrap(),
///     "`id` IN ('1', '2')"
/// );
/// assert_eq!(where_clause(Kind::Sqlite, "x", Op::IsNull, "").unwrap(), "\"x\" IS NULL");
/// ```
pub fn where_clause(kind: Kind, column: &str, op: Op, value: &str) -> Result<String, FilterError> {
    let col = quote_ident(kind, column);
    if op.needs_value() && value.trim().is_empty() {
        return Err(FilterError::MissingValue);
    }
    let lit = |v: &str| quote_value(kind, v);
    let cmp = |sym: &str| format!("{col} {sym} {}", lit(value.trim()));
    Ok(match op {
        Op::Contains => {
            let pattern = lit(&like_pattern(value));
            match kind {
                Kind::Postgres => format!("CAST({col} AS TEXT) ILIKE {pattern} ESCAPE '!'"),
                Kind::Mysql => {
                    format!("LOWER(CAST({col} AS CHAR)) LIKE LOWER({pattern}) ESCAPE '!'")
                }
                Kind::Sqlite => {
                    format!("LOWER(CAST({col} AS TEXT)) LIKE LOWER({pattern}) ESCAPE '!'")
                }
            }
        }
        Op::Equals => cmp("="),
        Op::NotEqual => cmp("<>"),
        Op::Less => cmp("<"),
        Op::LessOrEqual => cmp("<="),
        Op::Greater => cmp(">"),
        Op::GreaterOrEqual => cmp(">="),
        Op::Between => {
            let (low, high) = parse_range(value)?;
            format!("{col} BETWEEN {} AND {}", lit(&low), lit(&high))
        }
        Op::In => {
            let items: Vec<String> = parse_list(value)?.iter().map(|v| lit(v)).collect();
            format!("{col} IN ({})", items.join(", "))
        }
        Op::IsNull => format!("{col} IS NULL"),
        Op::IsNotNull => format!("{col} IS NOT NULL"),
        Op::Raw => value.trim().to_string(),
    })
}

/// A `LIKE` pattern matching `value` anywhere: `%value%`, with `!`, `%` and
/// `_` escaped by `!` (the clause's `ESCAPE '!'`).
///
/// # Examples
///
/// ```
/// assert_eq!(vix_db::row_filter::like_pattern("a_b!"), "%a!_b!!%");
/// ```
#[must_use]
pub fn like_pattern(value: &str) -> String {
    let mut out = String::from("%");
    for c in value.chars() {
        if matches!(c, '!' | '%' | '_') {
            out.push('!');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// Split a `low..high` range at its first `..`, both sides trimmed and
/// non-empty.
///
/// # Errors
///
/// [`FilterError::BadRange`] when there is no `..` or a side is empty.
///
/// # Examples
///
/// ```
/// use vix_db::row_filter::parse_range;
/// assert_eq!(parse_range("1.5..2").unwrap(), ("1.5".to_string(), "2".to_string()));
/// assert!(parse_range("5").is_err());
/// ```
pub fn parse_range(value: &str) -> Result<(String, String), FilterError> {
    let (low, high) = value.split_once("..").ok_or(FilterError::BadRange)?;
    let (low, high) = (low.trim(), high.trim());
    if low.is_empty() || high.is_empty() {
        return Err(FilterError::BadRange);
    }
    Ok((low.to_string(), high.to_string()))
}

/// Split an `IN` list on commas. An item may be single- or double-quoted to
/// hold a comma or surrounding spaces (a doubled quote inside stands for
/// one); unquoted items are trimmed, and empty ones are dropped.
///
/// # Errors
///
/// [`FilterError::EmptyList`] for a list with no items, or
/// [`FilterError::UnclosedQuote`] for an unterminated quoted item.
///
/// # Examples
///
/// ```
/// use vix_db::row_filter::parse_list;
/// assert_eq!(parse_list("a, 'b, c', \"it''s\"").unwrap(), ["a", "b, c", "it''s"]);
/// assert_eq!(parse_list("'it''s',,x").unwrap(), ["it's", "x"]);
/// ```
pub fn parse_list(value: &str) -> Result<Vec<String>, FilterError> {
    let chars: Vec<char> = value.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i <= chars.len() {
        while chars.get(i).is_some_and(|c| c.is_whitespace()) {
            i += 1;
        }
        if let Some(&q @ ('\'' | '"')) = chars.get(i) {
            let mut item = String::new();
            i += 1;
            loop {
                let c = *chars.get(i).ok_or(FilterError::UnclosedQuote)?;
                if c == q {
                    if chars.get(i + 1) == Some(&q) {
                        item.push(q);
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                item.push(c);
                i += 1;
            }
            out.push(item);
            // Skip to the next comma (text after a closing quote is ignored).
            while chars.get(i).is_some_and(|&c| c != ',') {
                i += 1;
            }
        } else {
            let start = i.min(chars.len());
            while chars.get(i).is_some_and(|&c| c != ',') {
                i += 1;
            }
            let item: String = chars[start..i.min(chars.len())].iter().collect();
            let item = item.trim();
            if !item.is_empty() {
                out.push(item.to_string());
            }
        }
        i += 1;
    }
    if out.is_empty() {
        return Err(FilterError::EmptyList);
    }
    Ok(out)
}

/// The filter row being filled in: which field is selected and what each
/// holds. Column and operator are indices (into the grid headers and
/// [`OPS`]).
#[derive(Debug, Clone, Default)]
pub struct FilterForm {
    /// Whether the form is open over the workbench (it takes the keys while
    /// it is).
    pub open: bool,
    /// The selected field: 0 column, 1 operator, 2 value.
    pub field: usize,
    /// Index of the column (into the grid's headers).
    pub column: usize,
    /// Index of the operator into [`OPS`].
    pub op: usize,
    /// The typed value.
    pub value: String,
}

impl FilterForm {
    /// The selected operator.
    #[must_use]
    pub fn op(&self) -> Op {
        OPS[self.op % OPS.len()]
    }

    /// The condition this form describes for `kind`, given the grid's
    /// `headers`.
    ///
    /// # Errors
    ///
    /// A [`FilterError`] from [`where_clause`].
    pub fn clause(&self, kind: Kind, headers: &[String]) -> Result<String, FilterError> {
        let column = headers.get(self.column).map_or("", String::as_str);
        where_clause(kind, column, self.op(), &self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_is_ilike_on_postgres_and_lower_like_elsewhere() {
        assert_eq!(
            where_clause(Kind::Postgres, "n", Op::Contains, "Ab").unwrap(),
            "CAST(\"n\" AS TEXT) ILIKE '%Ab%' ESCAPE '!'"
        );
        assert_eq!(
            where_clause(Kind::Mysql, "n", Op::Contains, "Ab").unwrap(),
            "LOWER(CAST(`n` AS CHAR)) LIKE LOWER('%Ab%') ESCAPE '!'"
        );
        assert_eq!(
            where_clause(Kind::Sqlite, "n", Op::Contains, "Ab").unwrap(),
            "LOWER(CAST(\"n\" AS TEXT)) LIKE LOWER('%Ab%') ESCAPE '!'"
        );
    }

    #[test]
    fn comparisons_quote_per_engine() {
        assert_eq!(
            where_clause(Kind::Postgres, "id", Op::Equals, " 5 ").unwrap(),
            "\"id\" = '5'"
        );
        assert_eq!(
            where_clause(Kind::Sqlite, "id", Op::GreaterOrEqual, "5").unwrap(),
            "\"id\" >= 5"
        );
        assert_eq!(
            where_clause(Kind::Mysql, "my`col", Op::NotEqual, "x").unwrap(),
            "`my``col` <> 'x'"
        );
        assert_eq!(
            where_clause(Kind::Postgres, "d", Op::Between, "2026-01-01..2026-02-01").unwrap(),
            "\"d\" BETWEEN '2026-01-01' AND '2026-02-01'"
        );
        assert_eq!(
            where_clause(Kind::Sqlite, "n", Op::Less, "-1.5").unwrap(),
            "\"n\" < -1.5"
        );
        assert_eq!(
            where_clause(Kind::Sqlite, "n", Op::LessOrEqual, "a").unwrap(),
            "\"n\" <= 'a'"
        );
        assert_eq!(
            where_clause(Kind::Sqlite, "n", Op::Greater, "a").unwrap(),
            "\"n\" > 'a'"
        );
        assert_eq!(
            where_clause(Kind::Postgres, "n", Op::IsNotNull, "").unwrap(),
            "\"n\" IS NOT NULL"
        );
    }

    #[test]
    fn literals_cannot_escape() {
        let evil = "x' OR '1'='1";
        assert_eq!(
            where_clause(Kind::Postgres, "n", Op::Equals, evil).unwrap(),
            "\"n\" = 'x'' OR ''1''=''1'"
        );
        assert_eq!(
            where_clause(Kind::Mysql, "n", Op::Equals, r"\' OR 1=1 -- ").unwrap(),
            r"`n` = '\\'' OR 1=1 --'"
        );
        assert_eq!(
            where_clause(Kind::Sqlite, "n\"; DROP TABLE t; --", Op::IsNull, "").unwrap(),
            "\"n\"\"; DROP TABLE t; --\" IS NULL"
        );
        assert_eq!(
            where_clause(Kind::Sqlite, "n", Op::In, "'a'', b', c").unwrap(),
            "\"n\" IN ('a'', b', 'c')"
        );
    }

    #[test]
    fn raw_sql_is_passed_through_only_for_raw() {
        let raw = "price * 2 > 10 OR name LIKE 'A%'";
        assert_eq!(
            where_clause(Kind::Mysql, "ignored", Op::Raw, raw).unwrap(),
            raw
        );
        assert_ne!(
            where_clause(Kind::Mysql, "c", Op::Equals, raw).unwrap(),
            raw
        );
    }

    #[test]
    fn value_errors() {
        assert_eq!(
            where_clause(Kind::Sqlite, "n", Op::Equals, "  "),
            Err(FilterError::MissingValue)
        );
        assert_eq!(
            where_clause(Kind::Sqlite, "n", Op::Between, "1.."),
            Err(FilterError::BadRange)
        );
        assert_eq!(parse_list(" , ,"), Err(FilterError::EmptyList));
        assert_eq!(parse_list("a, 'b"), Err(FilterError::UnclosedQuote));
        assert_eq!(parse_list("a,b ,  c ").unwrap(), ["a", "b", "c"]);
        assert_eq!(parse_list("' x ' , \"y,z\"").unwrap(), [" x ", "y,z"]);
    }

    #[test]
    fn form_builds_from_the_headers() {
        let headers = vec!["id".to_string(), "name".to_string()];
        let form = FilterForm {
            column: 1,
            op: 1,
            value: "ada".into(),
            ..FilterForm::default()
        };
        assert_eq!(form.op(), Op::Equals);
        assert_eq!(
            form.clause(Kind::Sqlite, &headers).unwrap(),
            "\"name\" = 'ada'"
        );
        assert!(OPS.iter().all(|op| !op.label().is_empty()));
    }
}
