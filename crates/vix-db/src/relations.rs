//! The relations view of one table (T779): its outgoing foreign keys, the
//! incoming ones (other tables that reference it) and the views that depend
//! on it.
//!
//! Like [`crate::catalog`], this is pure: [`relations_sql`] builds the catalog
//! queries per engine, and [`fk_relations`] / [`view_relations`] turn their
//! rows into [`Relation`]s the workbench shows in the results grid. The
//! queries' result *shapes* are the same on every engine, so callers never
//! branch on it:
//!
//! - foreign-key queries return `(key, constraint, from_schema, from_table,
//!   from_column, to_schema, to_table, to_column)`, one row per column pair,
//!   ordered so a constraint's columns are adjacent and in key order. `key`
//!   groups those rows (`SQLite`'s constraints have no names, so there it is
//!   the constraint's id and `constraint` is empty).
//! - view queries return `(schema, view, definition)`.
//!
//! Per engine:
//!
//! - **`PostgreSQL`**: foreign keys from `pg_constraint`, pairing `conkey` with
//!   `confkey` through `unnest … WITH ORDINALITY` (which `information_schema`
//!   cannot do for multi-column keys); dependent views and materialized views
//!   from `pg_depend` through each view's `pg_rewrite` rule.
//! - **`MySQL`**: `information_schema.KEY_COLUMN_USAGE` rows whose
//!   `REFERENCED_TABLE_NAME` is set; dependent views from
//!   `information_schema.VIEW_TABLE_USAGE` (8.0.13+). Older servers lack that
//!   view, so [`RelationsSql::views_fallback`] searches each view's
//!   `VIEW_DEFINITION` for the backtick-quoted table name instead.
//! - **`SQLite`**: `pragma_foreign_key_list` on the table for outgoing keys and
//!   over every table for incoming ones (a key written `REFERENCES t` without
//!   columns resolves to `t`'s primary key). `SQLite` records no view
//!   dependencies, so every view's SQL is read and [`mentions_table`] keeps the
//!   ones naming the table: **approximate**, since a column or alias with the
//!   table's name also counts.
//!
//! Owning spec: `crates/vix-db/spec/index.md` ("Database Browser").

use super::connect::Kind;

/// Which way a [`Relation`] points, seen from the selected table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    /// A foreign key on this table, referencing another.
    Outgoing,
    /// A foreign key on another table, referencing this one.
    Incoming,
    /// A view that reads from this table.
    View,
}

impl RelationKind {
    /// The i18n key of this kind's label in the relations grid.
    #[must_use]
    pub fn label_key(self) -> &'static str {
        match self {
            RelationKind::Outgoing => "ui.db_rel_outgoing",
            RelationKind::Incoming => "ui.db_rel_incoming",
            RelationKind::View => "ui.db_rel_view",
        }
    }
}

/// The i18n keys of the relations grid's headers, in column order:
/// relation, constraint, from, to.
pub const HEADER_KEYS: [&str; 4] = [
    "ui.db_rel_relation",
    "ui.db_rel_constraint",
    "ui.db_rel_from",
    "ui.db_rel_to",
];

/// One row of the relations view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    /// Which way it points.
    pub kind: RelationKind,
    /// The constraint's name (empty for views and `SQLite` keys).
    pub constraint: String,
    /// The referencing side: `table (col, …)`, or the view's name.
    pub from: String,
    /// The referenced side: `table (col, …)`, or the selected table for a view.
    pub to: String,
}

/// The catalog queries behind one table's relations view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationsSql {
    /// Foreign keys on the table.
    pub outgoing: String,
    /// Foreign keys on other tables referencing it.
    pub incoming: String,
    /// Views depending on it.
    pub views: String,
    /// What to run when [`RelationsSql::views`] fails (`MySQL` before 8.0.13,
    /// which has no `VIEW_TABLE_USAGE`).
    pub views_fallback: Option<String>,
    /// Whether the view rows still need [`mentions_table`] on their
    /// `definition` (`SQLite`, which records no dependencies).
    pub views_scan: bool,
}

