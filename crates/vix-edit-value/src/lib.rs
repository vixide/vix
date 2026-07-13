//! The structured-value editor: a foldable tree for JSON and YAML.
//!
//! Vix's Tools menu offers *Edit JSON* and *Edit YAML* commands that parse the
//! active buffer into a tree of objects, arrays, and scalars, then let the user
//! fold/unfold containers, navigate, and edit scalar values — code folding, but
//! for structured data. Saving serializes the tree back to JSON or YAML.
//!
//! JSON and YAML share one model: both parse (via `serde_yaml`, which also reads
//! JSON) into [`Val`], and only the serializer differs (a key-order-preserving
//! JSON pretty-printer, or `serde_yaml` for YAML). The [`Format`] chosen at open
//! time decides the title and the serializer.
//!
//! Like the other editors, this module owns the data, cursor, fold state, and an
//! undo history, and interprets keys itself, returning an [`Outcome`] telling the
//! host when to close or save.
//!
//! Keys: **↑/↓** (or `k`/`j`) move; **←/→** (or `h`/`l`) collapse/expand (or step
//! to parent/first child); **Enter**/**F2** edit the selected scalar (or toggle a
//! container); **Space** toggles fold; **u**/**Ctrl+R** undo/redo; **Ctrl+S**
//! save; **Esc**/**q** close.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Which serialization the buffer uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// JSON (pretty-printed on save, preserving key order).
    Json,
    /// YAML.
    Yaml,
}

/// What the host should do after the tree handled a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Handled internally; nothing further for the host to do.
    Consumed,
    /// The user asked to close the editor (Esc/`q`).
    Close,
    /// The user asked to save (Ctrl+S); the host should persist the tree.
    Save,
}

/// A structured value: scalar, array, or object (object key order preserved).
#[derive(Clone, PartialEq)]
enum Val {
    Null,
    Bool(bool),
    /// A number kept in its textual form so it round-trips unchanged.
    Num(String),
    Str(String),
    Arr {
        collapsed: bool,
        items: Vec<Val>,
    },
    Obj {
        collapsed: bool,
        entries: Vec<(String, Val)>,
    },
}

/// One step of a path locating a node within the tree.
#[derive(Clone, PartialEq)]
enum Seg {
    Key(String),
    Index(usize),
}

/// Whether a display row is a scalar leaf or a container (with collapse state).
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Scalar,
    Container(bool),
}

/// A flattened display row computed from the tree and its fold state.
struct Row {
    depth: usize,
    label: String,
    value: String,
    kind: Kind,
    path: Vec<Seg>,
}

/// Current interaction mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Edit,
}

/// Maximum number of undo steps retained.
const HISTORY_CAP: usize = 200;

/// A foldable, editable JSON/YAML value tree with a cursor and undo history.
pub struct Tree {
    root: Val,
    format: Format,
    rows: Vec<Row>,
    sel: usize,
    scroll: usize,
    dirty: bool,
    mode: Mode,
    edit_buf: String,
    undo: Vec<Val>,
    redo: Vec<Val>,
}

impl Tree {
    /// Parse `text` as JSON or YAML into a value tree. Returns `None` when the
    /// text does not parse (the host can warn). YAML's parser also accepts JSON.
    #[must_use]
    pub fn from_text(text: &str, format: Format) -> Option<Self> {
        let parsed: serde_yaml::Value = serde_yaml::from_str(text).ok()?;
        let mut tree = Tree {
            root: from_yaml(&parsed),
            format,
            rows: Vec::new(),
            sel: 0,
            scroll: 0,
            dirty: false,
            mode: Mode::Normal,
            edit_buf: String::new(),
            undo: Vec::new(),
            redo: Vec::new(),
        };
        tree.rebuild();
        Some(tree)
    }

    /// The chosen format.
    #[must_use]
    pub fn format(&self) -> Format {
        self.format
    }

    /// Number of visible rows.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// The selected row index.
    #[must_use]
    pub fn sel(&self) -> usize {
        self.sel
    }

    /// The first visible row (scroll offset).
    #[must_use]
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Whether there are unsaved edits.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Whether a scalar is currently being edited.
    #[must_use]
    pub fn is_editing(&self) -> bool {
        matches!(self.mode, Mode::Edit)
    }

