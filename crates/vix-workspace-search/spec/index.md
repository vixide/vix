# Workspace Search

**Status:** Shipped. Search (and replace) across every file under the
workspace root, not just the active buffer — `Ctrl Shift F` / **Edit → Find
In Workspace…**.

## Contents

- `Hit` — one matching line: absolute `path`, `rel` (path relative to the
  workspace root, exactly as shown), 1-based `line`/`col` (for a text
  search, `col` counts Unicode scalar values from the start of the full
  source line, not bytes and not the clipped preview), the matching line's
  own (clipped) `text`, and `display`, the `rel:line: text` string shown in
  the results list (and, for a real text search, the shape a T211
  editable-results buffer's lines take).
- `WorkspaceSearch` — the panel's state: `query`/`replace` text, the
  `include_path`/`exclude_path` filter regexes, `replacing` (search-only vs.
  search-and-replace mode), the focused `field` (`vix_find_panel::Field`),
  `case_sensitive`/`regex` toggles, the `hits` list, `selected` index, a
  `status` summary line, and `static_results` (true when the hit list is
  fixed input — e.g. go-to-definition candidates — rather than something
  typing re-runs; also gates T211's editable-results buffer off, since a
  symbol/reference list's `display` isn't in the `rel:line: text` shape),
  and an optional `title`: a fixed list that names itself sets it, and the
  panel is drawn under that title instead of the default one. Every static
  list the App opens names itself: Diagnostics, Document Symbols, Workspace
  Symbols, References, TODO Comments (`ui.results_*`) and the call or type
  hierarchy (T620 — `crates/vix-lsp/spec`); only go-to-definition's
  candidates use the "Go to Definition" fallback. A row's `rel` is relative
  to the workspace root, matched by its real path too
  (`App::root_relative`), since language servers report canonical paths and
  a root opened through a symlink would otherwise show every row as an
  absolute path.
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
- `clip` / `CLIP_CHARS` / `LineEdit` / `LineRewrite` / `rewrite_lines`
  (T653) — the shared clip a `Hit::text` uses, and the pure,
  baseline-checked, line-ending-preserving rewrite an editable-results save
  applies to each file (see *Write safety* below).

- `tool` (T715) — `ToolQuery` (the Rust regex for `rg`, the fixed strings
  `grep -F` can run instead, case folding, a file-size cap; built by
  `search`, `any_of` and `regex_only`), `Tool` (`Rg`/`Grep`: `program()`,
  `args(&query)`, `parse(line, known)`) and `ToolLine` (`rel`, `line`,
  `text`). `parse_rg_json` reads `rg --json` `match` messages (a non-UTF-8
  path or line is skipped); `parse_grep` splits `path:line:text` at the
  first `:digits:` whose path the caller knows, so a `:` in a file name
  can't misplace it. `pattern()` is `ToolQuery::search(…).regex`, so the
  panel and the tool can't drift.

## What lives where

This crate holds only the panel's *state* — pure data plus the small pieces
of logic that don't touch the filesystem or the editor (field cycling, the
compiled pattern/filter, hit-list navigation). `App` (`tui/src/app.rs`) owns the
open buffers, so it drives the actual scan: reading each candidate file
(open buffers in their current, possibly-unsaved state; everything else from
disk), matching `pattern()` against each line, and filtering paths through
`path_filter()`. Rendering lives in `tui/src/ui.rs`.

### Why it depends on `vix-find-panel` (T860)

The crate layering report (T833) flagged `vix-workspace-search ->
vix-find-panel` as one panel crate depending on another. The edge is kept:
it uses two items, both part of the find engine rather than any panel
rendering. `Field` is the one focus vocabulary of the search stages: the
find box (`SearchBar::field`) and this panel (`WorkspaceSearch::field`)
share it because widening the find box hands the search over (spec
`crates/vix-find-panel`, "Scope"), and the App shell's input handling and
`tui/src/ui/search.rs` read both with the same variants. `PathFilter` is
part of the pure matching engine (`matches`, `replace_all`, `PathFilter`).
`vix-find-panel` itself has no workspace dependency (layer 0; only
`bitflags` and `regex`, which this crate depends on anyway) and no
rendering, so the edge adds nothing to the build. Moving the two items would
need a new crate for them alone, which AGENTS.md's "When to add a new crate"
argues against.

