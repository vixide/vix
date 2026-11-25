# Nerd Font Picker

A curated set of [Nerd Font](https://www.nerdfonts.com/) glyphs and the
character-picker grid state.

Vix's Tools menu offers a *Nerd Font Palette*: a small grid of icon glyphs
the user can browse with the arrow keys (or the mouse) and insert into the
active editor. This crate is pure data — it lists the glyphs and tracks which
cell is highlighted in a fixed-width grid. The host renders the grid, maps
clicks to cells, and inserts the chosen glyph.

The glyphs are drawn from the common Nerd Font ranges (Font Awesome, Devicons,
Powerline, Octicons) that almost every patched font ships, so the picker shows
something useful regardless of which Nerd Font the terminal uses. A glyph that
a particular font lacks simply renders as a fallback box; nothing breaks.

## See also

- [ascii-character-picker spec](../../vix-ascii-character-picker/spec/) — shared character-picker behavior

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Navigation uses `checked_add` and `checked_sub`, and a grid position whose index does not fit is not a glyph (`select_at` returns `false`). `selected_glyph` and `selected_name` use `get` and return `Option`: a public `selected` past the last glyph is `None`, not another glyph. The app reports it, and the preview shows no name for it.
