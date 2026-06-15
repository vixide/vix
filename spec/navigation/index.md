# Navigation

**Status:** Shipped. The `path:line[:col]` jump works from both the Open prompt
and the command palette; Position History (`Alt+Left`/`Alt+Right`) navigates
back/forward through jump locations; and "Go to Definition" (`F12`) jumps to the
symbol under the cursor. The definition lookup is a fast, offline heuristic
(keyword-prefixed declarations like `fn name`, `class name`, `def name`, `#define
name`) rather than a semantic LSP — a real LSP client could replace it later.


Go to Definition: Press `F12`, or use the command palette (Ctrl+P >) and search
for "Go to Definition", to jump to the definition of the symbol under the cursor.
A single match jumps directly; multiple matches open a chooser. The lookup scans
the workspace for declaration-style lines, so it works for any language without a
language server (but is heuristic, not semantically precise).

Go to Symbol in File: Open the command palette and type `@` (or run the
"Go to Symbol in File" command) to list the current file's declarations —
functions, types, classes, traits, modules, `#define`s, and the like. Type to
fuzzy-filter, then Enter to jump. Like go-to-definition, it is a fast, offline,
language-agnostic heuristic over declaration-style lines (local `let`/`var`
bindings are excluded to keep the outline structural), not a semantic LSP.

Position History: Navigate back and forward through your edit locations using Alt+Left and Alt+Right.

Recent Locations (jump list): Press Alt+J (or **Go → Recent Locations…**, or the command palette command "Recent Locations") to open a chooser listing the recorded position history, most-recent first with consecutive duplicates removed. Each row shows the file name, line, and directory; ↑/↓ select, Enter (or a click) jumps to the location, Esc cancels. Where Alt+Left/Right step linearly through the history one entry at a time, this lists them all so you can jump straight to any one.

Open File Jump: The Open File prompt and Quick Open (Ctrl+O) support path:line[:col] syntax to jump directly to a location after opening (e.g. src/main.rs:42:10).