## Remote search (T715)

The App's scans (this panel, **Search in Workspace (to dock)**, the TODO
finder and Todo Tree, and go-to-definition without a language server) share
one walk, `App::scan_index` (`tui/src/app/remote_search.rs`). Under the
local authority it reads every indexed file in-process, exactly as before.
Under any authority whose `is_local()` is `false` (SSH, the test fake) it
runs one search tool there through `Authority::command`, in each workspace
folder:

- `rg --json --no-config --no-messages --hidden [--max-filesize N] -e
  <regex> -- .` when the authority's `which("rg")` finds it; rg's default
  regex engine is the `regex` crate, so the pattern is the panel's own.
- else, for a literal query only, `grep -r -n -I -H -F [-i]
  --exclude-dir=.git --exclude-dir=node_modules --exclude-dir=target -e
  <word>… -- .` (`-i` only for an ASCII query: `grep`'s case folding
  follows the remote locale).
- else (a regex without `rg`), or when the tool can't start or fails
  without output (exit status other than 0 or 1), the in-process walk —
  through the authority, one read per file.

The tool is a prefilter. Every line it reports is re-checked with the same
Rust regex, only files the file index holds (and the path filter allows)
count, and lines are ordered by the file's position in the index and then
line number — the in-process order — so results and counts match the local
search (`tui/tests/authority_search.rs` compares the two). Paths in the
output are relative to the tool's working directory and joined onto the
workspace folder: they are authority paths, never looked up locally. Files
an open buffer shows are scanned from the buffer (unsaved text, as
locally). A tool may report at most 50,000 lines before it is stopped.

## The in-process scan (T830)

