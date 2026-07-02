# Increment / Decrement Number

Editor actions `edit.increment_number` and `edit.decrement_number`.

Bump the integer at or after the cursor on its line by one (Vim's Ctrl-A / Ctrl-X), leaving the cursor on the number. A leading `-` is treated as part of the token.

From the **Edit** menu or the command palette. Pure logic in `crate::textops::bump_number_at(text, cursor, delta)`; host method `App::bump_number` via `App::rewrite_at_cursor`.

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
