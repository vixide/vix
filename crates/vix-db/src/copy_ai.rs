//! Copy for AI (T782): put a result, or a table's structure with its rows, on
//! the clipboard as Markdown ready to paste into a chat with an assistant.
//!
//! - **Results** (`A` in the results pane): the statement behind the grid in
//!   an `sql` fence, the column types when they are known (a table preview,
//!   whose types come from the catalog — the workbench's result stream
//!   carries column names, not types), and the rows from
//!   the top of the visible page down, in the grid's filtered and sorted
//!   order, at most [`ROW_CAP`].
//! - **Table** (`A` on a schema-tree table or view): its columns, primary key,
//!   foreign keys, constraints and indexes from the catalog, then up to
//!   [`ROW_CAP`] preview rows.
//!
//! The tables are rendered by [`crate::export`]'s Markdown format. This is a
//! copy the user starts, of rows the user is looking at; it does not change
//! the AI assistant's schema-only rule (the assistant itself is never sent a
//! row). Owning spec: `crates/vix-db/spec/index.md`.

use std::fmt::Write as _;

use super::{Browser, catalog, connect, export};

/// The most rows one copy carries, so a paste stays a reasonable size for a
/// chat prompt.
pub(crate) const ROW_CAP: usize = 50;

/// A fenced `sql` block whose fence is longer than any run of backticks in
/// `sql`, so the statement cannot close it early.
fn sql_block(sql: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in sql.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    let fence = "`".repeat((longest + 1).max(3));
    format!("{fence}sql\n{}\n{fence}\n", sql.trim())
}

/// A Markdown table of `rows` under `headers`, or `—` when there are none.
fn table_or_dash(headers: &[String], rows: &[&Vec<String>]) -> String {
    if headers.is_empty() {
        "—\n".to_string()
    } else {
        export::render(export::Format::Markdown, headers, rows, "")
    }
}

/// The headings a results copy uses, already translated.
pub(crate) struct ResultLabels {
    /// "Query".
    pub query: String,
    /// "Columns".
    pub columns: String,
    /// "Column".
    pub column: String,
    /// "Type".
    pub type_: String,
    /// "Rows (n of m)".
    pub rows: String,
}

/// Markdown for a result: the statement (when known), a column/type table
/// (when any type is known), and `rows` under `headers`.
pub(crate) fn results(
    sql: Option<&str>,
    headers: &[String],
    types: &[Option<String>],
    rows: &[&Vec<String>],
    labels: &ResultLabels,
) -> String {
    let mut out = String::new();
    if let Some(sql) = sql.filter(|s| !s.trim().is_empty()) {
        let _ = write!(out, "### {}\n\n{}\n", labels.query, sql_block(sql));
    }
    if types.iter().any(Option::is_some) {
        let pairs: Vec<Vec<String>> = headers
            .iter()
            .enumerate()
            .map(|(i, h)| {
                vec![
                    h.clone(),
                    types.get(i).cloned().flatten().unwrap_or_default(),
                ]
            })
            .collect();
        let refs: Vec<&Vec<String>> = pairs.iter().collect();
        let _ = write!(
            out,
            "### {}\n\n{}\n",
            labels.columns,
            table_or_dash(&[labels.column.clone(), labels.type_.clone()], &refs)
        );
    }
    let _ = write!(
        out,
        "### {}\n\n{}",
        labels.rows,
        table_or_dash(headers, rows)
    );
    out
}

/// One section of a table copy: its heading and its catalog report (`None`
/// when the query failed or the engine has no such report).
pub(crate) type Section = (String, Option<(Vec<String>, Vec<Vec<String>>)>);

/// Markdown for a table's structure: a `## title` heading, then each section
/// as `### heading` with its report as a Markdown table, rows capped at
/// [`ROW_CAP`] (the caller puts the row count in the rows heading).
pub(crate) fn table(title: &str, sections: &[Section]) -> String {
    let mut out = format!("## {title}\n\n");
    for (heading, report) in sections {
        let body = match report {
            Some((headers, rows)) => {
                let refs: Vec<&Vec<String>> = rows.iter().take(ROW_CAP).collect();
                table_or_dash(headers, &refs)
            }
            None => "—\n".to_string(),
        };
        let _ = write!(out, "### {heading}\n\n{body}\n");
    }
    out
}

