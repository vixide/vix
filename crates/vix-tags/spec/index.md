# Matching Tag

Navigation action `nav.matching_tag`.

Jump the cursor between an HTML/XML tag and its partner -- the closing tag for an opening one, or the opening tag for a closing one -- accounting for nested same-name tags. Self-closing and unmatched tags do nothing.

From **Go -> Matching Tag** or the command palette. Pure logic in `crate::tags::matching_tag`; jump via `Editor::goto_offset`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
