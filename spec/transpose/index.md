# Transpose

Editor actions `edit.transpose_chars` and `edit.transpose_words`.

Transpose Characters swaps the two characters around the cursor (Emacs `C-t`; at line or buffer end it swaps the last two, never across a newline). Transpose Words swaps the two neighboring words (Emacs `M-t`), preserving the separator between them.

From the **Edit** menu or the command palette. Pure logic in `transpose_chars_at` / `transpose_words_at`; host method `App::transpose`.

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
