# ROT13

Editor action `tools.convert.rot13`.

Rotate ASCII letters by 13 places (its own inverse); all other characters are unchanged. Applies to the selection or whole buffer.

From **Tools -> Convert -> ROT13** or the command palette. Pure logic in `crate::textops::rot13`, applied via `App::transform_selection_or_buffer`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
