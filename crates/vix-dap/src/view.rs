//! The Debug panel's model (T617): threads, the selected thread's call stack,
//! the selected frame's scopes as a lazily loaded variable tree, and the
//! watches, flattened into display rows with a highlight.
//!
//! Pure state, no I/O: the host feeds it the client's [`crate::DapEvent`]s
//! and turns the [`Activation`] a key returns into the matching client call
//! ([`crate::Dap::select_thread`], [`crate::Dap::select_frame`],
//! [`crate::Dap::variables`]). Labels are data from the adapter; section
//! headings are named by [`Section`] and translated by the host.

use crate::{Frame, Scope, Thread, Variable, auto_expand_scope};

/// Most variables shown in one line's inline values.
const INLINE_MAX: usize = 8;
/// Longest single value shown inline, in characters.
const INLINE_VALUE_MAX: usize = 40;

/// A section of the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Section {
    /// The program's threads.
    Threads,
    /// The selected thread's call stack.
    Stack,
    /// The selected frame's scopes and variables.
    Variables,
    /// The watch expressions.
    Watch,
}

/// What one display row is.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RowKind {
    /// A section heading (not selectable).
    Header(Section),
    /// The "nothing here" placeholder of a section (not selectable).
    Empty(Section),
    /// A thread, by id.
    Thread(i64),
    /// A stack frame, by id.
    Frame(i64),
    /// A scope (`path` of length 1) or a variable under it: child indexes
    /// from the scope list down.
    Node(Vec<usize>),
    /// The watch at this index.
    Watch(usize),
}

/// One display row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// What the row is.
    pub kind: RowKind,
    /// Tree depth below its section (0 for top-level rows).
    pub depth: usize,
    /// The name (thread, frame, scope, variable or watch expression).
    pub label: String,
    /// The secondary text: a frame's `file:line`, a variable's or watch's value.
    pub detail: String,
    /// Whether it can be expanded (a scope or a structured variable).
    pub expandable: bool,
    /// Whether it is expanded.
    pub expanded: bool,
    /// Whether it is the selected thread or frame.
    pub current: bool,
}

impl Row {
    fn new(kind: RowKind, label: impl Into<String>) -> Self {
        Row {
            kind,
            depth: 0,
            label: label.into(),
            detail: String::new(),
            expandable: false,
            expanded: false,
            current: false,
        }
    }

    /// Whether the highlight may rest on it.
    #[must_use]
    pub fn selectable(&self) -> bool {
        !matches!(self.kind, RowKind::Header(_) | RowKind::Empty(_))
    }
}

/// What a key on the highlighted row asks the host to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Activation {
    /// Nothing more (the view already changed, or nothing applies).
    None,
    /// Select this thread on the adapter (fetch its stack).
    SelectThread(i64),
    /// Select this frame on the adapter (fetch its scopes; jump there).
    SelectFrame(i64),
    /// Fetch the children of this reference.
    Load(i64),
}

/// A scope or variable in the tree.
#[derive(Clone, Debug)]
struct Node {
    name: String,
    value: String,
    reference: i64,
    expanded: bool,
    loaded: bool,
    children: Vec<Node>,
}

impl Node {
    fn scope(s: &Scope) -> Self {
        Node {
            name: s.name.clone(),
            value: String::new(),
            reference: s.variables_reference,
            expanded: false,
            loaded: false,
            children: Vec::new(),
        }
    }

    fn variable(v: &Variable) -> Self {
        Node {
            name: v.name.clone(),
            value: v.value.clone(),
            reference: v.variables_reference,
            expanded: false,
            loaded: false,
            children: Vec::new(),
        }
    }

    /// Record the name paths of the expanded nodes under (and including) this
    /// one; `prefix` is the path of its parent.
    fn collect_expanded(
        &self,
        prefix: &mut Vec<String>,
        out: &mut std::collections::HashSet<Vec<String>>,
    ) {
        prefix.push(self.name.clone());
        if self.expanded {
            out.insert(prefix.clone());
        }
        for child in &self.children {
            child.collect_expanded(prefix, out);
        }
        prefix.pop();
    }

    /// When the node with `reference` is this one or below it, leave in
    /// `path` the names from this node down to it and return `true`.
    fn path_to(&self, reference: i64, path: &mut Vec<String>) -> bool {
        path.clear();
        self.path_inner(reference, path)
    }

    fn path_inner(&self, reference: i64, path: &mut Vec<String>) -> bool {
        path.push(self.name.clone());
        if self.reference == reference {
            return true;
        }
        if self.children.iter().any(|c| c.path_inner(reference, path)) {
            return true;
        }
        path.pop();
        false
    }

