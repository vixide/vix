# Line-Ending Conversion

Editor actions `edit.eol_lf` and `edit.eol_crlf`.

Convert every line ending in the selection (or whole buffer) to LF (Unix) or CRLF (Windows). Mixed input is normalized first, so CRLF conversion never produces a doubled carriage return.

From **Edit -> Lines -> Line Endings** or the command palette. Pure logic in `crate::textops::to_lf` / `to_crlf`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
