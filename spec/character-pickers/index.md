# Character & Color Pickers

**Status:** Shipped — four overlay pickers in the **Tools** menu insert glyphs,
characters, color values, and HTML references into the active editor.

Vix's **Tools** menu offers four small overlays for finding and inserting
hard-to-type characters and values without leaving the keyboard:

| Tools menu entry | Action | What a selection inserts |
|-|-|-|
| Nerd Font Characters… | `tools.nerd_palette` | A single Nerd Font glyph (e.g. `\u{f015}`) |
| ASCII Characters… | `tools.ascii` | The literal ASCII character for the highlighted code |
| HTML Characters… | `tools.html_chars` | The entity's rendered glyph (or a chosen cell — see below) |
| X11 Colors… | `tools.x11_colors` | The color's hex string, e.g. `#F0F8FF` |

All four share the same overlay conventions:

- Open from the **Tools** menu (or by running the action).
- **Enter** inserts the highlighted item and **keeps the picker open**, so several
  items can be picked in a row.
- A **left click** on an item highlights and inserts it (also leaving the picker
  open).
- **Esc** closes the picker.
- Inserting into a tab with no editable buffer (e.g. an image tab) is a **no-op**;
  the editor's `insert_str` reports whether anything was inserted, and the status
  line is only updated when it was.

The picker crates are **pure data** — they own the item tables and the
selection/scroll state, and expose what to insert. The host (`src/app.rs`,
`src/ui.rs`) renders each overlay, maps key and mouse events to it, and performs
the insertion.

## Nerd Font Characters

A grid of curated [Nerd Font](https://www.nerdfonts.com/) icon glyphs. The
overlay shows a fixed-width grid (8 columns), the highlighted glyph's name, and a
key hint.

- **Navigation:** the arrow keys move the highlight within the grid (`←` `↑` `↓`
  `→`), clamping at the edges. There is no text filter — the set is small and
  curated.
- **Selection inserts** the highlighted glyph (a single `char`, typically from a
  font's private-use area) at the cursor.
- **Mouse:** a click maps to a grid cell using the per-cell column width
  (`ui::NERD_CELL_W`) and inserts that glyph.

The glyphs are drawn from the common Nerd Font ranges (Font Awesome, Devicons,
Powerline, Octicons) that virtually every patched font ships, so the picker is
useful regardless of which Nerd Font the terminal uses. A glyph a particular font
lacks simply renders as a fallback box; nothing breaks.

See the per-crate spec: `nerd_font_picker/spec/index.md`.

## ASCII Characters

A scrollable table of the 128 ASCII codes (`0..=127`). Each row shows three
columns:

| Column | Example | Notes |
|-|-|-|
| Dec | `65` | The decimal value |
| Hex | `41` | Two-digit uppercase hexadecimal (`0F`, `7F`) |
| Char | `A` | A control mnemonic (`NUL`, `ESC`, `DEL`), the word `space` for 32, or the literal glyph |

- **Navigation:** the arrow keys move one row; **PageUp/PageDown** move a page (the
  visible height); **Home/End** jump to the first/last row. The window scrolls to
  keep the highlight in view.
- **Selection inserts** the highlighted row's *actual* character
  (`char::from(code)`) — so picking the `ESC` row inserts the real escape
  character, not the letters `ESC`. The `Char` column is only the human-readable
  label.
- **Mouse:** a click selects the clicked row and inserts its character.

See the per-crate spec: `ascii_character_picker/spec/index.md`.

## HTML Characters

A scrollable table of the HTML named character references (well over 1,000
entries). Each row shows three cells:

| Cell | Example | Notes |
|-|-|-|
| Glyph | `Á` | The rendered character(s) the entity expands to |
| Name | `Aacute;` | The entity name (carries its trailing `;` when present) |
| Code | `U+000C1` | The Unicode code point label |

- **Navigation:** the arrow keys move one row; **PageUp/PageDown** move a page;
  **Home/End** jump to the first/last row.
- **Selection (keyboard):** **Enter** inserts the highlighted entity's **rendered
  glyph** — the keyboard equivalent of clicking the glyph cell.
- **Selection (mouse):** a click is *cell-aware* — it inserts just the text of the
  cell under the pointer, so you can insert the glyph, the entity name (e.g.
  `Aacute;`), or the code, depending on which column you click. (The cell is chosen
  by the click's relative column via `html_cell_at`.)

Note: the crate also exposes an entity `reference()` helper that prepends `&` to
the name (e.g. `&Aacute;`); the shipped picker inserts the glyph or the chosen
cell's text rather than the `&name;` reference.

See the per-crate spec: `html_character_picker/spec/index.md`.

## X11 Colors

A scrollable table of the standard X11 colors (well over 100 entries). Each row
shows a color swatch, the `#RRGGBB` hex string, and the color name.

- **Navigation:** the arrow keys move one row; **PageUp/PageDown** move a page;
  **Home/End** jump to the first/last row.
- **Selection inserts** the highlighted color's **hex string** (e.g. `#F0F8FF` for
  `AliceBlue`) at the cursor.
- **Mouse:** a click selects the clicked row and inserts its hex.

See the per-crate spec: `x11_color_picker/spec/index.md`.

## As implemented in Vix

**Status:** Shipped. Each picker is backed by a small internal crate of pure data
plus selection state, and is wired into the host via a Tools-menu action:

| Picker | Action | Crate | Host type |
|-|-|-|-|
| Nerd Font Characters | `tools.nerd_palette` | `nerd_font_picker` | `NerdPalette` |
| ASCII Characters | `tools.ascii` | `ascii_character_picker` | `AsciiPanel` |
| HTML Characters | `tools.html_chars` | `html_character_picker` | `HtmlPanel` |
| X11 Colors | `tools.x11_colors` | `x11_color_picker` | `X11Panel` |

- The Tools menu entries live in `src/menu.rs` (`menu.item.tools.nerd_palette`,
  `…ascii`, `…html_chars`, `…x11_colors`).
- The actions dispatch in `src/app.rs` to `open_nerd_palette`,
  `open_ascii_panel`, `open_html_panel`, and `open_x11_panel`, each storing an
  `Option<…>` overlay on the `App`.
- While an overlay is open, the host routes keys (`nerd_key`, `ascii_key`,
  `x11_key`, `html_key`) and mouse events (`nerd_mouse`, `ascii_mouse`,
  `x11_mouse`, `html_mouse`) to it before the editor sees them.
- Insertion goes through `insert_selected_glyph`, `insert_selected_ascii`,
  `insert_selected_x11`, and `insert_selected_html`, which call the editor's
  `insert_str` and update the status line (`status.ascii_inserted`,
  `status.x11_inserted`, `status.html_inserted`) only when something was inserted.
- The Nerd Font grid is 8 columns wide; navigation and the mouse hit-test share
  that width and the per-cell width `ui::NERD_CELL_W`. The ASCII, HTML, and X11
  panels use a row index plus a scroll offset, with `ensure_visible` keeping the
  highlight inside the rendered window.
- The X11 and HTML color/character tables are bundled as TSV
  (`x11-color-list.tsv`, `html-character-list.tsv`) and parsed once on first use.
