# Tutorial 1: Your First Session

This is the first page of Vix's tutorial series: a guided walkthrough of
opening Vix for the first time and getting oriented. If you haven't
installed Vix yet, do that first — see
[`docs/getting-started/index.md`](../../getting-started/index.md). This
page assumes the binary is already on your `PATH`.

We'll use the repository's own demo workspace as the example project, so
everything below is something you can actually type and see happen.

## Open the demo workspace

Vix doesn't have a "File → Open Folder…" menu item — the folder you launch
it *from* becomes the workspace root. So to open a folder, `cd` into it
first:

```sh
git clone https://github.com/vixide/vix.git
cd vix/examples/demo-workspace
vix
```

Take a look at [`examples/demo-workspace/README.md`](../../../examples/demo-workspace/README.md)
for what's in this project — a tiny Rust binary, a Python script, some
Markdown notes, and a couple of data files. We'll open several of these as
we go.

If this is the very first time Vix has run anywhere on your machine, it
opens a scrollable welcome overlay before anything else. Press `Esc` to
dismiss it — it won't appear automatically again, but you can always
reopen it from **Help → Welcome…**. See
[`docs/welcome-panel/index.md`](../../welcome-panel/index.md).

Everything below shows the default **Apple** keymap. Vix ships nine other
keymaps (VS Code, Emacs, Vim, Spacemacs, IntelliJ, Eclipse, Sublime Text,
…) — switch under **View → Keymap**, and see
[`docs/keymaps/index.md`](../../keymaps/index.md). Whichever keymap you
use, every action in this tutorial is also reachable from its menu or the
command palette, so nothing here is keymap-specific in spirit, only in
which key you press.

## The file explorer

The left dock is Vix's file explorer — a directory tree rooted at your
workspace. If you don't see it, press **`Ctrl+B`** to show it; press it
again to hide it. **`Ctrl+E`** jumps focus between the explorer and the
editor without closing either.

With the explorer focused:

- **`↑` / `↓`** move the selection.
- **`→`** or **`Enter`** opens a file, or expands a directory.
- **`←`** collapses an expanded directory, or steps up to its parent.

Move the selection down to `rust-app/`, press `→` to expand it, then drill
into `src/` the same way until you reach `main.rs`. Notice that as you
move the selection with the arrow keys, Vix opens each highlighted file in
a **preview tab** automatically — a lightweight, throwaway tab that the
next file you preview simply replaces, so browsing the tree doesn't pile
up tabs. Press **`Enter`** on `main.rs` to commit to it: that promotes the
preview into a real, permanent tab and moves focus into the editor.

Full detail — multi-select, cut/copy/paste, delete-to-trash, and more —
lives in [`docs/file-explorer/index.md`](../../file-explorer/index.md).

## Reading and editing a file

You should now be looking at
`examples/demo-workspace/rust-app/src/main.rs`, a small word-frequency
counter. Scroll to the bottom of `main()` and you'll find two comments:

```rust
// TODO: report the total word count too, not just the per-word breakdown.
// FIXME: word_counts lowercases nothing, so "The" and "the" count separately.
```

That's deliberate — the demo workspace ships a few real TODO/FIXME markers
for tutorials to use as targets. Click anywhere on the `FIXME` line, or
move the cursor there with the arrow keys, and type a few characters —
anything you like, this is just to see editing happen. Look at the bottom
of the window: the status bar now shows a small dirty-buffer glyph next to
the file path, meaning there are unsaved changes.

Undo your edit with **`Ctrl+Z`** (redo is **`Ctrl+Shift+Z`**) — these two
are wired the same in every keymap, so they never change no matter which
one you're using.

## Saving

Press **`Ctrl+S`** to save (**File → Save**). The dirty glyph in the
status bar clears once the write succeeds. **`Ctrl+Shift+S`** is Save As,
for writing the buffer out under a different name or location.

## Working with multiple tabs

Let's open a few more of the demo files. Switch focus back to the
explorer (**`Ctrl+E`**), navigate to `scripts/hello.py`, and press
`Enter`. Do the same for `notes.md` and `data/sample.csv`. Each opens in
its own tab across the top of the editor pane.

- **`Ctrl+Tab`** moves to the next tab; **`Ctrl+Shift+Tab`** moves to the
  previous one. Both work the same in every keymap.
- **`Ctrl+W`** closes the active tab (**File → Close**).
- **`Ctrl+Shift+T`** reopens the most recently closed tab, if you close
  one by mistake (**File → Reopen Closed Tab**).
- **`Ctrl+Shift+O`** opens a chooser of recently opened files
  (**File → Open Recent…**), handy once you've closed a few tabs.

With `notes.md` active, try **Tools → Markdown Preview** — a read-only,
formatted view of the file, scrolled to wherever your cursor was. Press
`Esc` or `q` to close it and return to editing. See
[`docs/markdown-preview/index.md`](../../markdown-preview/index.md).

## The command palette

