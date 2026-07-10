//! Natural-language → SQL assistant for the workbench (schema-only, read-only).
//!
//! The surus text-to-SQL model: the assistant is shown the database *schema* —
//! tables, columns, types, and foreign keys — but never a single row of data,
//! and it is told the connection is read-only so it drafts a `SELECT`. The
//! workbench then validates the draft with `EXPLAIN` before anything runs, and
//! [`optimize_context`] feeds a query plus its plan back for a faster rewrite
//! (the "tight loop"). Everything here is pure and unit-tested; the host
//! (`app.rs`) spawns the configured assistant CLI over the text these builders
//! produce.
//!
//! Splitting instruction from context matters for safety: the fixed
//! [`instruction`] is what lands on the assistant's command line, while the
//! schema and the user's free-text question travel on stdin — so no
//! user-supplied text is ever interpolated into a shell command.

use std::fmt::Write as _;

/// Column triples `(table, column, type)` as the catalog returns them.
pub type Columns = [(String, String, String)];

/// Foreign-key edges `(child_table, child_column, parent_table, parent_column)`.
pub type Relationships = [(String, String, String, String)];

/// The fixed instruction passed on the assistant's command line. The schema
/// and question travel on stdin, so this string never contains user text.
#[must_use]
pub fn instruction(read_only: bool) -> String {
    let access = if read_only {
        "The connection is READ-ONLY: answer with a single SELECT (never INSERT, UPDATE, DELETE, or DDL)."
    } else {
        "Prefer a SELECT; use a write statement only if the question clearly asks to change data."
    };
    format!(
        "You are a SQL assistant. Read the database schema and the question from standard input, \
         then reply with exactly one SQL query that answers it. {access} \
         Output only the SQL statement — no explanation and no Markdown code fences."
    )
}

/// The instruction for a plain-English explanation of a query (no SQL back).
#[must_use]
pub fn explain_instruction() -> String {
    "You are a SQL tutor. Read the schema and the SQL query from standard input \
     and explain in plain English, for a developer, what the query does — the \
     tables and joins it touches, the filters, and the shape of its result. Be \
     concise. Do not output SQL."
        .to_string()
}

/// The instruction for answering a data-model question about the schema in
/// plain English (no query run).
#[must_use]
pub fn answer_instruction() -> String {
    "You are a database expert. Read the schema and the question from standard \
     input and answer in plain English, referring to specific tables and \
     columns. Do not output a runnable query unless a short example clarifies \
     the answer."
        .to_string()
}

/// The stdin brief for a fresh question: engine, schema summary, and the
/// question. Row data is never included — privacy by construction.
#[must_use]
pub fn context(engine: &str, columns: &Columns, rels: &Relationships, question: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Engine: {engine}");
    out.push_str(&schema_block(columns, rels));
    let _ = write!(out, "\nQuestion: {}", question.trim());
    out
}

/// The stdin brief for an optimization round: the schema, the current query,
/// and its `EXPLAIN` plan, asking for a faster equivalent with the same result.
#[must_use]
pub fn optimize_context(
    engine: &str,
    columns: &Columns,
    rels: &Relationships,
    sql: &str,
    plan: &str,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Engine: {engine}");
    out.push_str(&schema_block(columns, rels));
    let _ = write!(
        out,
        "\nRewrite this query to run faster (avoid full scans, use indexes) while returning \
         exactly the same rows.\nQuery:\n{}\n\nEXPLAIN plan:\n{}",
        sql.trim(),
        plan.trim()
    );
    out
}

/// The stdin brief for fixing a failed query: schema, the query, and the
/// database's error message, asking for a corrected statement.
#[must_use]
pub fn error_context(
    engine: &str,
    columns: &Columns,
    rels: &Relationships,
    sql: &str,
    error: &str,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Engine: {engine}");
    out.push_str(&schema_block(columns, rels));
    let _ = write!(
        out,
        "\nThis query failed. Return a corrected version.\nQuery:\n{}\n\nError:\n{}",
        sql.trim(),
        error.trim()
    );
    out
}

/// The stdin brief for explaining a query: schema plus the query to explain.
#[must_use]
pub fn explain_context(engine: &str, columns: &Columns, rels: &Relationships, sql: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Engine: {engine}");
    out.push_str(&schema_block(columns, rels));
    let _ = write!(out, "\nExplain this query:\n{}", sql.trim());
    out
}

/// A `Tables:` / `Foreign keys:` summary of the schema, tables in first-seen
/// order with typed columns.
fn schema_block(columns: &Columns, rels: &Relationships) -> String {
    let mut out = String::from("Tables:\n");
    let mut order: Vec<&str> = Vec::new();
    for (table, _, _) in columns {
        if !order.contains(&table.as_str()) {
            order.push(table);
        }
    }
    for table in &order {
        let cols: Vec<String> = columns
            .iter()
            .filter(|(t, _, _)| t == table)
            .map(|(_, c, ty)| if ty.trim().is_empty() { c.clone() } else { format!("{c} {ty}") })
            .collect();
        let _ = writeln!(out, "  {table}({})", cols.join(", "));
    }
    if !rels.is_empty() {
        out.push_str("Foreign keys:\n");
        for (child, child_col, parent, parent_col) in rels {
            if child.is_empty() || parent.is_empty() {
                continue;
            }
            let target =
                if parent_col.is_empty() { parent.clone() } else { format!("{parent}.{parent_col}") };
            let _ = writeln!(out, "  {child}.{child_col} -> {target}");
        }
    }
    out
}

