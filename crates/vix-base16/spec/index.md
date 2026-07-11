# Base16

Themes generated from [base16](https://github.com/chriskempson/base16)
palettes.

A base16 scheme is 16 colors (`base00`–`base0F`): `base00`–`base07` run dark
to light (backgrounds → foregrounds), `base08`–`base0F` are accents. From that
compact data a whole [`CustomTheme`] falls out, so this module ships a dozen
well-known schemes from a small table rather than a JSON file each. The themes
are merged into the bundled set, so they appear in View → Theme automatically.

Names are prefixed `Base16 …` to avoid colliding with the hand-authored JSON
themes in `themes/`.
