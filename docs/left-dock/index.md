# Left Dock

The left dock is the **file explorer**: a lazily-expanded directory tree rooted
at the workspace directory. Directories are listed first, then files, sorted
case-insensitively by name; dotfiles are hidden.

## Show / Hide and Focus

- **Ctrl+B** toggles the file explorer sidebar. When a nested file is active,
  toggling on expands the tree and reveals that file.
- **Ctrl+E** switches focus between the file explorer and the editor.

## Navigation

- `↑` / `↓` move the selection up and down the tree. As you move, the
  highlighted file opens automatically in a preview tab, so you can scan files
  without leaving the keyboard.
- `→` / `Enter` opens a file or expands a directory.
- `←` collapses an expanded directory, or jumps to its parent directory; it
  never expands.
- `PgUp` / `PgDn` move a page at a time; `Home` / `End` jump to the first / last
  row.
- The mouse wheel moves the selection.

## Opening Files

- **Enter** opens the selected file and focuses the editor.
- **Single-click** opens a file in an ephemeral preview tab. The next
  single-click on another file replaces that preview tab instead of piling up
  tabs.
- **Double-click** opens the file in a permanent tab and focuses the editor.
- A preview tab is promoted to a permanent tab by any real commitment: editing
  the file, pressing Enter, double-clicking, clicking the tab itself, or a
  layout action such as splitting.
- Clicking a directory expands or collapses it.

Preview tabs are enabled by default. Turn them off with the `preview_tabs`
setting (see `../configuration/index.md`) if you prefer every click to open a
permanent tab.

## Cut / Copy / Paste

- **Ctrl+C** / **Ctrl+X** / **Ctrl+V** copy, cut, or paste the selection.
- A same-directory copy auto-appends `copy`, `copy 2`, and so on. A
  same-directory cut is a no-op.
- Pasting into a different directory with a name conflict prompts per file:
  `(o)` overwrite, `(O)` overwrite all, `(s)` skip, `(S)` skip all, `(c)`
  cancel.
- Cut-pending items are visually dimmed. Cancel a pending cut with `Escape`, or
  by pasting back into the same directory.

## Multi-Selection and Delete

- **Shift+Up** / **Shift+Down** extend a multi-select range from the current
  anchor. All clipboard operations and delete act on the whole selection.
- **Delete** removes the selection, after a confirmation prompt.

## Buffers Follow Files

Open buffers track their files. Renaming or moving a file (via cut and paste)
relocates any open buffers pointing at it; deleting a file closes its buffer.
Renaming a directory relocates buffers for every file inside it.

## Git Badges

Changed tracked files show a colored one-letter badge: `M` modified (yellow),
`A` added (green), `?` untracked (green), `D` deleted (red), `R` renamed
(cyan), `U` conflicted (magenta). See `docs/git-panel/index.md`.

## Example

To rename `foo.txt` to `bar.txt`:

1. Select `foo.txt` in the explorer and press **Ctrl+X**.
2. Press **Ctrl+V** to paste it back, then rename — any open buffer for the file
   follows along automatically.

---

Vix™ and Vix IDE™ are trademarks.