/// Keywords that mark the start of a SQL statement, for recovering the query
/// from a reply that leaked prose despite the instruction.
const SQL_STARTS: &[&str] =
    &["select", "with", "insert", "update", "delete", "create", "explain", "pragma", "alter", "drop"];

/// Recover one runnable statement from the assistant's reply: prefer the first
/// fenced ```` ``` ```` block, otherwise drop any leading prose up to the first
/// line that begins with a SQL keyword. Falls back to the trimmed reply.
#[must_use]
pub fn extract_sql(reply: &str) -> String {
    if let Some(fenced) = fenced_block(reply) {
        return fenced;
    }
    let starts_sql = |line: &str| {
        let low = line.trim_start().to_ascii_lowercase();
        SQL_STARTS.iter().any(|kw| low.starts_with(kw))
    };
    let lines: Vec<&str> = reply.lines().collect();
    if let Some(pos) = lines.iter().position(|l| starts_sql(l)) {
        return lines[pos..].join("\n").trim().to_string();
    }
    reply.trim().to_string()
}

/// The content of the first triple-backtick fenced block, dropping an optional
/// language tag on the opening fence line; `None` when there is no closing
/// fence.
fn fenced_block(reply: &str) -> Option<String> {
    let start = reply.find("```")?;
    let after = &reply[start + 3..];
    let body_start = after.find('\n').map_or(after, |nl| &after[nl + 1..]);
    let end = body_start.find("```")?;
    Some(body_start[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols() -> Vec<(String, String, String)> {
        vec![
            ("users".into(), "id".into(), "integer".into()),
            ("users".into(), "name".into(), "text".into()),
            ("orders".into(), "id".into(), "integer".into()),
            ("orders".into(), "user_id".into(), "integer".into()),
        ]
    }

    #[test]
    fn context_is_schema_only_and_names_no_rows() {
        let rels = vec![("orders".into(), "user_id".into(), "users".into(), "id".into())];
        let ctx = context("sqlite", &cols(), &rels, "  how many orders per user?  ");
        assert!(ctx.contains("Engine: sqlite"));
        assert!(ctx.contains("users(id integer, name text)"), "{ctx}");
        assert!(ctx.contains("orders.user_id -> users.id"), "{ctx}");
        assert!(ctx.trim_end().ends_with("Question: how many orders per user?"), "{ctx}");
    }

    #[test]
    fn instruction_reflects_read_only() {
        assert!(instruction(true).contains("READ-ONLY"));
        assert!(!instruction(false).contains("READ-ONLY"));
    }

    #[test]
    fn optimize_context_carries_query_and_plan() {
        let out = optimize_context("postgres", &cols(), &[], "SELECT * FROM users", "Seq Scan on users");
        assert!(out.contains("Query:\nSELECT * FROM users"), "{out}");
        assert!(out.contains("EXPLAIN plan:\nSeq Scan on users"), "{out}");
    }

    #[test]
    fn error_context_carries_query_and_error() {
        let out = error_context("sqlite", &cols(), &[], "SELECT * FROM userss", "no such table: userss");
        assert!(out.contains("Query:\nSELECT * FROM userss"), "{out}");
        assert!(out.contains("Error:\nno such table: userss"), "{out}");
    }

    #[test]
    fn explain_context_and_prose_instructions() {
        let out = explain_context("mysql", &cols(), &[], "SELECT 1");
        assert!(out.contains("Explain this query:\nSELECT 1"), "{out}");
        assert!(explain_instruction().contains("plain English"));
        assert!(answer_instruction().contains("plain English"));
    }

    #[test]
    fn extract_sql_prefers_a_fenced_block() {
        let reply = "Here you go:\n```sql\nSELECT 1;\n```\nHope that helps!";
        assert_eq!(extract_sql(reply), "SELECT 1;");
    }

    #[test]
    fn extract_sql_strips_leading_prose_without_fences() {
        let reply = "Sure! This query does it:\nSELECT name FROM users WHERE id = 1;";
        assert_eq!(extract_sql(reply), "SELECT name FROM users WHERE id = 1;");
    }

    #[test]
    fn extract_sql_handles_bare_sql_and_empty() {
        assert_eq!(extract_sql("  WITH t AS (SELECT 1) SELECT * FROM t  "), "WITH t AS (SELECT 1) SELECT * FROM t");
        assert_eq!(extract_sql("no sql here"), "no sql here");
        assert_eq!(extract_sql("   "), "");
    }
}
