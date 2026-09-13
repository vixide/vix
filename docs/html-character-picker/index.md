# HTML Character Picker

A picker for HTML named character references. You browse a scrollable table
of entities — glyph, entity name, and Unicode code point — and insert one
into the active editor.

## Opening the picker

Open it from the menu bar: **Tools → Characters → HTML Characters…**. An
overlay appears showing the table of HTML named character references, each
row rendered as its glyph, its entity name, and its Unicode code point.

## Keybindings

| Key                       | Action                                          |
| ------------------------- | ----------------------------------------------- |
| `↑` `↓`                   | Move the highlight one row                      |
| `Page Up` `Page Down`     | Move the highlight one page                     |
| `Home` `End`              | Jump to the first / last row                     |
| `Enter`                   | Insert the highlighted entity's rendered character; keep the picker open |
| `Esc`                     | Close the picker                                |

Pressing `Enter` keeps the picker open, so you can pick several entities in
a row.

## Mouse

A click highlights that row. Which text is inserted depends on which column
you click: the glyph column inserts the rendered character, the name column
inserts the bare entity name, and the code-point column inserts the code
point label — each without the surrounding `&`/`;`.

## Notes

The table is bundled as a TSV and parsed once on first use; the crate itself
is pure data (the character table plus the picker's row-selection and scroll
state) — the host renders the rows, maps clicks to columns, and performs the
insertion. Inserting into an image tab (or when there is no editable buffer)
does nothing. See the specification at
`crates/vix-html-character-picker/spec/index.md`.

## Example

To insert a non-breaking space: open **Tools → Characters → HTML
Characters…**, use the arrow keys (or type to scroll) to reach the `nbsp;`
entry, and press `Enter`. Its rendered character is inserted at the cursor
and the picker stays open for the next pick.

---

Vix™ and Vix IDE™ are trademarks.