impl Browser {
    /// Copy the current result for an AI chat (`A` in the results pane).
    pub(super) fn copy_results_for_ai(&mut self) {
        if self.grid.headers.is_empty() {
            self.message = Some(t!("msg.db_copy_ai_empty").to_string());
            return;
        }
        let types = self.result_column_types();
        let order = self.grid.filtered();
        let start = self.grid.scroll.min(order.len());
        let rows: Vec<&Vec<String>> = order[start..]
            .iter()
            .take(ROW_CAP)
            .map(|&i| &self.grid.rows[i])
            .collect();
        let labels = ResultLabels {
            query: t!("ui.db_copy_ai_query").to_string(),
            columns: t!("ui.db_copy_ai_columns").to_string(),
            column: t!("ui.db_copy_ai_column").to_string(),
            type_: t!("ui.db_copy_ai_type").to_string(),
            rows: t!(
                "ui.db_copy_ai_rows",
                shown = rows.len(),
                total = order.len()
            )
            .to_string(),
        };
        let sql = self.grid_source.as_ref().map(|(sql, _)| sql.as_str());
        let text = results(sql, &self.grid.headers, &types, &rows, &labels);
        let count = rows.len();
        self.yank_with(&text, t!("msg.db_copied_ai", rows = count).to_string());
    }

    /// The type of each grid column, when the grid is a table preview (from
    /// the catalog's column report); all `None` otherwise.
    fn result_column_types(&mut self) -> Vec<Option<String>> {
        let none = vec![None; self.grid.headers.len()];
        let (Some(kind), Some((schema, table))) = (
            self.conn.as_ref().map(|c| c.kind),
            self.grid_source.as_ref().and_then(|(_, t)| t.clone()),
        ) else {
            return none;
        };
        let sql = catalog::detail_sql(kind, catalog::Detail::Columns, &schema, &table);
        let Ok((_, rows)) = self.run_catalog(&sql) else {
            return none;
        };
        // `PRAGMA table_info` is (cid, name, type, …); the information_schema
        // reports are (name, type, …).
        let (name_at, type_at) = if kind == connect::Kind::Sqlite {
            (1, 2)
        } else {
            (0, 1)
        };
        let types: std::collections::HashMap<&str, &str> = rows
            .iter()
            .filter_map(|r| Some((r.get(name_at)?.as_str(), r.get(type_at)?.as_str())))
            .collect();
        self.grid
            .headers
            .iter()
            .map(|h| types.get(h.as_str()).map(|t| (*t).to_string()))
            .collect()
    }