    /// The first node (depth-first, this one included) with `reference`.
    fn find_mut(&mut self, reference: i64) -> Option<&mut Node> {
        if self.reference == reference {
            return Some(self);
        }
        self.children.iter_mut().find_map(|c| c.find_mut(reference))
    }
}

/// The Debug panel's state.
#[derive(Clone, Debug, Default)]
pub struct DebugView {
    /// The threads, from the last `threads` reply.
    pub threads: Vec<Thread>,
    /// The selected thread.
    pub thread_id: Option<i64>,
    /// The selected thread's frames, top first.
    pub frames: Vec<Frame>,
    /// The selected frame.
    pub frame_id: Option<i64>,
    /// Watch expressions and their last results.
    pub watches: Vec<(String, String)>,
    /// Index of the highlighted row in [`DebugView::rows`].
    pub selected: usize,
    /// First visible row, kept by the renderer.
    pub scroll: usize,
    /// The selected frame's scopes, each the root of a variable tree.
    scopes: Vec<Node>,
    /// The scope expanded automatically (its variables feed inline values).
    auto_scope: Option<usize>,
    /// Name paths (scope name, then variable names) of the nodes that were
    /// expanded when the tree was last discarded, so a step re-expands them.
    remembered: std::collections::HashSet<Vec<String>>,
}

impl DebugView {
    /// An empty view.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The session ended: drop everything but the watch expressions, whose
    /// stale results are cleared.
    pub fn clear(&mut self) {
        let watches = std::mem::take(&mut self.watches);
        *self = DebugView {
            watches: watches
                .into_iter()
                .map(|(e, _)| (e, String::new()))
                .collect(),
            ..Self::default()
        };
    }

    /// Execution stopped in `thread_id`: a fresh stack and tree follow.
    pub fn stopped(&mut self, thread_id: i64) {
        self.thread_id = Some(thread_id);
        self.frames.clear();
        self.frame_id = None;
        self.reset_tree();
    }

    /// A `threads` reply.
    pub fn set_threads(&mut self, threads: Vec<Thread>) {
        self.threads = threads;
    }

    /// A `stackTrace` reply for the selected thread: its top frame becomes
    /// the selected one.
    pub fn set_stack(&mut self, frames: Vec<Frame>) {
        self.frame_id = frames.first().map(|f| f.id);
        self.frames = frames;
        self.reset_tree();
    }

    /// A `scopes` reply; ignored unless it is for the selected frame.
    /// Returns the references the host must fetch with `variables`: scopes
    /// that were expanded before the last step (the auto-expanded scope is
    /// fetched by the client itself).
    pub fn set_scopes(&mut self, frame_id: i64, scopes: &[Scope]) -> Vec<i64> {
        if self.frame_id != Some(frame_id) {
            return Vec::new();
        }
        self.scopes = scopes.iter().map(Node::scope).collect();
        self.auto_scope = auto_expand_scope(scopes);
        if let Some(node) = self.auto_scope.and_then(|i| self.scopes.get_mut(i)) {
            node.expanded = true;
        }
        let mut load = Vec::new();
        for (i, node) in self.scopes.iter_mut().enumerate() {
            if self.remembered.contains(std::slice::from_ref(&node.name)) && node.reference != 0 {
                node.expanded = true;
                if self.auto_scope != Some(i) {
                    load.push(node.reference);
                }
            }
        }
        load
    }

    /// A `variables` reply: the children of `reference`. Returns the
    /// references of children that were expanded before the last step, for
    /// the host to fetch in turn.
    pub fn set_variables(&mut self, reference: i64, variables: &[Variable]) -> Vec<i64> {
        let remembered = &self.remembered;
        let mut prefix = Vec::new();
        let Some(i) = self
            .scopes
            .iter()
            .position(|n| n.path_to(reference, &mut prefix))
        else {
            return Vec::new();
        };
        let Some(node) = self.scopes[i].find_mut(reference) else {
            return Vec::new();
        };
        node.children = variables.iter().map(Node::variable).collect();
        node.loaded = true;
        let mut load = Vec::new();
        for child in &mut node.children {
            let mut path = prefix.clone();
            path.push(child.name.clone());
            if child.reference != 0 && remembered.contains(&path) {
                child.expanded = true;
                load.push(child.reference);
            }
        }
        load
    }

