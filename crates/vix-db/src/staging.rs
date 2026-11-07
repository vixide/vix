//! Staged changes on an editable table preview (T777): cell updates, new
//! rows and row deletions, and the SQL that commits them.
//!
//! Nothing here touches the database. [`Staged`] records what the user
//! changed, keyed by the grid's *underlying* row index (stable across
//! filtering and sorting) and column; a new row is appended to the grid's
//! rows as a blank row and marked inserted. [`statements`] turns the staged
//! set into one statement per row — `DELETE`s first, then `UPDATE`s, then
//! `INSERT`s, so a deleted row's key is free before a new row reuses it —
//! each paired with the optimistic conflict check an `UPDATE` needs. The
//! workbench runs them in one transaction and rolls the whole batch back
//! unless each one affects exactly one row.
//!
//! A typed value is a literal, except that one starting with `=` is an SQL
//! expression the server computes on commit (`=now()`, `=NULL`), and `\=`
//! escapes a literal that starts with `=` (see [`value_sql`]). Owning spec:
//! `crates/vix-db/spec/index.md`.

use std::collections::{BTreeSet, HashMap};

use super::catalog::{quote_ident, quote_value};
use super::columns::ColumnInfo;
use super::connect::Kind;

/// One staged edit of a row being updated: its column and `(original, new)`.
type RowEdit<'a> = (usize, &'a (String, String));

/// What a staged change does to its row (the grid colors rows by it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// Some cells of an existing row change (`UPDATE`).
    Modified,
    /// A new row (`INSERT`).
    Inserted,
    /// The row is removed (`DELETE`).
    Deleted,
}

/// Staged cell edits: `(underlying row, column)` → `(original value, new
/// value)`; for a new row the original is empty. The workbench keeps them in
/// `Browser::edits`, beside the row-level [`Staged`] changes.
pub type CellEdits = HashMap<(usize, usize), (String, String)>;

/// Stage `new` for a cell whose loaded value is `original`. Staging the
/// original again un-stages the cell, so a commit never issues a no-op.
///
/// # Examples
///
/// ```
/// use vix_db::staging::{set_cell, CellEdits};
/// let mut cells = CellEdits::new();
/// set_cell(&mut cells, 0, 1, "a", "b");
/// assert_eq!(cells.len(), 1);
/// set_cell(&mut cells, 0, 1, "a", "a");
/// assert!(cells.is_empty());
/// ```
pub fn set_cell(cells: &mut CellEdits, row: usize, col: usize, original: &str, new: &str) {
    if new == original {
        cells.remove(&(row, col));
    } else {
        cells.insert((row, col), (original.to_string(), new.to_string()));
    }
}

/// The row-level staged changes of one table preview: new rows and row
/// deletions. Cell edits live in a [`CellEdits`] map beside it, passed in
/// where both matter.
#[derive(Debug, Clone, Default)]
pub struct Staged {
    /// Rows added by the user (underlying indices into the grid's rows).
    inserted: BTreeSet<usize>,
    /// Existing rows staged for deletion.
    deleted: BTreeSet<usize>,
}

impl Staged {
    /// Whether nothing is staged, counting the cell edits too.
    #[must_use]
    pub fn is_empty(&self, cells: &CellEdits) -> bool {
        cells.is_empty() && self.inserted.is_empty() && self.deleted.is_empty()
    }

    /// The number of statements a commit would run (one per changed row).
    ///
    /// # Examples
    ///
    /// ```
    /// use vix_db::staging::{set_cell, CellEdits, Staged};
    /// let (mut s, mut cells) = (Staged::default(), CellEdits::new());
    /// set_cell(&mut cells, 0, 1, "a", "b");
    /// set_cell(&mut cells, 0, 2, "c", "d");
    /// s.insert_row(5);
    /// assert_eq!(s.len(&cells), 2);
    /// ```
    #[must_use]
    pub fn len(&self, cells: &CellEdits) -> usize {
        let (u, i, d) = self.counts(cells);
        u + i + d
    }

    /// `(updated, inserted, deleted)` row counts, as a commit reports them.
    #[must_use]
    pub fn counts(&self, cells: &CellEdits) -> (usize, usize, usize) {
        let updated: BTreeSet<usize> = cells
            .keys()
            .map(|&(r, _)| r)
            .filter(|r| !self.inserted.contains(r) && !self.deleted.contains(r))
            .collect();
        (updated.len(), self.inserted.len(), self.deleted.len())
    }

    /// Drop every staged new row and deletion (the caller clears the cells).
    pub fn clear(&mut self) {
        self.inserted.clear();
        self.deleted.clear();
    }