**`Ctrl+P`** opens the command palette — the fastest way into almost
anything in Vix, including things with no keybinding at all. With no
prefix it fuzzy-finds files, so from anywhere you can type `hello` and
press `Enter` to jump straight to `scripts/hello.py`, no matter which tab
is currently active.

A few more things worth trying right now, still inside the demo
workspace:

- `main.rs:20` — opens `examples/demo-workspace/rust-app/src/main.rs` at
  line 20 (append `:<col>` too, if you want a specific column).
- `>save` — type `>` to switch into command mode, then search commands by
  name instead of remembering a keybinding.
- `@word_counts` — with `main.rs` focused, type `@` to fuzzy-jump to a
  declaration in the current file; `word_counts` jumps straight to that
  function.
- `#notes` — type `#` to switch between open buffers by name.
- `:15` — type `:` and a line number to preview and jump within the
  current file as you type.

The full prefix table and matching rules are in
[`docs/command-palette/index.md`](../../command-palette/index.md).

## The menu bar

Everything reachable by keybinding or palette is also on the menu bar
along the top of the window:
**Vix · File · Edit · View · Go · Run · AI · DB · JJ · Git · Org ·
Project · Tools · Help**.

Press **`F10`** to open the bar, or jump straight into one menu with its
Alt mnemonic — **`Alt+F`** for File, **`Alt+E`** for Edit, **`Alt+T`** for
Tools, and so on. Once a menu is open, **arrow keys** navigate, **Enter**
runs the highlighted item, and **Esc** closes it. An item marked `▸` opens
a one-level submenu with `→` or a click.

Try it: press `Alt+F` to open **File**, and look at the shape of the
items — **New**, **Open…**, **Save**, **Save As…**, each with its
keybinding right-aligned next to it. That's the same layout every menu
uses. The complete menu map, including every submenu, lives in
[`docs/menus/index.md`](../../menus/index.md).

## A basic find

With `examples/demo-workspace/rust-app/src/main.rs` focused, press
**`Ctrl+F`** to open the find box and type `FIXME`. Press **Enter** to
jump to the match; **`Ctrl+G`** (or `F3`) repeats the search forward,
**`Ctrl+Shift+G`** (or `Shift+F3`) repeats it backward. **`Esc`** closes
the box.

That's the basic case — searching one open file. Find-and-replace,
interactive query-replace, and workspace-wide search across every file in
`examples/demo-workspace/` all build on the same box and get their own
tutorial (`02-editing-power-techniques`). For the full reference now, see
[`docs/find/index.md`](../../find/index.md).

## A tour of the status bar

The band across the very bottom of the window is the status bar. Toggle it
with **View → Layout → Bottom Status**. With
`examples/demo-workspace/rust-app/src/main.rs` open and a change unsaved,
it reads roughly:

```
 rust-app/src/main.rs ●  —  Saved        Rust  LF  UTF-8   Ln 20:Col 9   
```

From left to right:

- the active **file path**, with a small dirty-buffer glyph (`●`) when
  there are unsaved changes;
- the latest **status message**, after an em-dash;
- if the workspace is a git repository, the current **branch name** —
  clicking it opens the Git Changes panel;
- on the right, the buffer's **language**, **line ending** (LF/CRLF),
  **encoding**, the **selection size** when text is selected, and the
  **cursor position** as `Ln <line>:Col <col>`;
- a small **calendar icon** at the far right.

Full detail is in [`docs/status-panel/index.md`](../../status-panel/index.md).

## A tour of the docks

You've already met the left dock (the file explorer). Two more panes frame
the editor:

- The **right dock** is a message drawer — dismissable advice and
  notifications. Toggle it from **View → Layout → Right Dock**. See
  [`docs/right-dock/index.md`](../../right-dock/index.md).
- The **bottom dock** is a scrollable strip for command output, search
  results, and similar text-heavy views — it stays empty and hidden until
  something needs it, such as running a task or a workspace-wide search.
  Toggle it from **View → Layout → Bottom Dock**. See
  [`docs/bottom-dock/index.md`](../../bottom-dock/index.md).

Try the bottom dock now: press **`Ctrl+Shift+F`** to search the whole
workspace and type `TODO` — matches from
`examples/demo-workspace/rust-app/src/main.rs`, `scripts/hello.py`, and
`notes.md` all stream into the bottom dock at once, each with its file and
line number.

## Quitting

**`Ctrl+Q`** (**Vix → Quit**) exits Vix. If any buffer has unsaved
changes, Vix asks before discarding it — so it's safe to press even if
you forgot to save something above.

## Next

That covers first-session orientation: opening a workspace, moving around
the explorer and tabs, the palette, the menu bar, a basic find, and what
the status bar and docks are telling you. Multi-cursor editing, structural
replace, and the rest of Vix's editing toolkit are next, in
[Tutorial 2: Editing Power Techniques](../02-editing-power-techniques/index.md).

---

Vix™ and Vix IDE™ are trademarks.
