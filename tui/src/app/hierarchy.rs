//! Call and type hierarchy (T620, `crates/vix-lsp/spec` § Call and type
//! hierarchy): the four `lsp.call_hierarchy*` / `lsp.type_hierarchy_*`
//! actions, the results shown in the static result list under a title naming
//! the direction, and `Alt+D` in that list flipping the direction against the
//! same prepared item and server.

use std::path::PathBuf;

use crate::lsp::{HierarchyDirection, HierarchyItem};
use vix_input::{KeyCode, KeyEvent};

use super::App;

/// The hierarchy being shown or asked for: the direction requested, and the
/// item its server prepared (once the prepare reply has arrived).
pub(crate) struct HierarchyView {
    /// The direction most recently asked for.
    direction: HierarchyDirection,
    /// The prepared item, tagged with its server; `None` until prepared.
    item: Option<HierarchyItem>,
}

impl App {
    /// Start a hierarchy request in `direction` for the symbol under the
    /// cursor: prepare it (call or type hierarchy, by direction) on every
    /// server handling the file; the reply continues in
    /// [`App::on_hierarchy_prepared`].
    pub(super) fn start_hierarchy(&mut self, direction: HierarchyDirection) {
        let Some(path) = self.active_path().filter(|p| self.lsp.handles(p)) else {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        };
        let (line, character) = self.cursor_lsp_position(&path);
        self.hierarchy = Some(HierarchyView {
            direction,
            item: None,
        });
        if direction.is_call() {
            self.lsp
                .request_prepare_call_hierarchy(&path, line, character);
        } else {
            self.lsp
                .request_prepare_type_hierarchy(&path, line, character);
        }
    }

    /// A server prepared `item` (`is_call`: a call-hierarchy item, else a
    /// type-hierarchy one): ask that server for the direction the action
    /// requested, or the hierarchy's first direction when none of that kind
    /// is pending.
    pub(super) fn on_hierarchy_prepared(&mut self, item: HierarchyItem, is_call: bool) {
        let default = if is_call {
            HierarchyDirection::Incoming
        } else {
            HierarchyDirection::Supertypes
        };
        let direction = self
            .hierarchy
            .as_ref()
            .map(|h| h.direction)
            .filter(|d| d.is_call() == is_call)
            .unwrap_or(default);
        self.lsp.request_hierarchy(&item, direction);
        self.hierarchy = Some(HierarchyView {
            direction,
            item: Some(item),
        });
    }

    /// A hierarchy reply: show its locations in the static result list,
    /// titled by direction, or say there are none (an open list is left as
    /// it was, so a flip with no results keeps the previous direction shown).
    pub(super) fn on_hierarchy_results(
        &mut self,
        direction: HierarchyDirection,
        locations: &[(PathBuf, u32, u32)],
    ) {
        let label = direction_label(direction);
        if locations.is_empty() {
            self.status = t!("status.hierarchy_none", direction = label).to_string();
            return;
        }
        self.show_references(locations);
        if let Some(view) = self.hierarchy.as_mut() {
            view.direction = direction;
        }
        if let Some(ps) = self.workspace_search.as_mut() {
            ps.title = Some(label);
            ps.status = t!("status.hierarchy_n", n = ps.hits.len()).to_string();
        }
    }

    /// Whether the open result list is a hierarchy list with a prepared item
    /// behind it (only hierarchy lists set the panel's `title`).
    fn hierarchy_list_open(&self) -> bool {
        self.hierarchy.as_ref().is_some_and(|h| h.item.is_some())
            && self
                .workspace_search
                .as_ref()
                .is_some_and(|ps| ps.title.is_some())
    }

    /// `Alt+D` in a hierarchy list: ask the same server for the other
    /// direction (callers ⇄ callees, supertypes ⇄ subtypes).
    pub(super) fn flip_hierarchy(&mut self) {
        let Some(view) = self.hierarchy.as_ref() else {
            return;
        };
        let Some(item) = view.item.clone() else {
            return;
        };
        self.lsp.request_hierarchy(&item, view.direction.flipped());
    }

    /// Keys for the workspace-search panel: `Alt+D` flips an open hierarchy
    /// list; everything else is the panel's own (`ps_key`). Closing the panel
    /// forgets the hierarchy.
    pub(super) fn workspace_search_key(&mut self, key: KeyEvent) {
        if Self::alt(&key)
            && matches!(key.code, KeyCode::Char('d' | 'D'))
            && self.hierarchy_list_open()
        {
            self.flip_hierarchy();
            return;
        }
        self.ps_key(key);
        if self.workspace_search.is_none() {
            self.hierarchy = None;
        }
    }
}

/// The translated name of `direction`, used as the list's title.
fn direction_label(direction: HierarchyDirection) -> String {
    match direction {
        HierarchyDirection::Outgoing => t!("ui.hierarchy_outgoing"),
        HierarchyDirection::Supertypes => t!("ui.hierarchy_supertypes"),
        HierarchyDirection::Subtypes => t!("ui.hierarchy_subtypes"),
        // `HierarchyDirection` is `#[non_exhaustive]`.
        _ => t!("ui.hierarchy_incoming"),
    }
    .to_string()
}
