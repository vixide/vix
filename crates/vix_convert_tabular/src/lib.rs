//! Shared CSV/TSV/JSON conversion helpers for Vix's Tools → Convert tools.
//!
//! Tabular text comes in two delimited flavors — CSV (comma-separated, with
//! RFC 4180 quoting) and TSV (tab-separated, no quoting) — and one structured
//! flavor, JSON (an array of objects). Every per-direction tool crate is a thin
//! wrapper over the functions here:
//!
//! - [`parse_csv`] / [`write_csv`] — RFC 4180 quoting (`"` quotes, `""` escapes).
//! - [`parse_tsv`] / [`write_tsv`] — plain tab split/join (tabs and newlines are
//!   assumed absent from fields, as TSV has no escape mechanism).
//! - [`rows_to_json`] — the first row is the header; each later row becomes an
//!   object keyed by the headers (string values), emitted as pretty JSON.
//! - [`json_to_rows`] — an array of objects becomes a header row (the union of
//!   keys, in first-seen order) followed by one row per object.
//!
//! Centralizing the logic means CSV→JSON and JSON→CSV (and the TSV pair) share
//! exactly one parser and one mapper, so the directions stay consistent.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use serde_json::{Map, Value};

/// Parse CSV text into rows of fields, honoring RFC 4180 quoting: fields may be
/// wrapped in `"`, a doubled `""` is a literal quote, and quoted fields may
/// contain commas and newlines. A trailing newline does not add an empty row.
#[must_use]
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if in_quotes {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => in_quotes = false,
                _ => field.push(c),
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => row.push(std::mem::take(&mut field)),
                '\r' => {} // tolerate CRLF
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
    }
    // Flush the final field/row unless the text ended exactly on a newline.
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    } else if any && !text.ends_with('\n') {
        // A single empty field with no trailing newline (e.g. input "").
        rows.push(vec![String::new()]);
    }
    rows
}

/// Write rows as CSV, quoting any field that contains a comma, quote, CR or LF.
#[must_use]
pub fn write_csv(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        for (i, field) in row.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            if field.contains([',', '"', '\n', '\r']) {
                out.push('"');
                out.push_str(&field.replace('"', "\"\""));
                out.push('"');
            } else {
                out.push_str(field);
            }
        }
        out.push('\n');
    }
    out
}

/// Parse TSV text into rows of fields by splitting on tabs and newlines. There
/// is no quoting in TSV. A trailing newline does not add an empty row.
#[must_use]
pub fn parse_tsv(text: &str) -> Vec<Vec<String>> {
    text.strip_suffix('\n')
        .unwrap_or(text)
        .split('\n')
        .map(|line| {
            line.trim_end_matches('\r')
                .split('\t')
                .map(str::to_string)
                .collect()
        })
        .collect()
}

/// Write rows as TSV: fields joined by tabs, rows by newlines.
#[must_use]
pub fn write_tsv(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        out.push_str(&row.join("\t"));
        out.push('\n');
    }
    out
}

/// Convert header+data rows into a pretty-printed JSON array of objects. The
/// first row supplies the keys; each later row becomes one object with string
/// values. Returns `"[]"` when there is no data row.
#[must_use]
pub fn rows_to_json(rows: &[Vec<String>]) -> String {
    let Some((header, data)) = rows.split_first() else {
        return "[]".to_string();
    };
    let records: Vec<Value> = data
        .iter()
        .map(|row| {
            let mut obj = Map::new();
            for (i, key) in header.iter().enumerate() {
                let val = row.get(i).cloned().unwrap_or_default();
                obj.insert(key.clone(), Value::String(val));
            }
            Value::Object(obj)
        })
        .collect();
    serde_json::to_string_pretty(&Value::Array(records)).unwrap_or_else(|_| "[]".to_string())
}

/// Convert a JSON array of objects into header+data rows. The header is the
/// union of all keys in first-seen order; each object yields one row, with
/// missing keys rendered as empty fields. String values are used verbatim; other
/// JSON values are rendered compactly (`42`, `true`, `null` → empty string).
///
/// # Errors
/// Returns an error message when the text is not valid JSON, is not an array, or
/// contains a non-object element.
pub fn json_to_rows(json: &str) -> Result<Vec<Vec<String>>, String> {
    let value: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let Value::Array(items) = value else {
        return Err("expected a JSON array of objects".to_string());
    };
    let mut headers: Vec<String> = Vec::new();
    let mut objects: Vec<&Map<String, Value>> = Vec::new();
    for item in &items {
        let Value::Object(obj) = item else {
            return Err("expected every array element to be an object".to_string());
        };
        for key in obj.keys() {
            if !headers.iter().any(|h| h == key) {
                headers.push(key.clone());
            }
        }
        objects.push(obj);
    }
    let mut rows = Vec::with_capacity(objects.len() + 1);
    rows.push(headers.clone());
    for obj in objects {
        let row = headers
            .iter()
            .map(|key| obj.get(key).map(value_to_field).unwrap_or_default())
            .collect();
        rows.push(row);
    }
    Ok(rows)
}

/// Render a JSON value as a flat cell string: strings verbatim, null as empty,
/// everything else via its compact JSON form.
fn value_to_field(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_round_trips_quoted_fields() {
        let csv = "name,note\n\"Smith, J\",\"says \"\"hi\"\"\"\n";
        let rows = parse_csv(csv);
        assert_eq!(
            rows,
            vec![
                vec!["name".to_string(), "note".to_string()],
                vec!["Smith, J".to_string(), "says \"hi\"".to_string()],
            ]
        );
        assert_eq!(write_csv(&rows), csv);
    }

    #[test]
    fn tsv_splits_and_joins() {
        let rows = parse_tsv("a\tb\n1\t2\n");
        assert_eq!(
            rows,
            vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["1".to_string(), "2".to_string()],
            ]
        );
        assert_eq!(write_tsv(&rows), "a\tb\n1\t2\n");
    }

    #[test]
    fn rows_to_json_uses_header_keys() {
        let rows = parse_csv("a,b\n1,2\n3,4\n");
        let json = rows_to_json(&rows);
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v[0]["a"], "1");
        assert_eq!(v[1]["b"], "4");
        assert_eq!(v.as_array().unwrap().len(), 2);
    }

    #[test]
    fn json_to_rows_unions_keys_in_first_seen_order() {
        let rows = json_to_rows(r#"[{"a":"1","b":"2"},{"a":"3","c":"4"}]"#).unwrap();
        assert_eq!(rows[0], vec!["a", "b", "c"]);
        assert_eq!(rows[1], vec!["1", "2", ""]);
        assert_eq!(rows[2], vec!["3", "", "4"]);
    }

    #[test]
    fn json_to_rows_renders_non_strings() {
        let rows = json_to_rows(r#"[{"n":42,"ok":true,"x":null}]"#).unwrap();
        assert_eq!(rows[1], vec!["42", "true", ""]);
    }

    #[test]
    fn json_to_rows_rejects_non_array() {
        assert!(json_to_rows(r#"{"a":1}"#).is_err());
        assert!(json_to_rows("not json").is_err());
    }

    #[test]
    fn empty_data_is_empty_json_array() {
        assert_eq!(rows_to_json(&parse_csv("a,b\n")), "[]");
    }
}
