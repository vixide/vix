//! The structured-value editor: a foldable tree for JSON and YAML.
//!
//! Vix's Tools menu offers *Edit JSON* and *Edit YAML* commands that parse the
//! active buffer into a tree of objects, arrays, and scalars, then let the user
//! fold/unfold containers, navigate, and edit scalar values — code folding, but
//! for structured data. Saving serializes the tree back to JSON or YAML.
//!
//! JSON and YAML share one model: both parse (via `serde_norway`, which also reads
//! JSON) into `Val` (private), and only the serializer differs (a key-order-preserving
//! JSON pretty-printer, or `serde_norway` for YAML). The [`Format`] chosen at open
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
//!
//! Spec: `crates/vix-edit-value/spec/index.md`. The host (the App shell's
//! JSON/YAML edit overlay) draws from the read accessors ([`Tree::label`],
//! [`Tree::value`], [`Tree::depth`], ...) and never touches the tree directly,
//! so all fold/undo invariants live here.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// The nesting depth one level below `depth`. Depth counts the levels of a
/// parsed document, which the parser's recursion limit keeps far below `usize::MAX`.
fn deeper(depth: usize) -> usize {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "RCA-EXC-2: depth is a nesting level of a parsed document, bounded by the parser's recursion limit"
    )]
    let next = depth + 1;
    next
}

/// Whether an item at `index` of `len` items is followed by another.
fn has_next(index: usize, len: usize) -> bool {
    index.checked_add(1).is_some_and(|next| next < len)
}

/// The index of the last of `len` rows; with none, 0.
fn last_index(len: usize) -> usize {
    sub_or(len, 1, 0)
}

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

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
    /// JSON `null` / YAML `~`.
    Null,
    /// A boolean scalar.
    Bool(bool),
    /// A number kept in its textual form so it round-trips unchanged.
    Num(String),
    /// A string scalar.
    Str(String),
    /// An array; `collapsed` is UI fold state, not part of the data.
    Arr {
        /// Whether the children are hidden in the flattened view.
        collapsed: bool,
        /// Elements in order.
        items: Vec<Val>,
    },
    /// An object, kept as an ordered pair list (not a map) so key order
    /// round-trips; duplicate keys are not expected from the parser.
    Obj {
        /// Whether the children are hidden in the flattened view.
        collapsed: bool,
        /// Key/value pairs in source order.
        entries: Vec<(Key, Val)>,
    },
    /// A value with a YAML tag other than a core-schema one (`!custom x`),
    /// kept so it is written back (T853). Rows and edits see the inner value.
    Tagged {
        /// The tag, written back as read.
        tag: serde_norway::value::Tag,
        /// The tagged value.
        value: Box<Val>,
    },
}

/// An object key: the parsed key exactly as read (its type and any tag), and
/// the one-line label shown for it. Two keys can share a label (`1` and
/// `"1"`, or two complex keys both shown as `?`); saving writes `raw`, never
/// the label, so they stay distinct (T850).
#[derive(Clone, PartialEq)]
struct Key {
    /// The row label.
    label: String,
    /// The key as parsed, written back unchanged.
    raw: serde_norway::Value,
}

/// One step of a path locating a node within the tree.
#[derive(Clone, PartialEq)]
enum Seg {
    /// An object entry, by its position (labels can repeat, positions cannot).
    Entry(usize),
    /// An array position.
    Index(usize),
}

/// Whether a display row is a scalar leaf or a container (with collapse state).
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// A leaf value.
    Scalar,
    /// An object/array; the flag is `true` when collapsed.
    Container(bool),
}

/// A flattened display row computed from the tree and its fold state.
struct Row {
    /// Nesting level (0 for the root), used for indentation.
    depth: usize,
    /// Object key, or `[i]` for array elements; empty for the root.
    label: String,
    /// Scalar text, or a `[n]` / `{n}` child-count summary for containers,
    /// after the value's tag if it has one (`!custom x`).
    value: String,
    /// The scalar text an edit starts from: `value` without the tag.
    text: String,
    /// Scalar or container (with fold state).
    kind: Kind,
    /// Path from the root; rows are re-found after a rebuild by this path.
    path: Vec<Seg>,
}

/// Current interaction mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Navigating and folding.
    Normal,
    /// Typing into the edit buffer for a scalar.
    Edit,
}

/// Maximum number of undo steps retained.
const HISTORY_CAP: usize = 200;

/// Why [`Tree::from_text`] could not build a tree.
///
/// `Display` is the parser's own message, line and column included.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    /// The text is not valid JSON or YAML.
    Syntax {
        /// The parser's own message.
        message: String,
    },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for ParseError {}

/// Why [`Tree::save_text`] refused to write the tree back as text.
///
/// Nothing should be saved in any of these cases; the editor reports the
/// error instead.
#[derive(Debug)]
#[non_exhaustive]
pub enum SaveError {
    /// Two keys of one object would be written as the same key, so the
    /// saved file would lose one entry (YAML) or repeat a key (JSON).
    DuplicateKey {
        /// The key as it would be written.
        key: String,
    },
    /// A sequence or mapping used as a key (YAML `? [a, b]`) has no JSON form.
    ComplexKey,
    /// The YAML serializer failed.
    Yaml(serde_norway::Error),
    /// A mapping whose only key is tagged (`!k x: 0`): `serde_norway`
    /// cannot write one, so the YAML save is refused before it is tried.
    LoneTaggedKey {
        /// The key's label, tag included.
        key: String,
    },
    /// A tag holding `,`, `[` or `]` (read from `!t%5D`): `serde_norway`
    /// writes those raw, and the saved file would not parse.
    UnwritableTag {
        /// The tag, as shown in the row.
        tag: String,
    },
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateKey { key } => {
                write!(f, "two entries would be saved with the key {key}")
            }
            Self::ComplexKey => f.write_str("a sequence or mapping key cannot be written as JSON"),
            Self::Yaml(e) => write!(f, "{e}"),
            Self::LoneTaggedKey { key } => write!(
                f,
                "the YAML writer cannot save a mapping whose only key is tagged ({key})"
            ),
            Self::UnwritableTag { tag } => write!(
                f,
                "the YAML writer cannot save the tag {tag}, which holds a comma or bracket"
            ),
        }
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Yaml(e) => Some(e),
            _ => None,
        }
    }
}

