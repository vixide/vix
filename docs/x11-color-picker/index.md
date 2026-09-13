# X11 Color Picker

A picker for the standard X11 color names. You browse a scrollable table of
colors — swatch, `#RRGGBB` hex string, and name — and insert the highlighted
color's hex value into the active editor.

## Opening the picker

Open it from the menu bar: **Tools → X11 Colors…**. An overlay appears
showing the table of named X11 colors, each row rendered as a swatch, its
hex string, and its name.

## Keybindings

| Key                       | Action                                          |
| ------------------------- | ----------------------------------------------- |
| `↑` `↓`                   | Move the highlight one row                      |
| `Page Up` `Page Down`     | Move the highlight one page                     |
| `Home` `End`              | Jump to the first / last row                     |
| `Enter`                   | Insert the highlighted color's hex value; keep the picker open |
| `Esc`                     | Close the picker                                |

Pressing `Enter` keeps the picker open, so you can pick several colors in a
row. A click on a row does the same as `Enter`.

## Reused by the theme editor

The same picker also opens when choosing a color slot's new value inside the
theme editor (**View → Edit Theme…**): pressing `Enter` on a slot there
opens this picker, and a pick applies the color to that slot and closes the
picker (returning you to the theme editor) instead of inserting hex text and
staying open. It's the same overlay and the same underlying
`crate::x11_color_picker::Panel` either way — only what happens when you
pick a color differs, based on how the picker was opened.

## Notes

The color table is bundled as a TSV and parsed once on first use; the crate
itself is pure data (the color table plus the picker's row-selection and
scroll state) — the host renders the rows, maps clicks to rows, and performs
the insertion or the theme-slot update. Inserting into an image tab (or when
there is no editable buffer) does nothing. See the specification at
`crates/vix-x11-color-picker/spec/index.md`.

## Example

To insert `#F0F8FF` (AliceBlue) into a stylesheet: open **Tools → X11
Colors…**, move the highlight to `AliceBlue`, and press `Enter`. The hex
value is inserted at the cursor and the picker stays open for the next pick.

---

Vix™ and Vix IDE™ are trademarks.
