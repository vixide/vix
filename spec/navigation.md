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
the project for declaration-style lines, so it works for any language without a
language server (but is heuristic, not semantically precise).

Position History: Navigate back and forward through your edit locations using Alt+Left and Alt+Right.

Open File Jump: The Open File prompt and Quick Open (Ctrl+O) support path:line[:col] syntax to jump directly to a location after opening (e.g. src/main.rs:42:10).