    /// Mark `row` (a blank row the caller appended to the grid) as new.
    pub fn insert_row(&mut self, row: usize) {
        self.inserted.insert(row);
    }

    /// Whether `row` is a staged new row.
    #[must_use]
    pub fn is_inserted(&self, row: usize) -> bool {
        self.inserted.contains(&row)
    }

    /// Stage an existing `row` for deletion, or restore it when it already
    /// is; returns whether it is now staged for deletion. Its cell edits are
    /// kept, so restoring the row brings them back.
    pub fn toggle_delete(&mut self, row: usize) -> bool {
        if self.deleted.remove(&row) {
            false
        } else {
            self.deleted.insert(row);
            true
        }
    }

    /// Discard a staged new row: remove it from `rows` and shift every staged
    /// index above it (in `cells` too) down by one, so the rest stay attached
    /// to their rows.
    pub fn remove_inserted(
        &mut self,
        cells: &mut CellEdits,
        row: usize,
        rows: &mut Vec<Vec<String>>,
    ) {
        if !self.inserted.remove(&row) || row >= rows.len() {
            return;
        }
        rows.remove(row);
        let shift = |r: usize| if r > row { r - 1 } else { r };
        *cells = std::mem::take(cells)
            .into_iter()
            .filter(|((r, _), _)| *r != row)
            .map(|((r, c), v)| ((shift(r), c), v))
            .collect();
        self.inserted = std::mem::take(&mut self.inserted)
            .into_iter()
            .map(shift)
            .collect();
        self.deleted = std::mem::take(&mut self.deleted)
            .into_iter()
            .map(shift)
            .collect();
    }

    /// Un-stage every change to an existing `row` (its cell edits and its
    /// deletion). A new row is discarded with [`Staged::remove_inserted`].
    pub fn discard_row(&mut self, cells: &mut CellEdits, row: usize) {
        cells.retain(|&(r, _), _| r != row);
        self.deleted.remove(&row);
    }

    /// What is staged for `row`, if anything; a deletion wins over edits.
    #[must_use]
    pub fn row_kind(&self, cells: &CellEdits, row: usize) -> Option<ChangeKind> {
        if self.deleted.contains(&row) {
            Some(ChangeKind::Deleted)
        } else if self.inserted.contains(&row) {
            Some(ChangeKind::Inserted)
        } else if cells.keys().any(|&(r, _)| r == row) {
            Some(ChangeKind::Modified)
        } else {
            None
        }
    }
}

/// The SQL for a typed cell value: an `=expr` is passed through for the
/// server to compute, `\=` escapes a literal starting with `=`, and anything
/// else is quoted with [`quote_value`]. A lone `=` is the literal text `=`.
///
/// # Examples
///
/// ```
/// use vix_db::{connect::Kind, staging::value_sql};
/// assert_eq!(value_sql(Kind::Postgres, "=now()"), "now()");
/// assert_eq!(value_sql(Kind::Postgres, r"\=1+1"), "'=1+1'");
/// assert_eq!(value_sql(Kind::Postgres, "O'Brien"), "'O''Brien'");
/// ```
#[must_use]
pub fn value_sql(kind: Kind, raw: &str) -> String {
    if let Some(literal) = raw.strip_prefix('\\').filter(|r| r.starts_with('=')) {
        return quote_value(kind, literal);
    }
    match raw.strip_prefix('=').map(str::trim) {
        Some(expr) if !expr.is_empty() => expr.to_string(),
        _ => quote_value(kind, raw),
    }
}

/// The table a commit writes to.
#[derive(Debug, Clone, Copy)]
pub struct Target<'a> {
    /// The engine, for quoting.
    pub kind: Kind,
    /// The quoted table name ([`crate::catalog::table_target`]).
    pub table: &'a str,
    /// The grid's column headers.
    pub headers: &'a [String],
    /// Indices into `headers` of the primary-key columns.
    pub pk: &'a [usize],
    /// The grid's rows as loaded (new rows are blank).
    pub rows: &'a [Vec<String>],
    /// Column facts; a column named here as generated is never written.
    pub columns: &'a [ColumnInfo],
}

/// A check run before the transaction: the row must still hold the loaded
/// values, or someone else changed it (optimistic concurrency).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictCheck {
    /// `SELECT` of the edited columns of the row, by primary key.
    pub sql: String,
    /// The edited columns, in `SELECT` order.
    pub columns: Vec<String>,
    /// The values loaded into the grid, in the same order.
    pub expected: Vec<String>,
}

/// One statement of a commit, which must affect exactly one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// What the statement does.
    pub kind: ChangeKind,
    /// The SQL.
    pub sql: String,
    /// The conflict check to run first (`UPDATE`s only).
    pub check: Option<ConflictCheck>,
}

