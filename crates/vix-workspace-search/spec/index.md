# Workspace Search

**Status:** Shipped. Search (and replace) across every file under the
workspace root, not just the active buffer — `Ctrl Shift F` / **Edit → Find
In Workspace…**.

## Contents

- `Hit` — one matching line: absolute `path`, `rel` (path relative to the
  workspace root, exactly as shown), 1-based `line`/`col`, the matching
  line's own (clipped) `text`, and `display`, the `rel:line: text` string
  shown in the results list (and, for a real text search, the shape a T211
  editable-results buffer's lines take).
- `WorkspaceSearch` — the panel's state: `query`/`replace` text, the
  `include_path`/`exclude_path` filter regexes, `replacing` (search-only vs.
  search-and-replace mode), the focused `field` (`vix_find_panel::Field`),
  `case_sensitive`/`regex` toggles, the `hits` list, `selected` index, a
  `status` summary line, and `static_results` (true when the hit list is
  fixed input — e.g. go-to-definition candidates — rather than something
  typing re-runs; also gates T211's editable-results buffer off, since a
  symbol/reference list's `display` isn't in the `rel:line: text` shape).
- `WorkspaceSearch::pattern()` compiles `query` into an effective regex
  (case-insensitive prefix unless `case_sensitive`; the literal query
  regex-escaped unless `regex`); `path_filter()` compiles `include_path`/
  `exclude_path` into a `vix_find_panel::PathFilter`.
- `WgrepBaseline` / `parse_result_line` (T211) — the pieces an editable
  search-results buffer needs: `WgrepBaseline` records what a `Hit` looked
  like when the buffer was opened (so a later save can tell whether a
  surviving line was actually edited); `parse_result_line` parses one
  buffer line back into `(rel, line, text)`, the inverse of `Hit::display`'s
  own format.

## What lives where

This crate holds only the panel's *state* — pure data plus the small pieces
of logic that don't touch the filesystem or the editor (field cycling, the
compiled pattern/filter, hit-list navigation). `App` (`src/app.rs`) owns the
open buffers, so it drives the actual scan: reading each candidate file
(open buffers in their current, possibly-unsaved state; everything else from
disk), matching `pattern()` against each line, and filtering paths through
`path_filter()`. Rendering lives in `src/ui.rs`.

## Editable search results ("wgrep"-style, T211)

**Alt+E** (or the `search.edit_results` command) turns the current hit list
into a real, editable buffer — one line per hit, in `Hit::display`'s own
`rel:line: text` shape — instead of the read-only results list. It's a
genuine `Tab`, not an overlay: the search panel closes so normal editing
keys reach it directly.

The buffer carries a synthetic `path` (`App::WGREP_RESULTS_PATH`, never a
real file) so `App::save` can recognize it and reroute `Ctrl+S` to
`App::prepare_wgrep_confirm` instead of trying to write a file literally
named that. That diff step re-identifies each **surviving** buffer line by
parsing its own `rel:line:` prefix (`parse_result_line`) and comparing the
text after it to the matching `WgrepBaseline`'s recorded text — not by
buffer position, so:

- **Editing** a line's text (the part after the prefix) is detected as a
  change to write back to that exact source line.
- **Deleting** a line just removes it from what gets parsed — no bookkeeping
  needed, it's simply excluded from the diff, i.e. skipped.
- **A line typed from scratch** (not matching the `rel:line: ` shape at all)
  is ignored rather than misread as an edit.

Only *surviving lines that actually changed* go into the confirm step —
reuses `App::ReplaceConfirm`'s own shape (per-file new contents, a
`rel (count)` summary line, a scroll offset) and its exact `y`/Enter-apply,
`n`/Esc-cancel key handling, as a sibling `wgrep_confirm` field with its own
wording, since "N edits written" and "N replaced" are different claims.
Applying writes each affected file whole (not just the changed lines) and
re-baselines exactly the lines that changed, so saving again without
further edits is a no-op.

**Known limitation**: a `rel` (workspace-relative path) containing a
literal `:` would parse wrong — essentially never seen in practice, and
not something `parse_result_line` tries to work around.

## Roadmap

- Streaming results for very large workspaces (current scan is synchronous;
  see `docs/performance/index.md` for the existing benchmark numbers).