    /// A `setVariable` reply: `name` under `reference` is now `value`.
    pub fn variable_set(&mut self, reference: i64, name: &str, value: &str) {
        if let Some(node) = self.scopes.iter_mut().find_map(|n| n.find_mut(reference))
            && let Some(child) = node.children.iter_mut().find(|c| c.name == name)
        {
            child.value = value.to_string();
        }
    }

    /// The selected frame.
    #[must_use]
    pub fn current_frame(&self) -> Option<&Frame> {
        self.frames.iter().find(|f| Some(f.id) == self.frame_id)
    }

    /// Discard the tree, remembering which nodes were expanded (an empty
    /// tree, as between a stop and its scopes, keeps the earlier memory).
    fn reset_tree(&mut self) {
        if !self.scopes.is_empty() {
            let mut paths = std::collections::HashSet::new();
            for scope in &self.scopes {
                scope.collect_expanded(&mut Vec::new(), &mut paths);
            }
            self.remembered = paths;
        }
        self.scopes.clear();
        self.auto_scope = None;
    }

    fn node(&self, path: &[usize]) -> Option<&Node> {
        let (first, rest) = path.split_first()?;
        let mut node = self.scopes.get(*first)?;
        for i in rest {
            node = node.children.get(*i)?;
        }
        Some(node)
    }

    fn node_mut(&mut self, path: &[usize]) -> Option<&mut Node> {
        let (first, rest) = path.split_first()?;
        let mut node = self.scopes.get_mut(*first)?;
        for i in rest {
            node = node.children.get_mut(*i)?;
        }
        Some(node)
    }