    /// The in-progress edit text (valid while [`Tree::is_editing`]).
    #[must_use]
    pub fn edit_buffer(&self) -> &str {
        &self.edit_buf
    }

    /// Indentation depth of visible row `i`.
    #[must_use]
    pub fn depth(&self, i: usize) -> usize {
        self.rows.get(i).map_or(0, |r| r.depth)
    }

    /// The label (key or index) of visible row `i`.
    #[must_use]
    pub fn label(&self, i: usize) -> &str {
        self.rows.get(i).map_or("", |r| r.label.as_str())
    }

    /// The value/summary text of visible row `i`.
    #[must_use]
    pub fn value(&self, i: usize) -> &str {
        self.rows.get(i).map_or("", |r| r.value.as_str())
    }

    /// Whether visible row `i` is a container (object/array).
    #[must_use]
    pub fn is_container(&self, i: usize) -> bool {
        matches!(self.rows.get(i).map(|r| r.kind), Some(Kind::Container(_)))
    }

    /// Whether visible row `i` is a collapsed container.
    #[must_use]
    pub fn is_collapsed(&self, i: usize) -> bool {
        matches!(
            self.rows.get(i).map(|r| r.kind),
            Some(Kind::Container(true))
        )
    }

    /// Serialize the tree back to text in its format.
    #[must_use]
    pub fn to_text(&self) -> String {
        match self.format {
            Format::Json => {
                let mut out = String::new();
                json_push(&self.root, 0, &mut out);
                out.push('\n');
                out
            }
            Format::Yaml => serde_yaml::to_string(&to_yaml(&self.root)).unwrap_or_default(),
        }
    }

    /// Mark the tree as saved (called by the host after a successful write).
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Adjust the scroll so the selected row stays within a `height`-row window.
    pub fn ensure_visible(&mut self, height: usize) {
        let height = height.max(1);
        if self.sel < self.scroll {
            self.scroll = self.sel;
        } else if self.sel >= self.scroll + height {
            self.scroll = self.sel + 1 - height;
        }
        let max = self.rows.len().saturating_sub(height);
        self.scroll = self.scroll.min(max);
    }