/// A foldable, editable JSON/YAML value tree with a cursor and undo history.
pub struct Tree {
    /// The data (with per-container fold flags).
    root: Val,
    /// Serialization format chosen at open time.
    format: Format,
    /// Visible rows, derived from `root`; rebuilt after every structural change.
    rows: Vec<Row>,
    /// Selected index into `rows`.
    sel: usize,
    /// First visible row.
    scroll: usize,
    /// Unsaved edits exist.
    dirty: bool,
    /// Navigating or editing.
    mode: Mode,
    /// Text being typed while in [`Mode::Edit`] (private enum; see [`Tree::is_editing`]).
    edit_buf: String,
    /// Whole-tree snapshots, capped at `HISTORY_CAP`; whole-tree clones keep
    /// undo trivially correct at the cost of memory (values here are small).
    undo: Vec<Val>,
    /// Snapshots undone and re-appliable; cleared by any new edit.
    redo: Vec<Val>,
}

impl Tree {
    /// Parse `text` as JSON or YAML into a value tree. YAML's parser also
    /// accepts JSON.
    ///
    /// # Errors
    ///
    /// Returns the parser's own message (line/column included) when `text`
    /// does not parse, instead of discarding it (Run H, T543: this used to
    /// return a bare `Option`, so the caller could only show a static "not
    /// valid JSON or YAML" with no indication of what was actually wrong or
    /// where).
    pub fn from_text(text: &str, format: Format) -> Result<Self, ParseError> {
        // The YAML parser reads JSON too, but treats U+0085, U+2028 and
        // U+2029 as line breaks and refuses non-printable characters, all of
        // which JSON allows raw inside strings. In text that is strict JSON
        // they can only be inside strings, where the `\uXXXX` escape means
        // the same character (T854).
        let escaped;
        let source = if text.chars().any(yaml_unsafe)
            && serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok()
        {
            escaped = escape_yaml_unsafe(text);
            escaped.as_str()
        } else {
            text
        };
        let parsed: serde_norway::Value =
            serde_norway::from_str(source).map_err(|e| ParseError::Syntax {
                message: e.to_string(),
            })?;
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
        Ok(tree)
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
    ///
    /// # Errors
    ///
    /// The YAML serializer's error when the tree cannot be written as YAML;
    /// the caller must not save in that case (an empty text would erase the file).
    /// A tree [`Tree::save_text`] refuses (two keys that would be written
    /// alike) is an error here too, carrying the same English message.
    pub fn to_text(&self) -> Result<String, serde_norway::Error> {
        self.save_text().map_err(|e| match e {
            SaveError::Yaml(e) => e,
            other => <serde_norway::Error as serde::ser::Error>::custom(other),
        })
    }

    /// Serialize the tree back to text in its format, refusing a tree that
    /// would not read back as the same data.
    ///
    /// Each object key is written in the form it was read (a number stays a
    /// number, a string a string, a tag stays on the key), so keys that share
    /// a label stay distinct.
    ///
    /// # Errors
    ///
    /// [`SaveError::DuplicateKey`] when two keys of one object would be
    /// written as the same key (in JSON every key is a string, so YAML keys
    /// `1` and `"1"` collide there); [`SaveError::ComplexKey`] for a
    /// sequence or mapping key in JSON; [`SaveError::LoneTaggedKey`] for a
    /// YAML mapping whose only key is tagged, and [`SaveError::UnwritableTag`]
    /// for a tag holding `,`, `[` or `]`, which the YAML writer cannot
    /// write; [`SaveError::Yaml`] when the YAML serializer fails. The caller
    /// must not save in any of these cases.
    pub fn save_text(&self) -> Result<String, SaveError> {
        match self.format {
            Format::Json => {
                check_json_keys(&self.root)?;
                let mut out = String::new();
                json_push(&self.root, 0, &mut out);
                out.push('\n');
                Ok(out)
            }
            Format::Yaml => {
                check_yaml_keys(&self.root)?;
                let doc = to_yaml(&self.root);
                if let Some(limit) = writer_limit(&doc) {
                    return Err(limit);
                }
                serde_norway::to_string(&doc).map_err(SaveError::Yaml)
            }
        }
    }

    /// Mark the tree as saved (called by the host after a successful write).
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Adjust the scroll so the selected row stays within a `height`-row window.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.sel, self.scroll, height, self.rows.len());
    }

    /// Interpret a key event and report what the host should do next.
    ///
    /// `page` is the host's visible row count, used for PageUp/PageDown.
    /// Ctrl+S is handled before the edit-mode check so a pending edit is
    /// committed first and is included in what the host saves.
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
            KeyCode::Up | KeyCode::Char('k') => self.sel = sub_or(self.sel, 1, 0),
            KeyCode::Down | KeyCode::Char('j') => self.step_down(1),
            KeyCode::PageUp => self.sel = sub_or(self.sel, page.max(1), 0),
            KeyCode::PageDown => self.step_down(page.max(1)),
            KeyCode::Home => self.sel = 0,
            KeyCode::End => self.sel = last_index(self.rows.len()),
            KeyCode::Left | KeyCode::Char('h') => self.collapse_or_parent(),
            KeyCode::Right | KeyCode::Char('l') => self.expand_or_child(),
            KeyCode::Char(' ') => self.toggle(),
            KeyCode::Enter | KeyCode::F(2) => self.activate(),
            KeyCode::Char('u') => self.undo(),
            // Plain `r` is deliberately unbound; only Ctrl+R redoes.
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
        self.sel = add_or(self.sel, n, usize::MAX).min(last_index(self.rows.len()));
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
            // The root has no parent.
            let Some((_, parent)) = path.split_last() else {
                return;
            };
            self.select_path(parent);
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
                let next = add_or(self.sel, 1, usize::MAX);
                if next < self.rows.len() {
                    self.sel = next;
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
        match self.rows.get(self.sel) {
            Some(row) if row.kind == Kind::Scalar => {
                self.edit_buf = row.text.clone();
                self.mode = Mode::Edit;
            }
            Some(row) if matches!(row.kind, Kind::Container(_)) => self.toggle(),
            _ => {}
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
            // An unchanged value is not an edit: no undo step, not dirty.
            if *node == new {
                return;
            }
            // Delegates to `vix-capped-stack` (Run H, T525) -- this crate
            // and four siblings used to each hand-roll this identical clamp.
            let snap = self.root.clone();
            vix_capped_stack::push_capped(&mut self.undo, snap, HISTORY_CAP);
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
            self.sel = last_index(self.rows.len());
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
        node = match (untag_mut(node), seg) {
            (Val::Arr { items, .. }, Seg::Index(i)) => items.get_mut(*i)?,
            (Val::Obj { entries, .. }, Seg::Entry(i)) => entries.get_mut(*i).map(|(_, v)| v)?,
            _ => return None,
        };
    }
    Some(untag_mut(node))
}

/// The value under any tag: rows, folds and edits act on it, so editing a
/// tagged scalar keeps its tag.
fn untag_mut(node: &mut Val) -> &mut Val {
    match node {
        Val::Tagged { value, .. } => untag_mut(value),
        other => other,
    }
}

