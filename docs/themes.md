# Themes

Vix ships two built-in themes and supports user-defined custom themes loaded from
JSON. Pick a theme live in **View → Theme…** (↑↓ to preview, Enter to apply, Esc
to cancel). The choice is saved to the `theme` setting.

## Built-in themes

The built-ins are intentionally **monochrome** (see `spec/theme-chooser.md`):

- **Dark** (default) — white foreground on a black background.
- **Light** — black foreground on a white background.

Rules that keep them clean and legible on any terminal:

- No colors other than the one foreground and one background.
- No italics.
- No reversed video, except for the selection (and the editor's block cursor).

Emphasis comes from **bold** (focused titles) and **dim** (hints, secondary
text) intensity, never from hue.

## Custom themes

Custom themes are JSON files in the themes directory:

```
~/.config/vix/themes/<name>.json     # Linux (and platform equivalents)
```

### Ready-made themes

A set of themes is **bundled into the binary** (from the repo's `themes/`
directory) and appears in **View → Theme…** automatically — no installation
needed: `Darker`, `Darkest`, `Lighter`, `Lightest`, `Matrix`, `Turbo`,
`Solarized Dark`, `Solarized Light`, `Dracula`, `Nord`, `Gruvbox Dark`,
`Monokai`, `One Dark`, and `Tokyo Night`. (The bundled `Dark`/`Light` files are
hidden because the built-in monochrome Dark/Light modes already cover them.)

A theme you install in your own themes directory **overrides** a bundled one of
the same name, so you can customize any of them by dropping an edited copy there.

### Writing your own

Each file defines per-region colors. Any region, channel, or section you omit
falls back to the active built-in monochrome theme, so a partial theme is valid.
Colors are `[R, G, B]` arrays, each channel `0–255`.

```json
{
  "name": "my-theme",
  "menu-bar":   { "foreground": [212, 212, 212], "background": [30, 30, 30] },
  "status-bar": { "foreground": [212, 212, 212], "background": [30, 30, 30] },
  "left-dock":  { "foreground": [212, 212, 212], "background": [30, 30, 30] },
  "right-dock": { "foreground": [212, 212, 212], "background": [30, 30, 30] },
  "editor": {
    "foreground":  [212, 212, 212],
    "background":  [30, 30, 30],
    "cursor":      [82, 139, 255],
    "font-style":  "normal",
    "font-weight": "normal"
  },
  "syntax": {
    "keyword": [86, 156, 214],
    "string":  [206, 145, 120],
    "comment": [106, 153, 85]
  }
}
```

### Regions

| Key          | Region                                  |
| ------------ | --------------------------------------- |
| `menu-bar`   | The top menu bar.                       |
| `status-bar` | The bottom status bar.                  |
| `left-dock`  | The file explorer drawer.               |
| `right-dock` | The message drawer.                     |
| `editor`     | The center text area.                   |

Each region takes `foreground` and `background`, and optional font attributes:

| Field         | Values                          | Default    |
| ------------- | ------------------------------- | ---------- |
| `foreground`  | `[R, G, B]`                     | theme fg   |
| `background`  | `[R, G, B]`                     | theme bg   |
| `font-style`  | `"normal"` or `"italic"`        | `"normal"` |
| `font-weight` | `"normal"` or `"bold"`          | `"normal"` |

`editor` additionally takes a `cursor` color (drawn as the block-cursor cell).
The optional `syntax` block colors Tree-sitter tokens — `keyword`, `string`, and
`comment` are recognized.

The built-in monochrome themes use no italic, no bold, and no token colors, so
those effects appear only under a custom theme that opts in.

### Selecting a custom theme

Drop a file in the themes directory and reopen **View → Theme…**: it appears in
the list after Dark and Light. Selecting it saves its `name` to the `theme`
setting, so it is restored on the next launch. If a saved custom theme name can
no longer be found, Vix falls back to Dark.

## See also

- `spec/theme-chooser.md` — the theme specification (source of truth).
- [configuration.md](configuration.md) — the `theme` setting and file locations.
