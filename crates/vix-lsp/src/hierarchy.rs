//! Call and type hierarchy (T620): preparing an item, and asking the server
//! that prepared it for its callers, callees, supertypes or subtypes. See
//! `spec/index.md` § Call and type hierarchy.

use std::path::{Path, PathBuf};

use serde_json::Value;
use vix_lsp_core::message;

use crate::{Lsp, LspEvent, Pending, uri_to_path};

/// A prepared call- or type-hierarchy item, tagged with the language id of
/// the server that produced it (T620). The item is opaque to every other
/// server, so the follow-up request goes to that one only.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct HierarchyItem {
    /// The `language_id` of the server that prepared the item.
    pub server: String,
    /// The `CallHierarchyItem` / `TypeHierarchyItem` exactly as sent.
    pub item: Value,
}

impl HierarchyItem {
    /// Tag `item` with `server`.
    #[must_use]
    pub fn new(server: impl Into<String>, item: Value) -> Self {
        HierarchyItem {
            server: server.into(),
            item,
        }
    }
}

/// Which way a hierarchy request walks (T620).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum HierarchyDirection {
    /// Callers (`callHierarchy/incomingCalls`).
    Incoming,
    /// Callees (`callHierarchy/outgoingCalls`).
    Outgoing,
    /// Supertypes (`typeHierarchy/supertypes`).
    Supertypes,
    /// Subtypes (`typeHierarchy/subtypes`).
    Subtypes,
}

impl HierarchyDirection {
    /// The other direction of the same hierarchy: callers ⇄ callees,
    /// supertypes ⇄ subtypes.
    #[must_use]
    pub fn flipped(self) -> Self {
        match self {
            HierarchyDirection::Incoming => HierarchyDirection::Outgoing,
            HierarchyDirection::Outgoing => HierarchyDirection::Incoming,
            HierarchyDirection::Supertypes => HierarchyDirection::Subtypes,
            HierarchyDirection::Subtypes => HierarchyDirection::Supertypes,
        }
    }

    /// Whether this is a call-hierarchy direction (else a type-hierarchy one).
    #[must_use]
    pub fn is_call(self) -> bool {
        matches!(
            self,
            HierarchyDirection::Incoming | HierarchyDirection::Outgoing
        )
    }

    /// The request method and pending kind for this direction.
    fn request(self) -> (&'static str, Pending) {
        match self {
            HierarchyDirection::Incoming => ("callHierarchy/incomingCalls", Pending::IncomingCalls),
            HierarchyDirection::Outgoing => ("callHierarchy/outgoingCalls", Pending::OutgoingCalls),
            HierarchyDirection::Supertypes => ("typeHierarchy/supertypes", Pending::Supertypes),
            HierarchyDirection::Subtypes => ("typeHierarchy/subtypes", Pending::Subtypes),
        }
    }
}

impl Lsp {
    /// Step 1 of type hierarchy: prepare the type at `(line, character)`,
    /// from every server handling `path` (T123b: fans out). Each server's
    /// reply arrives as [`LspEvent::TypeHierarchyPrepared`].
    pub fn request_prepare_type_hierarchy(&mut self, path: &Path, line: u32, character: u32) {
        self.request(
            path,
            "textDocument/prepareTypeHierarchy",
            line,
            character,
            Pending::PrepareTypeHierarchy,
        );
    }

    /// Step 2 of call or type hierarchy: ask the server that prepared `item`
    /// for the results in `direction`. The reply arrives as
    /// [`LspEvent::Hierarchy`]. Nothing is sent when that server is no longer
    /// running (its items mean nothing to a restarted one).
    pub fn request_hierarchy(&mut self, item: &HierarchyItem, direction: HierarchyDirection) {
        let Some(server) = self.servers.get_mut(&item.server) else {
            return;
        };
        let (method, kind) = direction.request();
        let id = server.alloc_id();
        server.pending.insert(id, kind);
        server.send(message::request(
            id,
            method,
            &message::hierarchy_item_params(&item.item),
        ));
    }

    /// Request the incoming calls (callers) of a prepared call-hierarchy
    /// `item`, from the server that prepared it (T620, which also lifts
    /// T123b's first-server-only limit).
    pub fn request_incoming_calls(&mut self, item: &HierarchyItem) {
        self.request_hierarchy(item, HierarchyDirection::Incoming);
    }

    /// Request the outgoing calls (callees) of a prepared call-hierarchy
    /// `item`, from the server that prepared it (T620).
    pub fn request_outgoing_calls(&mut self, item: &HierarchyItem) {
        self.request_hierarchy(item, HierarchyDirection::Outgoing);
    }