Under the local authority (and as the fallback for a remote one without a
usable tool), `App::scan_index` reads the file index in batches of 2,048
files, in index order. For each batch, worker threads (as many as
`available_parallelism`, each taking 32 neighboring files at a time) read
the files and drop each one in which no line matches the query's regex —
the same prefilter contract as the remote tool: every caller's own check
(this panel's pattern, the dock search's, the TODO scan's tags, the
definition keywords) can only accept a line that regex matches. The
scanning thread then visits the kept files' lines in index order, so
results, their order, the file count and every cap (5,000 hits here and in
the dock search, 2,000 TODOs, 200 definitions; Replace All's scope is not
capped, see [find-and-replace](../../../spec/find-and-replace/index.md#scope-and-display-cap-t864-pq-04)) are what the
one-file-at-a-time walk produced; a cap still stops the scan after the
batch it is reached in. Open buffers are read on the scanning thread, from
their unsaved text. A remote authority's fallback reads one file at a time,
as before, so it never sees concurrent requests.

Two savings, both exact:

- **No `canonicalize` per file.** Deciding whether an open buffer shows a
  file needs its canonical path. A file that is not itself a symlink (one
  `lstat`, which also gives the size the 2 MB limit needs) has its
  directory's canonical path joined with its name, the directory resolved
  once per worker per directory; a symlink, or any error, takes the old
  `canonicalize` + `metadata` path. `canonicalize` cost about four
  stats per file and did not get faster on more threads (macOS, 100,000
  files: 0.65 s on 1–16 threads).
- **A narrower query skips remembered misses (`ScanMemo`).** Each local
  scan remembers the files it read and found no candidate line in, with
  their size and modification time. The next scan uses that memory when
  its regex can only match lines the remembered one matched: the same
  regex, or a literal query (`ToolQuery::fixed` with one word) that
  contains the remembered literal with the same case rule — what typing a
  query further produces, since case folding is per character. A
  remembered file whose `lstat` shows the same size and time is not read.
  A file modified less than 2 s before the scan is never remembered (git's
  "racy" rule: it may change again without its time changing), and a file
  an open buffer shows is always read from the buffer. A shorter or
  unrelated query reads every file again. Like `make` and git, a rewrite
  that keeps both the size and the modification time is not noticed.

Design decision (T830, "incremental workspace search via filesystem
watch"): no file watcher and no on-disk or trigram index. The repository has
no watcher dependency, and the measurements (`docs/performance`,
§ Workspace search in a large checkout) show the scan above meets "100,000
files in under 1 s" without one: a full read of 100,000 small files takes
about 0.9 s on a heavily loaded 16-core machine (3.7 s before) and a
narrowing keystroke about 0.18 s, most of it the `lstat`s a watcher could
save. The file list is the T845 file index, so a search never re-walks the
tree; opening the panel refreshes the index in the background as before.
The remote path is unchanged (`rg`/`grep` on the remote host). The memory
is one map of the missed files' paths and stamps (about 10 MB for 100,000
files), replaced by each scan.

**Off the input thread (T869).** The panel's keystrokes run this scan on a
worker thread over a snapshot (`ScanSnapshot`: the index, the workspace
folders, the authority and each named buffer's text at the keystroke), as
[find-and-replace](../../../spec/find-and-replace/index.md#responsiveness-t869-pq-08)
describes; other callers (the dock search, the TODO scan, the fallback
go-to-definition, Replace All's scope) still scan on the spot over the App's
own state (`Buffers::Live`). The scan can be cancelled: workers stop taking
files, a remote tool is killed, and a cancelled scan stores no memo. The
memo is shared (`Arc<Mutex<ScanMemo>>`): a scan copies it when it starts
and stores its own when it ends, never holding the lock while it reads.

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

### Write safety (T653)

The line rewrite is `rewrite_lines`, a pure function over one file's text:

- **Stale lines are not written.** Each target line's current text is passed
  through `clip` (the same left-trim and `CLIP_CHARS` cut a `Hit` uses) and
  compared to its baseline. A line that no longer matches — or no longer
  exists — is reported as drifted (a warning per line, a count in the status
  line) and skipped, so a file changed since the search never has the wrong
  line replaced.
- **Only the visible part changes.** A rewritten line keeps its leading
  whitespace and any text past `CLIP_CHARS` the buffer didn't show.
- **Line endings are kept.** Each line keeps its own `\r\n` or `\n`, and
  the file keeps its final-newline state.

Applying a plan — here, in **Replace in Files** and in **Structural Replace
in Workspace**, which share the step (`App::apply_reviewed`,
`tui/src/app/replace_guard.rs`) — writes with `vix_fileops::write_atomic`,
refuses a file open in a **read-only** tab (reported, not written), and
changes a file open in a **dirty** tab in that buffer only, leaving it dirty:
writing it to disk would also save that tab's unrelated unsaved edits. The
on-save normalizations are not applied, since the preview showed exactly what
is written.

Each target is also bound to the version the user previewed (T864): the plan
records a fingerprint of the whole text it was computed from, and applying
re-reads the target (the dirty buffer, or the file through the authority) and
writes only while it still holds that text. A file changed since the preview
is skipped and offered again in a refreshed preview, planned from its current
text (here: the line-level drift check above runs again over the new text); a
deleted file is reported and not recreated. The summary counts only the edits
of files actually written. See
[find-and-replace](../../../spec/find-and-replace/index.md#bound-to-the-previewed-version-t864-pq-01).

**Known limitation**: a `rel` (workspace-relative path) containing a
literal `:` would parse wrong — essentially never seen in practice, and
not something `parse_result_line` tries to work around.

## Roadmap

- Streaming results for very large workspaces (current scan is synchronous;
  see `docs/performance/index.md` for the existing benchmark numbers).

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Result-line parsing and line rewriting use `strip_suffix`, `strip_prefix`, `get` and `split_inclusive` instead of slicing. **Declared (SEARCH-F1):** selection stops at the first and last hit; a rewritten line with nothing past the clip has an empty tail. A line number of 0 or past the file is reported as drifted, as before.