    /// Interpret a key event and report what the host should do next.
    pub fn handle_key(&mut self, key: KeyEvent, page: usize) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('s') {
            if self.mode == Mode::Edit {
                self.commit_edit();
            }
            return Outcome::Save;
        }
        if self.mode == Mode::Edit {
            self.edit_key(key);
            return Outcome::Consumed;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.sel = self.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.step_down(1),
            KeyCode::PageUp => self.sel = self.sel.saturating_sub(page.max(1)),
            KeyCode::PageDown => self.step_down(page.max(1)),
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = self.rows.len().saturating_sub(1),
            KeyCode::Left | KeyCode::Char('h') => self.collapse_or_parent(),
            KeyCode::Right | KeyCode::Char('l') => self.expand_or_child(),
            KeyCode::Char(' ') => self.toggle(),
            KeyCode::Enter | KeyCode::F(2) => self.activate(),
            KeyCode::Char('u') => self.undo(),
            KeyCode::Char('r') if ctrl => self.redo(),
            KeyCode::Esc | KeyCode::Char('q') => return Outcome::Close,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Handle a key while editing a scalar.
    fn edit_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.commit_edit(),
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Backspace => {
                self.edit_buf.pop();
            }
            KeyCode::Char(c) => self.edit_buf.push(c),
            _ => {}
        }
    }

    /// Move the selection down by `n`, clamped.
    fn step_down(&mut self, n: usize) {
        self.sel = (self.sel + n).min(self.rows.len().saturating_sub(1));
    }

    /// Collapse the selected container, or jump to its parent.
    fn collapse_or_parent(&mut self) {
        let Some(row) = self.rows.get(self.sel) else {
            return;
        };
        if matches!(row.kind, Kind::Container(false)) {
            let path = row.path.clone();
            self.set_collapsed(&path, true);
            self.rebuild();
            self.select_path(&path);
        } else {
            let path = row.path.clone();
            if path.is_empty() {
                return;
            }
            self.select_path(&path[..path.len() - 1]);
        }
    }

    /// Expand the selected container, or step into its first child.
    fn expand_or_child(&mut self) {
        let Some(row) = self.rows.get(self.sel) else {
            return;
        };
        match row.kind {
            Kind::Container(true) => {
                let path = row.path.clone();
                self.set_collapsed(&path, false);
                self.rebuild();
                self.select_path(&path);
            }
            Kind::Container(false) => {
                if self.sel + 1 < self.rows.len() {
                    self.sel += 1;
                }
            }
            Kind::Scalar => {}
        }
    }

    /// Toggle the selected container's fold state.
    fn toggle(&mut self) {
        let Some(row) = self.rows.get(self.sel) else {
            return;
        };
        if let Kind::Container(c) = row.kind {
            let path = row.path.clone();
            self.set_collapsed(&path, !c);
            self.rebuild();
            self.select_path(&path);
        }
    }

    /// Enter: edit a scalar, or toggle a container.
    fn activate(&mut self) {
        match self.rows.get(self.sel).map(|r| r.kind) {
            Some(Kind::Scalar) => {
                self.edit_buf = self.rows[self.sel].value.clone();
                self.mode = Mode::Edit;
            }
            Some(Kind::Container(_)) => self.toggle(),
            None => {}
        }
    }

    /// Commit the edit buffer into the selected scalar as one undo step.
    fn commit_edit(&mut self) {
        self.mode = Mode::Normal;
        let Some(row) = self.rows.get(self.sel) else {
            return;
        };
        let path = row.path.clone();
        let new = parse_scalar(&self.edit_buf);
        if let Some(node) = at_mut(&mut self.root, &path) {
            if *node == new {
                return;
            }
            self.undo.push(self.root.clone());
            if self.undo.len() > HISTORY_CAP {
                self.undo.remove(0);
            }
            self.redo.clear();
            if let Some(node) = at_mut(&mut self.root, &path) {
                *node = new;
            }
            self.dirty = true;
            self.rebuild();
            self.select_path(&path);
        }
    }

    /// Set the collapse flag of the container at `path`, if any.
    fn set_collapsed(&mut self, path: &[Seg], collapsed: bool) {
        if let Some(Val::Arr { collapsed: c, .. } | Val::Obj { collapsed: c, .. }) =
            at_mut(&mut self.root, path)
        {
            *c = collapsed;
        }
    }

    /// Recompute the visible rows from the tree, clamping the selection.
    fn rebuild(&mut self) {
        self.rows.clear();
        flatten(&self.root, 0, String::new(), Vec::new(), &mut self.rows);
        if self.sel >= self.rows.len() {
            self.sel = self.rows.len().saturating_sub(1);
        }
    }

    /// Select the row whose path equals `path` (else leave the selection clamped).
    fn select_path(&mut self, path: &[Seg]) {
        if let Some(i) = self.rows.iter().position(|r| r.path == path) {
            self.sel = i;
        }
    }

    /// Undo the most recent value edit.
    fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.root, prev));
            self.dirty = true;
            self.rebuild();
        }
    }

    /// Redo the most recently undone edit.
    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.root, next));
            self.dirty = true;
            self.rebuild();
        }
    }
}

/// Locate a mutable node by path.
fn at_mut<'a>(mut node: &'a mut Val, path: &[Seg]) -> Option<&'a mut Val> {
    for seg in path {
        node = match (node, seg) {
            (Val::Arr { items, .. }, Seg::Index(i)) => items.get_mut(*i)?,
            (Val::Obj { entries, .. }, Seg::Key(k)) => {
                entries.iter_mut().find(|(ek, _)| ek == k).map(|(_, v)| v)?
            }
            _ => return None,
        };
    }
    Some(node)
}

/// The display text of a scalar (`""` for containers).
fn scalar_text(val: &Val) -> String {
    match val {
        Val::Null => "null".to_string(),
        Val::Bool(b) => b.to_string(),
        Val::Num(s) | Val::Str(s) => s.clone(),
        _ => String::new(),
    }
}

