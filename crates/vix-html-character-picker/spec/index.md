# Html Character Picker

The HTML named-character table and the picker's row-selection + scroll state.

Vix's Tools menu offers an *HTML Characters* panel: a scrollable table of the
HTML named character references, each shown as its rendered glyph, its entity
name, and its Unicode code point. The user browses with the arrow keys (or the
mouse) and inserts the highlighted entity reference (`&name;`) into the active
editor. This crate is pure data — the table is bundled as a TSV and parsed
once on first use, and a [`Panel`] tracks the highlighted row and scroll
offset. The host renders the rows, maps clicks to rows, and inserts the chosen
reference.

## See also

- [ascii-character-picker spec](../../vix-ascii-character-picker/spec/) — shared character-picker behavior
