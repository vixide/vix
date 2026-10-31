# Multiple Cursors

Vix™ can place many cursors (carets) at once and edit them together — type, delete,
or move and every caret follows.

## Add the next occurrence

With a word or selection active, the "spawn multi cursor" action adds the next
occurrence of that text as a new caret (the familiar Ctrl+D-style flow). Repeat to
keep adding matches one at a time. *Skip Multi-Cursor* moves the newest match on to
the next occurrence instead (useful when one match shouldn't change), and *Skip
Multi-Cursor Back* moves it to the previous one.

## Select all occurrences

**Edit → Select → Select All Occurrences** (action `edit.select_all_occurrences`)
selects *every* occurrence of the current selection (or the word at the cursor) in
one keystroke, putting a caret on each match. Then type to change them all at once.

## Split a selection into lines

**Edit → Select → Split Selection into Lines** (`edit.split_selection_into_lines`,
like Helix and Kakoune's `Alt-s`) turns each selection into one selection per line
it covers. Select a block, split it, then press End and type to append to every
line.

## Select regex matches in a selection

**Edit → Select → Select Regex Matches in Selection**
(`edit.select_regex_in_selection`, like Kakoune and Helix's `s`) asks for a regular
expression and puts a selection on each of its matches inside the current
selections — or in the whole buffer when nothing is selected. A pattern with no
match leaves the carets as they were.

## Column (rectangular) selection

**Alt + Shift + ↑ / ↓** (also **Edit → Select → Column Select Up/Down**) extend a
vertical, rectangular selection: a caret is added on the next/previous line over
the same columns as the current selection (or a bare caret with none), clamped to
each line's length. Typing or deleting then applies to the whole block — handy for
editing aligned columns of text.

## Vertical carets

**Add Caret Above / Below** place a bare caret on the line above or below the
newest caret at the same column, without a column span; repeat to keep going.
Alt+click adds a caret where you click.

## Moving and editing

With several carets, the arrows, Ctrl+← / →, Home, End and the named motions
(word, line start/end, buffer start/end, with or without Shift) move every caret.
Copy copies every selection, one per line; pasting text with exactly one line per
caret puts one line at each caret, and otherwise pastes the whole text at every
caret. A multi-caret edit is one undo step; undoing it returns to a single caret.

Completion and snippet expansion work with one caret only: with extra carets the
completion popup stays closed and Tab doesn't expand a snippet.

## Removing carets

*Remove Multi Cursor* removes the caret you added last; *Remove All Multi Cursors*
(or Esc) returns to a single cursor. See
`crates/vix-editor-core/spec/index.md` (Multiple carets),
`crates/vix-editor/spec/select-all-occurrences/index.md` and
`crates/vix-editor/spec/column-select/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
