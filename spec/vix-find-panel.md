# Vix Find Panel

**Status:** Shipped — `Ctrl+F` find, `Ctrl+R` replace, `F3`/`Shift+F3` next/prev,
the Case/Word/Regex toggles (`Alt+C`/`Alt+W`/`Alt+R`), capture groups, the
`\n`/`\t`/`\r`/`\\` escapes, Replace All, **interactive query-replace**
(`Ctrl+Alt+R` or palette "Query Replace" → step through with `y`/`n`/`!`/`q`), and
**project-wide search & replace** (`Ctrl+Shift+F` or palette "Search in Project" /
"Search and Replace in Project"; searches open buffers in their unsaved state), and
**find-occurrence-of-selection** (`Alt+N`/`Alt+P`, or the palette, jumping to the
next/previous occurrence of the selection — or the word under the cursor — without
opening the search bar).


| Shortcut              | Action                                            |
| --------------------- | ------------------------------------------------- |
| Ctrl+F                | Search in buffer; open search prompt              |
| Ctrl+R                | Replace in buffer; open search-and-replace prompt |
| Ctrl+Alt+R            | Interactive replace (y/n/!/q for each match)      |
| F3 / Ctrl+G           | Find next match (repeats the last search)         |
| Shift+F3 / Ctrl+Shift+G | Find previous match                             |
| Alt+N                 | Find next occurrence of selection                 |
| Alt+P                 | Find previous occurrence of selection             |

**Find Next** (`Ctrl+G` / `F3`) and **Find Previous** (`Ctrl+Shift+G` /
`Shift+F3`) repeat the last completed search, and keep working after the find box
has closed — the last pattern is remembered. With nothing searched yet, they fall
back to the selection (or word under the cursor), like **Find Selection**
(`Alt+N`). All three are also on the Edit menu.

Query Replace: Use "Query Replace" from the command palette for interactive replacement (y/n/!/q prompts for each match).

## Find / replace box

The in-buffer find / replace box has a Find field and (in replace mode) a Replace
field below it. Switch the focused field with **`Tab`** or by **clicking** the
field's row; the hint line reads `Tab / click: switch field` in replace mode.
With the cursor in the Replace field, **`Enter`** (or **`Alt+Enter`** from either
field) replaces all matches; `Enter` from the Find field finds the next match.

The internal `vix-find-panel` crate owns both the box's **state** (query and
replacement text, focused field, toggles, effective-pattern builder) and the
**search/replace engine** over text — `matches`, `next_match`, `replace_all`,
`replace_one`, and the replacement-template `unescape` (all pure functions over
`&str` with character offsets). The app renders the box, owns the buffer, and
applies the returned text.

## Search toolbar

Shows toggle buttons.

| Toggle         | Action                    |
| -------------- | ------------------------- |
| Case Sensitive | match exact case          |
| Whole Word     | match complete words only |
| Regex          | use regular expressions   |

## Regex and Capture Groups

When regex mode is enabled, the replacement string supports capture groups: $1, $2, or ${name} for named groups. For example, searching for (\w+): (\w+) and replacing with $2: $1 swaps the two words around the colon.

The replacement also interprets the standard escape sequences \n (newline), \t (tab), \r (carriage return), and \\ (literal backslash), so you can insert line breaks or indentation. Plain-text (non-regex) replacement treats these as literal characters.

## Project-Wide Search and Replace

Open with `Ctrl+Shift+F`, or "Search in Project" / "Search and Replace in Project"
from the command palette. Type to search incrementally across every file under
the project root; results list as `path:line: text`. Use ↑/↓ to navigate and
Enter to open a match. In the replace variant, `Tab` switches to the replacement
field and `Alt+Enter` (or Enter from the replace field) rewrites every match
across the project. Open buffers are searched and replaced in their current
(possibly unsaved) state; files larger than 2 MB and binary files are skipped,
and results are capped at 5,000.

**Path filters.** Two extra fields narrow the file set by regular expression
against each file's project-relative path (forward-slashed): **Include path**
(only paths matching the regex are searched) and **Exclude path** (paths matching
the regex are skipped). `Tab` cycles through Find → (Replace) → Include path →
Exclude path. Empty filters impose no constraint, and an invalid (half-typed)
regex is treated as empty rather than hiding every file. For example, Include
`\.rs$` searches only Rust files; Exclude `(^|/)target/` skips the build dir.

## Search in Project → Dock

A variant that lists results in the **bottom dock** instead of a panel: **Edit →
Find → Search in Project → Dock** (or the palette). It prompts for a term —
`Alt+C` toggles case-sensitivity, `Alt+R` toggles regex (default: case-insensitive
literal) — then pushes `relpath:line:col: text` lines into the dock, each
**click-to-jump** to the match. See `vix-bottom-dock.md`.
