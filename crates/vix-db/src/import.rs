//! Import a delimited file (CSV / TSV) into a new table — the inverse of
//! [`crate::export`].
//!
//! [`parse`] reads RFC 4180-ish records (quoted fields may contain the
//! delimiter, quotes, and newlines); [`statements`] turns the first record
//! (the header) into a `CREATE TABLE` of `TEXT` columns and the rest into a
//! single multi-row `INSERT`. Pure and unit-tested; the workbench reads the
//! file and runs the statements (writes, so write mode is required).

use super::catalog::{quote_ident, quote_literal};
use super::connect::Kind;

/// The field delimiter for `path`: a tab for `.tsv` / `.tab`, else a comma.
#[must_use]
pub fn delimiter(path: &str) -> char {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ext == "tsv" || ext == "tab" {
        '\t'
    } else {
        ','
    }
}

/// A table name derived from `path`'s file stem, sanitized to identifier
/// characters (`import` if nothing usable remains).
#[must_use]
pub fn table_name(path: &str) -> String {
    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("import");
    let name: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim_matches('_').to_string();
    if name.is_empty() || name.as_bytes()[0].is_ascii_digit() {
        format!("t_{name}")
    } else {
        name
    }
}

/// Parse `content` into records of fields on `delim`, honoring double-quoted
/// fields (a `""` inside a quoted field is a literal quote).
#[must_use]
pub fn parse(content: &str, delim: char) -> Vec<Vec<String>> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = content.chars().peekable();
    let mut seen_field = false;
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' => {
                in_quotes = true;
                seen_field = true;
            }
            _ if c == delim => {
                record.push(std::mem::take(&mut field));
                seen_field = true;
            }
            '\r' => {}
            '\n' => {
                if seen_field || !field.is_empty() || !record.is_empty() {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                }
                seen_field = false;
            }
            _ => {
                field.push(c);
                seen_field = true;
            }
        }
    }
    if seen_field || !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

/// `CREATE TABLE` + multi-row `INSERT` statements loading `records` (first row
/// = header) into `table`. Empty when there is no header row.
#[must_use]
pub fn statements(kind: Kind, table: &str, records: &[Vec<String>]) -> Vec<String> {
    let Some(header) = records.first() else {
        return Vec::new();
    };
    let width = header.len();
    let cols: Vec<String> = header
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let name: String = h
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let name = name.trim_matches('_');
            if name.is_empty() {
                format!("col{}", i + 1)
            } else {
                name.to_string()
            }
        })
        .collect();
    let ident = quote_ident(kind, table);
    let col_defs: Vec<String> = cols
        .iter()
        .map(|c| format!("{} TEXT", quote_ident(kind, c)))
        .collect();
    let create = format!("CREATE TABLE {ident} ({})", col_defs.join(", "));

    let data = &records[1..];
    if data.is_empty() {
        return vec![create];
    }
    let tuples: Vec<String> = data
        .iter()
        .map(|row| {
            let vals: Vec<String> = (0..width)
                .map(|i| {
                    row.get(i)
                        .map_or_else(|| "''".to_string(), |v| quote_literal(v))
                })
                .collect();
            format!("({})", vals.join(", "))
        })
        .collect();
    let col_list: Vec<String> = cols.iter().map(|c| quote_ident(kind, c)).collect();
    let insert = format!(
        "INSERT INTO {ident} ({}) VALUES {}",
        col_list.join(", "),
        tuples.join(", ")
    );
    vec![create, insert]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delimiter_and_table_name_from_path() {
        assert_eq!(delimiter("/x/data.tsv"), '\t');
        assert_eq!(delimiter("/x/data.csv"), ',');
        assert_eq!(table_name("/x/my data.csv"), "my_data");
        assert_eq!(table_name("/x/9lives.csv"), "t_9lives");
    }

    #[test]
    fn parses_quoted_fields_with_commas_and_quotes() {
        let recs = parse("a,b\n1,\"x,y\"\n2,\"she \"\"said\"\"\"\n", ',');
        assert_eq!(recs[0], vec!["a", "b"]);
        assert_eq!(recs[1], vec!["1", "x,y"]);
        assert_eq!(recs[2], vec!["2", "she \"said\""]);
    }

    #[test]
    fn statements_create_and_insert() {
        let recs = parse("id,name\n1,ada\n2,O'Hara\n", ',');
        let sql = statements(Kind::Sqlite, "people", &recs);
        assert_eq!(
            sql[0],
            "CREATE TABLE \"people\" (\"id\" TEXT, \"name\" TEXT)"
        );
        assert_eq!(
            sql[1], "INSERT INTO \"people\" (\"id\", \"name\") VALUES (1, 'ada'), (2, 'O''Hara')",
            "values quoted as literals; numbers bare",
        );
    }

    #[test]
    fn header_only_creates_an_empty_table() {
        let sql = statements(Kind::Sqlite, "t", &parse("a,b\n", ','));
        assert_eq!(sql.len(), 1, "just the CREATE");
    }
}
