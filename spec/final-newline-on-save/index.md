# Final Newline on Save

View toggle `view.final_newline_on_save`.

When on, saving ensures the file ends with exactly one trailing newline. Toggle it
from **View → Editor → Final Newline on Save** or the command palette. The
preference is the `ensure_final_newline` setting (on by default), read into
`SaveOptions` by `App::save_options` and applied by `Editor::save_active`.