    /// Request the supertypes of a prepared type-hierarchy `item`, from the
    /// server that prepared it (T620).
    pub fn request_supertypes(&mut self, item: &HierarchyItem) {
        self.request_hierarchy(item, HierarchyDirection::Supertypes);
    }

    /// Request the subtypes of a prepared type-hierarchy `item`, from the
    /// server that prepared it (T620).
    pub fn request_subtypes(&mut self, item: &HierarchyItem) {
        self.request_hierarchy(item, HierarchyDirection::Subtypes);
    }

    /// Turn a hierarchy reply from server `lang` into its event. Returns
    /// whether `kind` was a hierarchy request (handled here, `null` result
    /// included).
    pub(crate) fn hierarchy_reply(
        lang: &str,
        kind: Pending,
        result: &Value,
        events: &mut Vec<LspEvent>,
    ) -> bool {
        let direction = match kind {
            Pending::PrepareCallHierarchy | Pending::PrepareTypeHierarchy => {
                if let Some(item) = message::first_hierarchy_item(result) {
                    let item = HierarchyItem::new(lang, item);
                    events.push(if matches!(kind, Pending::PrepareCallHierarchy) {
                        LspEvent::CallHierarchyPrepared(item)
                    } else {
                        LspEvent::TypeHierarchyPrepared(item)
                    });
                }
                return true;
            }
            Pending::IncomingCalls => HierarchyDirection::Incoming,
            Pending::OutgoingCalls => HierarchyDirection::Outgoing,
            Pending::Supertypes => HierarchyDirection::Supertypes,
            Pending::Subtypes => HierarchyDirection::Subtypes,
            _ => return false,
        };
        let parsed = match direction {
            HierarchyDirection::Incoming => message::parse_incoming_calls(result),
            HierarchyDirection::Outgoing => message::parse_outgoing_calls(result),
            HierarchyDirection::Supertypes | HierarchyDirection::Subtypes => {
                message::parse_type_hierarchy_items(result)
            }
        };
        let locations: Vec<(PathBuf, u32, u32)> = parsed
            .into_iter()
            .map(|l| {
                (
                    uri_to_path(&l.uri),
                    l.range.start.line,
                    l.range.start.character,
                )
            })
            .collect();
        events.push(LspEvent::Hierarchy {
            direction,
            locations,
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn directions_flip_within_their_own_hierarchy() {
        use HierarchyDirection::{Incoming, Outgoing, Subtypes, Supertypes};
        assert_eq!(Incoming.flipped(), Outgoing);
        assert_eq!(Outgoing.flipped(), Incoming);
        assert_eq!(Supertypes.flipped(), Subtypes);
        assert_eq!(Subtypes.flipped(), Supertypes);
        assert!(Incoming.is_call() && Outgoing.is_call());
        assert!(!Supertypes.is_call() && !Subtypes.is_call());
    }

    #[test]
    fn a_prepared_item_is_tagged_with_its_server() {
        let mut events = Vec::new();
        let result = json!([{ "name": "f", "uri": "file:///a.rs" }]);
        assert!(Lsp::hierarchy_reply(
            "linter",
            Pending::PrepareTypeHierarchy,
            &result,
            &mut events
        ));
        let [LspEvent::TypeHierarchyPrepared(item)] = events.as_slice() else {
            panic!("expected one prepared type item");
        };
        assert_eq!(item.server, "linter");
        assert_eq!(item.item["name"], "f");
    }

    #[test]
    fn an_empty_hierarchy_reply_still_reports_its_direction() {
        let mut events = Vec::new();
        assert!(Lsp::hierarchy_reply(
            "rust",
            Pending::Subtypes,
            &Value::Null,
            &mut events
        ));
        assert!(matches!(
            events.as_slice(),
            [LspEvent::Hierarchy { direction: HierarchyDirection::Subtypes, locations }]
                if locations.is_empty()
        ));
        assert!(!Lsp::hierarchy_reply(
            "rust",
            Pending::Hover,
            &Value::Null,
            &mut events
        ));
    }

    #[test]
    fn hierarchy_requests_go_only_to_a_running_server() {
        // No server is running under "gone", so nothing is sent (and nothing
        // panics): an item from a server that has since stopped is dropped.
        let mut lsp = Lsp::new(true, vec![], &[]);
        let item = HierarchyItem::new("gone", json!({}));
        lsp.request_incoming_calls(&item);
        lsp.request_outgoing_calls(&item);
        lsp.request_supertypes(&item);
        lsp.request_subtypes(&item);
        assert!(!lsp.busy());
    }
}
