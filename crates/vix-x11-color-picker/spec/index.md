# X11 Color Picker

The X11 color table and the picker's row-selection + scroll state.

Vix's Tools menu offers an *X11 Colors* panel: a scrollable table of the
standard X11 colors, each shown as a swatch, its `#RRGGBB` hex string, and its
name. The user browses with the arrow keys (or the mouse) and inserts the
highlighted color's hex value into the active editor. This crate is pure data
— the color table is bundled as a TSV and parsed once on first use, and a
[`Panel`] tracks the highlighted row and scroll offset. The host renders the
rows, maps clicks to rows, and inserts the chosen hex.

## See also

- [ascii-character-picker spec](../../vix-ascii-character-picker/spec/) — shared character-picker behavior

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). The selected-color lookups (`selected_color`, `selected_hex`, `selected_name`) use `get` and return `Option`: a public `selected` past the last color is not moved to the nearest one, it is `None`. The app reports it.
