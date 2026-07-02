# Wiki-Link Completion

Triggered by typing `[[` in an Org buffer.

Typing `[[` opens a completion popup of Org-roam node titles; accepting one inserts the link, closing `]]` unless the auto-pair already added it.

`App::maybe_complete_wiki_link` / `open_node_link_completion` (reuses the LSP `CompletionPopup`). See `spec/org/index.md`.

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