/// Escape a name for a single-quoted SQL literal (doubles each `'`).
fn lit(name: &str) -> String {
    name.replace('\'', "''")
}

/// The relations queries for `schema`.`table` on `kind`. Names are compared
/// as single-quoted literals, never spliced in as identifiers.
///
/// # Examples
///
/// ```
/// use vix_db::{connect::Kind, relations::relations_sql};
/// let q = relations_sql(Kind::Sqlite, "main", "orders");
/// assert!(q.outgoing.contains("pragma_foreign_key_list('orders')"));
/// assert!(q.views_scan);
/// ```
#[must_use]
pub fn relations_sql(kind: Kind, schema: &str, table: &str) -> RelationsSql {
    let (s, t) = (lit(schema), lit(table));
    match kind {
        Kind::Sqlite => sqlite_sql(&t),
        Kind::Postgres => postgres_sql(&s, &t),
        Kind::Mysql => mysql_sql(table),
    }
}

/// `SQLite`: `pragma_foreign_key_list` both ways and every view's SQL.
fn sqlite_sql(t: &str) -> RelationsSql {
    // A key written `REFERENCES p` (no columns) has a NULL `to`: it points at
    // `p`'s primary key, whose `seq`-th column is the `pk = seq + 1` one.
    let to = "coalesce(f.\"to\", (SELECT p.name FROM pragma_table_info(f.\"table\") p \
              WHERE p.pk = f.seq + 1))";
    RelationsSql {
        outgoing: format!(
            "SELECT f.id, '', 'main', '{t}', f.\"from\", 'main', f.\"table\", {to} \
             FROM pragma_foreign_key_list('{t}') f ORDER BY f.id, f.seq;"
        ),
        incoming: format!(
            "SELECT f.id, '', 'main', m.name, f.\"from\", 'main', f.\"table\", {to} \
             FROM sqlite_master m JOIN pragma_foreign_key_list(m.name) f \
             WHERE m.type = 'table' AND f.\"table\" = '{t}' COLLATE NOCASE \
             ORDER BY m.name, f.id, f.seq;"
        ),
        views: "SELECT 'main', name, sql FROM sqlite_master WHERE type = 'view' ORDER BY name;"
            .to_string(),
        views_fallback: None,
        views_scan: true,
    }
}

/// `PostgreSQL`: the foreign-key query over `pg_constraint`, filtered by
/// `side` (`c.conrelid` for outgoing, `c.confrelid` for incoming).
fn postgres_fk_sql(side: &str, s: &str, t: &str) -> String {
    format!(
        "SELECT c.oid::text, c.conname, sn.nspname, sc.relname, sa.attname, \
         tn.nspname, tc.relname, ta.attname \
         FROM pg_constraint c \
         JOIN pg_class sc ON sc.oid = c.conrelid \
         JOIN pg_namespace sn ON sn.oid = sc.relnamespace \
         JOIN pg_class tc ON tc.oid = c.confrelid \
         JOIN pg_namespace tn ON tn.oid = tc.relnamespace \
         CROSS JOIN LATERAL unnest(c.conkey, c.confkey) WITH ORDINALITY AS k(src, dst, n) \
         JOIN pg_attribute sa ON sa.attrelid = c.conrelid AND sa.attnum = k.src \
         JOIN pg_attribute ta ON ta.attrelid = c.confrelid AND ta.attnum = k.dst \
         WHERE c.contype = 'f' AND {side} = (SELECT r.oid FROM pg_class r \
         JOIN pg_namespace rn ON rn.oid = r.relnamespace \
         WHERE rn.nspname = '{s}' AND r.relname = '{t}') \
         ORDER BY sn.nspname, sc.relname, c.conname, k.n;"
    )
}

