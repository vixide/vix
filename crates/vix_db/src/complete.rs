//! Intelligent autocomplete for the query editor.
//!
//! Three suggestion sources, per `spec/db`: SQL keywords (prefix-matched,
//! offered `UPPERCASE`), table names from the connected database, and — when
//! the prefix is `table.partial` — that table's columns. The engine is pure:
//! the workbench feeds it the schema at connect time and asks for suggestions
//! after each keystroke; the UI renders the popup and Tab accepts.

use super::highlight::KEYWORDS;

/// Most suggestions shown in the popup.
const MAX_SUGGESTIONS: usize = 8;

/// Minimum bare-prefix length before suggestions appear (a lone letter would
/// pop up on almost every keystroke). Column completion after `table.` has no
/// minimum.
const MIN_PREFIX: usize = 2;

/// A suggestion list plus where the to-be-replaced prefix starts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Suggestions {
    /// Char column where the replaced prefix begins.
    pub start: usize,
    /// Candidate completions, best-first.
    pub items: Vec<String>,
}

/// The autocomplete engine: keywords plus the connected database's schema.
#[derive(Debug, Clone, Default)]
pub struct Completer {
    /// Table and view names.
    tables: Vec<String>,
    /// `(table, column)` pairs.
    columns: Vec<(String, String)>,
    /// Foreign-key edges `(child, child_col, parent, parent_col)`.
    rels: Vec<(String, String, String, String)>,
}

impl Completer {
    /// Replace the schema portion (tables and columns) of the engine.
    pub fn set_schema(&mut self, tables: Vec<String>, columns: Vec<(String, String)>) {
        self.tables = tables;
        self.columns = columns;
    }

    /// Replace the foreign-key edges used for `JOIN … ON` suggestions.
    pub fn set_relationships(&mut self, rels: Vec<(String, String, String, String)>) {
        self.rels = rels;
    }

    /// Suggest completions for the identifier ending at char column `col` of
    /// `line`. Empty items means "no popup".
    #[must_use]
    pub fn suggest(&self, line: &str, col: usize) -> Suggestions {
        let chars: Vec<char> = line.chars().collect();
        let col = col.min(chars.len());
        let mut start = col;
        while start > 0 && (chars[start - 1].is_ascii_alphanumeric() || chars[start - 1] == '_' || chars[start - 1] == '.') {
            start -= 1;
        }
        let token: String = chars[start..col].iter().collect();
        if let Some(dot) = token.rfind('.') {
            let table = &token[..dot];
            let partial = &token[dot + 1..];
            let items = self.column_matches(table, partial);
            let partial_chars = partial.chars().count();
            return Suggestions { start: col - partial_chars, items };
        }
        // Right after a `JOIN` keyword, offer whole `table ON …` clauses drawn
        // from the foreign-key graph (no minimum prefix — `JOIN ` alone lists
        // the joinable tables).
        let before: String = chars[..start].iter().collect();
        let prev_word = before.split_whitespace().last().unwrap_or("");
        if prev_word.eq_ignore_ascii_case("join") {
            let items = self.join_matches(&token);
            if !items.is_empty() {
                return Suggestions { start, items };
            }
        }
        if token.chars().count() < MIN_PREFIX {
            return Suggestions::default();
        }
        let mut items: Vec<String> = self
            .tables
            .iter()
            .filter(|t| starts_ci(t, &token))
            .cloned()
            .collect();
        items.extend(
            KEYWORDS.iter().filter(|k| starts_ci(k, &token)).map(|k| k.to_ascii_uppercase()),
        );
        items.dedup();
        items.truncate(MAX_SUGGESTIONS);
        Suggestions { start, items }
    }

    /// `table ON child.fk = parent.pk` clauses for foreign keys touching a
    /// table whose name starts with `partial` (either side of each edge).
    fn join_matches(&self, partial: &str) -> Vec<String> {
        let mut items = Vec::new();
        for (child, child_col, parent, parent_col) in &self.rels {
            let on = format!("{child}.{child_col} = {parent}.{parent_col}");
            if starts_ci(parent, partial) {
                items.push(format!("{parent} ON {on}"));
            }
            if starts_ci(child, partial) {
                items.push(format!("{child} ON {on}"));
            }
        }
        items.dedup();
        items.truncate(MAX_SUGGESTIONS);
        items
    }

    /// Columns of `table` starting with `partial` (both case-insensitive).
    fn column_matches(&self, table: &str, partial: &str) -> Vec<String> {
        let mut items: Vec<String> = self
            .columns
            .iter()
            .filter(|(t, _)| t.eq_ignore_ascii_case(table))
            .filter(|(_, c)| partial.is_empty() || starts_ci(c, partial))
            .map(|(_, c)| c.clone())
            .collect();
        items.truncate(MAX_SUGGESTIONS);
        items
    }
}

/// ASCII case-insensitive `starts_with` (non-ASCII boundaries never match).
fn starts_ci(s: &str, prefix: &str) -> bool {
    s.get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Completer {
        let mut c = Completer::default();
        c.set_schema(
            vec!["orders".into(), "users".into()],
            vec![
                ("users".into(), "id".into()),
                ("users".into(), "name".into()),
                ("users".into(), "email".into()),
                ("orders".into(), "id".into()),
            ],
        );
        c
    }

    #[test]
    fn keywords_and_tables_match_by_prefix() {
        let s = engine().suggest("SELECT * FROM us", 16);
        assert!(s.items.contains(&"users".to_string()));
        assert!(s.items.contains(&"USING".to_string()), "keywords offered uppercase: {:?}", s.items);
        assert_eq!(s.start, 14, "prefix 'us' starts at column 14");
    }

    #[test]
    fn dot_completes_columns_of_that_table() {
        let s = engine().suggest("SELECT users.", 13);
        assert_eq!(s.items, vec!["id", "name", "email"], "all columns right after the dot");
        assert_eq!(s.start, 13);
        let s = engine().suggest("SELECT users.na", 15);
        assert_eq!(s.items, vec!["name"]);
        assert_eq!(s.start, 13, "only the partial after the dot is replaced");
    }

    #[test]
    fn join_offers_on_clauses_from_foreign_keys() {
        let mut c = engine();
        c.set_relationships(vec![("orders".into(), "user_id".into(), "users".into(), "id".into())]);
        // `JOIN ` with no prefix lists both joinable sides.
        let s = c.suggest("SELECT * FROM orders JOIN ", 26);
        assert!(s.items.contains(&"users ON orders.user_id = users.id".to_string()), "{:?}", s.items);
        // Typing the parent name narrows to it and replaces from the prefix.
        let s = c.suggest("SELECT * FROM orders JOIN us", 28);
        assert_eq!(s.items, vec!["users ON orders.user_id = users.id"]);
        assert_eq!(s.start, 26);
    }

    #[test]
    fn short_or_unknown_prefixes_stay_quiet() {
        assert!(engine().suggest("SELECT u", 8).items.is_empty(), "one char is too eager");
        assert!(engine().suggest("SELECT nosuch.", 14).items.is_empty(), "unknown table");
        assert!(engine().suggest("", 0).items.is_empty());
    }

    #[test]
    fn suggestion_count_is_capped() {
        let s = engine().suggest("se", 2);
        assert!(s.items.len() <= MAX_SUGGESTIONS);
        assert!(s.items.contains(&"SELECT".to_string()));
    }
}
