# Left Dock

Left dock: the file explorer's lazily-expanded directory tree + selection.
This crate holds that state, plus multi-selection and scroll offset.

Pure logic over `std::fs` — the host (the `vix` app) renders the tree and
routes keys/clicks/file operations; this crate owns the tree state.

## See also

- [bottom-dock spec](../../vix-bottom-dock/spec/) — shared dock behavior
