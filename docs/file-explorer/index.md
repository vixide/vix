# File Explorer

The file explorer is Vix™'s built-in **left dock**: a directory tree for browsing,
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
  first, and the prompt says which will happen.
- By default, deleted items go to the OS trash / Recycle Bin — undoable from
  there. Set `explorer_delete = "hard"` in `config.toml` to remove outright
  instead (no undo). See `docs/configuration/index.md`.

## Editing a Directory as Text

**File → Edit Directory…** (palette: *Explorer: Edit Directory*, action
`explorer.edit_directory`) opens a directory as an editable buffer, one line
per entry, in the style of Emacs's wdired and Neovim's Oil. It lists the
directory highlighted in the explorer (or a highlighted file's directory) when
the explorer has focus, otherwise the active file's directory, otherwise the
workspace root.

```text
/001 docs/
/002 a.txt
/003 b.txt
```

The `/001` prefix is the entry's id: it is how Vix knows which entry a line
was, so leave it alone. Directories end in `/`. Then edit the text:

- **Rename** an entry by changing the name after its id. Swapping two names
  works: Vix moves one aside under a temporary name first.
- **Create** an entry by adding a line without an id: `notes.md` makes an
  empty file, `drafts/` a directory.
- **Delete** an entry by deleting its line. Deletes follow the
  `explorer_delete` setting: the OS trash by default, or removed outright with
  `"hard"`.

Save (`Ctrl+S`) to see what will change: one row per rename, create and
delete. `y` or `Enter` applies it, `n` or `Esc` cancels. Nothing on disk
changes before that. A listing that can't be applied is refused with a
reason on the status line: a repeated or unknown id, a name that isn't a
single entry (empty, `.`, `..`, or containing `/`), or two lines with one
name. Nothing is ever overwritten; if a step fails, the rest are skipped and
the error goes to Messages. Afterwards the buffer is re-read from disk, and
open buffers follow renamed files and close for deleted ones.

## Buffers Follow Files

Open buffers stay in sync with file operations:

- Renaming or moving a file (via cut + paste) relocates any open buffers pointing
  at it.
- Deleting a file closes its buffer.
- Renaming a directory relocates buffers for every file inside it.

## Roadmap

Drag-and-drop is a planned nice-to-have, not yet built.

---

Vix™ and Vix IDE™ are trademarks.
