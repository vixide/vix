# Call Hierarchy (Incoming Calls)

Editor action `lsp.call_hierarchy`.

List the callers of the symbol under the cursor via LSP, shown in the references jump list. Implemented as the two-step `textDocument/prepareCallHierarchy` then `callHierarchy/incomingCalls` flow.

From **Tools -> Language Server -> Call Hierarchy**. `App::call_hierarchy`; parsers `first_call_hierarchy_item` / `parse_incoming_calls`. See `spec/actions/index.md`.

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
