# Left Dock

Left dock: the file explorer's lazily-expanded directory tree + selection.
This crate holds that state, plus multi-selection and scroll offset.

Pure logic over `std::fs` — the host (the `vix` app) renders the tree and
routes keys/clicks/file operations; this crate owns the tree state.

## See also

- [bottom-dock spec](../../vix-bottom-dock/spec/) — shared dock behavior

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Row selection and tree depth use `checked_*`; the old `saturating_sub` calls are gone.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `DOCK-F1` | `len - 1` on an empty tree | row `0` | An empty tree has no row to select, and row 0 is the neutral position. |
| `DOCK-F2` | `depth + 1` does not fit | the directory is not walked deeper | A depth that cannot grow cannot be a real directory depth. |
