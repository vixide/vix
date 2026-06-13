# HTML Character Picker

A Tools-menu picker for the HTML named character references. **Tools → HTML
Characters…** opens a scrollable table; each row shows the rendered glyph, the
entity name (e.g. `Aacute;`), and the Unicode code point (e.g. `U+000C1`).

## As implemented in Vix

**Status:** Shipped. The table lives in the internal `vix-html-character-picker`
crate (bundled as `src/html-character-list.tsv`, parsed once on first use); the host
(`src/app.rs`, `src/ui.rs`) renders the overlay and inserts the result.

| Key / action  | Effect                                                |
| ------------- | ----------------------------------------------------- |
| `↑` / `↓`     | Move the highlight one row                            |
| `PgUp`/`PgDn` | Move one page                                         |
| `Home`/`End`  | Jump to the first / last entry                        |
| `Enter`       | Insert the highlighted row's glyph (panel stays open) |
| click a cell  | Insert just that cell's text — glyph, name, or code   |
| `Esc`         | Close the panel                                       |

**Per-cell picking.** A mouse click inserts the individual cell under the pointer,
not a fixed per-row value: clicking the glyph inserts the character (e.g. `Á`),
clicking the name inserts the entity name (`Aacute;`), and clicking the code
inserts the code point (`U+000C1`). The keyboard `Enter` inserts the glyph (the
primary cell). The panel stays open after each insert.
