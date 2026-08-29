# Transpose

Editor actions `edit.transpose_chars`, `edit.transpose_words`,
`edit.transpose_lines`, `edit.transpose_sentences`,
`edit.transpose_paragraphs`, and `edit.transpose_sections`.

Each command swaps the unit before the cursor with the unit at (or after) it,
leaving the separator between them in place and the cursor just after the pair:

| Item (Edit -> Transpose ->) | Action | Swaps |
| --------------------------- | ------ | ----- |
| Characters | `edit.transpose_chars` | The two characters around the cursor (Emacs `C-t`; at line or buffer end the last two, never across a newline) |
| Words | `edit.transpose_words` | The two neighboring words (Emacs `M-t`), keeping the separator between them |
| Lines | `edit.transpose_lines` | The cursor's line and the line above it (Emacs `C-x C-t`) |
| Sentences | `edit.transpose_sentences` | The two sentences around the cursor, split as in **Go -> Sentence** |
| Paragraphs | `edit.transpose_paragraphs` | The two paragraphs around the cursor (runs of non-blank lines), keeping the blank lines between them |
| Sections | `edit.transpose_sections` | The two sections around the cursor (delimited by two or more blank lines), keeping the break between them |

With the cursor past the last unit (end of buffer), the last two units swap.
A command is a no-op when there is no pair — for example on the first line, or
in a buffer with a single paragraph.

From the **Edit -> Transpose** submenu. Pure logic in
`crate::textops::transpose_chars_at` / `transpose_words_at` /
`transpose_lines_at` / `transpose_sentences_at` / `transpose_paragraphs_at` /
`transpose_sections_at`; host method `App::transpose` via
`App::rewrite_at_cursor`. Sentence boundaries come from
`crate::textops::sentence_starts`, shared with the Go -> Sentence navigation, so
both split text the same way.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
