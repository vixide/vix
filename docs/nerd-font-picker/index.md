# Nerd Font Picker

The Nerd Font picker is a character picker for [Nerd Font](https://www.nerdfonts.com/)
glyphs. You browse a grid of icon glyphs and insert one into the active editor.

## Opening the picker

Open it from the menu bar: **Tools → Nerd Font Palette…**. A small overlay
appears showing a fixed-width grid of curated glyphs, the highlighted glyph's
name, and a key hint.

## Glyph grid

The grid is `8` columns wide. The highlighted glyph's name is shown alongside
the grid.

The glyphs come from the common Nerd Font ranges (Font Awesome, Devicons,
Powerline, Octicons) that virtually every patched font ships, so the picker is
useful regardless of which Nerd Font your terminal uses. A glyph that a
particular font lacks simply renders as a fallback box; nothing breaks.

## Keybindings

| Key                       | Action                                          |
| ------------------------- | ----------------------------------------------- |
| `←` `↑` `↓` `→`           | Move the highlight within the grid (clamps at edges) |
| `Enter`                   | Insert the highlighted glyph; keep the picker open |
| `Esc`                     | Close the picker                                |

Pressing `Enter` keeps the picker open, so you can pick several glyphs in a row.

## Mouse

A click on a cell highlights and inserts that glyph, also leaving the picker
open.

## Notes

Inserting into an image tab (or when there is no editable buffer) does nothing.

## Example

To add a folder icon to a status line: open **Tools → Nerd Font Palette…**, use
the arrow keys to move the highlight to the glyph you want (its name appears
next to the grid), and press `Enter`. The glyph is inserted at the cursor and
the picker stays open for the next pick.

---

Vix™ and Vix IDE™ are trademarks.
