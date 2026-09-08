# Workspace Search

**Status:** Shipped. Search (and replace) across every file under the
workspace root, not just the active buffer — `Ctrl Shift F` / **Edit → Find
In Workspace…**.

## Contents

- `Hit` — one matching line: absolute `path`, 1-based `line`/`col`, and the
  `relpath:line: text` string shown in the results list.
- `WorkspaceSearch` — the panel's state: `query`/`replace` text, the
  `include_path`/`exclude_path` filter regexes, `replacing` (search-only vs.
  search-and-replace mode), the focused `field` (`vix_find_panel::Field`),
  `case_sensitive`/`regex` toggles, the `hits` list, `selected` index, a
  `status` summary line, and `static_results` (true when the hit list is
  fixed input — e.g. go-to-definition candidates — rather than something
  typing re-runs).
- `WorkspaceSearch::pattern()` compiles `query` into an effective regex
  (case-insensitive prefix unless `case_sensitive`; the literal query
  regex-escaped unless `regex`); `path_filter()` compiles `include_path`/
  `exclude_path` into a `vix_find_panel::PathFilter`.

## What lives where

This crate holds only the panel's *state* — pure data plus the small pieces
of logic that don't touch the filesystem or the editor (field cycling, the
compiled pattern/filter, hit-list navigation). `App` (`src/app.rs`) owns the
open buffers, so it drives the actual scan: reading each candidate file
(open buffers in their current, possibly-unsaved state; everything else from
disk), matching `pattern()` against each line, and filtering paths through
`path_filter()`. Rendering lives in `src/ui.rs`.

## Roadmap

- Streaming results for very large workspaces (current scan is synchronous;
  see `docs/performance/index.md` for the existing benchmark numbers).
