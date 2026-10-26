# Themes

Nerd Font icons, plus a re-export of the `vix_theme_model` theme model.
Vix's colors and font attributes come from a **theme**: a JSON file describing
per-region colors. There is always exactly one active theme. The model and the
ratatui styles derived from it live in the `theme_model` crate; the host
loads themes, applies one, and persists the choice.

## Choosing a theme

**View → Theme** is a submenu listing every available theme by name; selecting
one applies it immediately and persists it. The list is built once at startup
from the available themes (de-duplicated case-insensitively — a user theme
shadows a bundled theme of the same name — then sorted alphabetically). Picking
an item dispatches the action `view.theme:<name>`.

## Where themes come from

- **Bundled** themes are compiled into the binary from `tui/themes/*.json` (via
  `include_dir`), so Dark, Light, and the other shipped themes are always present
  with no installation.
- **User** themes are JSON files in the config themes directory
  (`<config>/themes/`). They are loaded at startup and win on a name clash.

The persisted value is the theme **name** (`settings.theme`, default `"dark"`,
matched case-insensitively so the default resolves to the bundled `Dark`). On
startup the saved theme is applied; an unknown name falls back to `Dark`, then to
the first available theme.

## Theme file format

Each theme is a JSON object with a `name` and optional per-region color blocks.
Regions: `menu-bar`, `status-bar`, `left-dock`, `right-dock`, and `editor`, plus
an optional `syntax` block (`keyword`, `string`, `comment`, `number`). Colors are
`[R, G, B]` (0–255). A region may also set `font-style` (`"italic"`) and
`font-weight` (`"bold"`). The `editor` block may add a `cursor` color. Any
omitted value falls back to the primary editor color. The `syntax` colors drive
both the code editor's Tree-sitter highlighting and the DB workbench's SQL
query editor (`crates/vix-db/spec`).

```json
{
  "name": "Example",
  "menu-bar":   { "foreground": [215, 215, 215], "background": [40, 40, 40] },
  "status-bar": { "foreground": [180, 180, 180], "background": [30, 30, 30] },
  "editor":     { "foreground": [215, 215, 215], "background": [40, 40, 40],
                  "cursor": [255, 255, 255] },
  "syntax":     { "keyword": [197, 134, 192], "string": [152, 195, 121],
                  "comment": [106, 153, 85], "number": [181, 206, 168] }
}
```

## Single-color themes

Four bundled themes imitate single-color displays, and each uses one color
only:

| Theme | Color (RGB) |
| ----- | ----------- |
| Phosphor Green Monochrome | `[65, 255, 0]` |
| Phosphor Amber Monochrome | `[255, 191, 0]` |
| Phosphor White Monochrome | `[230, 230, 215]` (warm P4 white) |
| Safelight Red Monochrome | `[255, 0, 0]` |

- **One color.** Every region's foreground is that color, on a black
  background.
- **No syntax colors.** The theme has no `syntax` block, so the editor shows
  all code in the one color.
- **Reverse-video cursor.** The theme has no `cursor` color, so the cursor is a
  reversed block: a block of that color with the glyph in black.
- **The whole screen too.** Colors drawn outside the theme (the git, coverage
  and compare gutters, spell-check underlines, diagnostics, the terminal pane,
  half-block images) are repainted as well: each finished frame passes through
  `single_color::flatten`, in the terminal UI and in every GUI front end. A
  color within a small distance of the background stays background; every
  other color becomes the theme color; a colored highlight under colored text
  becomes reverse video.

A theme is treated as single-color when all five regions set the same
foreground and background and it sets no syntax and no cursor colors, so a
user theme of that shape gets the same treatment.

**Renamed in 2.0.** These were `Phosphor Green`, `Phosphor Amber` and
`Safelight Red`. A saved setting that still names an old theme resolves to its
new name (`single_color::current_name`).

`tui/tests/integration/themes.rs` pins the theme files, and the
`single_color` module's unit tests pin the repaint rule.

## As implemented in Vix

`theme_model` owns the model (`CustomTheme`, `Region`), the style helpers
(`fg`, `bg`, `base`, `title`, `region_base`, …) that the renderer calls, theme
loading (`parse_theme`, `load_custom_themes`), the active-theme state
(`set_custom`, `apply`, `custom_name`), and `theme_names` (the de-duplicated,
sorted list for the View → Theme submenu). The host gathers bundled + user themes
(`available_custom_themes`), feeds the names to the menu
(`menu::set_theme_names`), and applies a chosen theme by name. See
`crates/vix-theme-model/spec/index.md`.
