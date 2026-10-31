# Structural Selection (Offline)

Part of Expand Selection (`lsp.expand_selection`).

When the file has no language server, Expand Selection falls back to the Tree-sitter parse tree: it grows the selection to the smallest enclosing node, climbing to the parent so repeats keep growing.

Shrink Selection then works offline too (T616): every offline expansion — and every textobject selection (`edit.select_function_inside` and friends, Vi's `vaf`) — pushes the previous selection on an expansion stack, and Shrink Selection pops it. The stack is only trusted while the buffer revision and the selection are exactly what the last step left; after an edit, a cursor move or a selection change it is dropped and Shrink Selection says there is nothing to shrink back to.

`Code::expand_to_node`; `App::request_selection_range` chooses LSP or the offline fallback; the stack is `App::node_expansions` (`tui/src/app/textobjects.rs`). See `crates/vix-editor/spec/index.md`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
