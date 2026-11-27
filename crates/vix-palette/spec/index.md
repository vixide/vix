# Command Palette

Command palette (Ctrl+P) with prefix-driven modes.

**Status:** Shipped — all six prefix modes, space-separated fuzzy matching, Tab
to accept, `path:line[:col]` jumping, and the live cursor *preview* while typing
a `:` line number (the cursor follows the number; `Esc` reverts).

Press Ctrl+P to open the command palette. 

Use prefix characters to switch modes:

| Prefix | Mode | Description |
|-|-|-|
| (none) | File finder | Fuzzy search for files in your workspace
| > | Commands | Search and run editor commands |
| #	| Buffers | Switch between open buffers by name |
| :	| Go to line | Jump to a specific line number |
| @	| Symbols | Jump to a declaration in the current file |
| @@	| Workspace symbols | Jump to a declaration anywhere in the workspace |

Tips:

- A hints line at the bottom shows available prefixes
- Press Tab to accept the top suggestion
- Type > to access commands, or # followed by a buffer name to switch files
- Space-separated terms match independently (e.g., "feat group" matches "features/groups/view.tsx") — so etc hosts finds /etc/hosts, save file finds save_file.rs
- In file finder mode, use path:line[:col] syntax to jump to a location after opening (e.g. tui/src/main.rs:42:10)
- In go-to-line mode (:) the cursor previews the target line *as you type* the number and scrolls it into view; press Enter to commit (recording the original position in the jump history) or Esc to revert to where you were. In file-finder mode, append `:<N>` (optionally `:<col>`) to jump to that position after opening.
- In symbol mode (@), type to fuzzy-filter the current file's declarations (functions, types, classes, traits, modules, `#define`s, …) and press Enter to jump to one. The list is a fast, offline heuristic (the same family as go-to-definition), so it works for any language without a language server. Also reachable as the palette command "Go to Symbol in File".
- In workspace-symbol mode (@@, or Ctrl+T, or the command "Go to Symbol in Workspace"), type to fuzzy-filter declarations across every file in the workspace; each result shows the symbol with its `file:line`, and Enter opens that file at the symbol. The query is required (an empty `@@` lists nothing, since the workspace is too large to enumerate); the search reads files on demand, skips images and files over 512 KB, and caps results at 200 to stay responsive.

## Symbols one line at a time (T858)

`symbols(text)` lists a buffer's declarations; `symbol_in_line(line, n)` is
the same test on one line (without its line break), numbered `n`, and
`symbols` is built on it. The sticky-scroll header and the breadcrumb use
`symbol_in_line` to read up from one line to the nearest declaration instead
of listing the whole buffer's (`crates/vix-editor/spec/sticky-scroll/index.md`).

## Preview pane (T630)

When the screen is at least `MIN_PREVIEW_WIDTH` (100) columns wide, the file
finder, `@` symbols, `@@` workspace symbols, and the workspace-search panel
(**Edit → Find In Workspace…**) split their result area: the list on the
left, a preview of the highlighted result on the right. The preview shows a
window of the file centered on the result's line, numbered, with that line
highlighted. On a narrower screen the preview is hidden and the list keeps
the full width. Commands, buffers, and `:` go-to-line have no preview (`:`
already previews by moving the cursor).

The preview must stay cheap, so it never loads a whole file:

- An open buffer previews its own (possibly unsaved) text, read line by line
  from the rope — no copy of the buffer.
- Anything else is read through the current `Authority` with
  `read_limited(PREVIEW_BYTES)` (256 KiB), so a huge file — the kind the
  large-file viewer exists for — costs one bounded read. A target line past
  that window shows a "beyond preview" note instead of loading more.
- A file with a NUL byte in that window is treated as binary and not shown.
- Under a non-local authority (an SSH workspace) the preview is skipped:
  one round trip per highlight change would stall typing.
- The App caches the last `(path, line)` preview, so redrawing the same
  highlight reads nothing.

`preview_from_bytes(bytes, line, truncated)` / `preview_from_lines(total, line, get)`
build a `Preview` (the first line number, up to `PREVIEW_RADIUS` lines either
side of the target, and the target line); `Preview::visible(height)` picks the
rows a pane of that height shows with the target as near the middle as fits.
The shared renderer is `draw_preview` in `tui/src/ui/search.rs`.

## Resume and query history (T630)

