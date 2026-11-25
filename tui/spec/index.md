# Terminal shell

The `vix-tui` package is the thin App shell that builds the `vix` command: it owns the event loop, the `App` state and the ratatui rendering, and delegates every feature to a member crate. Feature behavior is specified in each member crate's own spec; this file records only what is true of the shell itself.

## Checked arithmetic

The package follows the `rust-checked-arithmetic` spec and, outside tests, the whole package (library and binary) denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Screen and text offsets go through the small `geo` helpers (`plus`, `minus`, `times`, `quot`, `modulo`, `chunk`, `later`); everything else reads through `get`, `first`, `split_at_checked` and `try_from`.

Declared fallbacks, each the correct answer to a failed operation on valid data, never a replacement for malformed input: screen geometry saturates (a sum past the top of its type is the top, a difference below zero is zero, a division by zero is zero); a layout rectangle the split did not produce is the empty rectangle, so nothing is drawn there; a menu, tab, kernel or column index that does not exist skips the operation or draws nothing (a results-grid column without a width is one cell wide); a text slice that is not on a character boundary is empty; a line or column from a language server or a collaborator that does not fit `usize` points past the end of the text and is skipped or clamped; a path buffered by a remote search that is missing from the order map is skipped; a debounce deadline that cannot be represented is now.
