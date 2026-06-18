# Find and Replace

**Status:** Shipped — in-buffer `Ctrl+F` find with the Replace field revealed by
`Ctrl+R` or `Tab`, `Find Next` / `Find Previous` (`Ctrl+G` / `Ctrl+Shift+G`, also
`F3` / `Shift+F3` while the box is open), `Find Selection` (`Alt+N` / `Alt+P`),
the Case / Whole Word / Regex toggles (`Alt+C` / `Alt+W` / `Alt+R`), capture
groups and `\n` / `\t` / `\r` / `\\` escapes in the replacement, Replace All,
interactive **query-replace** (`Ctrl+Alt+R`, step through with `y` / `n` / `!` /
`q`), workspace-wide search & replace, and **Find In Workspace → Dock** with
click-to-jump results.

All find-and-replace capability lives **inside the Find box** — there is no
separate "Find & Replace" menu item. You open the box with `Ctrl+F`, reveal the
Replace field, set the toggles, and drive everything from a single replacement up
to Replace All and interactive query-replace from there (or from the matching
command-palette entries).

## Keyboard shortcuts

| Shortcut                  | Action                                                |
| ------------------------- | ---------------------------------------------------- |
| Ctrl+F                    | Open the Find box (search the current buffer)        |
| Ctrl+R                    | Open the Find box in replace mode (Replace revealed) |
| Ctrl+Alt+R                | Interactive query-replace (`y` / `n` / `!` / `q`)    |
| Ctrl+G / F3               | Find next match (repeats the last search)            |
| Ctrl+Shift+G / Shift+F3   | Find previous match                                  |
| Alt+N                     | Find next occurrence of the selection / word         |
| Alt+P                     | Find previous occurrence of the selection / word     |
| Alt+C / Alt+W / Alt+R     | Toggle Case Sensitive / Whole Word / Regex (in box)  |
| Tab                       | Switch between the Find and Replace fields            |
| Enter                     | Find next (Find field) or Replace All (Replace field)|
| Alt+Enter                 | Replace All (from either field)                       |
| Esc                       | Close the Find box                                    |

`Ctrl+G` / `F3` and their `Shift` variants are also on the **Edit → Find** menu
along with **Find Selection** (`Alt+N`) and **Find In Workspace…**.

## Opening Find

`Ctrl+F` (Edit → Find → Find…) opens the Find box for the active buffer. As you
type in the Find field the next match is **previewed live** — the cursor moves to
and selects the next match, and every match in the buffer is highlighted. As you
step through matches the status line reports the **current position and total**,
e.g. `Match 3 of 12`, or `no matches`.

The Find box keeps state for the duration it is open: the search pattern, the
replacement text, which field has focus, and the three toggles. Press `Esc` to
close it; it **remembers the last completed search** so `Find Next` / `Find
Previous` keep working afterward.

## Sticky highlights

By default the in-buffer match highlights are **sticky**: they stay visible after
the Find box closes (controlled by the `sticky_search_highlight` setting, default
`true`; set it `false` to clear highlights on close). **Toggle Search Highlights**
(Edit → Find, or the command palette) clears the highlights when shown, or
re-applies the last search when hidden; **Reset Search** / **Unhighlight Search**
clears them outright.

## Revealing Replace

`Ctrl+R` (Edit → Find → Find & Replace) opens the box already in **replace mode**,
which shows a Replace field below the Find field. From an already-open find box you
reveal and reach the Replace field by pressing **`Tab`** (or by **clicking** its
row). `Tab` toggles focus back and forth between the two fields; in find-only mode
`Tab` does nothing because there is no Replace field.

With focus on the **Find** field, `Enter` runs Find Next. With focus on the
**Replace** field, `Enter` runs **Replace All**; `Alt+Enter` runs Replace All from
either field. Replace All rewrites every match in the buffer in one pass, marks the
buffer dirty, and reports the count (e.g. `Replaced 7`).

## Clickable buttons

The find box renders clickable buttons (in addition to the keyboard shortcuts):

- A toggle-button row under the Find field for **Case**, **Word**, and **Regex** —
  clicking one flips that option (highlighted when on), the same as `Alt+C` /
  `Alt+W` / `Alt+R`.
- In replace mode, a button row under the Replace field with **Once** (replace the
  next match at/after the cursor, then highlight the following one), **Ask** (start
  the interactive query-replace), and **All** (Replace All).

## Find Next / Find Previous

`Ctrl+G` (`F3`) finds the next match and `Ctrl+Shift+G` (`Shift+F3`) the previous.
Each repeats the last completed search and **wraps around** the ends of the buffer
— the first match after the last, the last match before the first. They work
whether or not the box is open: while the box is open they use its current query,
and once it has closed they replay the remembered pattern. With nothing searched
yet they fall back to the selection, behaving like **Find Selection**.

## Find Selection

`Alt+N` / `Alt+P` (Edit → Find → Find Selection) jump to the next / previous
occurrence of the current selection **without opening the Find box**. With no
selection they use the word (symbol) under the cursor. The text is matched
literally (regex-escaped). This also seeds the remembered search, so a following
`Ctrl+G` continues from it.

## Toggles: Case, Whole Word, Regex

Inside the box, `Alt+C`, `Alt+W`, and `Alt+R` toggle the three match modes.
Toggling re-runs the search immediately (except during interactive query-replace,
which keeps the cursor put).

| Toggle         | Effect                                                     |
| -------------- | --------------------------------------------------------- |
| Case Sensitive | Match exact case (default: case-insensitive)              |
| Whole Word     | Match complete words only (wraps the pattern in `\b…\b`)  |
| Regex          | Treat the query as a regular expression                   |