/// Flatten `val` (named `label`, located at `path`) into display rows; recurses
/// into expanded containers only.
fn flatten(val: &Val, depth: usize, label: String, path: Vec<Seg>, out: &mut Vec<Row>) {
    match val {
        Val::Arr { collapsed, items } => {
            out.push(Row {
                depth,
                label,
                value: format!("[{}]", items.len()),
                kind: Kind::Container(*collapsed),
                path: path.clone(),
            });
            if !collapsed {
                for (i, item) in items.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(Seg::Index(i));
                    flatten(item, depth + 1, format!("[{i}]"), p, out);
                }
            }
        }
        Val::Obj { collapsed, entries } => {
            out.push(Row {
                depth,
                label,
                value: format!("{{{}}}", entries.len()),
                kind: Kind::Container(*collapsed),
                path: path.clone(),
            });
            if !collapsed {
                for (k, v) in entries {
                    let mut p = path.clone();
                    p.push(Seg::Key(k.clone()));
                    flatten(v, depth + 1, k.clone(), p, out);
                }
            }
        }
        scalar => out.push(Row {
            depth,
            label,
            value: scalar_text(scalar),
            kind: Kind::Scalar,
            path,
        }),
    }
}

/// Parse edited text into a scalar: `true`/`false`/`null`, else a number when it
/// parses as one, else a string.
fn parse_scalar(s: &str) -> Val {
    match s {
        "true" => Val::Bool(true),
        "false" => Val::Bool(false),
        "null" => Val::Null,
        _ if s.parse::<f64>().is_ok() => Val::Num(s.to_string()),
        _ => Val::Str(s.to_string()),
    }
}

/// Convert a parsed `serde_yaml` value into [`Val`] (preserving map order).
fn from_yaml(v: &serde_yaml::Value) -> Val {
    use serde_yaml::Value as Y;
    match v {
        Y::Null => Val::Null,
        Y::Bool(b) => Val::Bool(*b),
        Y::Number(n) => Val::Num(n.to_string()),
        Y::String(s) => Val::Str(s.clone()),
        Y::Sequence(items) => Val::Arr {
            collapsed: false,
            items: items.iter().map(from_yaml).collect(),
        },
        Y::Mapping(m) => Val::Obj {
            collapsed: false,
            entries: m.iter().map(|(k, v)| (yaml_key(k), from_yaml(v))).collect(),
        },
        Y::Tagged(t) => from_yaml(&t.value),
    }
}

/// Render a mapping key as a string.
fn yaml_key(k: &serde_yaml::Value) -> String {
    use serde_yaml::Value as Y;
    match k {
        Y::String(s) => s.clone(),
        Y::Bool(b) => b.to_string(),
        Y::Number(n) => n.to_string(),
        Y::Null => "null".to_string(),
        _ => "?".to_string(),
    }
}

/// Convert [`Val`] into a `serde_yaml` value for YAML serialization.
fn to_yaml(val: &Val) -> serde_yaml::Value {
    use serde_yaml::Value as Y;
    match val {
        Val::Null => Y::Null,
        Val::Bool(b) => Y::Bool(*b),
        Val::Num(s) => s
            .parse::<i64>()
            .map(Y::from)
            .or_else(|_| s.parse::<f64>().map(Y::from))
            .unwrap_or_else(|_| Y::String(s.clone())),
        Val::Str(s) => Y::String(s.clone()),
        Val::Arr { items, .. } => Y::Sequence(items.iter().map(to_yaml).collect()),
        Val::Obj { entries, .. } => {
            let mut m = serde_yaml::Mapping::new();
            for (k, v) in entries {
                m.insert(Y::String(k.clone()), to_yaml(v));
            }
            Y::Mapping(m)
        }
    }
}

/// Append two spaces per `depth` to `out`.
fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// A JSON string literal with proper escaping.
fn json_quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
}

