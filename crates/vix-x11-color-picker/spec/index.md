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

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). The selected-color lookup uses `get`; the old `saturating_sub` is gone.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `X11-F1` | `selected` is past the last color | the last color (black if the table were empty) | A caller who sets the public field too far still gets a real color. |
