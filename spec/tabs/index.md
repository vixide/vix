# Tabs

Vix keeps every open buffer in a horizontal **tab strip** drawn just above the
editor text area. Each tab is one buffer: a text file, an untitled scratch
buffer, or a read-only image. Exactly one tab is **active** at a time, and the
active buffer is the one shown (and edited) in the editor below the strip.

There is always at least one tab. Vix starts with a single empty untitled
buffer, and closing the last tab immediately opens a fresh untitled one rather
than leaving the editor empty.

## The tab strip

Tabs are listed left to right in the order they were opened. Each tab shows a
file-type icon, the file's base name, and — when there are unsaved edits — a
trailing dirty marker:

```
 file.rs   main.rs ●   notes.md   image.png
```

| Element | Meaning |
| --- | --- |
| Icon | File-type glyph derived from the file name. |
| Name | The file's base name, or `untitled` for a buffer with no path. |
| Dirty marker | A `●` shown only while the buffer has unsaved edits. |
| Active tab | Drawn **underlined** (not reversed video, so it keeps the editor background). |
| Preview tab | Drawn **dimmed** (see [Preview tabs](#preview-tabs-ephemeral)). |
| Divider | A dim `│` separates adjacent tabs. |

Clicking a tab activates it (and promotes it from a preview tab to a permanent
one — see below). The strip itself does not scroll or show a close button per
tab; tabs are closed with the keyboard or the **File** menu.

## How tabs open

A buffer becomes a tab in several ways:

- **File → New** (`Ctrl+N`) creates an empty untitled buffer and focuses it.
- **File → Open** (`Ctrl+O`), **Open Recent** (`Ctrl+Shift+O`), and the
  command palette open a file. If the file is already open, Vix simply
  activates its existing tab instead of opening a duplicate.
- **Double-clicking** a file in the explorer, or pressing **Enter** on a
  selected file, opens it as a permanent tab.
- **Single-clicking** or arrow-scanning a file in the explorer opens it as an
  **ephemeral preview tab** (see below).
- **Image files** open as read-only image tabs.

Opening a file matches against the canonicalized path, so the same file opened
through different relative paths still resolves to one tab.

### Preview tabs (ephemeral)

When you browse the file explorer, Vix can open files in a single, reusable
**preview tab** so that skimming through a directory does not litter the strip
with dozens of tabs. A preview tab is drawn dimmed in the strip.

Preview tabs open when you:

- **Single-click** a file row in the explorer (the first click on a not-yet-
  selected file), or
- **Arrow-scan** by moving the explorer selection with the **Up**/**Down**
  keys.

Only one preview tab exists at a time: opening another preview **replaces** the
current preview tab in place rather than adding a new one. A preview tab is
**promoted** to a permanent tab — and stops being reused — as soon as you commit
to it:

- editing it (any text insertion or line edit),
- saving it,
- double-clicking it (or pressing Enter on it) in the explorer,
- clicking its tab in the strip, or
- opening it again as a non-preview open.

Preview tabs are governed by the `preview_tabs` setting (enabled by default).
When `preview_tabs` is off, explorer single-clicks and arrow-scans do not open
preview tabs; files open only on an explicit double-click / Enter.

Image files are never previewed — arrow-scanning or single-clicking an image in
the explorer does not open it; it opens only on an explicit action.

## Dirty indicator

A tab is **dirty** when its buffer has unsaved edits. The dirty state shows as a
`●` after the file name in the tab strip (and is mirrored in the status bar for
the active buffer). Saving the buffer clears the dirty flag.

Closing or quitting on a dirty buffer is guarded: Vix raises an unsaved-changes
prompt offering to **Save** (`S`), **Discard** (`D`), or **Cancel** (`C` /
`Esc`) before the buffer is discarded. Image tabs are never dirty (they are
read-only), so they close without a prompt.

## Switching tabs

| Action | Shortcut | Behavior |
| --- | --- | --- |
| Next Tab | `Ctrl+Tab` | Activate the next tab, **wrapping** from the last back to the first. |
| Previous Tab | `Ctrl+Shift+Tab` | Activate the previous tab, **wrapping** from the first to the last. |

Both commands are also available under **View → Next Tab / Previous Tab**.
Switching tabs only changes which buffer is active; it does not promote a
preview tab (only clicking, editing, or an explicit open does that).

## Closing tabs

| Action | Shortcut | Menu | Behavior |
| --- | --- | --- | --- |
| Close | `Ctrl+W` | File → Close | Close the active tab. |
| Close All Tabs | `Ctrl+Shift+W` | File → Close All Tabs | Close every tab. |
| Reopen Closed Tab | `Ctrl+Shift+T` | File → Reopen Closed Tab | Reopen the most recently closed file. |

**Close** removes the active tab. If it was the last tab, a fresh untitled
buffer takes its place; otherwise the active index shifts to a remaining tab. A
dirty buffer triggers the unsaved-changes prompt first.

**Close All Tabs** drains every tab and leaves a single empty untitled buffer.

**Reopen Closed Tab** maintains a stack of recently closed **files** (paths
only; untitled buffers are not tracked). The stack is de-duplicated, capped at
the most recent 20, and `Ctrl+Shift+T` pops it most-recent-first, skipping any
entry whose file no longer exists on disk. If nothing reopenable remains, the
status line reports that there is no closed tab to reopen.

## Image tabs are read-only

Files with image extensions (`png`, `jpg`/`jpeg`, `gif`, `bmp`, `webp`, `ico`,
`tiff`/`tif`) open as **image tabs** that render the picture in the editor area
instead of a text editor. Image tabs are view-only:

- Text-editing actions (insert, duplicate line, move line, word-selection,
  etc.) are no-ops on an image tab.
- An image tab is never dirty and never prompts on close.
- Re-opening an already-open image just re-activates its tab.

## As implemented in Vix

- `src/editor.rs` defines the `Tab` struct (`path`, `dirty`, `preview`, optional
  `image`, `title()`, `is_image()`) and the `Editor` tab stack (`tabs`,
  `active`, and the `new_tab`, `open`, `open_image`, `promote_active`,
  `close_active`, `close_all`, `next_tab`, `prev_tab` operations). `open` reuses
  an existing tab by canonical path and replaces the lone preview tab in place
  when `preview` is requested.
- `src/app.rs` wires the actions and shortcuts: `tab.next` / `tab.prev`
  (`Ctrl+Tab` / `Ctrl+Shift+Tab`), `file.close` (`Ctrl+W`, via
  `request_close_active` → `do_close_active`), `file.close_all`
  (`Ctrl+Shift+W`), and `file.reopen_closed` (`Ctrl+Shift+T`, backed by the
  `closed_tabs` stack with `push_closed_tab` / `reopen_closed_tab`). Explorer
  preview tabs come from `preview_selected` (gated on `settings.preview_tabs`)
  and the single-click / arrow-scan handlers; `open_path` promotes a tab on a
  real open. `tab_click` activates and promotes a clicked tab.
- `src/ui.rs` (`draw_tabs`) renders the strip: the active tab underlined,
  preview tabs dimmed, and a dim `│` divider; `draw_center` renders either the
  active buffer's editor or its image.
- `src/menu.rs` lists the File-menu entries (New, Open, Close, Close All Tabs,
  Reopen Closed Tab) and the View-menu Next Tab / Previous Tab entries.
- `src/settings.rs` defines `preview_tabs` (default on).
