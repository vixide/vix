# ASCII Code Picker

The ASCII code picker is a scrolling overlay listing all 128 ASCII codes. It
lets you browse the table and insert any character into the active editor
without leaving the keyboard.

## Opening the picker

Open it from the menu bar: **Tools → ASCII**. The picker appears as a modal
overlay over the editor.

## The table

Each row shows three columns:

1. **Decimal** — the code as a decimal number (`0`–`127`).
2. **Hex** — the same code in hexadecimal (`00`–`7F`).
3. **Char** — the character representation. For the control codes this is a
   mnemonic such as `NUL`, `ESC`, or `DEL`; code `32` shows the word `space`;
   the printable codes show the literal glyph.

The full table runs from `0`/`00`/`NUL` through `127`/`7F`/`DEL`.

## Keybindings

| Key                       | Action                                          |
| ------------------------- | ----------------------------------------------- |
| `↑` / `↓`                 | Move the highlight up / down one row            |
| `PageUp` / `PageDown`     | Move the highlight by a page                    |
| `Home` / `End`            | Jump to the first / last row                    |
| `Enter`                   | Insert the highlighted character; keep open     |
| `Esc`                     | Close the picker                                |

The window scrolls to keep the highlighted row in view as you move.

Pressing `Enter` keeps the panel open, so you can pick several characters in a
row without reopening it.

## Mouse

A left click on any row inserts that row's character into the editor.

## Example

To insert an ESC control character: open **Tools → ASCII**, move the highlight
to the `27` / `1B` / `ESC` row (or scroll to it and click), and press `Enter`.
The escape character is inserted at the cursor and the panel stays open for the
next pick. Press `Esc` when you are done.

---

Vix™ and Vix IDE™ are trademarks.
