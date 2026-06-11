# File Explorer

The file explorer is the **left dock**; its tree state lives in the internal
`vix-left-dock` crate (see `vix-left-dock.md`), and the host renders it and runs
the file operations.

**Status:** Shipped — keyboard navigation (`→`/`Enter` opens a file or expands a
directory; `←` collapses an expanded directory or jumps to its parent, and never
expands), arrow-scan preview tabs, `Ctrl+B` toggle (revealing the active file),
`Ctrl+E`
focus switching, mouse support (wheel to move the selection; click a file to
preview it and click again to open it permanently; click a directory to
expand/collapse), the file clipboard (`Ctrl+C`/`Ctrl+X`/`Ctrl+V` with same-dir
copy suffixing, cut dimming, and an (o)verwrite/(s)kip/(c)ancel conflict prompt),
`Shift+Up`/`Shift+Down` multi-selection, `Delete` (with confirmation), and
buffers that follow files on move and close on delete. Roadmap: per-file
buffers-follow on directory rename is covered; remaining nice-to-haves are
drag-and-drop and trash (vs. permanent delete).

Built-in file explorer.

Toggle Sidebar: Use Ctrl+B to show/hide the file explorer sidebar. When a nested file is active, toggling on expands the tree and reveals the file.

Focus: Use Ctrl+E to switch focus between the file explorer and editor.

Navigation: Use the arrow keys to move up and down the file tree.

## Opening Files

Enter opens the selected file and focuses the editor.

Arrow Up/Down also opens the highlighted file in a preview tab automatically as you move — so you can scan files without leaving the keyboard.

Single-click opens a file in an ephemeral preview tab — the next single-click on another file replaces it instead of piling up tabs. Any real commitment — editing the file, pressing Enter, double-clicking, clicking the tab itself, or a layout action like splitting — promotes the preview to a permanent tab.

Double-click opens the file in a permanent tab and focuses the editor.

Preview tabs are enabled by default. Turn them off via the `preview_tabs` setting (see `docs/configuration.md`) if you prefer every click to open a permanent tab.

## Cut / Copy / Paste and Multi-Selection

Ctrl+C / Ctrl+X / Ctrl+V — copy, cut, or paste the selection. Same-directory copy auto-appends copy / copy 2 etc. Same-directory cut is a no-op. Paste into a different directory with a name conflict prompts per-file: (o)verwrite, (O) all, (s)kip, (S) all, (c)ancel.

Cut-pending items are visually dimmed. Cancel a pending cut with Escape or by pasting back into the same directory.

Shift+Up / Shift+Down extend a multi-select range from the current anchor; all clipboard operations (and delete) act on the whole selection.

Buffers follow files — renaming or moving a file (via cut+paste) relocates any open buffers pointing at it; deleting a file closes its buffer. Renaming a directory relocates buffers for every file inside it.
