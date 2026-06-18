# File Explorer

The file explorer is Vix's built-in **left dock**: a directory tree for browsing,
opening, and managing your workspace files without leaving the keyboard.

**Status:** Shipped.

## Showing and Focusing the Explorer

- **`Ctrl+B`** toggles the file explorer sidebar on or off. When a nested file is
  active, toggling the explorer on expands the tree and reveals that file.
- **`Ctrl+E`** switches focus between the file explorer and the editor.

## Navigation

Use the arrow keys to move through the tree:

- **Up / Down** move the selection up and down.
- **`→` / `Enter`** open a file or expand a directory.
- **`←`** collapse an expanded directory, or jump to its parent. `←` never
  expands.

## Opening Files

- **`Enter`** opens the selected file and focuses the editor.
- **Arrow Up / Down** also open the highlighted file in a **preview tab**
  automatically as you move, so you can scan files without leaving the keyboard.

### Preview tabs

- **Single-click** opens a file in an ephemeral **preview tab**. The next
  single-click on another file replaces it instead of piling up tabs.
- Any real commitment promotes the preview to a permanent tab: editing the file,
  pressing `Enter`, double-clicking, clicking the tab itself, or a layout action
  like splitting.
- **Double-click** opens the file in a permanent tab and focuses the editor.

Preview tabs are enabled by default. Turn them off with the `preview_tabs`
setting (see `../configuration/index.md`) if you prefer every click to open a
permanent tab.

## Mouse Support

- **Wheel** moves the selection.
- **Click a file** to preview it; click it again to open it permanently.
- **Click a directory** to expand or collapse it.

## Cut / Copy / Paste

- **`Ctrl+C` / `Ctrl+X` / `Ctrl+V`** copy, cut, or paste the selection.
- A **same-directory copy** auto-appends a suffix: `copy`, `copy 2`, and so on.
- A **same-directory cut** is a no-op.
- Pasting into a different directory with a name conflict prompts per file:
  **(o)verwrite**, **(O)** all, **(s)kip**, **(S)** all, **(c)ancel**.

Cut-pending items are visually **dimmed**. Cancel a pending cut with `Escape`, or
by pasting it back into the same directory.

## Multi-Selection

- **`Shift+Up` / `Shift+Down`** extend a multi-select range from the current
  anchor.
- All clipboard operations — and delete — act on the whole selection.

## Deleting

- **`Delete`** removes the selection. Deletion always asks for confirmation
  first.

## Buffers Follow Files

Open buffers stay in sync with file operations:

- Renaming or moving a file (via cut + paste) relocates any open buffers pointing
  at it.
- Deleting a file closes its buffer.
- Renaming a directory relocates buffers for every file inside it.

## Roadmap

Drag-and-drop and a trash option (as an alternative to permanent delete) are
planned nice-to-haves, not yet built.
