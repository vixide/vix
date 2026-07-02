# Relative Line Numbers

Editor action `view.relative_line_numbers`; setting `relative_line_numbers`.

Show each line's distance from the cursor line in the gutter (hybrid: the cursor line shows its absolute number). Off by default. Applies to every buffer and persists.

From **View -> Editor -> Relative Line Numbers** or the command palette. `editor_core::Editor::set_relative_line_numbers`; the renderer computes the value from the cursor line.

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
