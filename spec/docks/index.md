# Docks and Layout

Vix arranges its window as a central **editor** surrounded by three optional
**docks** and a bottom **status bar**. Each dock is a self-contained pane with
its own state, can be shown or hidden independently, and (for the side docks and
the bottom dock) resized by dragging its inner edge. The choices persist across
sessions in your settings.

```
┌───────────────────────────────────────────────────────┐
│ Menu bar                                                │
├──────────────┬──────────────────────────┬──────────────┤
│              │ Tabs                      │              │
│  Left dock   ├──────────────────────────┤  Right dock  │
│  (Explorer)  │                          │  (Messages)  │
│              │        Editor            │              │
│              │                          │              │
│              ├──────────────────────────┴──────────────┤
│              │ Bottom dock (Output)                     │
├──────────────┴──────────────────────────────────────────┤
│ Status bar                                               │
└─────────────────────────────────────────────────────────┘
```

## The three docks

| Dock | Position | Shows | Toggle | State crate |
|------|----------|-------|--------|-------------|
| **Left** | Left edge | File explorer tree | `Ctrl+B` (or View → Layout) | `left_dock` |
| **Right** | Right edge | Message drawer (advice and notifications) | View → Layout | `right_dock` |
| **Bottom** | Below the editor | Scrollable line buffer (logs, command output, data) | View → Layout | `bottom_dock` |

Each dock's logic lives in a small, pure crate that owns only its own data; the
`vix` host renders the dock, routes keyboard and mouse events to it, and feeds it
content. The docks do no I/O of their own beyond what the explorer needs to read
the directory tree.

### Left dock — file explorer

The left dock is the **file explorer**: a lazily-expanded directory tree of the
workspace root, with selection, multi-selection, and scroll state. Toggle it with
`Ctrl+B`; toggling it on while a nested file is active expands the tree and
reveals that file. `Ctrl+E` switches focus between the explorer and the editor.

The explorer is documented in full in
[`spec/file-explorer/index.md`](../file-explorer/index.md) — including opening
files, preview tabs, cut/copy/paste, multi-selection, delete, and the
include/exclude path filters. This page does not repeat that detail.

### Right dock — message drawer

The right dock is a **message drawer**: a list of advice and notifications, each
on its own row and individually dismissable. The drawer title is **Messages**
(with a bell icon). When empty it shows a dim "no messages" hint.

Each message carries a severity **level** that selects its icon:

| Level | Meaning | Icon |
|-------|---------|------|
| `Info` | Neutral information | info |
| `Advice` | A helpful tip | info |
| `Warn` | A non-fatal warning | bell |
| `Error` | An error | close |

Messages are kept oldest-first. Every row ends with a dim **close** mark. With
the drawer focused:

- `Up` / `Down` — move the selection.
- `x`, `Delete`, or `Enter` — dismiss the selected message.
- `Esc` — return focus to the editor.

A vertical scrollbar appears in a one-column gutter when the list is taller than
the dock, honoring the same **Show/Hide Scroll Bar** toggle as the editor. The
right dock draws only its top and left borders, so it reads as attached to the
editor on its left.

### Bottom dock — logs, output, and data

The bottom dock is a **scrollable line buffer** for log messages, terminal and
command output, data views, and similar. Its title is **Output**. When empty it
shows a dim placeholder line.

Key behaviors:

- **Scrollback cap.** The buffer retains a maximum number of lines (default
  `1000`); the oldest lines drop off the top once the cap is exceeded.
- **Follow mode.** While the view is pinned to the newest line, appended output
  keeps the bottom in view. Scrolling up *stops following*, so streamed output
  does not yank the view away from something you are reading; scrolling back to
  the bottom *resumes following*.
- **`path:line` jump.** Clicking a line that looks like a `path:line` (or
  `path:line:col`) location opens that file in the editor and jumps to the
  position, recording it in the jump history.

With the bottom dock focused:

- `Up` / `Down` — scroll one line.
- `PageUp` / `PageDown` — scroll one page (the dock's visible height).
- `Home` — jump to the top (stops following).
- `End` — jump to the bottom (resumes following).
- `Esc` — return focus to the editor.

A left click focuses the dock; the mouse wheel scrolls it. Like the other docks,
a one-column scrollbar gutter appears when the buffer overflows and the scroll
bar is enabled. Hiding the bottom dock while it is focused moves focus back to
the editor.

## The status bar

The bottom **status bar** is a single row beneath the body, separated from it by
a top border. It is not a dock — it cannot be focused or resized — but it shares
the View → Layout visibility toggle. It shows, left to right:

- The current **mode indicator** (when one applies), the **active file path**,
  a **dirty flag** when the buffer has unsaved changes, and the latest transient
  **status message**.
- On the right: the **git branch** (with a dot when the working tree is dirty) —
  clicking it opens the Git Panel (see
  [`spec/git-panel/index.md`](../git-panel/index.md)) — the file's **language**,
  **line ending**, and **selection** info for text tabs, and the **cursor
  line and column**.

## View → Layout toggles

The **View → Layout** submenu (see `spec/menus/index.md`) holds the four
visibility toggles. Each is also reachable from the command palette and dispatches
the same `action` string used by the menu:

| Menu item | Action | Shortcut | Effect |
|-----------|--------|----------|--------|
| Show/Hide Left Dock | `view.left_dock` | `Ctrl+B` | Toggle the file explorer; revealing it reveals the active file |
| Show/Hide Right Dock | `view.right_dock` | — | Toggle the message drawer |
| Show/Hide Bottom Dock | `view.bottom_dock` | — | Toggle the output dock; hiding it while focused refocuses the editor |
| Show/Hide Bottom Status | `view.status_bar` | — | Toggle the status bar |

The actions also accept the aliases `view.explorer` (left) and `view.messages`
(right). Toggling a dock updates both the live view flag and the persisted
setting, so the layout is restored on the next launch.

## Resizing docks

The side docks and the bottom dock are resized by **dragging their inner edge**:

- **Left dock** — drag its **right** border column.
- **Right dock** — drag its **left** border column.
- **Bottom dock** — drag its **top** border row.

Press the edge, then drag; the drag continues even if the pointer drifts off that
exact column or row. Because the bottom dock's edge is a row, it is checked before
the column edges so a horizontal drag wins on that row.

Resizing is clamped so the layout stays usable:

| Dock | Minimum | Other constraints |
|------|---------|-------------------|
| Left / Right | 12 columns | Leaves at least 20 columns for the editor, accounting for the opposite dock if it is shown |
| Bottom | 3 rows | Leaves at least 3 body rows above it |

The resulting size is written to the corresponding setting (`explorer_width`,
`messages_width`, `bottom_dock_height`), so it persists.

## Focus

Only one pane holds keyboard focus at a time: the editor, the explorer, the
message drawer, or the bottom dock. `Ctrl+E` toggles between the explorer and the
editor; clicking a dock focuses it; pressing `Esc` in a dock returns focus to the
editor.

## As implemented in Vix

- **State crates.** `left_dock` owns the explorer tree (`Explorer`,
  flattened `Node` rows, selection, multi-selection, expansion set, include/
  exclude regex filters, scroll). `right_dock` owns the `Messages` drawer
  (`Message` rows with a `Level`, selection, `push`/`info`/`advice`/`warn`/`error`,
  `close_selected`). `bottom_dock` owns `BottomDock` (a capped `Vec<String>`
  line buffer with `scroll`, a `follow` flag, and a `DEFAULT_SCROLLBACK` of
  `1000`). All three are pure logic with no rendering; each `#![forbid(unsafe_code)]`,
  `#![deny(missing_docs)]`, and `#![warn(clippy::pedantic)]`.
- **Host wiring** lives in `src/app.rs`: `show_explorer`, `show_messages`,
  `show_bottom_dock`, and `show_status_bar` track visibility (mirrored into
  `Settings`); `toggle_left_dock`, `toggle_right_dock`, `toggle_bottom_dock`, and
  `toggle_status_bar` flip them. `run_action` maps `view.left_dock`/`view.explorer`,
  `view.right_dock`/`view.messages`, `view.bottom_dock`, and `view.status_bar` to
  those togglers.
- **Resize** is handled by `resize_dock` (columns, `MIN_DOCK = 12`,
  `MIN_EDITOR = 20`) and `resize_bottom_dock` (rows, `MIN_DOCK = 3`,
  `MIN_BODY = 3`), driven by a `DockResize` drag started when the pointer presses
  the explorer's right edge, the messages drawer's left edge, or the bottom dock's
  top edge.
- **Rendering** is in `src/ui.rs`: `draw_messages` (title `ui.messages`,
  top+left borders, per-level icons, close marks), `draw_bottom_dock` (title
  `ui.bottom_dock` = "Output", a `visible(height)` window, follow-aware scroll),
  and `draw_status_bar` (segments built by the `status_bar_panel` crate). Each
  dock reserves a one-column scrollbar gutter via the shared `draw_scrollbar`
  helper when its content overflows and `show_scrollbar` is on.
- **Menu** entries are defined in `src/menu.rs` under `VIEW_LAYOUT`; labels are
  i18n keys translated from `locales/app.yml`.
- **Defaults** (from `src/settings.rs`): left and right docks and the status bar
  start shown; the bottom dock starts hidden; `explorer_width = 30`,
  `messages_width = 32`, `bottom_dock_height = 9`.
