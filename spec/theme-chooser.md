# Theme chooser

Every theme is a **JSON theme** with per-region colors and font attributes
(a bundled collection plus any the user installs). There are no hardcoded
"modes" — **Dark** and **Light** are ordinary bundled themes
(`themes/dark.json` / `themes/light.json`).

The **default** theme is Dark.

Pick a theme live in **View → Theme…**: arrow keys preview it, Enter applies and
persists it (the theme's name is written to the `theme` setting), Esc cancels.
The chooser lists every theme, sorted alphabetically.

## Theme chooser

All themes are sorted alphabetically (case-insensitively) by name. For example:

Dark, Darker, Darkest, Dracula, Gruvbox Dark, Light, Lighter, Lightest, Matrix,
Monokai, Nord, One Dark, Solarized Dark, Solarized Light, Tokyo Night, Turbo.

## JSON themes

JSON themes provide per-region colors and font attributes. Anything a theme leaves
unset falls back to the primary editor color, so a partial theme is valid.

Colors are `[R, G, B]` arrays, each channel `0–255`.

### Locations and precedence

- **Bundled:** the themes in `../themes/*.json` are embedded in the binary and
  appear in the chooser automatically (no installation). The bundled set is:
  `Dark`, `Light`, `Darker`, `Darkest`, `Lighter`, `Lightest`, `Matrix`, `Turbo`,
  `Solarized Dark`, `Solarized Light`, `Dracula`, `Nord`, `Gruvbox Dark`,
  `Monokai`, `One Dark`, and `Tokyo Night`.
- **User-installed:** JSON files in `~/.config/vix/themes/` (platform config
  directory). Edit or add files here directly.
- A user-installed theme **overrides** a bundled one of the same name
  (case-insensitively), so you can replace `Dark`/`Light` with your own.

### File format

```json
{
  "name": "my-theme",
  "menu-bar": { "foreground": [212, 212, 212], "background": [30, 30, 30] },
  "status-bar": { "foreground": [212, 212, 212], "background": [30, 30, 30] },
  "left-dock": { "foreground": [212, 212, 212], "background": [30, 30, 30] },
  "right-dock": { "foreground": [212, 212, 212], "background": [30, 30, 30] },
  "editor": {
    "foreground": [212, 212, 212],
    "background": [30, 30, 30],
    "cursor": [82, 139, 255],
    "font-style": "normal",
    "font-weight": "normal"
  },
  "syntax": {
    "keyword": [86, 156, 214],
    "string": [206, 145, 120],
    "comment": [106, 153, 85]
  }
}
```

- `name` — display name (also the value saved to the `theme` setting).
- Regions: `menu-bar`, `status-bar`, `left-dock` (explorer), `right-dock`
  (messages), `editor`. Each takes:
  - `foreground`, `background` — `[R, G, B]`.
  - `font-style` — `"normal"` (default) or `"italic"`.
  - `font-weight` — `"normal"` (default) or `"bold"`.
- `editor` additionally takes `cursor` — the block-cursor color.
- `syntax` — optional token colors: `keyword`, `string`, `comment`. A theme that
  omits this block gets no token colors (plain text); colors appear only when a
  theme sets them.

Themes may use arbitrary colors, italic, and bold (via the per-region
`font-style` / `font-weight`).
