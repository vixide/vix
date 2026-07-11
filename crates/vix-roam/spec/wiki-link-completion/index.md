# Wiki-Link Completion

Triggered by typing `[[` in an Org buffer.

Typing `[[` opens a completion popup of Org-roam node titles; accepting one inserts the link, closing `]]` unless the auto-pair already added it.

`App::maybe_complete_wiki_link` / `open_node_link_completion` (reuses the LSP `CompletionPopup`). See `crates/vix-org/spec/index.md`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
