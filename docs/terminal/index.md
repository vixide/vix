# Integrated Terminal

Run a shell without leaving Vix™. **Tools → Terminal** (or the command palette)
opens a real interactive terminal — your usual shell, in the workspace directory —
right inside the editor.

## Using it

- Open with **Tools → Terminal**. Vix launches your shell (`$SHELL`, or `/bin/sh`;
  `cmd.exe` on Windows) on a pseudo-terminal.
- Type as you would in any terminal: full-screen programs (`vim`, `htop`, `git`
  pagers), colors, and the cursor all work — output is parsed with a real terminal
  emulator (`vt100`).
- **Hide** the terminal with `Ctrl+]` (or **Tools → Terminal → Show/Hide
  Terminal**). Hidden terminals keep running; show them again the same way.
- **Close** a terminal with **Tools → Terminal → Close Terminal**. It also closes
  automatically when its shell exits.

While the terminal is shown it receives every keystroke (so Vix's own shortcuts
are paused), apart from a few chords of its own:

| Keys | Does |
|------|------|
| `Ctrl+]` | Hide the terminal and return to the editor |
| `Shift+PgUp` / `Shift+PgDn` | Scroll back / forward a page |
| `Shift+Home` / `Shift+End` | Jump to the oldest kept line / back to the live screen |
| `Ctrl+PgUp` / `Ctrl+PgDn` | Previous / next terminal |

The terminal resizes with the window so programs lay out correctly.

## Several terminals

**Tools → Terminal → New Terminal** opens another shell as a new tab. With more
than one, a row of numbered tabs shows above the screen, the active one
highlighted. Switch with `Ctrl+PgUp` / `Ctrl+PgDn` or **Next Terminal** /
**Previous Terminal**; **Close Terminal** stops the active one.

## Scrollback

Each terminal keeps its last 2000 lines. Scroll back with `Shift+PgUp` or the
mouse wheel; the title says how far back you are, and typing anything returns to
the live screen. Change how many lines are kept with the `terminal_scrollback`
setting (at most 50 000; `0` keeps none); it applies to terminals opened
afterwards.

## Mouse and copy

- **Select** text by dragging with the left button; letting go copies it to the
  clipboard. **Tools → Terminal → Copy Terminal Text** copies the selection, or
  the whole visible screen when nothing is selected.
- **Programs that use the mouse** (`htop`, `vim` with `mouse=a`, `tmux`) get your
  clicks, drags and wheel. Hold `Shift` to select text in Vix instead.

## Notes

- In a remote workspace every terminal is a shell on the remote machine; see
  [Remote editing](../remote-editing/index.md).

See the specification at `crates/vix-terminal/spec/index.md`. For one-off, non-interactive
commands whose output you want to keep, use **Tools → Run Command** (it streams to
the bottom dock) or define a [task](../tasks/index.md).

---

Vix™ and Vix IDE™ are trademarks.