    /// The panel's rows, top to bottom: Threads (when known), Call Stack,
    /// Variables, and Watch (when there are watches).
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        if !self.threads.is_empty() {
            rows.push(Row::new(RowKind::Header(Section::Threads), ""));
            for t in &self.threads {
                let mut row = Row::new(RowKind::Thread(t.id), t.name.clone());
                row.current = Some(t.id) == self.thread_id;
                rows.push(row);
            }
        }
        rows.push(Row::new(RowKind::Header(Section::Stack), ""));
        if self.frames.is_empty() {
            rows.push(Row::new(RowKind::Empty(Section::Stack), ""));
        }
        for f in &self.frames {
            let mut row = Row::new(RowKind::Frame(f.id), f.name.clone());
            row.detail = f
                .path
                .as_deref()
                .and_then(|p| p.rsplit(['/', '\\']).next())
                .map_or(String::new(), |n| format!("{n}:{}", f.line));
            row.current = Some(f.id) == self.frame_id;
            rows.push(row);
        }
        rows.push(Row::new(RowKind::Header(Section::Variables), ""));
        if self.scopes.is_empty() {
            rows.push(Row::new(RowKind::Empty(Section::Variables), ""));
        }
        for (i, node) in self.scopes.iter().enumerate() {
            push_node(&mut rows, node, &[i]);
        }
        if !self.watches.is_empty() {
            rows.push(Row::new(RowKind::Header(Section::Watch), ""));
            for (i, (expr, result)) in self.watches.iter().enumerate() {
                let mut row = Row::new(RowKind::Watch(i), expr.clone());
                row.detail.clone_from(result);
                rows.push(row);
            }
        }
        rows
    }

    /// The highlighted row, when it is a selectable one.
    #[must_use]
    pub fn selected_row(&self) -> Option<Row> {
        self.rows()
            .into_iter()
            .nth(self.selected)
            .filter(Row::selectable)
    }

    /// Move the highlight by `delta` selectable rows (clamped at the ends).
    pub fn move_by(&mut self, delta: isize) {
        let rows = self.rows();
        let selectable: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.selectable())
            .map(|(i, _)| i)
            .collect();
        let Some(last) = selectable.len().checked_sub(1) else {
            self.selected = 0;
            return;
        };
        // Where the highlight is now among the selectable rows: the first one
        // at or after it (it may sit on a heading after the rows changed).
        let at = selectable
            .iter()
            .position(|&i| i >= self.selected)
            .unwrap_or(last);
        let target = at.saturating_add_signed(delta).min(last);
        self.selected = selectable[target];
    }

    /// Highlight the first selectable row.
    pub fn home(&mut self) {
        self.selected = 0;
        self.move_by(0);
    }

    /// Highlight the last selectable row.
    pub fn end(&mut self) {
        self.selected = usize::MAX;
        self.move_by(0);
    }

    /// Enter on the highlighted row: select a thread or frame, or toggle a
    /// scope or variable.
    pub fn activate(&mut self) -> Activation {
        let Some(row) = self.selected_row() else {
            return Activation::None;
        };
        match row.kind {
            RowKind::Thread(id) => {
                if self.thread_id == Some(id) {
                    return Activation::None;
                }
                self.stopped(id);
                Activation::SelectThread(id)
            }
            RowKind::Frame(id) => {
                self.frame_id = Some(id);
                self.reset_tree();
                Activation::SelectFrame(id)
            }
            RowKind::Node(path) => self.toggle(&path, !row.expanded),
            _ => Activation::None,
        }
    }

    /// Right: expand the highlighted scope or variable (or act like Enter on
    /// a thread or frame).
    pub fn expand(&mut self) -> Activation {
        match self.selected_row().map(|r| (r.kind, r.expanded)) {
            Some((RowKind::Node(path), false)) => self.toggle(&path, true),
            Some((RowKind::Thread(_) | RowKind::Frame(_), _)) => self.activate(),
            _ => Activation::None,
        }
    }

    /// Left: collapse the highlighted scope or variable, or move to its parent.
    pub fn collapse(&mut self) {
        let Some(RowKind::Node(path)) = self.selected_row().map(|r| r.kind) else {
            return;
        };
        if let Some(node) = self.node_mut(&path)
            && node.expanded
        {
            node.expanded = false;
            return;
        }
        if path.len() > 1 {
            let parent = RowKind::Node(path[..path.len() - 1].to_vec());
            if let Some(i) = self.rows().iter().position(|r| r.kind == parent) {
                self.selected = i;
            }
        }
    }

    fn toggle(&mut self, path: &[usize], expand: bool) -> Activation {
        let Some(node) = self.node_mut(path) else {
            return Activation::None;
        };
        if node.reference == 0 {
            return Activation::None;
        }
        node.expanded = expand;
        if expand && !node.loaded {
            Activation::Load(node.reference)
        } else {
            Activation::None
        }
    }

    /// The highlighted variable as `(container reference, name, value)`, for
    /// `setVariable`; `None` on anything but a variable.
    #[must_use]
    pub fn selected_variable(&self) -> Option<(i64, String, String)> {
        let Some(RowKind::Node(path)) = self.selected_row().map(|r| r.kind) else {
            return None;
        };
        let (_, parent) = path.split_last()?;
        if parent.is_empty() {
            return None; // a scope, not a variable
        }
        let container = self.node(parent)?;
        let node = self.node(&path)?;
        Some((container.reference, node.name.clone(), node.value.clone()))
    }

    /// The inline values for a stopped line whose text is `line`: the
    /// auto-expanded scope's variables whose names occur in it as whole
    /// identifiers, as `name = value` joined by two spaces. `None` when none
    /// do.
    #[must_use]
    pub fn inline_values(&self, line: &str) -> Option<String> {
        let scope = self.scopes.get(self.auto_scope?)?;
        let idents = identifiers(line);
        let parts: Vec<String> = scope
            .children
            .iter()
            .filter(|v| idents.iter().any(|i| *i == v.name))
            .take(INLINE_MAX)
            .map(|v| format!("{} = {}", v.name, shorten(&v.value)))
            .collect();
        (!parts.is_empty()).then(|| parts.join("  "))
    }
}

/// Push `node` and, when expanded, its descendants.
fn push_node(rows: &mut Vec<Row>, node: &Node, path: &[usize]) {
    let mut row = Row::new(RowKind::Node(path.to_vec()), node.name.clone());
    row.depth = path.len() - 1;
    row.detail.clone_from(&node.value);
    row.expandable = node.reference != 0;
    row.expanded = row.expandable && node.expanded;
    let expanded = row.expanded;
    rows.push(row);
    if expanded {
        for (i, child) in node.children.iter().enumerate() {
            let mut p = path.to_vec();
            p.push(i);
            push_node(rows, child, &p);
        }
    }
}

/// The identifiers in `text` (runs of alphanumerics, `_` and `$`).
fn identifiers(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .filter(|s| !s.is_empty())
        .collect()
}

