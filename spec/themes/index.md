# Themes

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

- **Bundled** themes are compiled into the binary from `themes/*.json` (via
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
an optional `syntax` block. Colors are `[R, G, B]` (0–255). A region may also set
`font-style` (`"italic"`) and `font-weight` (`"bold"`). The `editor` block may
add a `cursor` color. Any omitted value falls back to the primary editor color.

```json
{
  "name": "Example",
  "menu-bar":   { "foreground": [215, 215, 215], "background": [40, 40, 40] },
  "status-bar": { "foreground": [180, 180, 180], "background": [30, 30, 30] },
  "editor":     { "foreground": [215, 215, 215], "background": [40, 40, 40],
                  "cursor": [255, 255, 255] },
  "syntax":     { "keyword": [197, 134, 192], "string": [152, 195, 121],
                  "comment": [106, 153, 85] }
}
```

## As implemented in Vix

`theme_model` owns the model (`CustomTheme`, `Region`), the style helpers
(`fg`, `bg`, `base`, `title`, `region_base`, …) that the renderer calls, theme
loading (`parse_theme`, `load_custom_themes`), the active-theme state
(`set_custom`, `apply`, `custom_name`), and `theme_names` (the de-duplicated,
sorted list for the View → Theme submenu). The host gathers bundled + user themes
(`available_custom_themes`), feeds the names to the menu
(`menu::set_theme_names`), and applies a chosen theme by name. See
`theme_model/spec/index.md`.
