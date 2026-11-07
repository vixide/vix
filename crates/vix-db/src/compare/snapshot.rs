//! The pure data model of one schema, as read from one connection (T785).
//!
//! A [`Snapshot`] is plain data: no session, no driver types. The readers in
//! [`super::read`] fill it from catalog queries, the diff in [`super::diff`]
//! compares two of them, and tests build them by hand. Every list is sorted by
//! name so two snapshots of the same schema compare equal whatever order the
//! catalog returned rows in.
//!
//! The structs are `#[non_exhaustive]` so a later engine can add a field
//! without a semver break; build one from [`Default`] and set its fields.

use crate::connect::Kind;

/// One schema of one database: everything the schema compare looks at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Snapshot {
    /// The engine the snapshot was read from; it decides the SQL dialect of
    /// the migration.
    pub kind: Kind,
    /// The schema name (`public`, or `main` on `SQLite`).
    pub schema: String,
    /// `true` when the schema does not exist on this connection (only the
    /// `PostgreSQL` reader can say so); the migration then creates it.
    pub missing: bool,
    /// Tables, sorted by name.
    pub tables: Vec<Table>,
    /// Views and materialized views, sorted by name.
    pub views: Vec<View>,
    /// Functions and procedures, sorted by name and arguments.
    pub routines: Vec<Routine>,
    /// Triggers, sorted by table then name.
    pub triggers: Vec<Trigger>,
    /// Enum types, sorted by name.
    pub enums: Vec<EnumType>,
    /// Sequences not owned by an identity column, sorted by name.
    pub sequences: Vec<Sequence>,
}

/// A table: its columns, keys, constraints and indexes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Table {
    /// Table name.
    pub name: String,
    /// Columns in declaration order.
    pub columns: Vec<Column>,
    /// The primary key, if any.
    pub primary_key: Option<Constraint>,
    /// Unique (and, on `PostgreSQL`, exclusion) constraints, by name.
    pub uniques: Vec<Constraint>,
    /// Check constraints, by name.
    pub checks: Vec<Constraint>,
    /// Foreign keys, by name.
    pub foreign_keys: Vec<Constraint>,
    /// Indexes that do not back a constraint, by name.
    pub indexes: Vec<Index>,
    /// The table comment (empty for none).
    pub comment: String,
    /// The engine's own `CREATE TABLE` text where the catalog keeps one
    /// (`SQLite`); empty on `PostgreSQL`, whose statement is built from the
    /// parts.
    pub create_sql: String,
}

/// One column of a [`Table`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Column {
    /// Column name.
    pub name: String,
    /// The type as the engine prints it (`character varying(40)`).
    pub data_type: String,
    /// Whether `NULL` is allowed.
    pub nullable: bool,
    /// The default expression, or for a generated column its generation
    /// expression (empty for none).
    pub default: String,
    /// Whether the column is generated (`GENERATED ALWAYS AS (…) STORED`).
    pub generated: bool,
    /// `always` or `default` for an identity column, empty otherwise.
    pub identity: String,
    /// The column comment (empty for none).
    pub comment: String,
}

/// A named table constraint: primary key, unique, check or foreign key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Constraint {
    /// Constraint name.
    pub name: String,
    /// Its definition as the engine prints it (`UNIQUE (email)`,
    /// `FOREIGN KEY (user_id) REFERENCES users(id)`).
    pub definition: String,
    /// For a foreign key, the referenced table (empty otherwise).
    pub references: String,
}

/// An index that is not the implementation of a constraint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Index {
    /// Index name.
    pub name: String,
    /// The full `CREATE INDEX` statement.
    pub definition: String,
    /// Whether it is a unique index.
    pub unique: bool,
}

/// A view or materialized view.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct View {
    /// View name.
    pub name: String,
    /// The `SELECT` it runs (`PostgreSQL`), or the whole `CREATE VIEW`
    /// statement (`SQLite`).
    pub definition: String,
    /// Whether it is a materialized view.
    pub materialized: bool,
    /// What it reads: `(relation, column)` pairs in this schema, the column
    /// empty when the whole relation is meant. Used to drop and recreate the
    /// view around a change to something it uses.
    pub depends_on: Vec<(String, String)>,
    /// The view comment (empty for none).
    pub comment: String,
}

/// A function or procedure.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Routine {
    /// Routine name.
    pub name: String,
    /// Its identity arguments (`a integer, b text`), which with the name
    /// identify it.
    pub arguments: String,
    /// Whether it is a procedure rather than a function.
    pub procedure: bool,
    /// The full `CREATE OR REPLACE …` statement.
    pub definition: String,
}

