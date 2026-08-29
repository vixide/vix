# Delete Units

Editor actions `edit.delete.character`, `edit.delete.word`,
`edit.delete.sentence`, `edit.delete.paragraph`, and `edit.delete.section`.

Each command deletes the text unit at the cursor — or the next one, when the
cursor sits between units — and closes the gap by taking the separator that
follows the unit along with it. When nothing follows (the last unit in the
buffer, or a line break the command must not cross) the separator *before* the
unit goes instead, so the surrounding text still closes up:

| Item (Edit -> Delete ->) | Action | Deletes |
| ------------------------ | ------ | ------- |
| Character | `edit.delete.character` | The character at the cursor, newlines included (Emacs `C-d`); the cursor stays put |
| Word | `edit.delete.word` | The word at the cursor plus the spacing after it, never crossing a newline |
| Sentence | `edit.delete.sentence` | The sentence at the cursor, split as in **Go -> Sentence**, plus the spacing after it, never crossing a newline |
| Paragraph | `edit.delete.paragraph` | The paragraph at the cursor (a run of non-blank lines) plus the blank lines after it |
| Section | `edit.delete.section` | The section at the cursor (delimited by two or more blank lines) plus the break after it |

Character and word deletion stay inside the line, so deleting the last word of a
line leaves the line break — and the empty line — in place rather than joining
it to the next one. Sentence deletion does the same. Paragraph and section
deletion are line-based and do swallow the blank lines between blocks, so the
remaining blocks keep their usual spacing.

A command is a no-op when there is no such unit — Character at the end of the
buffer, or Paragraph in a buffer holding only blank lines.

From the **Edit -> Delete** submenu. Pure logic in
`crate::textops::delete_char_at` / `delete_word_at` / `delete_sentence_at` /
`delete_paragraph_at` / `delete_section_at`; host method `App::delete_unit` via
`App::rewrite_at_cursor`. The unit ranges come from the same helpers the
**Edit -> Transpose** family uses (`crates/vix-editor/spec/transpose/index.md`),
and sentence boundaries from `crate::textops::sentence_starts`, shared with the
Go -> Sentence navigation, so delete, transpose, and navigation all split text
the same way.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
