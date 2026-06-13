# Command Palette

**Status:** Shipped — all five prefix modes, space-separated fuzzy matching, Tab
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

Tips:

- A hints line at the bottom shows available prefixes
- Press Tab to accept the top suggestion
- Type > to access commands, or # followed by a buffer name to switch files
- Space-separated terms match independently (e.g., "feat group" matches "features/groups/view.tsx") — so etc hosts finds /etc/hosts, save file finds save_file.rs
- In file finder mode, use path:line[:col] syntax to jump to a location after opening (e.g. src/main.rs:42:10)
- In go-to-line mode (:) the cursor previews the target line *as you type* the number and scrolls it into view; press Enter to commit (recording the original position in the jump history) or Esc to revert to where you were. In file-finder mode, append `:<N>` (optionally `:<col>`) to jump to that position after opening.
- In symbol mode (@), type to fuzzy-filter the current file's declarations (functions, types, classes, traits, modules, `#define`s, …) and press Enter to jump to one. The list is a fast, offline heuristic (the same family as go-to-definition), so it works for any language without a language server. Also reachable as the palette command "Go to Symbol in File".
