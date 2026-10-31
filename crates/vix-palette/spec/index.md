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
