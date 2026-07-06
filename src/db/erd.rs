//! Mermaid entity-relationship diagrams from the live catalog.
//!
//! [`mermaid`] turns the flat `(table, column, type)` and
//! `(child, child_col, parent, parent_col)` rows that
//! [`crate::db::catalog::columns_typed_sql`] and
//! [`crate::db::catalog::relationships_sql`] return into a Mermaid
//! `erDiagram` — one entity block per table (attributes typed) and one
//! crow's-foot line per foreign key. The workbench shows the text in a
//! scrollable viewer the user can yank into a `.mmd` file or paste anywhere
//! Mermaid renders (this repo already speaks Mermaid via the Org suite).
//!
//! The function is pure and unit-tested; identifiers are sanitized to the
//! `[A-Za-z0-9_]` tokens Mermaid accepts so odd column or table names cannot
//! produce a diagram that fails to parse.

/// Sanitize `name` to a single Mermaid-safe token: non-word characters become
/// `_`, an empty result becomes `_`, and a leading digit is prefixed so the
/// token is a valid identifier.
fn ident(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    if out.is_empty() {
        out.push('_');
    } else if out.as_bytes()[0].is_ascii_digit() {
        out.insert(0, '_');
    }
    out
}

/// Render an `erDiagram` for `columns` (`(table, column, type)`, grouped in
/// order of appearance) and `relationships`
/// (`(child, child_col, parent, parent_col)`; `parent_col` may be empty).
#[must_use]
pub fn mermaid(
    columns: &[(String, String, String)],
    relationships: &[(String, String, String, String)],
) -> String {
    use std::fmt::Write as _;
    let mut out = String::from("erDiagram\n");

    // Entity blocks, tables in first-seen order, attributes as `type name`.
    let mut order: Vec<String> = Vec::new();
    for (table, _, _) in columns {
        if !order.contains(table) {
            order.push(table.clone());
        }
    }
    for table in &order {
        let _ = writeln!(out, "    {} {{", ident(table));
        for (_, col, ty) in columns.iter().filter(|(t, _, _)| t == table) {
            let ty = if ty.trim().is_empty() { "unknown".to_string() } else { ident(ty) };
            let _ = writeln!(out, "        {ty} {}", ident(col));
        }
        out.push_str("    }\n");
    }

    // One crow's-foot line per distinct foreign key (many children → one
    // parent), labelled with the referencing column.
    let mut seen: Vec<(String, String, String)> = Vec::new();
    for (child, child_col, parent, _parent_col) in relationships {
        if child.is_empty() || parent.is_empty() {
            continue;
        }
        let key = (ident(child), ident(parent), ident(child_col));
        if seen.contains(&key) {
            continue;
        }
        let _ = writeln!(out, "    {} }}o--|| {} : \"{child_col}\"", key.0, key.1);
        seen.push(key);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(t: &str, c: &str, ty: &str) -> (String, String, String) {
        (t.into(), c.into(), ty.into())
    }

    #[test]
    fn renders_entities_and_relationships() {
        let columns = vec![
            col("users", "id", "integer"),
            col("users", "name", "text"),
            col("orders", "id", "integer"),
            col("orders", "user_id", "integer"),
        ];
        let rels = vec![("orders".into(), "user_id".into(), "users".into(), "id".into())];
        let out = mermaid(&columns, &rels);
        assert!(out.starts_with("erDiagram\n"));
        assert!(out.contains("    users {\n"));
        assert!(out.contains("        integer id\n"));
        assert!(out.contains("    orders }o--|| users : \"user_id\""), "{out}");
    }

    #[test]
    fn sanitizes_odd_names_and_types() {
        let columns = vec![col("order items", "qty (int)", "character varying")];
        let out = mermaid(&columns, &[]);
        assert!(out.contains("    order_items {\n"), "{out}");
        assert!(out.contains("        character_varying qty__int_\n"), "{out}");
    }

    #[test]
    fn dedups_composite_edges_and_skips_blank_endpoints() {
        let rels = vec![
            ("a".into(), "x".into(), "b".into(), "y".into()),
            ("a".into(), "x".into(), "b".into(), "z".into()), // same child/parent/col
            (String::new(), "x".into(), "b".into(), "y".into()), // blank child, skipped
        ];
        let out = mermaid(&[col("a", "x", "int")], &rels);
        assert_eq!(out.matches("a }o--|| b").count(), 1, "one edge only: {out}");
    }

    #[test]
    fn missing_type_falls_back_to_unknown() {
        let out = mermaid(&[col("t", "c", "")], &[]);
        assert!(out.contains("        unknown c\n"), "{out}");
    }
}
