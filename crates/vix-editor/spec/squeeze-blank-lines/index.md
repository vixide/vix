# Squeeze Blank Lines

Editor action `edit.squeeze_blank_lines`.

Collapse runs of two or more blank (empty or whitespace-only) lines into a single empty line, over the selection or whole buffer. A trailing newline is preserved.

From **Edit -> Lines -> Squeeze Blank Lines** or the command palette. Pure logic in `crate::textops::squeeze_blank_lines`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
