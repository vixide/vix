# Structural Selection (Offline)

Part of Expand Selection (`lsp.expand_selection`).

When the file has no language server, Expand Selection falls back to the Tree-sitter parse tree: it grows the selection to the smallest enclosing node, climbing to the parent so repeats keep growing (expand only; shrink still needs LSP).

`Code::expand_to_node`; `App::request_selection_range` chooses LSP or the offline fallback. See `spec/editor/index.md`.

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