/// `PostgreSQL`: `pg_constraint` both ways, `pg_depend` for views.
fn postgres_sql(s: &str, t: &str) -> RelationsSql {
    RelationsSql {
        outgoing: postgres_fk_sql("c.conrelid", s, t),
        incoming: postgres_fk_sql("c.confrelid", s, t),
        // A view's dependencies hang off its `_RETURN` rewrite rule; DISTINCT
        // because a view using several columns depends once per column.
        views: format!(
            "SELECT DISTINCT vn.nspname, v.relname, '' \
             FROM pg_depend d \
             JOIN pg_rewrite rw ON rw.oid = d.objid \
             JOIN pg_class v ON v.oid = rw.ev_class \
             JOIN pg_namespace vn ON vn.oid = v.relnamespace \
             JOIN pg_class r ON r.oid = d.refobjid \
             JOIN pg_namespace rn ON rn.oid = r.relnamespace \
             WHERE d.classid = 'pg_rewrite'::regclass \
             AND d.refclassid = 'pg_class'::regclass \
             AND v.relkind IN ('v', 'm') AND v.oid <> r.oid \
             AND rn.nspname = '{s}' AND r.relname = '{t}' \
             ORDER BY 1, 2;"
        ),
        views_fallback: None,
        views_scan: false,
    }
}

/// `MySQL`: `KEY_COLUMN_USAGE` both ways, `VIEW_TABLE_USAGE` (or a search of
/// `VIEWS.VIEW_DEFINITION`) for views.
fn mysql_sql(table: &str) -> RelationsSql {
    let t = lit(table);
    let cols = "constraint_name, constraint_name, table_schema, table_name, column_name, \
                referenced_table_schema, referenced_table_name, referenced_column_name";
    // MySQL stores a view's definition with backtick-quoted names, so the
    // fallback searches for exactly that spelling (LOCATE: no LIKE wildcards).
    let quoted = lit(&format!("`{}`", table.replace('`', "``")));
    RelationsSql {
        outgoing: format!(
            "SELECT {cols} FROM information_schema.key_column_usage \
             WHERE table_schema = DATABASE() AND table_name = '{t}' \
             AND referenced_table_name IS NOT NULL \
             ORDER BY table_name, constraint_name, ordinal_position;"
        ),
        incoming: format!(
            "SELECT {cols} FROM information_schema.key_column_usage \
             WHERE referenced_table_schema = DATABASE() AND referenced_table_name = '{t}' \
             ORDER BY table_schema, table_name, constraint_name, ordinal_position;"
        ),
        views: format!(
            "SELECT view_schema, view_name, '' FROM information_schema.view_table_usage \
             WHERE table_schema = DATABASE() AND table_name = '{t}' ORDER BY view_name;"
        ),
        views_fallback: Some(format!(
            "SELECT table_schema, table_name, '' FROM information_schema.views \
             WHERE table_schema = DATABASE() AND LOCATE('{quoted}', view_definition) > 0 \
             ORDER BY table_name;"
        )),
        views_scan: false,
    }
}

/// `table`, qualified with `schema` when that differs from `home` (the
/// selected table's schema).
fn qualified(schema: &str, table: &str, home: &str) -> String {
    if schema.is_empty() || schema == home {
        table.to_string()
    } else {
        format!("{schema}.{table}")
    }
}

/// Group foreign-key query rows (the shape in the module docs) into one
/// [`Relation`] per constraint, its columns listed in key order:
/// `orders (customer_id)` → `customers (id)`. `home` is the selected table's
/// schema; other schemas are spelled out. Short rows are skipped.
///
/// # Examples
///
/// ```
/// use vix_db::relations::{RelationKind, fk_relations};
/// let row = |c: &str, p: &str| -> Vec<String> {
///     ["1", "fk", "main", "items", c, "main", "orders", p].map(String::from).to_vec()
/// };
/// let r = fk_relations(RelationKind::Incoming, &[row("a", "x"), row("b", "y")], "main");
/// assert_eq!(r[0].from, "items (a, b)");
/// assert_eq!(r[0].to, "orders (x, y)");
/// ```
#[must_use]
pub fn fk_relations(kind: RelationKind, rows: &[Vec<String>], home: &str) -> Vec<Relation> {
    let mut out: Vec<Relation> = Vec::new();
    // Columns of the constraint being built; flushed when the group changes.
    let mut group: Option<(Vec<&str>, Vec<&str>, Vec<&str>)> = None;
    let flush = |g: &mut Option<(Vec<&str>, Vec<&str>, Vec<&str>)>, out: &mut Vec<Relation>| {
        if let Some((head, from_cols, to_cols)) = g.take() {
            out.push(Relation {
                kind,
                constraint: head[1].to_string(),
                from: format!(
                    "{} ({})",
                    qualified(head[2], head[3], home),
                    from_cols.join(", ")
                ),
                to: format!(
                    "{} ({})",
                    qualified(head[5], head[6], home),
                    to_cols.join(", ")
                ),
            });
        }
    };
    for row in rows.iter().filter(|r| r.len() >= 8) {
        let head: Vec<&str> = row.iter().map(String::as_str).collect();
        // A constraint is identified by its key and both tables.
        let same = group.as_ref().is_some_and(|(h, _, _)| {
            h[0] == head[0] && h[2..4] == head[2..4] && h[5..7] == head[5..7]
        });
        if !same {
            flush(&mut group, &mut out);
        }
        let g = group.get_or_insert_with(|| (head.clone(), Vec::new(), Vec::new()));
        g.1.push(row[4].as_str());
        g.2.push(row[7].as_str());
    }
    flush(&mut group, &mut out);
    out
}

