# Emmet Expansion

Editor action `edit.emmet_expand`.

Expand the Emmet abbreviation before the cursor into HTML -- child `>`, sibling `+`, multiply `*N`, `#id`, `.class`, `{text}`, and `$` numbering (e.g. `ul>li.item$*3`). Grouping `()` is unsupported.

From **Edit -> Emmet Expand** or the command palette. Pure logic in `crate::emmet::expand`.

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
