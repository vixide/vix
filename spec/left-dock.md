# Left dock

The left dock is the **file explorer**: a lazily-expanded directory tree rooted at
the project directory.

**Status:** Shipped. The tree state lives in the internal `vix-left-dock` crate
(`Explorer`, `Node`); the host renders it (`src/ui.rs`), runs file operations
(`src/fileops.rs`), and routes keys/clicks (`src/app.rs`). The crate depends only
on `std` (it reads directories).

## State (`vix-left-dock`)

- `Node` — one visible row: `path`, `name`, `depth`, `is_dir`, `expanded`.
- `Explorer` — the flattened visible `nodes`, the `selected` row, the `top` scroll
  offset, the `marked` multi-selection (by path, so it survives a rebuild), and
  the set of expanded directories.
- Navigation: `up`/`down`, `page_up`/`page_down`, `first`/`last`, `extend`
  (Shift-range select), `ensure_visible(height)`.
- Tree ops: `toggle_selected` (expand/collapse), `collapse_or_parent` (the `←`
  behavior — collapse, else jump to parent; never expands), `reveal(path)`
  (expand ancestors and select), `rebuild` (re-read from disk; dotfiles hidden,
  directories first then case-insensitive name).
- `selected_paths` — the multi-selection, or the single cursor row.

See `file-explorer.md` for the full keyboard/mouse behavior.
