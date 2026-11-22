# Base16

base16-generated themes for the theme system. Themes generated from
[base16](https://github.com/chriskempson/base16) palettes.

A base16 scheme is 16 colors (`base00`–`base0F`): `base00`–`base07` run dark
to light (backgrounds → foregrounds), `base08`–`base0F` are accents. From that
compact data a whole [`CustomTheme`] falls out, so this module ships a dozen
well-known schemes from a small table rather than a JSON file each. The themes
are merged into the bundled set, so they appear in View → Theme automatically.

Names are prefixed `Base16 …` to avoid colliding with the hand-authored JSON
themes in `tui/themes/`.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). The palette slot lookup uses `get`.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `BASE16-F1` | A slot index is outside 0..16 | black | `rgb` already renders malformed hex as black; the callers pass constants below 16, so this never happens. |
