# Find Panel

The find panel searches and replaces text in the current buffer, and extends
to interactive query-replace and project-wide search and replace. It also
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
| Ctrl+Shift+F            | Project-wide search and replace                   |

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

## Project-wide search and replace

Open with **Ctrl+Shift+F**, or **Search in Project** / **Search and Replace in
Project** from the command palette. Type to search incrementally across every
file under the project root; results list as `path:line: text`. Use ↑/↓ to
navigate and Enter to open a match. In the replace variant, **Tab** switches to
the replacement field and **Alt+Enter** (or Enter from the replace field)
rewrites every match across the project.

Open buffers are searched and replaced in their current (possibly unsaved)
state. Files larger than 2 MB and binary files are skipped, and results are
capped at 5,000.

### Path filters

Two extra fields narrow the file set by regular expression against each file's
project-relative path (forward-slashed):

- **Include path** — only paths matching the regex are searched.
- **Exclude path** — paths matching the regex are skipped.

**Tab** cycles through Find → (Replace) → Include path → Exclude path. Empty
filters impose no constraint, and an invalid (half-typed) regex is treated as
empty rather than hiding every file.

For example, Include `\.rs$` searches only Rust files; Exclude `(^|/)target/`
skips the build directory.

## Search in Project → Dock

A variant lists results in the **bottom dock** instead of a panel: **Edit →
Find → Search in Project → Dock** (or the palette). It prompts for a term —
**Alt+C** toggles case-sensitivity, **Alt+R** toggles regex (default:
case-insensitive literal) — then pushes `relpath:line:col: text` lines into the
dock, each click-to-jump to the match.