/// The display text of a scalar (`""` for containers).
fn scalar_text(val: &Val) -> String {
    match val {
        Val::Null => "null".to_string(),
        Val::Bool(b) => b.to_string(),
        Val::Num(s) | Val::Str(s) => s.clone(),
        Val::Tagged { value, .. } => scalar_text(value),
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
                text: String::new(),
                kind: Kind::Container(*collapsed),
                path: path.clone(),
            });
            if !collapsed {
                for (i, item) in items.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(Seg::Index(i));
                    flatten(item, deeper(depth), format!("[{i}]"), p, out);
                }
            }
        }
        Val::Obj { collapsed, entries } => {
            out.push(Row {
                depth,
                label,
                value: format!("{{{}}}", entries.len()),
                text: String::new(),
                kind: Kind::Container(*collapsed),
                path: path.clone(),
            });
            if !collapsed {
                for (i, (k, v)) in entries.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(Seg::Entry(i));
                    flatten(v, deeper(depth), k.label.clone(), p, out);
                }
            }
        }
        Val::Tagged { tag, value } => {
            // The tagged value's own row shows the tag before its value.
            let first = out.len();
            flatten(value, depth, label, path, out);
            if let Some(row) = out.get_mut(first) {
                row.value = format!("{tag} {}", row.value);
            }
        }
        scalar => out.push(Row {
            depth,
            label,
            value: scalar_text(scalar),
            text: scalar_text(scalar),
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
        // Any f64-parsable text counts as a number (kept verbatim, so `1.50`
        // is not normalized); notably `nan`/`inf` also parse as f64.
        _ if s.parse::<f64>().is_ok() => Val::Num(s.to_string()),
        _ => Val::Str(s.to_string()),
    }
}

/// Convert a parsed `serde_norway` value into [`Val`] (preserving map order).
fn from_yaml(v: &serde_norway::Value) -> Val {
    use serde_norway::Value as Y;
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
            entries: m
                .iter()
                .map(|(k, v)| {
                    let key = Key {
                        label: yaml_key(k),
                        raw: k.clone(),
                    };
                    (key, from_yaml(v))
                })
                .collect(),
        },
        // Core-schema tags (`!!str`) are already resolved by the parser;
        // any other tag is kept and written back (T853).
        Y::Tagged(t) => Val::Tagged {
            tag: t.tag.clone(),
            value: Box::new(from_yaml(&t.value)),
        },
    }
}

/// Render a mapping key as a string.
fn yaml_key(k: &serde_norway::Value) -> String {
    use serde_norway::Value as Y;
    match k {
        Y::String(s) => s.clone(),
        Y::Bool(b) => b.to_string(),
        Y::Number(n) => n.to_string(),
        Y::Null => "null".to_string(),
        // A tagged key shows its tag, as YAML writes it: `!k x`.
        Y::Tagged(t) => format!("{} {}", t.tag, yaml_key(&t.value)),
        // Complex (sequence/mapping) keys have no sensible one-line label.
        Y::Sequence(_) | Y::Mapping(_) => "?".to_string(),
    }
}

/// The JSON name a key is written as: a string as itself, a number, boolean
/// or null as its text, a tagged key as its value (JSON has no tags). `None`
/// for a sequence or mapping key, which has no JSON form.
fn json_key(k: &serde_norway::Value) -> Option<String> {
    use serde_norway::Value as Y;
    match k {
        Y::String(s) => Some(s.clone()),
        Y::Bool(b) => Some(b.to_string()),
        Y::Number(n) => Some(n.to_string()),
        Y::Null => Some("null".to_string()),
        Y::Tagged(t) => json_key(&t.value),
        Y::Sequence(_) | Y::Mapping(_) => None,
    }
}

