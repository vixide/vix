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