impl Target<'_> {
    /// Whether column `col` is generated (never written).
    fn generated(&self, col: usize) -> bool {
        let Some(name) = self.headers.get(col) else {
            return true;
        };
        self.columns.iter().any(|c| &c.name == name && c.generated)
    }

    /// `pk1 = v1 AND pk2 = v2` for a loaded row, or `None` when a key cell is
    /// missing (a partial key could match more than one row).
    fn where_pk(&self, row: usize) -> Option<String> {
        let cells = self.rows.get(row)?;
        let parts: Option<Vec<String>> = self
            .pk
            .iter()
            .map(|&i| {
                Some(format!(
                    "{} = {}",
                    quote_ident(self.kind, self.headers.get(i)?),
                    quote_value(self.kind, cells.get(i)?)
                ))
            })
            .collect();
        parts.filter(|p| !p.is_empty()).map(|p| p.join(" AND "))
    }
}

/// The statements that commit `cells` and `staged` to `target`, in run order: `DELETE`s,
/// then `UPDATE`s (one per row, with its conflict check), then `INSERT`s. A
/// deleted row's cell edits are dropped; a row whose primary key cannot be
/// resolved is skipped rather than risk a broader `WHERE`.
///
/// # Examples
///
/// ```
/// use vix_db::{connect::Kind, staging::{set_cell, statements, CellEdits, Staged, Target}};
/// let headers = vec!["id".to_string(), "name".to_string()];
/// let rows = vec![vec!["1".to_string(), "ada".to_string()], vec![String::new(), String::new()]];
/// let (mut staged, mut cells) = (Staged::default(), CellEdits::new());
/// set_cell(&mut cells, 0, 1, "ada", "Ada");
/// staged.insert_row(1);
/// set_cell(&mut cells, 1, 1, "", "grace");
/// let target = Target { kind: Kind::Sqlite, table: "\"users\"", headers: &headers, pk: &[0], rows: &rows, columns: &[] };
/// let sql: Vec<String> = statements(&target, &cells, &staged).into_iter().map(|s| s.sql).collect();
/// assert_eq!(sql, [
///     "UPDATE \"users\" SET \"name\" = 'Ada' WHERE \"id\" = 1",
///     "INSERT INTO \"users\" (\"name\") VALUES ('grace')",
/// ]);
/// ```
#[must_use]
pub fn statements(target: &Target<'_>, cells: &CellEdits, staged: &Staged) -> Vec<Statement> {
    let kind = target.kind;
    let table = target.table;
    let mut out = Vec::new();
    for &row in &staged.deleted {
        if staged.inserted.contains(&row) {
            continue;
        }
        if let Some(where_sql) = target.where_pk(row) {
            out.push(Statement {
                kind: ChangeKind::Deleted,
                sql: format!("DELETE FROM {table} WHERE {where_sql}"),
                check: None,
            });
        }
    }
    let mut by_row: Vec<(usize, Vec<RowEdit<'_>>)> = Vec::new();
    let mut keys: Vec<&(usize, usize)> = cells.keys().collect();
    keys.sort();
    for &(row, col) in keys {
        let Some(edit) = cells.get(&(row, col)) else {
            continue;
        };
        if target.generated(col) {
            continue;
        }
        match by_row.last_mut() {
            Some((r, list)) if *r == row => list.push((col, edit)),
            _ => by_row.push((row, vec![(col, edit)])),
        }
    }
    let mut inserts = Vec::new();
    for (row, edits) in by_row {
        if staged.deleted.contains(&row) {
            continue;
        }
        if staged.inserted.contains(&row) {
            continue; // written by the INSERT below
        }
        let Some(where_sql) = target.where_pk(row) else {
            continue;
        };
        let names: Vec<String> = edits
            .iter()
            .map(|(c, _)| target.headers[*c].clone())
            .collect();
        let set: Vec<String> = edits
            .iter()
            .map(|(c, (_, new))| {
                format!(
                    "{} = {}",
                    quote_ident(kind, &target.headers[*c]),
                    value_sql(kind, new)
                )
            })
            .collect();
        let select: Vec<String> = names.iter().map(|n| quote_ident(kind, n)).collect();
        out.push(Statement {
            kind: ChangeKind::Modified,
            sql: format!("UPDATE {table} SET {} WHERE {where_sql}", set.join(", ")),
            check: Some(ConflictCheck {
                sql: format!(
                    "SELECT {} FROM {table} WHERE {where_sql}",
                    select.join(", ")
                ),
                columns: names,
                expected: edits.iter().map(|(_, (orig, _))| orig.clone()).collect(),
            }),
        });
    }
    for &row in &staged.inserted {
        inserts.push(Statement {
            kind: ChangeKind::Inserted,
            sql: insert_sql(target, cells, row),
            check: None,
        });
    }
    out.extend(inserts);
    out
}

/// The `INSERT` for staged new `row`: only its non-generated cells that have
/// a staged value, so blank cells take the column default; a row with no
/// values is `DEFAULT VALUES` (`() VALUES ()` on `MySQL`).
fn insert_sql(target: &Target<'_>, cells: &CellEdits, row: usize) -> String {
    let (kind, table) = (target.kind, target.table);
    let cols: Vec<usize> = (0..target.headers.len())
        .filter(|&c| !target.generated(c) && cells.contains_key(&(row, c)))
        .collect();
    if cols.is_empty() {
        return match kind {
            Kind::Mysql => format!("INSERT INTO {table} () VALUES ()"),
            Kind::Postgres | Kind::Sqlite => format!("INSERT INTO {table} DEFAULT VALUES"),
        };
    }
    let names: Vec<String> = cols
        .iter()
        .map(|&c| quote_ident(kind, &target.headers[c]))
        .collect();
    let values: Vec<String> = cols
        .iter()
        .filter_map(|&c| cells.get(&(row, c)).map(|(_, new)| new.as_str()))
        .map(|v| value_sql(kind, v))
        .collect();
    format!(
        "INSERT INTO {table} ({}) VALUES ({})",
        names.join(", "),
        values.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    fn sql(target: &Target<'_>, cells: &CellEdits, staged: &Staged) -> Vec<String> {
        statements(target, cells, staged)
            .into_iter()
            .map(|s| s.sql)
            .collect()
    }

    #[test]
    fn values_are_literals_expressions_or_escaped() {
        assert_eq!(value_sql(Kind::Sqlite, "42"), "42");
        assert_eq!(value_sql(Kind::Mysql, "42"), "'42'");
        assert_eq!(value_sql(Kind::Mysql, "=NOW()"), "NOW()");
        assert_eq!(value_sql(Kind::Sqlite, "= upper('x') "), "upper('x')");
        assert_eq!(value_sql(Kind::Sqlite, r"\=x"), "'=x'");
        assert_eq!(value_sql(Kind::Sqlite, r"\x"), r"'\x'");
        assert_eq!(value_sql(Kind::Mysql, r"a\b"), r"'a\\b'");
        assert_eq!(value_sql(Kind::Postgres, "="), "'='");
        assert_eq!(value_sql(Kind::Postgres, "=NULL"), "NULL");
        // A quote in a literal never closes the string early.
        assert_eq!(
            value_sql(Kind::Postgres, "x'); DROP TABLE t; --"),
            "'x''); DROP TABLE t; --'"
        );
    }

    #[test]
    fn statements_run_deletes_then_updates_then_inserts_per_engine() {
        let headers = strings(&["id", "name", "total"]);
        let rows = vec![
            strings(&["1", "ada", "3"]),
            strings(&["2", "grace", "4"]),
            strings(&["", "", ""]),
        ];
        let columns = vec![ColumnInfo {
            name: "total".into(),
            generated: true,
            ..ColumnInfo::default()
        }];
        let (mut staged, mut cells) = (Staged::default(), CellEdits::new());
        set_cell(&mut cells, 0, 1, "ada", "=upper('ada')");
        set_cell(&mut cells, 0, 2, "3", "9"); // generated: never written
        staged.toggle_delete(1);
        set_cell(&mut cells, 1, 1, "grace", "ignored"); // the row is deleted
        staged.insert_row(2);
        set_cell(&mut cells, 2, 0, "", "7");
        set_cell(&mut cells, 2, 1, "", r"\=eq");
        set_cell(&mut cells, 2, 2, "", "1");

        let pg = Target {
            kind: Kind::Postgres,
            table: "\"public\".\"users\"",
            headers: &headers,
            pk: &[0],
            rows: &rows,
            columns: &columns,
        };
        assert_eq!(
            sql(&pg, &cells, &staged),
            [
                "DELETE FROM \"public\".\"users\" WHERE \"id\" = '2'",
                "UPDATE \"public\".\"users\" SET \"name\" = upper('ada') WHERE \"id\" = '1'",
                "INSERT INTO \"public\".\"users\" (\"id\", \"name\") VALUES ('7', '=eq')",
            ]
        );
        let check = statements(&pg, &cells, &staged)[1].check.clone().unwrap();
        assert_eq!(
            check.sql,
            "SELECT \"name\" FROM \"public\".\"users\" WHERE \"id\" = '1'"
        );
        assert_eq!(check.expected, ["ada"]);

        let my = Target {
            kind: Kind::Mysql,
            table: "`app`.`users`",
            ..pg
        };
        assert_eq!(
            sql(&my, &cells, &staged)[0],
            "DELETE FROM `app`.`users` WHERE `id` = '2'"
        );
        let lite = Target {
            kind: Kind::Sqlite,
            table: "\"users\"",
            ..pg
        };
        assert_eq!(
            sql(&lite, &cells, &staged)[0],
            "DELETE FROM \"users\" WHERE \"id\" = 2"
        );
        assert_eq!(staged.counts(&cells), (1, 1, 1));
    }

    #[test]
    fn composite_keys_and_multi_cell_updates() {
        let headers = strings(&["a", "b", "x", "y"]);
        let rows = vec![strings(&["1", "k'1", "p", "q"])];
        let (staged, mut cells) = (Staged::default(), CellEdits::new());
        set_cell(&mut cells, 0, 3, "q", "Q");
        set_cell(&mut cells, 0, 2, "p", "P");
        let t = Target {
            kind: Kind::Sqlite,
            table: "\"t\"",
            headers: &headers,
            pk: &[0, 1],
            rows: &rows,
            columns: &[],
        };
        let stmts = statements(&t, &cells, &staged);
        assert_eq!(
            stmts[0].sql,
            "UPDATE \"t\" SET \"x\" = 'P', \"y\" = 'Q' WHERE \"a\" = 1 AND \"b\" = 'k''1'"
        );
        assert_eq!(stmts[0].check.as_ref().unwrap().expected, ["p", "q"]);
    }

    #[test]
    fn empty_new_rows_insert_defaults() {
        let headers = strings(&["id"]);
        let rows = vec![strings(&[""])];
        let (mut staged, cells) = (Staged::default(), CellEdits::new());
        staged.insert_row(0);
        let t = Target {
            kind: Kind::Postgres,
            table: "\"t\"",
            headers: &headers,
            pk: &[0],
            rows: &rows,
            columns: &[],
        };
        assert_eq!(
            sql(&t, &cells, &staged),
            ["INSERT INTO \"t\" DEFAULT VALUES"]
        );
        let my = Target {
            kind: Kind::Mysql,
            table: "`t`",
            ..t
        };
        assert_eq!(sql(&my, &cells, &staged), ["INSERT INTO `t` () VALUES ()"]);
    }

    #[test]
    fn a_missing_key_cell_skips_the_row() {
        let headers = strings(&["id", "n"]);
        let rows = vec![strings(&["1"])]; // the key column (n) is missing
        let (mut staged, cells) = (Staged::default(), CellEdits::new());
        staged.toggle_delete(0);
        let t = Target {
            kind: Kind::Sqlite,
            table: "\"t\"",
            headers: &headers,
            pk: &[1],
            rows: &rows,
            columns: &[],
        };
        assert_eq!(statements(&t, &cells, &staged), [] as [Statement; 0]);
    }

    #[test]
    fn staging_bookkeeping() {
        let mut rows = vec![strings(&["1"]), strings(&[""]), strings(&[""])];
        let (mut s, mut cells) = (Staged::default(), CellEdits::new());
        s.insert_row(1);
        s.insert_row(2);
        set_cell(&mut cells, 2, 0, "", "x");
        set_cell(&mut cells, 0, 0, "1", "2");
        assert_eq!(s.row_kind(&cells, 0), Some(ChangeKind::Modified));
        assert_eq!(s.row_kind(&cells, 2), Some(ChangeKind::Inserted));
        s.remove_inserted(&mut cells, 1, &mut rows);
        assert_eq!(rows.len(), 2);
        assert!(s.is_inserted(1), "the later new row shifted down");
        assert_eq!(cells.get(&(1, 0)).map(|(_, n)| n.as_str()), Some("x"));
        assert!(s.toggle_delete(0));
        assert_eq!(s.row_kind(&cells, 0), Some(ChangeKind::Deleted));
        assert!(!s.toggle_delete(0));
        assert_eq!(
            s.row_kind(&cells, 0),
            Some(ChangeKind::Modified),
            "edits survive"
        );
        s.discard_row(&mut cells, 0);
        assert_eq!(s.row_kind(&cells, 0), None);
        set_cell(&mut cells, 1, 0, "", "");
        assert_eq!(cells.get(&(1, 0)), None, "staging the original un-stages");
        assert_eq!(s.len(&cells), 1);
        s.clear();
        assert!(s.is_empty(&cells));
    }
}