/// Pretty-print `val` as JSON into `out`, preserving object key order.
fn json_push(val: &Val, depth: usize, out: &mut String) {
    match val {
        Val::Null => out.push_str("null"),
        Val::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Val::Num(s) => out.push_str(s),
        Val::Str(s) => out.push_str(&json_quote(s)),
        Val::Arr { items, .. } => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                indent(out, depth + 1);
                json_push(item, depth + 1, out);
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, depth);
            out.push(']');
        }
        Val::Obj { entries, .. } => {
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (i, (k, v)) in entries.iter().enumerate() {
                indent(out, depth + 1);
                out.push_str(&json_quote(k));
                out.push_str(": ");
                json_push(v, depth + 1, out);
                if i + 1 < entries.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, depth);
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn code(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn json() -> Tree {
        Tree::from_text(r#"{"a": 1, "b": [10, 20], "c": "hi"}"#, Format::Json).unwrap()
    }

    #[test]
    fn parses_json_into_rows_in_order() {
        let t = json();
        // root + a + b + b[0] + b[1] + c = 6 rows
        assert_eq!(t.row_count(), 6);
        assert_eq!(t.label(1), "a");
        assert_eq!(t.value(1), "1");
        assert_eq!(t.label(2), "b");
        assert!(t.is_container(2));
        assert_eq!(t.label(5), "c");
        assert_eq!(t.value(5), "hi");
    }

    #[test]
    fn invalid_input_is_none() {
        assert!(Tree::from_text("{ not valid", Format::Json).is_none());
    }

    #[test]
    fn collapsing_hides_children() {
        let mut t = json();
        t.handle_key(code(KeyCode::Down), 8); // a
        t.handle_key(code(KeyCode::Down), 8); // b (array)
        assert!(t.is_container(t.sel()));
        t.handle_key(code(KeyCode::Left), 8); // collapse b
        assert!(t.is_collapsed(t.sel()));
        // root, a, b(collapsed), c = 4 rows
        assert_eq!(t.row_count(), 4);
    }

    #[test]
    fn edits_a_scalar_value() {
        let mut t = json();
        t.handle_key(code(KeyCode::Down), 8); // a = 1
        t.handle_key(code(KeyCode::Enter), 8); // edit
        assert!(t.is_editing());
        t.handle_key(code(KeyCode::Backspace), 8);
        t.handle_key(key('9'), 8);
        t.handle_key(code(KeyCode::Enter), 8); // commit
        assert_eq!(t.value(1), "9");
        assert!(t.is_dirty());
        assert!(t.to_text().contains("\"a\": 9"));
    }

    #[test]
    fn undo_and_redo_a_value_edit() {
        let mut t = json();
        t.handle_key(code(KeyCode::Down), 8); // a
        t.handle_key(code(KeyCode::Enter), 8);
        t.handle_key(code(KeyCode::Backspace), 8);
        t.handle_key(key('7'), 8);
        t.handle_key(code(KeyCode::Enter), 8);
        assert_eq!(t.value(1), "7");
        t.handle_key(key('u'), 8);
        assert_eq!(t.value(1), "1", "undo restores");
        t.handle_key(ctrl('r'), 8);
        assert_eq!(t.value(1), "7", "redo reapplies");
    }

    #[test]
    fn json_round_trips_and_preserves_key_order() {
        let t = Tree::from_text(r#"{"b": 1, "a": 2}"#, Format::Json).unwrap();
        assert_eq!(
            t.to_text(),
            "{\n  \"b\": 1,\n  \"a\": 2\n}\n",
            "keys stay in source order"
        );
    }

    #[test]
    fn yaml_parses_and_serializes() {
        let t = Tree::from_text("name: vix\ntags:\n  - tui\n  - editor\n", Format::Yaml).unwrap();
        assert_eq!(t.label(1), "name");
        assert_eq!(t.value(1), "vix");
        let out = t.to_text();
        assert!(out.contains("name: vix"), "got: {out:?}");
        assert!(out.contains("- tui"), "got: {out:?}");
    }

    #[test]
    fn save_and_close_outcomes() {
        let mut t = json();
        assert_eq!(t.handle_key(ctrl('s'), 8), Outcome::Save);
        assert_eq!(t.handle_key(key('q'), 8), Outcome::Close);
        assert_eq!(t.handle_key(code(KeyCode::Esc), 8), Outcome::Close);
    }

    proptest::proptest! {
        // Parsing arbitrary text as JSON or YAML never panics (invalid → None).
        #[test]
        fn from_text_never_panics(s in ".*") {
            let _ = Tree::from_text(&s, Format::Json);
            let _ = Tree::from_text(&s, Format::Yaml);
        }
    }

    #[test]
    fn deeply_nested_value_does_not_overflow() {
        // The recursive `from_yaml`/`rebuild` walk must not stack-overflow on
        // pathologically nested input; serde's depth limit rejects it as None.
        let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        assert!(Tree::from_text(&deep, Format::Json).is_none());
    }
}