Each picker remembers how it was left. Closing the palette (`Esc` or
accepting an entry) stores its `(mode, query, selection)` — and whether it
was Search Everywhere — and closing the workspace-search panel stores its
query, path filters, toggles, and selected hit. **Resume Last Picker**
(`palette.resume`, Tools menu and palette) reopens whichever picker closed
last with that state: the mode and query are typed back in, the list is
recomputed, and the selection is restored (clamped if the list got shorter).
With nothing to resume it says so in the status bar. A go-to-definition
candidate list (a static workspace-search result) is not remembered.

The palette also keeps a query history (`QueryHistory`, most recent last,
deduplicated, capped at `HISTORY_CAP` = 50) of every accepted input. `Up`
on an empty input, or while still showing a recalled entry unchanged, steps
to the previous (older) query; `Down` in that state steps to the next newer
one, and past the newest returns to an empty input. As soon as the input is
edited (or is non-empty and not a recall), `Up`/`Down` move the selection as
before. `Down` on an empty input that is not browsing history also moves the
selection, so the unfiltered file list stays navigable. The history is kept
for the session (it is not persisted).

Code: `Preview`, `preview_from_bytes`, `preview_from_lines`, `QueryHistory`,
`Mode::prefix` in this crate; `App::picker_preview`, `App::resume_picker`,
`App::remember_palette` in `tui/src/app/command_palette.rs`; rendering in
`tui/src/ui/search.rs`.

## File index (T845)

The file finder (no prefix) matches against the App's project file index,
walked through the current `Authority` (`walk_files` with
`WalkOptions::file_index`, so an SSH or container workspace is indexed on
its own side): `.gitignore`, `.ignore`, git's global excludes and hidden
files honored, `target`/`node_modules` pruned, depth 12.

- **First results at once.** Opening Quick Open walks the first
  `FILE_INDEX_FIRST_PASS` (5,000) files on the spot, the same walk the finder
  always made, and lists them.
- **The rest in the background.** When that pass is full, a worker thread
  walks the whole workspace (up to `FILE_INDEX_CAP`) and posts the list on a
  channel; `App::poll_file_index`, called from `App::tick` each loop, swaps
  it in and re-filters an open palette with the query as typed. While it runs
  the event loop polls at its fast (50 ms) interval.
- **Replace, don't pile up.** Opening Quick Open again while a walk of the
  same folders runs keeps that walk. A rebuild for other folders (a folder
  added or removed) cancels the running walk through its `WalkCancel` and
  starts a new one; a finished list for folders the workspace no longer has
  is dropped. Re-rooting the workspace (opening a folder or a remote
  workspace) cancels the walk and forgets which folders the index came from.
  A local walk stops at its next entry; a remote walk runs to its end and its
  answer is dropped.
- **Refresh without losing files.** When the index already holds a finished
  walk of the same folders that is larger than the first pass, reopening
  keeps showing it while a background walk refreshes it (a file created since
  is listed once the refresh lands).
- **Safety cap.** `FILE_INDEX_CAP` is 150,000 files. Each keystroke
  re-scores every indexed file; measured at about 0.1 ms per 1,000 files when
  every file matches (`palette/quick_open/keystroke`,
  `docs/performance/index.md`), so the cap keeps the worst keystroke inside
  the 16 ms frame budget.
- **The picker says so.** While the background walk runs, the palette's
  bottom line reads "Indexing files… N so far" instead of the prefix hint;
  when the walk stopped at the cap (or its worker died) it reads "File index
  cut short at N files; open a smaller folder to find the rest"
  (`ui.palette_indexing`, `ui.palette_index_cut_short`, every locale). Only
  the file finder shows it, not the other modes or a Cowork guest's list.
- **Per-keystroke cost.** Only the best 200 rows become entries: candidates
  keep a borrowed path while scored, a partial selection picks the top 200,
  and only those are sorted (score, then path). The path is made relative to
  the root by a text prefix test, not `Path::strip_prefix`, and
  `fuzzy_score` scores an ASCII haystack and query byte by byte without
  allocating (same scores as the general path, checked against the reference
  implementation by a property test).

Code: `tui/src/app/file_index.rs` (the walk, the worker, the notice),
`App::palette_file_entries` in `tui/src/app/command_palette.rs`, the
bottom line in `draw_palette` (`tui/src/ui/search.rs`); benchmarks in
`tui/benches/search_and_palette.rs` (`palette/quick_open`).

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Fuzzy scoring, history browsing, preview windows and path-target parsing use `get`, iterators and `checked_*`. **Declared (PALETTE-F1):** a relevance score past the top or bottom of the `i32` scale is that extreme; the highlight and history stop at their first and last entries; a preview line past the end of the buffer shows the nearest line (a bound on the display window, not a rewrite of the search result).
