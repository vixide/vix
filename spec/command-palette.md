# Command Palette

**Status:** Shipped — all four prefix modes, space-separated fuzzy matching, Tab
to accept, and `path:line[:col]` jumping. The live cursor *preview* while typing
a `:` line number (last paragraph) is roadmap; today the jump commits on Enter.

Press Ctrl+P to open the command palette. 

Use prefix characters to switch modes:

| Prefix | Mode | Description |
|-|-|-|
| (none) | File finder | Fuzzy search for files in your project
| > | Commands | Search and run editor commands |
| #	| Buffers | Switch between open buffers by name |
| :	| Go to line | Jump to a specific line number |

Tips:

- A hints line at the bottom shows available prefixes
- Press Tab to accept the top suggestion
- Type > to access commands, or # followed by a buffer name to switch files
- Space-separated terms match independently (e.g., "feat group" matches "features/groups/view.tsx") — so etc hosts finds /etc/hosts, save file finds save_file.rs
- In file finder mode, use path:line[:col] syntax to jump to a location after opening (e.g. src/main.rs:42:10)
- In go-to-line mode (:) and in file-finder mode when you append :<N> to a file, the cursor previews the target line live as you type and commits when you press Enter. If you move the mouse or hit Escape, the preview is reverted.
