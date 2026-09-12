# Find Panel

The find panel searches and replaces text in the current buffer, and extends
to interactive query-replace and workspace-wide search and replace. It also
includes find-occurrence-of-selection, which jumps to the next or previous
occurrence of the selection without opening any box.

## Keybindings

| Shortcut                | Action                                            |
| ----------------------- | ------------------------------------------------- |
| Ctrl+F                  | Search in buffer; open the find prompt            |
| Ctrl+R                  | Replace in buffer; open the find-and-replace prompt |
| Ctrl+Alt+R              | Interactive query-replace (`y`/`n`/`!`/`q` per match) |
| F3 / Ctrl+G             | Find next match (repeats the last search)         |
| Shift+F3 / Ctrl+Shift+G | Find previous match                               |
| Alt+N                   | Find next occurrence of the selection             |
| Alt+P                   | Find previous occurrence of the selection         |
| Ctrl+Shift+F            | Workspace-wide search and replace                   |
| Alt+H                   | In the find dialog: turn replace on or off          |
| Alt+I                   | In the find dialog: where to look — buffer, files, workspace |
| Alt+E                   | In workspace search: edit the results as a real buffer |

**Find Next** and **Find Previous** repeat the last completed search, and keep
working after the find box has closed — the last pattern is remembered. With
nothing searched yet, they fall back to the selection (or the word under the
cursor), like **Find Selection** (`Alt+N`). All three are also on the Edit
menu.

## Find / replace box

The in-buffer box has a Find field and, in replace mode, a Replace field below
it. Switch the focused field with **Tab** or by **clicking** the field's row;
in replace mode the hint line reads `Tab / click: switch field`.

- From the Find field, **Enter** finds the next match.
- With the cursor in the Replace field, **Enter** replaces all matches.
- **Alt+Enter** replaces all matches from either field.

## One dialog, two options

Finding somewhere else is not a different command. The find box carries two
options, each a clickable button and each on an `Alt` key:

- **Replace** (`Alt+H`) turns the find into a find-and-replace in place and puts
  the cursor in the replacement field. Turning it off keeps the query.
- **In:** (`Alt+I`) chooses where to look — **Buffer**, **Files**, or
  **Workspace** — and hands the query, the replacement, and the toggles to
  whichever surface shows that many results: the workspace panel for Files, the
  bottom dock for Workspace. Nothing is retyped.

This is why the Edit → Find submenu no longer lists **Find in Files…**,
**Replace in Files…**, or **Find In Workspace…**; they were the same search with
a different destination. `Ctrl+Shift+F` still opens the workspace panel directly.

## Search toolbar

Toggle buttons control how matching works:

| Toggle         | Shortcut | Action                    |
| -------------- | -------- | ------------------------- |
| Case Sensitive | Alt+C    | Match exact case          |
| Whole Word     | Alt+W    | Match complete words only |
| Regex          | Alt+R    | Use regular expressions   |

## Regex and capture groups

When regex mode is enabled, the replacement string supports capture groups:
`$1`, `$2`, or `${name}` for named groups. For example, searching for
`(\w+): (\w+)` and replacing with `$2: $1` swaps the two words around the
colon.

The replacement also interprets the standard escape sequences `\n` (newline),
`\t` (tab), `\r` (carriage return), and `\\` (literal backslash), so you can
insert line breaks or indentation. Plain-text (non-regex) replacement treats
these as literal characters.

## Interactive query-replace

Press **Ctrl+Alt+R**, or run **Query Replace** from the command palette, to
step through matches one at a time. At each prompt:

- `y` — replace this match
- `n` — skip this match
- `!` — replace this and all remaining matches
- `q` — quit

## Structural search & replace

**Edit → Structural Replace…** (or **Structural Replace** from the command
palette) matches by *structure*, not regex: a pattern like
`if $COND { $$BODY }` matches any `if` statement regardless of how its
condition or body are formatted, ignoring whitespace differences between
the pattern and the source entirely.

- `$NAME` — a hole matching exactly one token, or one whole bracketed
  group (`(...)`, `[...]`, `{...}`) when the next token opens one.
- `$$NAME` — a hole matching a run of zero or more tokens (e.g. a whole
  argument list, or a block's whole body).

Type the pattern, then the replacement (using the same `$NAME`/`$$NAME`
syntax — each placeholder becomes the *original* text that hole matched,
formatting included), and step through matches exactly like interactive
query-replace above (`y`/`n`/`!`/`q`). With an active **selection**, only
matches inside it are offered; otherwise the whole buffer is searched.

**Edit → Structural Replace in Workspace…** does the same search across
every file under the workspace root and previews a summary ("N replaced in
M files") before writing anything — the same confirm step workspace
search-and-replace uses.

See the specification at `crates/vix-structural-replace/spec/index.md` for
the full pattern syntax (including its documented limitations — matching
is token/bracket-based, not a full language parser).

## Workspace-wide search and replace

Open with **Ctrl+Shift+F**, or **Search in Workspace** / **Search and Replace in
Workspace** from the command palette. Type to search incrementally across every
file under the workspace root; results list as `path:line: text`. Use ↑/↓ to
navigate and Enter to open a match. In the replace variant, **Tab** switches to
the replacement field and **Alt+Enter** (or Enter from the replace field) opens a
**preview**: a list of every affected file with its match count. Nothing is
written until you confirm with **y** / **Enter** (**n** / **Esc** cancels), so you
can review the scope before a project-wide rewrite.

Open buffers are searched and replaced in their current (possibly unsaved)
state. Files larger than 2 MB and binary files are skipped, and results are
capped at 5,000.

### Path filters

Two extra fields narrow the file set by regular expression against each file's
workspace-relative path (forward-slashed):

- **Include path** — only paths matching the regex are searched.
- **Exclude path** — paths matching the regex are skipped.

**Tab** cycles through Find → (Replace) → Include path → Exclude path. Empty
filters impose no constraint, and an invalid (half-typed) regex is treated as
empty rather than hiding every file.

For example, Include `\.rs$` searches only Rust files; Exclude `(^|/)target/`
skips the build directory.

### Editing results directly ("wgrep"-style)

**Alt+E** turns the current hit list into a real, editable buffer — one line
per hit, in the same `path:line: text` shape — instead of a read-only list.
Edit a line's text and **Ctrl+S** writes that edit back to its source file
at the recorded position; delete a line to skip that hit entirely, leaving
its source untouched. Saving previews a summary ("N edits in M files")
before writing, the same confirm step search-and-replace uses. Not
available for a static results list (go-to-definition, diagnostics, …) —
only a real text search's hits.

## Search in Workspace → Dock

A variant lists results in the **bottom dock** instead of a panel: **Edit →
Find → Search in Workspace → Dock** (or the palette). It prompts for a term —
**Alt+C** toggles case-sensitivity, **Alt+R** toggles regex (default:
case-insensitive literal) — then pushes `relpath:line:col: text` lines into the
dock, each click-to-jump to the match.

---

Vix™ and Vix IDE™ are trademarks.