The effective pattern is built from the query and the toggles: a non-regex query
is regex-escaped, Whole Word adds word boundaries, and case-insensitive search
prepends the `(?i)` flag. An empty query yields no pattern. An invalid regex shows
a "bad regex" message in the box rather than searching.

## Regex and capture groups

With Regex on, the query is a full regular expression and the **replacement string
supports capture groups**: `$1`, `$2`, … or `${name}` for named groups. For
example, finding `(\w+): (\w+)` and replacing with `$2: $1` swaps the two words
around the colon.

The replacement also interprets the escape sequences `\n` (newline), `\t` (tab),
`\r` (carriage return), and `\\` (literal backslash), so you can insert line breaks
or indentation. In plain-text (non-regex) mode the replacement is inserted
literally — `$1` and `\n` are taken verbatim, with no group expansion or escaping.

## Interactive query-replace

`Ctrl+Alt+R` (palette: **Query Replace**) starts an interactive, step-through
replacement. Type the query and replacement in the box, then press `Enter` to
begin. The box closes and Vix highlights the first match at or after the cursor and
prompts: `Query replace — y replace  n skip  ! rest  q quit`.

| Key            | Decision                                          |
| -------------- | ------------------------------------------------- |
| `y` / `Y` / Space | Replace this match, advance to the next        |
| `n` / `N` / Delete | Skip this match, advance to the next          |
| `!`            | Replace this and **all remaining** matches        |
| `q` / `Q` / Esc / Enter | Quit                                     |

Replacements honor the Regex toggle (capture groups and escapes expand exactly as
in Replace All). When the matches run out, the status line reports how many were
replaced (e.g. `Query replace: replaced 5`); if there were none to begin with it
reports `Query replace: no matches`.

## Find In Workspace → Dock

**Edit → Find → Find In Workspace…** (action `search.workspace_dock`, also on the
command palette as **Search in Workspace → Dock**) searches across every file
under the workspace root and lists the hits in the **bottom dock**.

It prompts for a term. While typing, `Alt+C` toggles case-sensitivity and `Alt+R`
toggles regex; the default is a case-insensitive literal search. On `Enter` it
builds the file index, scans each file's current text (open buffers are searched in
their possibly-unsaved state), and pushes one line per match into the dock in
`relpath:line:col: text` form, with a header `$ search "term"` and a trailing
`[N matches in M files]` summary. Matches are capped at 5,000.

Each result line in the dock is **click-to-jump**: clicking it opens the file and
moves the cursor to that line and column. See `bottom_dock/spec/index.md` for
the dock itself.

## Workspace-wide search & replace

A panel-based variant (`Ctrl+Shift+F`, or palette **Search in Workspace** /
**Search and Replace in Workspace**) searches incrementally across every file under
the workspace root, listing results as `path:line: text`; `↑` / `↓` navigate and
`Enter` opens a match. In the replace variant, `Tab` reaches the replacement field
and `Alt+Enter` (or `Enter` from the replace field) rewrites every match across the
workspace via the same `replace_all` engine. Open buffers are matched and rewritten
in their current state; files over 2 MB and binary files are skipped, and results
are capped.

**Path filters** narrow the file set by regex against each file's workspace-relative
path: **Include path** (only matching paths are searched) and **Exclude path**
(matching paths are skipped). `Tab` cycles Find → (Replace) → Include path →
Exclude path. Empty filters impose no constraint, and an invalid (half-typed) regex
is treated as empty rather than hiding every file — for example Include `\.rs$`
limits to Rust files and Exclude `(^|/)target/` skips the build directory. See
`find_panel/spec/index.md`.

## As implemented in Vix

The internal **`find_panel`** crate owns both the box's **state** and the
**search/replace engine**, all pure functions over `&str` with **character**
offsets; the app renders the box, owns the buffer, and applies the returned text.

- `SearchBar` holds the `query`, `replace`, `replacing`, `interactive`, focused
  `field`, the `case_sensitive` / `whole_word` / `regex` toggles, and `status`.
  `SearchBar::pattern` builds the effective regex (escaping, `\b…\b`, `(?i)`);
  `toggle_field` and `active_field_mut` drive `Tab` / typing; `Field` is one of
  `Query`, `Replace`, `IncludePath`, `ExcludePath`.
- Engine functions: `matches` (all `(start, end)` char ranges), `next_match` (first
  match at/after an offset), `replace_all` (returns new text + count), `replace_one`
  (single match at an offset, returns resume offset), and `unescape` (`\n` `\t` `\r`
  `\\` in a replacement template). `PathFilter::new` / `allows` implement the
  workspace include/exclude filters.
- In `src/app.rs`: `start_search` opens the box; `find_step` and `find_with`
  implement Find Next / Previous with wrap-around and highlight marks;
  `find_selection` implements Find Selection; `replace_all` does in-buffer Replace
  All; `begin_query_replace`, `qr_key`, and `qr_apply` run interactive query-replace
  (`Decision::Replace` / `Skip` / `ReplaceRest` / `Quit`); `search_workspace_to_dock`
  feeds the bottom dock; `workspace_replace_all` does the workspace-wide rewrite.
- Actions and bindings: `edit.find` (`Ctrl+F`), `edit.find_next` (`Ctrl+G`),
  `edit.find_prev` (`Ctrl+Shift+G`), `edit.query_replace` (`Ctrl+Alt+R`),
  `search.next_selection` / `search.prev_selection` (`Alt+N` / `Alt+P`), and
  `search.workspace_dock`. The Edit → Find submenu is defined by `EDIT_FIND` in
  `src/menu.rs`. The `search` field and `crate::search` re-export (`Field`,
  `SearchBar`) connect the app to `find_panel`.
