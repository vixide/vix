//! Result-grid export (pgsavvy-style): render rows in interchange formats.
//!
//! Pure renderers — the workbench passes the currently visible (filtered,
//! sorted) headers and rows and writes the returned text to a file or the
//! clipboard. Formats: CSV, TSV, JSON array, NDJSON, Markdown table, and SQL
//! `INSERT` statements (the caller supplies the table name; the workbench
//! uses `vix_export` when the source table is unknown).

use std::fmt::Write as _;

/// An export format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    /// Comma-separated values with RFC-4180 quoting.
    #[default]
    Csv,
    /// Tab-separated values, cells verbatim.
    Tsv,
    /// One JSON array of objects keyed by the headers.
    Json,
    /// Newline-delimited JSON, one object per row.
    Ndjson,
    /// A GitHub-flavored Markdown table.
    Markdown,
    /// One SQL `INSERT` statement per row.
    Sql,
}

/// Every format, in the order the export dialog cycles through them.
pub const FORMATS: &[Format] =
    &[Format::Csv, Format::Tsv, Format::Json, Format::Ndjson, Format::Markdown, Format::Sql];

impl Format {
    /// Display label (also the conventional file extension).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Tsv => "tsv",
            Format::Json => "json",
            Format::Ndjson => "ndjson",
            Format::Markdown => "md",
            Format::Sql => "sql",
        }
    }
}

/// Render `rows` under `headers` in `format`. `table` names the target of
/// SQL `INSERT` statements and is ignored by the other formats.
#[must_use]
pub fn render(format: Format, headers: &[String], rows: &[&Vec<String>], table: &str) -> String {
    match format {
        Format::Csv => separated(headers, rows, ",", csv_cell),
        Format::Tsv => separated(headers, rows, "\t", std::string::ToString::to_string),
        Format::Json => json(headers, rows, true),
        Format::Ndjson => json(headers, rows, false),
        Format::Markdown => markdown(headers, rows),
        Format::Sql => sql_inserts(headers, rows, table),
    }
}

/// Header line plus one line per row, cells mapped by `cell` and joined.
fn separated(
    headers: &[String],
    rows: &[&Vec<String>],
    sep: &str,
    cell: impl Fn(&str) -> String,
) -> String {
    let mut out = String::new();
    let line = |cells: &[String], out: &mut String| {
        let joined: Vec<String> = cells.iter().map(|c| cell(c)).collect();
        out.push_str(&joined.join(sep));
        out.push('\n');
    };
    line(headers, &mut out);
    for row in rows {
        line(row, &mut out);
    }
    out
}

/// RFC-4180: quote when a cell holds a separator, quote, or newline.
fn csv_cell(cell: &str) -> String {
    if cell.contains(['"', ',', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

/// A JSON array of row objects (`array = true`) or NDJSON lines.
fn json(headers: &[String], rows: &[&Vec<String>], array: bool) -> String {
    let objects: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            headers
                .iter()
                .enumerate()
                .map(|(i, h)| (h.clone(), serde_json::Value::from(row.get(i).map_or("", String::as_str))))
                .collect::<serde_json::Map<String, serde_json::Value>>()
                .into()
        })
        .collect();
    if array {
        let mut out =
            serde_json::to_string_pretty(&objects).unwrap_or_else(|_| "[]".to_string());
        out.push('\n');
        out
    } else {
        let mut out = String::new();
        for o in &objects {
            let _ = writeln!(out, "{o}");
        }
        out
    }
}

/// A GitHub-flavored Markdown table (pipes in cells escaped).
fn markdown(headers: &[String], rows: &[&Vec<String>]) -> String {
    let esc = |c: &str| c.replace('|', "\\|");
    let mut out = String::new();
    let joined: Vec<String> = headers.iter().map(|h| esc(h)).collect();
    let _ = writeln!(out, "| {} |", joined.join(" | "));
    let _ = writeln!(out, "|{}", " --- |".repeat(headers.len()));
    for row in rows {
        let cells: Vec<String> = row.iter().map(|c| esc(c)).collect();
        let _ = writeln!(out, "| {} |", cells.join(" | "));
    }
    out
}

/// One `INSERT INTO table (cols) VALUES (…);` per row. Numeric cells stay
/// bare; everything else is single-quoted with internal quotes doubled.
fn sql_inserts(headers: &[String], rows: &[&Vec<String>], table: &str) -> String {
    let cols = headers.join(", ");
    let mut out = String::new();
    for row in rows {
        let values: Vec<String> = row
            .iter()
            .map(|c| {
                if c.parse::<f64>().is_ok() {
                    c.clone()
                } else {
                    format!("'{}'", c.replace('\'', "''"))
                }
            })
            .collect();
        let _ = writeln!(out, "INSERT INTO {table} ({cols}) VALUES ({});", values.join(", "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> (Vec<String>, Vec<Vec<String>>) {
        (
            vec!["id".into(), "name".into()],
            vec![vec!["1".into(), "ada".into()], vec!["2".into(), "say \"hi\", ok".into()]],
        )
    }

    fn refs(rows: &[Vec<String>]) -> Vec<&Vec<String>> {
        rows.iter().collect()
    }

    #[test]
    fn csv_quotes_only_when_needed() {
        let (h, r) = rows();
        let out = render(Format::Csv, &h, &refs(&r), "t");
        assert_eq!(out.lines().next().unwrap(), "id,name");
        assert_eq!(out.lines().nth(1).unwrap(), "1,ada");
        assert_eq!(out.lines().nth(2).unwrap(), "2,\"say \"\"hi\"\", ok\"");
    }

    #[test]
    fn json_and_ndjson_key_by_header() {
        let (h, r) = rows();
        let out = render(Format::Json, &h, &refs(&r), "t");
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed[0]["name"], "ada");
        let nd = render(Format::Ndjson, &h, &refs(&r), "t");
        assert_eq!(nd.lines().count(), 2, "one object per line");
        let first: serde_json::Value = serde_json::from_str(nd.lines().next().unwrap()).unwrap();
        assert_eq!(first["id"], "1");
    }

    #[test]
    fn markdown_escapes_pipes() {
        let h = vec!["a".into()];
        let r = vec![vec!["x|y".into()]];
        let out = render(Format::Markdown, &h, &refs(&r), "t");
        assert!(out.contains("| a |"));
        assert!(out.contains("x\\|y"));
        assert!(out.lines().nth(1).unwrap().contains("---"));
    }

    #[test]
    fn sql_inserts_quote_text_and_leave_numbers_bare() {
        let (h, r) = rows();
        let out = render(Format::Sql, &h, &refs(&r), "users");
        let first = out.lines().next().unwrap();
        assert_eq!(first, "INSERT INTO users (id, name) VALUES (1, 'ada');");
        assert!(out.lines().nth(1).unwrap().contains("'say \"hi\", ok'"));
    }

    #[test]
    fn every_format_renders_and_has_a_label() {
        let (h, r) = rows();
        for f in FORMATS {
            assert!(!render(*f, &h, &refs(&r), "t").is_empty());
            assert!(!f.label().is_empty());
        }
    }
}
