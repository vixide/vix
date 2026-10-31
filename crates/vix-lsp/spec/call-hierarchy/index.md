# Call and Type Hierarchy

Editor actions `lsp.call_hierarchy` (callers), `lsp.call_hierarchy_outgoing`
(callees), `lsp.type_hierarchy_supertypes` and `lsp.type_hierarchy_subtypes`.

List the callers, callees, supertypes or subtypes of the symbol under the
cursor via LSP, shown in the static result list (the same list Find
References uses), titled by direction. `Alt+D` in that list flips the
direction (callers ⇄ callees, supertypes ⇄ subtypes) against the same
prepared item and server. Implemented as the two-step
`textDocument/prepareCallHierarchy` then `callHierarchy/incomingCalls` /
`outgoingCalls` flow, and `textDocument/prepareTypeHierarchy` then
`typeHierarchy/supertypes` / `subtypes`. Each prepared item is tagged with the
server that produced it, and the follow-up request goes to that server only
(T620).

From **Tools -> Language Server**. `App::call_hierarchy` and the rest of
`tui/src/app/hierarchy.rs`; parsers `first_hierarchy_item` /
`parse_incoming_calls` / `parse_outgoing_calls` / `parse_type_hierarchy_items`.
Full contract: [`../index.md`](../index.md) § Call and type hierarchy. See
`crates/vix-editor-core/spec/index.md`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
