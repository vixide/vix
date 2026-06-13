# X11 Color Picker

A Tools-menu picker for the standard X11 colors. **Tools → X11 Colors…** opens a
scrollable table; each row shows a color swatch, the `#RRGGBB` hex string, and the
color name. Picking a row inserts its hex value into the active editor.

## As implemented in Vix

**Status:** Shipped. The color table lives in the internal `vix-x11-color-picker`
crate (the data is bundled as `x11-color-list.tsv` and parsed once on first use);
the host (`src/app.rs`, `src/ui.rs`) renders the overlay and inserts the result.

| Key / action  | Effect                                                |
| ------------- | ----------------------------------------------------- |
| `↑` / `↓`     | Move the highlight one row                            |
| `PgUp`/`PgDn` | Move one page                                         |
| `Home`/`End`  | Jump to the first / last color                        |
| `Enter`       | Insert the highlighted color's hex (panel stays open) |
| click         | Insert the clicked row's hex                          |
| `Esc`         | Close the panel                                       |

Insertion uses the `#RRGGBB` hex (e.g. `#F0F8FF`) so it drops straight into CSS,
config, or code. The panel stays open after each insert so several colors can be
picked in a row.
