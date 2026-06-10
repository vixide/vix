# Search and Replace

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
| F3                    | Find next match                                   |
| Shift+F3              | Find previous match                               |
| Alt+N                 | Find next occurrence of selection                 |
| Alt+P                 | Find previous occurrence of selection             |

Query Replace: Use "Query Replace" from the command palette for interactive replacement (y/n/!/q prompts for each match).

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
