# Search and Replace

**Status:** Shipped — `Ctrl+F` find, `Ctrl+R` replace, `F3`/`Shift+F3` next/prev,
the Case/Word/Regex toggles (`Alt+C`/`Alt+W`/`Alt+R`), capture groups, the
`\n`/`\t`/`\r`/`\\` escapes, Replace All, and **interactive query-replace**
(`Ctrl+Alt+R` or palette "Query Replace" → step through with `y`/`n`/`!`/`q`).
Roadmap: find-occurrence-of-selection (`Alt+N`/`Alt+P`) and project-wide search
& replace.


| Shortcut              | Action                                            |
| --------------------- | ------------------------------------------------- |
| Ctrl+F                | Search in buffer; open search prompt              |
| Ctrl+R                | Replace in buffer; open search-and-replace prompt |
| Ctrl+Alt+R            | Interactive replace (y/n/!/q for each match)      |
| F3                    | Find next match                                   |
| Shift+F3              | Find previous match                               |
| Alt+N / Ctrl+F3       | Find next occurrence of selection                 |
| Alt+P / Ctrl+Shift+F3 | Find previous occurrence of selection             |

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

Use "Search and Replace in Project" from the command palette to search across all git-tracked files in the project. Press Alt+Enter to replace all matches across the project. Works with unsaved buffers and large files, up to 10,000 results.
