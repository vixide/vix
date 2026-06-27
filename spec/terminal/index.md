# Integrated Terminal

**Tools → Terminal** (action `tools.terminal`) opens a real interactive shell on a
pseudo-terminal inside Vix. Running the action again, or pressing `Ctrl+]`, closes
it.

## Behavior

- A PTY is created with `portable-pty` (Unix `openpty` / Windows ConPTY) and the
  user's shell (`$SHELL`, else `/bin/sh`; `cmd.exe` on Windows) is spawned in the
  workspace root with `TERM=xterm-256color`.
- A reader thread feeds the shell's output into a shared `vt100::Parser`, which
  maintains the screen grid; the UI renders that grid each frame (cell colors,
  bold/italic/underline/inverse, and the cursor).
- While open, the terminal **captures all key input** and forwards it to the shell
  (`terminal::encode_key` maps crossterm events to byte sequences: control combos,
  Alt-as-meta, arrows, function keys, etc.). The one exception is `Ctrl+]`, which
  closes the terminal.
- The PTY is resized to match the overlay each frame so the shell wraps correctly.
- When the shell exits, `poll_terminal` closes the overlay and reports
  `status.terminal_closed`.

## As implemented in Vix

The `terminal` module owns the PTY/child/parser and the key encoder (unit tested).
The host owns `toggle_terminal`, `terminal_key`, `poll_terminal`, and
`terminal_running`; `ui::draw_terminal` renders the `vt100` screen. The main loop
polls fast while a terminal is open so output appears promptly.

## Limitations

- Mouse reporting is not forwarded to the shell.
- Scrollback beyond the visible grid is not shown (the grid tracks the live
  screen only).
