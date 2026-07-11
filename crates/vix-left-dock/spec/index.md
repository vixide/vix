# Left Dock

State for the left dock: the file explorer's lazily-expanded directory tree,
its selection, multi-selection, and scroll offset.

Pure logic over `std::fs` — the host (the `vix` app) renders the tree and
routes keys/clicks/file operations; this crate owns the tree state.

## See also

- [bottom-dock spec](../../vix-bottom-dock/spec/) — shared dock behavior