/// `value`'s first line, cut to [`INLINE_VALUE_MAX`] characters.
fn shorten(value: &str) -> String {
    let first = value.lines().next().unwrap_or("");
    if first.chars().count() <= INLINE_VALUE_MAX {
        return first.to_string();
    }
    let mut out: String = first.chars().take(INLINE_VALUE_MAX - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view stopped in thread 1 of two, at frame 10 of two, with `Locals`
    /// (reference 5, auto-expanded and loaded) and `Globals` (6, expensive).
    fn stopped_view() -> DebugView {
        let mut v = DebugView::new();
        v.stopped(1);
        v.set_threads(vec![Thread::new(1, "main"), Thread::new(2, "worker")]);
        v.set_stack(vec![
            Frame::new(10, "inner", Some("/src/a.py".into()), 3),
            Frame::new(11, "outer", Some("/src/a.py".into()), 9),
        ]);
        v.set_scopes(
            10,
            &[
                Scope::new("Locals", 5),
                Scope::new("Globals", 6).expensive(),
            ],
        );
        v.set_variables(
            5,
            &[
                Variable::new("x", "1"),
                Variable::new("p", "Point").with_reference(7),
            ],
        );
        v
    }

    fn kinds(v: &DebugView) -> Vec<RowKind> {
        v.rows().into_iter().map(|r| r.kind).collect()
    }

    #[test]
    fn rows_list_threads_frames_and_the_auto_expanded_scope() {
        let v = stopped_view();
        assert_eq!(
            kinds(&v),
            vec![
                RowKind::Header(Section::Threads),
                RowKind::Thread(1),
                RowKind::Thread(2),
                RowKind::Header(Section::Stack),
                RowKind::Frame(10),
                RowKind::Frame(11),
                RowKind::Header(Section::Variables),
                RowKind::Node(vec![0]),
                RowKind::Node(vec![0, 0]),
                RowKind::Node(vec![0, 1]),
                RowKind::Node(vec![1]),
            ]
        );
        let rows = v.rows();
        assert!(rows[1].current && !rows[2].current);
        assert_eq!(rows[4].detail, "a.py:3");
        assert!(rows[4].current);
        assert!(rows[7].expanded, "Locals is auto-expanded");
        assert!(!rows[10].expanded, "an expensive scope is not");
        assert_eq!((rows[8].depth, rows[8].detail.as_str()), (1, "1"));
        assert!(!rows[8].expandable && rows[9].expandable);
    }

    #[test]
    fn expansion_is_kept_across_a_step_by_name() {
        let mut v = stopped_view();
        v.selected = 9; // p
        assert_eq!(v.activate(), Activation::Load(7));
        v.set_variables(7, &[Variable::new("y", "4").with_reference(8)]);
        // Step: a fresh stop with new references for the same names.
        v.stopped(1);
        v.set_stack(vec![Frame::new(12, "inner", Some("/src/a.py".into()), 4)]);
        let load = v.set_scopes(12, &[Scope::new("Locals", 15)]);
        assert!(load.is_empty(), "the auto scope is fetched by the client");
        let load = v.set_variables(
            15,
            &[
                Variable::new("x", "2"),
                Variable::new("p", "Point").with_reference(17),
            ],
        );
        assert_eq!(load, vec![17], "p was expanded before the step");
        let load = v.set_variables(17, &[Variable::new("y", "5").with_reference(18)]);
        assert!(load.is_empty(), "y was never expanded");
        assert!(v.rows().iter().any(|r| r.detail == "5"));
        // A non-auto scope that was open is re-requested.
        let mut w = stopped_view();
        w.selected = 10; // Globals
        assert_eq!(w.activate(), Activation::Load(6));
        w.stopped(1);
        w.set_stack(vec![Frame::new(12, "inner", None, 4)]);
        let load = w.set_scopes(12, &[Scope::new("Locals", 15), Scope::new("Globals", 16)]);
        assert_eq!(load, vec![16]);
    }

    #[test]
    fn an_empty_view_shows_placeholders_that_cannot_be_selected() {
        let mut v = DebugView::new();
        assert_eq!(
            kinds(&v),
            vec![
                RowKind::Header(Section::Stack),
                RowKind::Empty(Section::Stack),
                RowKind::Header(Section::Variables),
                RowKind::Empty(Section::Variables),
            ]
        );
        v.move_by(1);
        assert_eq!(v.selected, 0);
        assert!(v.selected_row().is_none());
        assert_eq!(v.activate(), Activation::None);
    }

    #[test]
    fn navigation_skips_headings_and_clamps() {
        let mut v = stopped_view();
        v.home();
        assert_eq!(v.selected_row().map(|r| r.kind), Some(RowKind::Thread(1)));
        v.move_by(2);
        assert_eq!(v.selected_row().map(|r| r.kind), Some(RowKind::Frame(10)));
        v.move_by(-1);
        assert_eq!(v.selected_row().map(|r| r.kind), Some(RowKind::Thread(2)));
        v.move_by(-99);
        assert_eq!(v.selected_row().map(|r| r.kind), Some(RowKind::Thread(1)));
        v.end();
        assert_eq!(
            v.selected_row().map(|r| r.kind),
            Some(RowKind::Node(vec![1]))
        );
        v.move_by(5);
        assert_eq!(
            v.selected_row().map(|r| r.kind),
            Some(RowKind::Node(vec![1]))
        );
    }

    #[test]
    fn expanding_a_structured_variable_loads_it_lazily_once() {
        let mut v = stopped_view();
        v.selected = 9; // p
        assert_eq!(v.activate(), Activation::Load(7));
        assert!(v.rows()[9].expanded);
        v.set_variables(
            7,
            &[
                Variable::new("x", "3"),
                Variable::new("y", "4").with_reference(8),
            ],
        );
        assert_eq!(v.rows()[10].kind, RowKind::Node(vec![0, 1, 0]));
        assert_eq!(v.rows()[10].depth, 2);
        // Deeper still: y has children of its own.
        v.selected = 11;
        assert_eq!(v.expand(), Activation::Load(8));
        v.set_variables(8, &[Variable::new("z", "5")]);
        assert_eq!(v.rows()[12].depth, 3);
        // Collapse and re-expand: already loaded, nothing to fetch.
        v.selected = 9;
        v.collapse();
        assert!(!v.rows()[9].expanded);
        assert_eq!(v.expand(), Activation::None);
        assert_eq!(v.rows()[10].label, "x");
        // Left on a child moves to its parent.
        v.selected = 10;
        v.collapse();
        assert_eq!(v.selected, 9);
        // A leaf does nothing.
        v.selected = 8;
        assert_eq!(v.activate(), Activation::None);
    }

    #[test]
    fn selecting_a_thread_or_frame_resets_what_hangs_below_it() {
        let mut v = stopped_view();
        v.selected = 5; // frame 11
        assert_eq!(v.activate(), Activation::SelectFrame(11));
        assert_eq!(v.frame_id, Some(11));
        assert_eq!(v.current_frame().map(|f| f.line), Some(9));
        assert!(
            v.rows()
                .iter()
                .any(|r| r.kind == RowKind::Empty(Section::Variables))
        );
        // A late scopes reply for the old frame is ignored.
        v.set_scopes(10, &[Scope::new("Locals", 5)]);
        assert!(
            v.rows()
                .iter()
                .any(|r| r.kind == RowKind::Empty(Section::Variables))
        );
        v.set_scopes(11, &[Scope::new("Locals", 15)]);
        assert!(v.rows().iter().any(|r| r.kind == RowKind::Node(vec![0])));

        v.selected = 2; // thread 2
        assert_eq!(v.activate(), Activation::SelectThread(2));
        assert_eq!(v.thread_id, Some(2));
        assert_eq!(v.frames, Vec::<Frame>::new());
        // The selected thread again: nothing to do.
        v.selected = 2;
        assert_eq!(v.activate(), Activation::None);
    }

    #[test]
    fn set_variable_targets_variables_not_scopes() {
        let mut v = stopped_view();
        v.selected = 7; // Locals
        assert_eq!(v.selected_variable(), None);
        v.selected = 8; // x
        assert_eq!(v.selected_variable(), Some((5, "x".into(), "1".into())));
        v.variable_set(5, "x", "99");
        assert_eq!(v.rows()[8].detail, "99");
    }

    #[test]
    fn inline_values_match_whole_identifiers_on_the_line() {
        let mut v = stopped_view();
        assert_eq!(
            v.inline_values("y = x + p.x"),
            Some("x = 1  p = Point".into())
        );
        assert_eq!(v.inline_values("xx = px"), None);
        v.set_variables(5, &[Variable::new("s", "a".repeat(60))]);
        let shown = v.inline_values("print(s)").unwrap_or_default();
        assert!(shown.ends_with('…') && shown.chars().count() == 4 + INLINE_VALUE_MAX);
        assert_eq!(DebugView::new().inline_values("x"), None);
    }

    #[test]
    fn clear_keeps_watch_expressions_only() {
        let mut v = stopped_view();
        v.watches.push(("x".into(), "1".into()));
        assert!(v.rows().iter().any(|r| r.kind == RowKind::Watch(0)));
        v.clear();
        assert_eq!(v.watches, vec![("x".to_string(), String::new())]);
        assert!(v.threads.is_empty() && v.frames.is_empty());
    }
}
