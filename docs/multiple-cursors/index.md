# Multiple Cursors

Vix™ can place many cursors (carets) at once and edit them together — type, delete,
or move and every caret follows.

## Add the next occurrence

With a word or selection active, the "spawn multi cursor" action adds the next
occurrence of that text as a new caret (the familiar Ctrl+D-style flow). Repeat to
keep adding matches one at a time.

## Select all occurrences

**Edit → Select → Select All Occurrences** (action `edit.select_all_occurrences`)
selects *every* occurrence of the current selection (or the word at the cursor) in
one keystroke, putting a caret on each match. Then type to change them all at once.

## Column (rectangular) selection

**Alt + Shift + ↓ / ↑** (also **Edit → Select → Column Select Down/Up**) extend a
vertical, rectangular selection: a caret is added on the next/previous line over
the same columns as the current selection (or a bare caret with none), clamped to
each line's length. Typing or deleting then applies to the whole block — handy for
editing aligned columns of text.

## Vertical carets

**Add Caret Above / Below** place a bare caret on the line above or below at the
same column, without a column span.

To drop the extra carets and return to a single cursor, use *Remove Multi Cursor*
(or *Remove All Multi Cursors*). See `spec/select-all-occurrences/index.md` and
`spec/column-select/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
