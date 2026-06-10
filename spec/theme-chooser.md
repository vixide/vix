# Theme chooser

Vix has two layers of theming:

1. **Built-in modes** — two monochrome themes baked into the binary.
2. **JSON themes** — colorful, per-region themes loaded from JSON files (a bundled
   collection plus any the user installs).

The **default** theme is Dark.

Pick a theme live in **View → Theme…**: arrow keys preview it, Enter applies and
persists it (to the `theme` setting), Esc cancels. The chooser lists the built-in
modes and every JSON theme together, sorted alphabetically (see Theme chooser).

## Built-in modes

Emphasis comes from **dim** (secondary/hint text) and full intensity (titles),
never from hue or weight.

## Theme chooser

All themes — the built-in modes **and** the JSON themes — are sorted together
alphabetically by their canonical (English) name. For example:

Dark, Darker, Darkest, Dracula, Gruvbox Dark, Light, Lighter, Lightest, Matrix,
Monokai, Nord, One Dark, Solarized Dark, Solarized Light, Tokyo Night, Turbo.

## JSON themes

JSON themes provide per-region colors and font attributes, layered over the
built-in defaults. Anything a JSON theme leaves unset falls back to the active
built-in mode, so a partial theme is valid.

Colors are `[R, G, B]` arrays, each channel `0–255`.

### Locations and precedence

- **Bundled:** the themes in `../themes/*.json` are embedded in the binary and
  appear in the chooser automatically (no installation). The bundled set is:
  `Darker`, `Darkest`, `Lighter`, `Lightest`, `Matrix`, `Turbo`,
  `Solarized Dark`, `Solarized Light`, `Dracula`, `Nord`, `Gruvbox Dark`,
  `Monokai`, `One Dark`, and `Tokyo Night`.
- **User-installed:** JSON files in `~/.config/vix/themes/` (platform config
  directory). Edit or add files here directly.
- A user-installed theme **overrides** a bundled one of the same name. A JSON
  theme named `Dark` or `Light` is ignored in favor of the built-in mode (so the
  chooser shows no duplicates).

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
- `syntax` — optional token colors: `keyword`, `string`, `comment`. The built-in
  monochrome modes leave this empty (no token colors); colors appear only under a
  JSON theme that sets them.

Unlike the built-in modes, JSON themes may use arbitrary colors, italic, and
bold (via the per-region `font-style` / `font-weight`).