impl Routine {
    /// The routine's identity, `name(arguments)`.
    #[must_use]
    pub fn signature(&self) -> String {
        format!("{}({})", self.name, self.arguments)
    }
}

/// A trigger on a table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Trigger {
    /// The table it fires on.
    pub table: String,
    /// Trigger name.
    pub name: String,
    /// The full `CREATE TRIGGER` statement.
    pub definition: String,
}

/// An enum type and its values in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EnumType {
    /// Type name.
    pub name: String,
    /// Its labels, in sort order.
    pub values: Vec<String>,
}

/// A sequence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Sequence {
    /// Sequence name.
    pub name: String,
    /// Its data type (`bigint`).
    pub data_type: String,
    /// `START WITH`.
    pub start: String,
    /// `INCREMENT BY`.
    pub increment: String,
    /// `MINVALUE`.
    pub min: String,
    /// `MAXVALUE`.
    pub max: String,
    /// `CACHE`.
    pub cache: String,
    /// Whether it cycles.
    pub cycle: bool,
}

impl Sequence {
    /// The sequence options as `CREATE`/`ALTER SEQUENCE` clauses.
    #[must_use]
    pub fn options(&self) -> String {
        let mut out = Vec::new();
        if !self.data_type.is_empty() {
            out.push(format!("AS {}", self.data_type));
        }
        for (key, value) in [
            ("INCREMENT BY", &self.increment),
            ("MINVALUE", &self.min),
            ("MAXVALUE", &self.max),
            ("START WITH", &self.start),
            ("CACHE", &self.cache),
        ] {
            if !value.is_empty() {
                out.push(format!("{key} {value}"));
            }
        }
        out.push(if self.cycle { "CYCLE" } else { "NO CYCLE" }.to_string());
        out.join(" ")
    }
}

impl Snapshot {
    /// An empty snapshot of `schema` on a `kind` engine.
    #[must_use]
    pub fn new(kind: Kind, schema: &str) -> Snapshot {
        Snapshot {
            kind,
            schema: schema.to_string(),
            ..Snapshot::default()
        }
    }

    /// The table called `name`, if any.
    #[must_use]
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }

    /// The view called `name`, if any.
    #[must_use]
    pub fn view(&self, name: &str) -> Option<&View> {
        self.views.iter().find(|v| v.name == name)
    }

    /// Sort every list by name, so snapshots compare regardless of the order
    /// the catalog returned rows in. Column order is kept: it is meaningful.
    pub fn sort(&mut self) {
        self.tables.sort_by(|a, b| a.name.cmp(&b.name));
        for t in &mut self.tables {
            t.uniques.sort_by(|a, b| a.name.cmp(&b.name));
            t.checks.sort_by(|a, b| a.name.cmp(&b.name));
            t.foreign_keys.sort_by(|a, b| a.name.cmp(&b.name));
            t.indexes.sort_by(|a, b| a.name.cmp(&b.name));
        }
        self.views.sort_by(|a, b| a.name.cmp(&b.name));
        for v in &mut self.views {
            v.depends_on.sort();
            v.depends_on.dedup();
        }
        self.routines
            .sort_by(|a, b| (&a.name, &a.arguments).cmp(&(&b.name, &b.arguments)));
        self.triggers
            .sort_by(|a, b| (&a.table, &a.name).cmp(&(&b.table, &b.name)));
        self.enums.sort_by(|a, b| a.name.cmp(&b.name));
        self.sequences.sort_by(|a, b| a.name.cmp(&b.name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_orders_by_name_but_keeps_column_order() {
        let mut s = Snapshot::new(Kind::Postgres, "public");
        s.tables = vec![
            Table {
                name: "b".into(),
                columns: vec![
                    Column {
                        name: "z".into(),
                        ..Column::default()
                    },
                    Column {
                        name: "a".into(),
                        ..Column::default()
                    },
                ],
                ..Table::default()
            },
            Table {
                name: "a".into(),
                ..Table::default()
            },
        ];
        s.sort();
        assert_eq!(s.tables[0].name, "a");
        assert_eq!(s.tables[1].columns[0].name, "z");
        assert!(s.table("b").is_some());
    }

    #[test]
    fn sequence_options_render_every_clause() {
        let seq = Sequence {
            name: "s".into(),
            data_type: "bigint".into(),
            start: "1".into(),
            increment: "1".into(),
            min: "1".into(),
            max: "100".into(),
            cache: "1".into(),
            cycle: false,
        };
        assert_eq!(
            seq.options(),
            "AS bigint INCREMENT BY 1 MINVALUE 1 MAXVALUE 100 START WITH 1 CACHE 1 NO CYCLE"
        );
    }
}