/// The view rows (`(schema, view, definition)`) as [`Relation`]s pointing at
/// `table`. With `scan` (`SQLite`), a view is kept only when
/// [`mentions_table`] finds `table` in its definition.
#[must_use]
pub fn view_relations(rows: &[Vec<String>], home: &str, table: &str, scan: bool) -> Vec<Relation> {
    rows.iter()
        .filter(|r| r.len() >= 2)
        .filter(|r| !r[1].eq_ignore_ascii_case(table))
        .filter(|r| !scan || r.get(2).is_some_and(|sql| mentions_table(sql, table)))
        .map(|r| Relation {
            kind: RelationKind::View,
            constraint: String::new(),
            from: qualified(&r[0], &r[1], home),
            to: table.to_string(),
        })
        .collect()
}

/// Whether `sql` names `table` as an identifier, bare or quoted, outside
/// strings and comments (ASCII case-insensitive, as `SQLite` compares
/// names). Approximate: a column or alias spelled like the table counts too.
///
/// # Examples
///
/// ```
/// use vix_db::relations::mentions_table;
/// assert!(mentions_table("CREATE VIEW v AS SELECT * FROM \"Orders\" o", "orders"));
/// assert!(!mentions_table("CREATE VIEW v AS SELECT 'orders' FROM orders_old", "orders"));
/// ```
#[must_use]
pub fn mentions_table(sql: &str, table: &str) -> bool {
    crate::alias::identifiers(sql)
        .iter()
        .any(|w| w.eq_ignore_ascii_case(table))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(cells: &[&str]) -> Vec<String> {
        cells.iter().map(|c| (*c).to_string()).collect()
    }

    #[test]
    fn every_engine_builds_three_queries_with_quoted_names() {
        for kind in [Kind::Sqlite, Kind::Postgres, Kind::Mysql] {
            let q = relations_sql(kind, "pub'lic", "o'rders");
            for sql in [&q.outgoing, &q.incoming, &q.views] {
                assert!(sql.trim_end().ends_with(';'), "{kind:?}: {sql}");
                assert!(!sql.contains("o'rders"), "{kind:?} unescaped: {sql}");
            }
            assert!(q.outgoing.contains("o''rders") && q.incoming.contains("o''rders"));
        }
    }

    #[test]
    fn sqlite_shape() {
        let q = relations_sql(Kind::Sqlite, "main", "orders");
        assert!(q.outgoing.contains("pragma_foreign_key_list('orders')"));
        assert!(q.incoming.contains("JOIN pragma_foreign_key_list(m.name)"));
        assert!(q.incoming.contains("f.\"table\" = 'orders' COLLATE NOCASE"));
        assert!(q.views.contains("type = 'view'"));
        assert!(q.views_scan && q.views_fallback.is_none());
    }

    #[test]
    fn postgres_shape() {
        let q = relations_sql(Kind::Postgres, "shop", "orders");
        assert!(q.outgoing.contains("FROM pg_constraint c"));
        assert!(q.outgoing.contains("c.conrelid = (SELECT r.oid"));
        assert!(q.incoming.contains("c.confrelid = (SELECT r.oid"));
        assert!(
            q.outgoing
                .contains("unnest(c.conkey, c.confkey) WITH ORDINALITY")
        );
        assert!(
            q.outgoing
                .contains("rn.nspname = 'shop' AND r.relname = 'orders'")
        );
        assert!(q.views.contains("pg_depend") && q.views.contains("pg_rewrite"));
        assert!(q.views.contains("relkind IN ('v', 'm')"));
        assert!(!q.views_scan && q.views_fallback.is_none());
    }

    #[test]
    fn mysql_shape_and_fallback() {
        let q = relations_sql(Kind::Mysql, "ignored", "orders");
        assert!(q.outgoing.contains("key_column_usage"));
        assert!(q.outgoing.contains("table_name = 'orders'"));
        assert!(q.outgoing.contains("referenced_table_name IS NOT NULL"));
        assert!(q.incoming.contains("referenced_table_name = 'orders'"));
        assert!(q.views.contains("view_table_usage"));
        let fallback = q.views_fallback.expect("pre-8.0.13 fallback");
        assert!(fallback.contains("LOCATE('`orders`', view_definition)"));
        let odd = relations_sql(Kind::Mysql, "", "a'b`c")
            .views_fallback
            .unwrap();
        assert!(odd.contains("LOCATE('`a''b``c`'"), "{odd}");
    }

    #[test]
    fn fk_rows_group_per_constraint() {
        let rows = vec![
            row(&["0", "", "main", "items", "order_id", "main", "orders", "id"]),
            row(&["1", "", "main", "items", "a", "main", "pairs", "x"]),
            row(&["1", "", "main", "items", "b", "main", "pairs", "y"]),
            row(&["0", "", "main", "notes", "order_id", "main", "orders", "id"]),
            row(&["short"]),
        ];
        let r = fk_relations(RelationKind::Outgoing, &rows, "main");
        let flat: Vec<(&str, &str)> = r.iter().map(|x| (x.from.as_str(), x.to.as_str())).collect();
        assert_eq!(
            flat,
            [
                ("items (order_id)", "orders (id)"),
                ("items (a, b)", "pairs (x, y)"),
                ("notes (order_id)", "orders (id)"),
            ]
        );
        assert!(r.iter().all(|x| x.kind == RelationKind::Outgoing));
    }

    #[test]
    fn other_schemas_are_spelled_out() {
        let rows = vec![row(&[
            "9",
            "fk_c",
            "sales",
            "orders",
            "cid",
            "crm",
            "customers",
            "id",
        ])];
        let r = fk_relations(RelationKind::Outgoing, &rows, "sales");
        assert_eq!(r[0].constraint, "fk_c");
        assert_eq!(r[0].from, "orders (cid)");
        assert_eq!(r[0].to, "crm.customers (id)");
    }

    #[test]
    fn views_are_scanned_only_when_asked() {
        let rows = vec![
            row(&["main", "v_hit", "CREATE VIEW v_hit AS SELECT * FROM Orders"]),
            row(&[
                "main",
                "v_miss",
                "CREATE VIEW v_miss AS SELECT 'orders' FROM orders_old",
            ]),
            row(&["main", "orders", "itself"]),
        ];
        let scanned = view_relations(&rows, "main", "orders", true);
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].from, "v_hit");
        assert_eq!(scanned[0].to, "orders");
        let server = view_relations(&rows[..2], "main", "orders", false);
        assert_eq!(server.len(), 2, "the server already filtered");
        let other = view_relations(&[row(&["rep", "v", ""])], "main", "orders", false);
        assert_eq!(other[0].from, "rep.v");
    }

    #[test]
    fn mentions_table_ignores_strings_and_comments() {
        assert!(mentions_table("SELECT * FROM main.orders", "orders"));
        assert!(mentions_table("SELECT * FROM `orders`", "ORDERS"));
        assert!(!mentions_table(
            "SELECT 1 -- orders\n/* orders */",
            "orders"
        ));
        assert!(!mentions_table("SELECT * FROM my_orders", "orders"));
    }
}