    /// Copy the selected schema-tree table's structure and preview rows for an
    /// AI chat (`A` in the schema tree).
    pub(super) fn copy_table_for_ai(&mut self) {
        let Some((schema, table, folder)) = self.tree.selected_object() else {
            self.message = Some(t!("msg.db_copy_ai_empty").to_string());
            return;
        };
        if folder == catalog::Folder::Functions {
            self.message = Some(t!("msg.db_copy_ai_empty").to_string());
            return;
        }
        let Some(kind) = self.conn.as_ref().map(|c| c.kind) else {
            self.message = Some(t!("msg.db_not_connected").to_string());
            return;
        };
        let report = |b: &mut Browser, sql: &str| b.run_catalog(sql).ok();
        let detail = |d| catalog::detail_sql(kind, d, &schema, &table);
        let mut sections: Vec<Section> = vec![
            (
                t!("ui.db_copy_ai_columns").to_string(),
                report(self, &detail(catalog::Detail::Columns)),
            ),
            (
                t!("ui.db_copy_ai_primary_key").to_string(),
                report(self, &catalog::primary_key_sql(kind, &schema, &table)),
            ),
            (
                t!("ui.db_copy_ai_foreign_keys").to_string(),
                report(self, &detail(catalog::Detail::ForeignKeys)),
            ),
            (
                t!("ui.db_copy_ai_constraints").to_string(),
                report(self, &detail(catalog::Detail::Constraints)),
            ),
            (
                t!("ui.db_copy_ai_indexes").to_string(),
                report(self, &detail(catalog::Detail::Indexes)),
            ),
        ];
        let preview = report(self, &catalog::preview_sql(kind, &schema, &table));
        let (shown, total) = preview
            .as_ref()
            .map_or((0, 0), |(_, rows)| (rows.len().min(ROW_CAP), rows.len()));
        sections.push((
            t!("ui.db_copy_ai_rows", shown = shown, total = total).to_string(),
            preview,
        ));
        let title = if kind == connect::Kind::Sqlite {
            format!("`{table}` ({})", kind.label())
        } else {
            format!("`{schema}.{table}` ({})", kind.label())
        };
        let text = self::table(&title, &sections);
        self.yank_with(&text, t!("msg.db_copied_ai", rows = shown).to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(rows: &str) -> ResultLabels {
        ResultLabels {
            query: "Q".into(),
            columns: "C".into(),
            column: "col".into(),
            type_: "type".into(),
            rows: rows.into(),
        }
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn results_markdown_has_statement_types_and_rows() {
        let headers = s(&["id", "name"]);
        let rows = [s(&["1", "a|b"]), s(&["2", "c"])];
        let refs: Vec<&Vec<String>> = rows.iter().collect();
        let out = results(
            Some("SELECT * FROM t;"),
            &headers,
            &[Some("INTEGER".into()), None],
            &refs,
            &labels("R"),
        );
        assert_eq!(
            out,
            "### Q\n\n```sql\nSELECT * FROM t;\n```\n\n\
             ### C\n\n| col | type |\n| --- | --- |\n| id | INTEGER |\n| name |  |\n\n\
             ### R\n\n| id | name |\n| --- | --- |\n| 1 | a\\|b |\n| 2 | c |\n"
        );
    }

    #[test]
    fn results_without_types_or_statement_omit_those_sections() {
        let headers = s(&["n"]);
        let rows = [s(&["7"])];
        let refs: Vec<&Vec<String>> = rows.iter().collect();
        let out = results(None, &headers, &[None], &refs, &labels("R"));
        assert!(!out.contains("### Q"));
        assert!(!out.contains("### C"));
        assert!(out.starts_with("### R\n\n| n |"));
        // An empty result keeps its column names.
        let empty = results(
            Some("select 1 where 0"),
            &headers,
            &[None],
            &[],
            &labels("R"),
        );
        assert!(empty.contains("| n |\n| --- |\n"));
    }

    #[test]
    fn a_statement_with_backticks_gets_a_longer_fence() {
        assert_eq!(sql_block("select 1"), "```sql\nselect 1\n```\n");
        let block = sql_block("select `a```b` from t");
        assert!(block.starts_with("````sql\n"), "{block}");
        assert!(block.ends_with("\n````\n"));
    }

    #[test]
    fn table_markdown_lists_sections_and_caps_rows() {
        let many: Vec<Vec<String>> = (0..ROW_CAP + 10).map(|i| vec![i.to_string()]).collect();
        let out = table(
            "`users` (sqlite)",
            &[
                (
                    "Columns".into(),
                    Some((s(&["name", "type"]), vec![s(&["id", "INTEGER"])])),
                ),
                ("Indexes".into(), None),
                ("Keys".into(), Some((Vec::new(), Vec::new()))),
                ("Rows".into(), Some((s(&["id"]), many))),
            ],
        );
        assert!(out.starts_with("## `users` (sqlite)\n\n### Columns\n\n| name | type |"));
        assert!(out.contains("### Indexes\n\n—\n"));
        assert!(out.contains("### Keys\n\n—\n"));
        assert!(out.contains(&format!("| {} |", ROW_CAP - 1)));
        assert!(!out.contains(&format!("| {ROW_CAP} |")), "rows are capped");
    }
}
