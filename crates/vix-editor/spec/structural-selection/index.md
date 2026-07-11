# Structural Selection (Offline)

Part of Expand Selection (`lsp.expand_selection`).

When the file has no language server, Expand Selection falls back to the Tree-sitter parse tree: it grows the selection to the smallest enclosing node, climbing to the parent so repeats keep growing (expand only; shrink still needs LSP).

`Code::expand_to_node`; `App::request_selection_range` chooses LSP or the offline fallback. See `crates/vix-editor/spec/index.md`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
