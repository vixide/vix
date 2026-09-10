# Theme editor

List a theme's color slots and edit each one, live-previewed on the real UI
as you go, then save the result as a new custom theme.

**Status:** Shipped (T202). **Vix → Theme → Edit Theme…** (also reachable
from the command palette) opens the editor over a copy of the active theme:
15 rows, one per color slot (menu bar / status bar / left dock / right dock
foreground+background, editor foreground/background/cursor, and the 4
syntax colors), each showing its current `#RRGGBB` value. `↑`/`↓` (or the
mouse) move the highlight; `Enter` opens the **existing X11 color picker**
(`crates/vix-x11-color-picker/spec/index.md`, the same panel Tools → X11
Colors uses to insert a hex value into a buffer) to choose the slot's new
color — the whole point of reusing it rather than building a second color
input from scratch. Picking a color there applies it to the highlighted
slot and returns to the editor.

Every change is applied live (`vix_theme_model::apply`) so the real UI
reflects the edit immediately, the same mechanism the View → Theme submenu's
hover-preview already uses. `Esc` closes the editor and reverts to the
committed theme (`settings.theme`) if nothing was saved — an edit you don't
save never persists or lingers as the active theme.

`Ctrl+S` (Save As) prompts for a name and writes the draft to
`~/.config/vix/themes/<name>.json` (`vix_theme_model::to_json`, the same
shape [`vix_theme_model::parse_theme`] reads back — round-trip verified),
then sets it as the active theme (persisted to `settings.theme`, like
choosing any other theme from the View → Theme submenu).

## As implemented in Vix

- `Panel` holds the being-edited `CustomTheme` draft, `selected`/`scroll`
  (`vix-list-state`, like every other scrollable panel), and `dirty`
  (whether anything has changed since `open`).
- `Slot` names each of the theme's 15 real color fields directly (not a
  generic lens over `CustomTheme`'s structure — 15 one-line `get`/`set`
  match arms is simpler than a path/lens abstraction would be here) and
  supplies each row's i18n label key.
- In `src/app.rs`: `open_theme_editor` snapshots the active theme;
  `theme_editor_key`'s `Enter` opens the X11 picker with a new
  `theme_editor_picking: bool` flag set, so the X11 panel's own `Enter`
  handler (`insert_selected_x11`, otherwise used by Tools → X11 Colors)
  applies the chosen color to the editor's highlighted slot instead of
  inserting it into a buffer, then closes the picker and returns to the
  editor — the same "one overlay reused for two purposes via a flag"
  pattern the workspace-search panel's `static_results` already
  establishes for go-to-definition. `theme_editor_save` prompts for a name,
  writes the JSON, and calls the same theme-setting path `set_theme_by_name`
  uses (persisted + `editor.refresh_theme()`).
