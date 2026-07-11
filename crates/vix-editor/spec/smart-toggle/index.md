# Toggle Value

Editor action `edit.toggle_value`.

Flip the token under the cursor to its opposite: word pairs (`true`/`false`, `yes`/`no`, `on`/`off`, `enable`/`disable`, `left`/`right`, `up`/`down`, `min`/`max`, `and`/`or`) matched whole-word with case preserved, and symbol pairs (`&&`/`||`, `==`/`!=`, `<=`/`>=`, `<`/`>`, `++`/`--`) at or just before the cursor.

From **Edit -> Toggle Value** or the command palette. Pure logic in `crate::textops::smart_toggle_at`; host method `App::smart_toggle` via `App::rewrite_at_cursor`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
