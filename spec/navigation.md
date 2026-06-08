# Navigation

**Status:** Partly shipped. The `path:line[:col]` jump works from both the Open
prompt and the command palette. "Go to Definition" (needs LSP) and Position
History (Alt+Left/Right) are roadmap.


Go to Definition: Use the command palette (Ctrl+P >) and search for "Go to Definition" to jump to the definition of a symbol under the cursor (requires LSP).

Position History: Navigate back and forward through your edit locations using Alt+Left and Alt+Right.

Open File Jump: The Open File prompt and Quick Open (Ctrl+O) support path:line[:col] syntax to jump directly to a location after opening (e.g. src/main.rs:42:10).
