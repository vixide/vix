# File Explorer

Filesystem helpers for the explorer's copy / cut / paste / delete. The file
explorer is the **left dock**; its tree state lives in the internal
`left_dock` crate (see `crates/vix-left-dock/spec/index.md`), and the host renders it and runs
the file operations.

When the tree is taller than the dock, a vertical **scrollbar** appears in a
one-column gutter on the right edge (its thumb tracks the highlighted entry). It
honors the same **Show/Hide Scroll Bar** toggle (`show_scrollbar`) as the editor.

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

Preview tabs are enabled by default. Turn them off via the `preview_tabs` setting (see `docs/configuration/index.md`) if you prefer every click to open a permanent tab.

## Cut / Copy / Paste and Multi-Selection

Ctrl+C / Ctrl+X / Ctrl+V — copy, cut, or paste the selection. Same-directory copy auto-appends copy / copy 2 etc. Same-directory cut is a no-op. Paste into a different directory with a name conflict prompts per-file: (o)verwrite, (O) all, (s)kip, (S) all, (c)ancel.

Cut-pending items are visually dimmed. Cancel a pending cut with Escape or by pasting back into the same directory.

Shift+Up / Shift+Down extend a multi-select range from the current anchor; all clipboard operations (and delete) act on the whole selection.

Buffers follow files — renaming or moving a file (via cut+paste) relocates any open buffers pointing at it; deleting a file closes its buffer. Renaming a directory relocates buffers for every file inside it.

## Atomic and private writes

Two crash-safe writers, both write-temp-then-rename (never a truncating
in-place write a reader could observe half-done) and both written through a
symlink rather than following it into some other file:

- `write_atomic(path, data)` — the general-purpose saver used across the
  app (editor save, workspace save, …). An *existing* file's permission
  bits are preserved exactly; a brand-new one gets the process's normal
  umask-default mode, same as a plain `fs::write` — silently narrowing
  every new file to owner-only would be a surprise for a file meant to be
  shared or version-controlled.
- `write_atomic_private(path, data)` — same mechanics, but a **new** file
  is always created owner-only (0600 on Unix) regardless of umask (T133).
  For persisted files whose content is inherently sensitive even though
  they're not secrets-in-transit the way `write_private_temp` below
  covers: `vix-undo-store`'s per-file undo histories (a full edit history
  can carry text no longer in the current buffer at all) and
  `vix-macros`' `macros.toml` (a recorded macro can carry literally-typed
  text). An existing file's mode is still left alone, exactly like
  `write_atomic`.
- `restrict_to_owner(path)` — best-effort, narrow an *already-written*
  file to owner-only on Unix. For persisted files written through
  [confy](https://docs.rs/confy) rather than this crate (`config.toml`,
  `session.toml`, `db_history.toml`, `db_queries.toml`), which give no
  hook to choose the mode as the file is created — only after. Not
  perfectly race-free at creation time, but these are single-user local
  desktop config files, not shared or network-exposed ones.
- `write_private_temp(prefix, data)` — a one-off scratch file in the OS
  temp directory (secrets briefly in transit, e.g. an AI-request payload),
  0600 and `O_EXCL` from creation, unrelated to any of the above.

`keybindings.toml` (`vix-keybindings::user_bindings`) is a deliberate
non-fix: its content (a key token paired with an action id) is never
sensitive, so it keeps its original plain, non-private write.