/// Refuse a tree whose objects would be written with a repeated or
/// unrepresentable JSON name. Names compare as decoded strings (RFC 8259
/// §7/§8.3), the form the writer escapes them back from.
fn check_json_keys(val: &Val) -> Result<(), SaveError> {
    match val {
        Val::Arr { items, .. } => items.iter().try_for_each(check_json_keys),
        Val::Tagged { value, .. } => check_json_keys(value),
        Val::Obj { entries, .. } => {
            let mut seen = std::collections::HashSet::new();
            for (k, v) in entries {
                let name = json_key(&k.raw).ok_or(SaveError::ComplexKey)?;
                if !seen.insert(name.clone()) {
                    return Err(SaveError::DuplicateKey {
                        key: json_quote(&name),
                    });
                }
                check_json_keys(v)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Refuse a tree whose objects hold the same YAML key twice. The parser
/// rejects duplicate keys, so this guards the writer, which would keep only
/// one of them.
fn check_yaml_keys(val: &Val) -> Result<(), SaveError> {
    match val {
        Val::Arr { items, .. } => items.iter().try_for_each(check_yaml_keys),
        Val::Tagged { value, .. } => check_yaml_keys(value),
        Val::Obj { entries, .. } => {
            let mut seen = std::collections::HashSet::new();
            for (k, v) in entries {
                if !seen.insert(&k.raw) {
                    return Err(SaveError::DuplicateKey {
                        key: k.label.clone(),
                    });
                }
                check_yaml_keys(v)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Convert [`Val`] into a `serde_norway` value for YAML serialization.
fn to_yaml(val: &Val) -> serde_norway::Value {
    use serde_norway::Value as Y;
    match val {
        Val::Null => Y::Null,
        Val::Bool(b) => Y::Bool(*b),
        // Prefer an integer, then a float, so `7` stays `7` rather than `7.0`;
        // text that is neither (an edit like `1e`) falls back to a string.
        Val::Num(s) => s
            .parse::<i64>()
            .map(Y::from)
            .or_else(|_| s.parse::<f64>().map(Y::from))
            .unwrap_or_else(|_| Y::String(s.clone())),
        Val::Str(s) => Y::String(s.clone()),
        Val::Arr { items, .. } => Y::Sequence(items.iter().map(to_yaml).collect()),
        Val::Tagged { tag, value } => Y::Tagged(Box::new(serde_norway::value::TaggedValue {
            tag: tag.clone(),
            value: to_yaml(value),
        })),
        Val::Obj { entries, .. } => {
            let mut m = serde_norway::Mapping::new();
            for (k, v) in entries {
                m.insert(k.raw.clone(), to_yaml(v));
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
///
/// `serde_json` writes U+0085, U+2028, U+2029, DEL, C1 controls and
/// U+FFFE/U+FFFF raw, as JSON allows; they are escaped here too, because the
/// YAML parser that reads the saved file back would take the first three as
/// line breaks and refuse the rest (T854).
fn json_quote(s: &str) -> String {
    let quoted = serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""));
    if quoted.chars().any(yaml_unsafe) {
        escape_yaml_unsafe(&quoted)
    } else {
        quoted
    }
}

/// Whether the YAML parser would not read `c` back as itself inside a
/// quoted string: a YAML 1.1 line break other than LF and CR (U+0085,
/// U+2028, U+2029), or a character outside YAML's printable set that JSON
/// allows raw (DEL, the C1 controls, U+FFFE, U+FFFF). Controls below U+0020
/// are not listed: JSON never has them raw and `serde_json` escapes them.
fn yaml_unsafe(c: char) -> bool {
    matches!(
        c,
        '\u{7f}'..='\u{9f}' | '\u{2028}' | '\u{2029}' | '\u{fffe}' | '\u{ffff}'
    )
}

/// `text` with every [`yaml_unsafe`] character written as a `\uXXXX`
/// escape. Only sound where those characters are inside JSON strings.
fn escape_yaml_unsafe(text: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if yaml_unsafe(c) {
            // Writing to a String cannot fail.
            let _ = write!(out, "\\u{:04x}", u32::from(c));
        } else {
            out.push(c);
        }
    }
    out
}

/// The first thing in `doc` that `serde_norway` cannot write so that it
/// reads back, anywhere (in keys too): a one-entry mapping whose key is
/// tagged (its emitter fails), or a tag holding `,`, `[` or `]` (its
/// emitter writes them raw, where the parser reads them as flow indicators).
fn writer_limit(doc: &serde_norway::Value) -> Option<SaveError> {
    use serde_norway::Value as Y;
    match doc {
        Y::Sequence(items) => items.iter().find_map(writer_limit),
        Y::Tagged(t) => {
            let tag = t.tag.to_string();
            if tag.contains([',', '[', ']']) {
                return Some(SaveError::UnwritableTag { tag });
            }
            writer_limit(&t.value)
        }
        Y::Mapping(m) => {
            if m.len() == 1
                && let Some((k @ Y::Tagged(_), _)) = m.iter().next()
            {
                return Some(SaveError::LoneTaggedKey { key: yaml_key(k) });
            }
            m.iter()
                .find_map(|(k, v)| writer_limit(k).or_else(|| writer_limit(v)))
        }
        _ => None,
    }
}

/// Pretty-print `val` as JSON into `out`, preserving object key order.
fn json_push(val: &Val, depth: usize, out: &mut String) {
    match val {
        Val::Null => out.push_str("null"),
        Val::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Val::Num(s) => out.push_str(s),
        Val::Str(s) => out.push_str(&json_quote(s)),
        // JSON has no tags: a tagged value is written as its value.
        Val::Tagged { value, .. } => json_push(value, depth, out),
        Val::Arr { items, .. } => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                indent(out, deeper(depth));
                json_push(item, deeper(depth), out);
                if has_next(i, items.len()) {
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
                indent(out, deeper(depth));
                // `save_text` checked every key has a JSON name first.
                out.push_str(&json_quote(&json_key(&k.raw).unwrap_or_default()));
                out.push_str(": ");
                json_push(v, deeper(depth), out);
                if has_next(i, entries.len()) {
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
    fn invalid_input_is_an_error() {
        assert!(matches!(
            Tree::from_text("{ not valid", Format::Json),
            Err(ParseError::Syntax { .. })
        ));
    }

    #[test]
    fn parse_error_names_the_real_problem() {
        // T543 (Run H): this used to be a bare `Option`, so the caller could
        // only show a generic "not valid JSON or YAML" -- now the parser's
        // own message (line/column included) is available.
        // `Tree` isn't `Debug`, so `unwrap_err` isn't available; match instead.
        let Err(err) = Tree::from_text("a: [1, 2\nb: 3", Format::Yaml) else {
            panic!("expected a parse error");
        };
        assert!(
            err.to_string().to_lowercase().contains("line"),
            "the parser's own diagnostic names a location: {err:?}"
        );
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
        assert!(t.to_text().unwrap().contains("\"a\": 9"));
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
            t.to_text().unwrap(),
            "{\n  \"b\": 1,\n  \"a\": 2\n}\n",
            "keys stay in source order"
        );
    }

    #[test]
    fn yaml_parses_and_serializes() {
        let t = Tree::from_text("name: vix\ntags:\n  - tui\n  - editor\n", Format::Yaml).unwrap();
        assert_eq!(t.label(1), "name");
        assert_eq!(t.value(1), "vix");
        let out = t.to_text().unwrap();
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
        // pathologically nested input; serde's depth limit rejects it as an error.
        let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        assert!(Tree::from_text(&deep, Format::Json).is_err());
    }

    fn row_of(t: &Tree, label: &str) -> usize {
        (0..t.row_count())
            .find(|&i| t.label(i) == label)
            .unwrap_or_else(|| panic!("no row labelled {label}"))
    }

    fn goto(t: &mut Tree, label: &str) {
        let target = row_of(t, label);
        t.handle_key(code(KeyCode::Home), 8);
        for _ in 0..target {
            t.handle_key(code(KeyCode::Down), 8);
        }
        assert_eq!(t.label(t.sel()), label);
    }

    fn type_value(t: &mut Tree, text: &str) {
        t.handle_key(code(KeyCode::Enter), 8);
        while !t.edit_buffer().is_empty() {
            t.handle_key(code(KeyCode::Backspace), 8);
        }
        for c in text.chars() {
            t.handle_key(key(c), 8);
        }
        t.handle_key(code(KeyCode::Enter), 8);
    }

    #[test]
    fn accessors_are_total_and_describe_the_tree() {
        let t = json();
        assert_eq!(t.format(), Format::Json);
        assert_eq!((t.sel(), t.scroll()), (0, 0));
        assert!(!t.is_dirty() && !t.is_editing());
        assert_eq!(t.edit_buffer(), "");
        assert_eq!(t.value(0), "{3}", "object rows show their entry count");
        assert_eq!(t.value(2), "[2]", "array rows show their length");
        assert_eq!(t.label(3), "[0]");
        assert_eq!(t.depth(0), 0);
        assert_eq!(t.depth(3), 2);
        assert_eq!(t.depth(99), 0);
        assert_eq!(t.label(99), "");
        assert_eq!(t.value(99), "");
        assert!(!t.is_container(99) && !t.is_collapsed(99));
        assert!(!t.is_container(1), "scalars are not containers");
        assert!(!t.is_collapsed(2));
    }

    #[test]
    fn parse_error_displays_its_message() {
        let err = ParseError::Syntax {
            message: "boom".into(),
        };
        assert_eq!(err.to_string(), "boom");
        let boxed: Box<dyn std::error::Error> = Box::new(err);
        assert_eq!(boxed.to_string(), "boom");
    }

    #[test]
    fn navigation_keys_move_and_clamp() {
        let mut t = json();
        t.handle_key(code(KeyCode::Up), 8);
        assert_eq!(t.sel(), 0);
        t.handle_key(key('j'), 8);
        t.handle_key(key('j'), 8);
        assert_eq!(t.sel(), 2);
        t.handle_key(key('k'), 8);
        assert_eq!(t.sel(), 1);
        t.handle_key(code(KeyCode::PageDown), 3);
        assert_eq!(t.sel(), 4);
        t.handle_key(code(KeyCode::PageDown), 100);
        assert_eq!(t.sel(), 5, "clamped to the last row");
        t.handle_key(code(KeyCode::Down), 8);
        assert_eq!(t.sel(), 5);
        t.handle_key(code(KeyCode::PageUp), 2);
        assert_eq!(t.sel(), 3);
        t.handle_key(code(KeyCode::PageUp), 0);
        assert_eq!(t.sel(), 2, "a zero page still moves one row");
        t.handle_key(code(KeyCode::PageUp), 100);
        assert_eq!(t.sel(), 0);
        t.handle_key(code(KeyCode::End), 8);
        assert_eq!(t.sel(), 5);
        t.handle_key(code(KeyCode::Home), 8);
        assert_eq!(t.sel(), 0);
        t.handle_key(key('x'), 8);
        assert_eq!(t.sel(), 0, "unbound keys are inert");
    }

    #[test]
    fn left_steps_to_the_parent_and_right_expands_or_enters() {
        let mut t = json();
        goto(&mut t, "[1]");
        t.handle_key(code(KeyCode::Left), 8);
        assert_eq!(t.label(t.sel()), "b", "a scalar's Left goes to its parent");
        t.handle_key(key('h'), 8);
        assert!(t.is_collapsed(t.sel()));
        t.handle_key(code(KeyCode::Left), 8);
        assert_eq!(
            t.label(t.sel()),
            "",
            "a collapsed container's Left goes to the root"
        );
        t.handle_key(code(KeyCode::Left), 8);
        assert!(t.is_collapsed(0), "an open root collapses");
        t.handle_key(code(KeyCode::Left), 8);
        assert_eq!(t.sel(), 0, "a collapsed root has no parent to go to");
        assert_eq!(t.row_count(), 1);
        t.handle_key(code(KeyCode::Right), 8);
        assert!(!t.is_collapsed(0));
        goto(&mut t, "b");
        t.handle_key(key('l'), 8);
        assert!(!t.is_collapsed(t.sel()), "b expanded");
        t.handle_key(code(KeyCode::Right), 8);
        assert_eq!(
            t.label(t.sel()),
            "[0]",
            "Right on an open container enters it"
        );
        t.handle_key(code(KeyCode::Right), 8);
        assert_eq!(t.label(t.sel()), "[0]", "Right on a scalar does nothing");
    }

    #[test]
    fn right_on_the_last_open_empty_container_stays_put() {
        let mut t = Tree::from_text("{\"e\": []}", Format::Json).unwrap();
        goto(&mut t, "e");
        t.handle_key(code(KeyCode::Right), 8);
        assert_eq!(t.label(t.sel()), "e", "nothing below to step into");
    }

    #[test]
    fn space_and_enter_toggle_containers_and_keep_the_selection() {
        let mut t = json();
        goto(&mut t, "b");
        t.handle_key(key(' '), 8);
        assert!(t.is_collapsed(t.sel()));
        assert_eq!(t.row_count(), 4);
        t.handle_key(code(KeyCode::Enter), 8);
        assert!(!t.is_collapsed(t.sel()), "Enter toggles a container");
        assert_eq!(t.label(t.sel()), "b");
        t.handle_key(code(KeyCode::F(2)), 8);
        assert!(t.is_collapsed(t.sel()), "F2 does too");
        assert!(!t.is_editing());
        goto(&mut t, "a");
        t.handle_key(key(' '), 8);
        assert!(
            !t.is_editing() && !t.is_dirty(),
            "space on a scalar is inert"
        );
    }

    #[test]
    fn edit_mode_cancel_and_ignored_keys() {
        let mut t = json();
        goto(&mut t, "c");
        t.handle_key(code(KeyCode::F(2)), 8);
        assert!(t.is_editing());
        assert_eq!(t.edit_buffer(), "hi");
        t.handle_key(key('!'), 8);
        t.handle_key(code(KeyCode::Down), 8);
        assert_eq!(
            t.edit_buffer(),
            "hi!",
            "navigation keys do nothing while editing"
        );
        assert_eq!(
            t.handle_key(key('q'), 8),
            Outcome::Consumed,
            "q is text here"
        );
        assert_eq!(t.edit_buffer(), "hi!q");
        assert_eq!(t.handle_key(code(KeyCode::Esc), 8), Outcome::Consumed);
        assert!(!t.is_editing() && !t.is_dirty());
        assert_eq!(t.value(t.sel()), "hi", "Esc discards");
    }

    #[test]
    fn ctrl_s_while_editing_commits_first_then_saves() {
        let mut t = json();
        goto(&mut t, "a");
        t.handle_key(code(KeyCode::Enter), 8);
        t.handle_key(code(KeyCode::Backspace), 8);
        t.handle_key(key('5'), 8);
        assert_eq!(t.handle_key(ctrl('s'), 8), Outcome::Save);
        assert!(!t.is_editing());
        assert_eq!(t.value(row_of(&t, "a")), "5");
        assert!(t.is_dirty());
        t.mark_saved();
        assert!(!t.is_dirty());
    }

    #[test]
    fn typed_values_become_the_right_json_types() {
        let mut t = json();
        goto(&mut t, "a");
        type_value(&mut t, "true");
        goto(&mut t, "c");
        type_value(&mut t, "null");
        goto(&mut t, "[0]");
        type_value(&mut t, "1.50");
        goto(&mut t, "[1]");
        type_value(&mut t, "two words");
        assert_eq!(
            t.to_text().unwrap(),
            "{\n  \"a\": true,\n  \"b\": [\n    1.50,\n    \"two words\"\n  ],\n  \"c\": null\n}\n"
        );
    }

    #[test]
    fn an_unchanged_edit_is_not_an_edit() {
        let mut t = json();
        goto(&mut t, "a");
        type_value(&mut t, "1");
        assert!(!t.is_dirty());
        t.handle_key(key('u'), 8);
        assert_eq!(t.value(row_of(&t, "a")), "1", "nothing to undo");
        assert!(!t.is_dirty(), "undo of nothing leaves it clean");
    }

    #[test]
    fn a_new_edit_clears_the_redo_stack_and_undo_marks_dirty() {
        let mut t = json();
        goto(&mut t, "a");
        type_value(&mut t, "2");
        t.handle_key(key('u'), 8);
        assert!(t.is_dirty());
        type_value(&mut t, "3");
        t.handle_key(ctrl('r'), 8);
        assert_eq!(
            t.value(row_of(&t, "a")),
            "3",
            "redo is gone after a fresh edit"
        );
        t.handle_key(key('r'), 8);
        assert_eq!(t.value(row_of(&t, "a")), "3", "plain r is unbound");
        t.handle_key(key('u'), 8);
        t.handle_key(key('u'), 8);
        assert_eq!(t.value(row_of(&t, "a")), "1");
    }

    #[test]
    fn undo_history_is_capped() {
        let mut t = json();
        goto(&mut t, "a");
        for n in 0..(HISTORY_CAP + 20) {
            type_value(&mut t, &format!("{}", n + 100));
        }
        let mut undone = 0;
        loop {
            let before = t.value(row_of(&t, "a")).to_string();
            t.handle_key(key('u'), 8);
            if t.value(row_of(&t, "a")) == before {
                break;
            }
            undone += 1;
        }
        assert_eq!(undone, HISTORY_CAP);
    }

    #[test]
    fn yaml_scalars_keys_and_tags_map_to_rows() {
        let text = "1: one\ntrue: yes_text\nnull: n\n? [a, b]\n: complex\nnum: 3.5\nflag: false\nnothing: ~\ntagged: !custom value\n";
        let t = Tree::from_text(text, Format::Yaml).unwrap();
        let labels: Vec<&str> = (0..t.row_count()).map(|i| t.label(i)).collect();
        assert!(labels.contains(&"1"));
        assert!(labels.contains(&"true"));
        assert!(labels.contains(&"null"));
        assert!(
            labels.contains(&"?"),
            "a complex key has a placeholder label"
        );
        let at = |l: &str| t.value(row_of(&t, l)).to_string();
        assert_eq!(at("num"), "3.5");
        assert_eq!(at("flag"), "false");
        assert_eq!(at("nothing"), "null");
        assert_eq!(at("tagged"), "!custom value", "the tag is shown");
    }

    #[test]
    fn yaml_numbers_keep_their_kind_when_saved() {
        let mut t = Tree::from_text("n: 7\nf: 2.5\ns: x\n", Format::Yaml).unwrap();
        goto(&mut t, "s");
        type_value(&mut t, "1e");
        goto(&mut t, "n");
        type_value(&mut t, "8");
        let out = t.to_text().unwrap();
        assert!(out.contains("n: 8\n"), "{out}");
        assert!(out.contains("f: 2.5\n"), "{out}");
        assert!(out.contains("s: 1e\n") || out.contains("s: '1e'"), "{out}");
        assert_eq!(t.format(), Format::Yaml);
    }

    #[test]
    fn empty_containers_and_nesting_serialize_as_json() {
        let t = Tree::from_text(
            r#"{"e": [], "o": {}, "n": [[1], {"k": "v\"q"}]}"#,
            Format::Json,
        )
        .unwrap();
        assert_eq!(
            t.to_text().unwrap(),
            "{\n  \"e\": [],\n  \"o\": {},\n  \"n\": [\n    [\n      1\n    ],\n    {\n      \"k\": \"v\\\"q\"\n    }\n  ]\n}\n"
        );
        assert_eq!(t.value(row_of(&t, "e")), "[0]");
        assert_eq!(t.value(row_of(&t, "o")), "{0}");
    }

    #[test]
    fn ensure_visible_scrolls_to_the_selection() {
        let mut t = json();
        t.handle_key(code(KeyCode::End), 8);
        t.ensure_visible(3);
        assert_eq!(t.scroll(), 3, "last row sits at the bottom of the window");
        t.handle_key(code(KeyCode::Home), 8);
        t.ensure_visible(3);
        assert_eq!(t.scroll(), 0);
    }

    #[test]
    fn a_scalar_root_is_one_editable_row() {
        let mut t = Tree::from_text("42", Format::Json).unwrap();
        assert_eq!(t.row_count(), 1);
        t.handle_key(code(KeyCode::Left), 8);
        assert_eq!(t.sel(), 0, "no parent to step to");
        type_value(&mut t, "43");
        assert_eq!(t.to_text().unwrap(), "43\n");
    }

    /// Assert \`out\` reads back as the same YAML data as \`text\`.
    fn same_yaml(text: &str, out: &str) {
        let a: serde_norway::Value = serde_norway::from_str(text).unwrap();
        let b: serde_norway::Value = serde_norway::from_str(out).unwrap();
        assert_eq!(a, b, "saved:\n{out}");
    }

    fn yaml_round_trip(text: &str) {
        let t = Tree::from_text(text, Format::Yaml).unwrap();
        same_yaml(text, &t.to_text().unwrap());
    }

    #[test]
    fn yaml_int_and_string_keys_that_look_alike_both_save() {
        // T850: both rows read \`1\`; the save used to keep only one.
        yaml_round_trip("1: a\n\"1\": b\n");
    }

    #[test]
    fn yaml_bool_and_string_true_keys_both_save() {
        yaml_round_trip("true: a\n'true': b\nnull: c\n'null': d\n");
    }

    #[test]
    fn yaml_tagged_keys_keep_their_type_and_tag() {
        let text = "!!str 1: a\n1: b\n!k x: c\nx: d\n";
        yaml_round_trip(text);
        let t = Tree::from_text(text, Format::Yaml).unwrap();
        let labels: Vec<&str> = (0..t.row_count()).map(|i| t.label(i)).collect();
        assert!(
            labels.contains(&"!k x"),
            "a tagged key shows its tag: {labels:?}"
        );
    }

    #[test]
    fn yaml_complex_keys_both_save() {
        // Both keys are labelled \`?\`; they used to be saved as one string key "?".
        yaml_round_trip("? [a, b]\n: one\n? {c: d}\n: two\n");
    }

    #[test]
    fn editing_one_of_two_alike_keys_changes_only_that_entry() {
        let mut t = Tree::from_text("1: a\n\"1\": b\n", Format::Yaml).unwrap();
        assert_eq!((t.label(1), t.label(2)), ("1", "1"));
        t.handle_key(code(KeyCode::End), 8);
        type_value(&mut t, "z");
        assert_eq!((t.value(1), t.value(2)), ("a", "z"));
        same_yaml("1: a\n'1': z\n", &t.to_text().unwrap());
    }

    #[test]
    fn json_escaped_and_plain_names_are_the_same_key() {
        // RFC 8259 compares names after unescaping, so the escape for U+0061
        // is \`a\`: a document naming it twice is refused on open, never
        // half-saved.
        let bs = char::from(92u8);
        let twice = format!("{{\"a\": 1, \"{bs}u0061\": 2}}");
        let Err(err) = Tree::from_text(&twice, Format::Json) else {
            panic!("a repeated JSON name must not open");
        };
        assert!(err.to_string().contains("duplicate"), "{err}");
        let once = format!("{{\"{bs}u0061\": 2}}");
        let t = Tree::from_text(&once, Format::Json).unwrap();
        assert_eq!(t.label(1), "a");
        assert_eq!(t.save_text().unwrap(), "{\n  \"a\": 2\n}\n");
    }

    #[test]
    fn json_refuses_keys_that_would_be_written_alike() {
        for (text, key) in [
            ("1: a\n\"1\": b\n", "\"1\""),
            ("true: a\n'true': b\n", "\"true\""),
            ("!!str 1: a\n1: b\n", "\"1\""),
            ("!k x: a\nx: b\n", "\"x\""),
            ("o:\n  null: a\n  'null': b\n", "\"null\""),
        ] {
            let t = Tree::from_text(text, Format::Json).unwrap();
            match t.save_text() {
                Err(SaveError::DuplicateKey { key: k }) => assert_eq!(k, key, "{text}"),
                other => panic!("{text}: expected a duplicate key, got {other:?}"),
            }
            assert!(t.to_text().is_err(), "to_text refuses too: {text}");
        }
    }

    #[test]
    fn json_refuses_a_complex_key() {
        let t = Tree::from_text("? [a]\n: x\n", Format::Json).unwrap();
        assert!(matches!(t.save_text(), Err(SaveError::ComplexKey)));
    }

    #[test]
    fn json_writes_non_string_keys_as_names() {
        let t = Tree::from_text("1: a\ntrue: b\nnull: c\n!k x: d\n", Format::Json).unwrap();
        let out = t.save_text().unwrap();
        let back: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            back,
            serde_json::json!({"1": "a", "true": "b", "null": "c", "x": "d"}),
            "{out}"
        );
    }

    #[test]
    fn save_errors_display_in_english() {
        let dup = SaveError::DuplicateKey {
            key: "\"1\"".into(),
        };
        assert_eq!(
            dup.to_string(),
            "two entries would be saved with the key \"1\""
        );
        assert!(std::error::Error::source(&dup).is_none());
        assert!(SaveError::ComplexKey.to_string().contains("JSON"));
        let yaml_err = <serde_norway::Error as serde::ser::Error>::custom("boom");
        let wrapped = SaveError::Yaml(yaml_err);
        assert_eq!(wrapped.to_string(), "boom");
        assert!(std::error::Error::source(&wrapped).is_some());
    }

    #[test]
    fn a_unicode_line_break_in_a_key_saves_to_json_that_reopens() {
        // T854, found by the fuzz target (selector 0x00, so JSON): the YAML
        // parser reads the saved JSON back and took the raw U+2029 that
        // serde_json writes for a line break.
        let text = "{\": 1\u{2029}a\", 2tr}";
        let t = Tree::from_text(text, Format::Json).unwrap();
        let out = t.save_text().unwrap();
        assert_eq!(out, "{\n  \": 1\\u2029a\": null,\n  \"2tr\": null\n}\n");
        let back = Tree::from_text(&out, Format::Json).unwrap();
        assert_eq!(back.label(1), ": 1\u{2029}a");
        assert_eq!(back.save_text().unwrap(), out);
        let yaml = Tree::from_text(text, Format::Yaml).unwrap();
        same_yaml(text, &yaml.to_text().unwrap());
    }

    #[test]
    fn json_with_raw_breaks_and_unprintables_opens_and_round_trips() {
        for c in [
            '\u{85}', '\u{2028}', '\u{2029}', '\u{7f}', '\u{80}', '\u{9f}', '\u{fffe}', '\u{ffff}',
        ] {
            let s = format!("a{c}b");
            let want = serde_json::json!({ s.as_str(): [s.as_str()] });
            let text = want.to_string();
            assert!(text.contains(c), "serde_json writes {c:?} raw");
            let t = Tree::from_text(&text, Format::Json)
                .unwrap_or_else(|e| panic!("{c:?}: valid JSON does not open: {e}"));
            assert_eq!(t.label(1), s);
            let out = t.save_text().unwrap();
            assert!(!out.contains(c), "{c:?} is escaped: {out}");
            let got: serde_json::Value = serde_json::from_str(&out).unwrap();
            assert_eq!(got, want);
            let again = Tree::from_text(&out, Format::Json).unwrap();
            assert_eq!(again.save_text().unwrap(), out);
            let yaml = Tree::from_text(&text, Format::Yaml).unwrap();
            let back: serde_norway::Value =
                serde_norway::from_str(&yaml.save_text().unwrap()).unwrap();
            assert_eq!(back, serde_norway::to_value(&want).unwrap(), "{c:?}");
        }
    }

    #[test]
    fn a_yaml_backslash_u_text_is_not_unescaped() {
        // Only strict JSON is pre-escaped; in YAML a plain backslash-u is text.
        let t = Tree::from_text("a: x\\u2028\n", Format::Yaml).unwrap();
        assert_eq!(t.value(1), "x\\u2028");
        // An escaped backslash before a raw U+2028 stays one backslash.
        let t = Tree::from_text("{\"k\\\\\u{2028}\": 1}", Format::Json).unwrap();
        assert_eq!(t.label(1), "k\\\u{2028}");
        let out = t.save_text().unwrap();
        assert_eq!(
            Tree::from_text(&out, Format::Json).unwrap().label(1),
            "k\\\u{2028}"
        );
    }

    #[test]
    fn yaml_value_tags_are_kept_on_save() {
        // T853: value tags used to be dropped when the tree was saved.
        let text = "a: !custom x\nb: !t [1, 2]\nc: !m {d: 1}\ne: !n 5\n";
        let t = Tree::from_text(text, Format::Yaml).unwrap();
        same_yaml(text, &t.save_text().unwrap());
        assert_eq!(t.value(row_of(&t, "b")), "!t [2]");
        assert_eq!(t.value(row_of(&t, "c")), "!m {1}");
        assert_eq!(t.value(row_of(&t, "d")), "1");
        let root = Tree::from_text("!top [1]\n", Format::Yaml).unwrap();
        same_yaml("!top [1]\n", &root.save_text().unwrap());
    }

    #[test]
    fn editing_a_tagged_value_keeps_its_tag() {
        let mut t = Tree::from_text("a: !custom x\nb: !t [1]\n", Format::Yaml).unwrap();
        goto(&mut t, "a");
        t.handle_key(code(KeyCode::Enter), 8);
        assert_eq!(
            t.edit_buffer(),
            "x",
            "the edit starts from the value, not the tag"
        );
        t.handle_key(code(KeyCode::Esc), 8);
        type_value(&mut t, "y");
        assert_eq!(t.value(row_of(&t, "a")), "!custom y");
        goto(&mut t, "[0]");
        type_value(&mut t, "2");
        same_yaml("a: !custom y\nb: !t [2]\n", &t.save_text().unwrap());
        goto(&mut t, "b");
        t.handle_key(code(KeyCode::Left), 8);
        assert!(t.is_collapsed(row_of(&t, "b")), "a tagged container folds");
        t.handle_key(key('u'), 8);
        t.handle_key(key('u'), 8);
        same_yaml("a: !custom x\nb: !t [1]\n", &t.save_text().unwrap());
    }

    #[test]
    fn json_writes_a_tagged_value_as_its_value() {
        let t = Tree::from_text("a: !custom x\nb: !t [1]\n", Format::Json).unwrap();
        assert_eq!(
            t.save_text().unwrap(),
            "{\n  \"a\": \"x\",\n  \"b\": [\n    1\n  ]\n}\n"
        );
    }

    #[test]
    fn a_mapping_whose_only_key_is_tagged_is_refused_with_its_own_error() {
        for (text, key) in [
            ("!k x: 0\n", "!k x"),
            ("a:\n  !k x: 0\n", "!k x"),
            ("- !k x: 0\n", "!k x"),
            ("a: !t {!k y: 0}\n", "!k y"),
            ("? {!k z: 0}\n: 1\n", "!k z"),
        ] {
            let t = Tree::from_text(text, Format::Yaml).unwrap();
            match t.save_text() {
                Err(SaveError::LoneTaggedKey { key: k }) => assert_eq!(k, key, "{text}"),
                other => panic!("{text}: expected a lone tagged key, got {other:?}"),
            }
        }
        // With a second entry, or as JSON, the same key saves.
        yaml_round_trip("!k x: 0\ny: 1\n");
        let t = Tree::from_text("!k x: 0\n", Format::Json).unwrap();
        assert_eq!(t.save_text().unwrap(), "{\n  \"x\": 0\n}\n");
        let t = Tree::from_text("a: !T%5D0 x\nb: 1\n", Format::Yaml).unwrap();
        assert_eq!(t.value(1), "!T]0 x");
        match t.save_text() {
            Err(SaveError::UnwritableTag { tag }) => assert_eq!(tag, "!T]0"),
            other => panic!("expected an unwritable tag, got {other:?}"),
        }
        let t = Tree::from_text("!a%2Cb k: 1\nc: 2\n", Format::Yaml).unwrap();
        assert!(
            matches!(t.save_text(), Err(SaveError::UnwritableTag { .. })),
            "in a key too"
        );
        // Other escaped characters are written back escaped.
        yaml_round_trip("a: !T%25 x\nb: !%C3%A9 y\n");
        let e = SaveError::UnwritableTag { tag: "!T]0".into() };
        assert!(e.to_string().contains("!T]0"), "{e}");
        let e = SaveError::LoneTaggedKey { key: "!k x".into() };
        assert!(e.to_string().contains("(!k x)"), "{e}");
        assert!(std::error::Error::source(&e).is_none());
    }

    proptest::proptest! {
        // T854: any characters in keys and string values survive a save in
        // both formats, and the saved text reopens.
        #[test]
        fn any_text_round_trips(
            pairs in proptest::collection::vec(
                (proptest::collection::vec(proptest::char::any(), 0..6),
                 proptest::collection::vec(proptest::char::any(), 0..6)),
                1..5,
            )
        ) {
            let mut want = serde_json::Map::new();
            for (k, v) in &pairs {
                want.insert(k.iter().collect(), serde_json::Value::String(v.iter().collect()));
            }
            let want = serde_json::Value::Object(want);
            let text = want.to_string();
            let json = Tree::from_text(&text, Format::Json).unwrap();
            let out = json.save_text().unwrap();
            let got: serde_json::Value = serde_json::from_str(&out).unwrap();
            proptest::prop_assert_eq!(&got, &want);
            let again = Tree::from_text(&out, Format::Json).unwrap();
            proptest::prop_assert_eq!(again.save_text().unwrap(), out);
            let yaml = Tree::from_text(&text, Format::Yaml).unwrap();
            let saved = yaml.save_text().unwrap();
            let back: serde_norway::Value = serde_norway::from_str(&saved).unwrap();
            proptest::prop_assert_eq!(back, serde_norway::to_value(&want).unwrap());
        }
    }

    /// Keys that render alike in a row label but are different YAML keys.
    fn tricky_key(i: u8) -> serde_norway::Value {
        use serde_norway::Value as Y;
        match i % 9 {
            0 => Y::from(1),
            1 => Y::String("1".into()),
            2 => Y::Bool(true),
            3 => Y::String("true".into()),
            4 => Y::Null,
            5 => Y::String("null".into()),
            6 => Y::Tagged(Box::new(serde_norway::value::TaggedValue {
                tag: serde_norway::value::Tag::new("k"),
                value: Y::String("x".into()),
            })),
            7 => Y::String("x".into()),
            _ => Y::Sequence(vec![Y::from(i)]),
        }
    }

    proptest::proptest! {
        // T850: any mix of look-alike keys survives a YAML save, and a JSON
        // save either writes every key once or refuses.
        #[test]
        fn look_alike_keys_round_trip(keys in proptest::collection::vec(0u8..40, 0..12)) {
            let mut m = serde_norway::Mapping::new();
            for (n, k) in keys.iter().enumerate() {
                m.insert(tricky_key(*k), serde_norway::Value::from(n));
            }
            let len = m.len();
            let doc = serde_norway::Value::Mapping(m);
            // serde_norway cannot write a one-entry mapping whose key is
            // tagged (an emitter error, which the editor reports and saves
            // nothing); such documents are not generated here.
            let text = serde_norway::to_string(&doc);
            proptest::prop_assume!(text.is_ok());
            let text = text.unwrap();
            let yaml = Tree::from_text(&text, Format::Yaml).unwrap();
            let back: serde_norway::Value = serde_norway::from_str(&yaml.to_text().unwrap()).unwrap();
            proptest::prop_assert_eq!(&back, &doc);
            let json = Tree::from_text(&text, Format::Json).unwrap();
            match json.save_text() {
                Ok(out) => {
                    let back: serde_json::Map<String, serde_json::Value> =
                        serde_json::from_str(&out).unwrap();
                    proptest::prop_assert_eq!(back.len(), len);
                }
                Err(SaveError::DuplicateKey { .. } | SaveError::ComplexKey) => {}
                Err(e) => proptest::prop_assert!(false, "unexpected {e}"),
            }
        }
    }
}
