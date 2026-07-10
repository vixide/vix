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
- **Close** the terminal with `Ctrl+]` (or run **Tools → Terminal** again). It also
  closes automatically when the shell exits.

While the terminal is focused it receives every keystroke (so Vix's own shortcuts
are paused) — `Ctrl+]` is the one reserved chord that returns you to the editor.

The terminal resizes with the window so programs lay out correctly.

## Notes

- Mouse events are not forwarded to the shell yet.
- The view shows the live screen; terminal scrollback isn't browsable from Vix.

See the specification at `spec/terminal/index.md`. For one-off, non-interactive
commands whose output you want to keep, use **Tools → Run Command** (it streams to
the bottom dock) or define a [task](../tasks/index.md).

---

Vix™ and Vix IDE™ are trademarks.
