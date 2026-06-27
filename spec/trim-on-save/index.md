# Trim Whitespace on Save

View toggle `view.trim_on_save`.

When on, saving trims trailing whitespace from every line. Toggle it from
**View → Editor → Trim Whitespace on Save** or the command palette. The
preference is the `trim_trailing_whitespace` setting (on by default), read into
`SaveOptions` by `App::save_options` and applied by `Editor::save_active`.
