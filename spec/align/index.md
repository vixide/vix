# Align on Delimiter

Editor actions `edit.align.equals` / `colon` / `comma` / `pipe`.

Pad the selected lines (or the whole buffer) so each line's first occurrence of the delimiter lands in a common column, normalized to one space on each side. Lines without the delimiter are left unchanged; a trailing newline is preserved.

From **Edit -> Align** or the command palette. Pure logic in `crate::align::on_delimiter`, applied via `App::transform_selection_or_buffer`.

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
