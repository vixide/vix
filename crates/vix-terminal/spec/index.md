# Integrated Terminal

Integrated terminal: a real PTY hosting a shell, parsed by `vt100`.
**Tools → Terminal → Show/Hide Terminal** (action `tools.terminal`) opens a real
interactive shell on a pseudo-terminal inside Vix, or shows and hides the
terminals already open. Several terminals can be open at once, as tabs (T631).

## Behavior

- A PTY is created with `portable-pty` (Unix `openpty` / Windows ConPTY) and the
  user's shell (`$SHELL`, else `/bin/sh`; `cmd.exe` on Windows) is spawned in the
  workspace root with `TERM=xterm-256color`.
- A reader thread feeds the shell's output into a shared `vt100::Parser`, which
  maintains the screen grid and a bounded scrollback; the UI renders the grid each
  frame (cell colors, bold/italic/underline/inverse, and the cursor).
- While shown, the terminal **captures all key input** and forwards it to the
  active shell (`terminal::encode_key` maps crossterm events to byte sequences:
  control combos, Alt-as-meta, arrows, function keys, etc.), except its own
  chords:
  - `Ctrl+]` hides the overlay. Hidden terminals keep running; `tools.terminal`
    shows them again.
  - `Shift+PgUp` / `Shift+PgDn` page the scrollback (a page is the screen height
    less one line); `Shift+Home` jumps to its top and `Shift+End` back to the live
    screen.
  - `Ctrl+PgUp` / `Ctrl+PgDn` switch to the previous / next terminal.

  Any other key first returns the view to the live screen, then goes to the
  shell.
- The PTY is resized to match the overlay each frame so the shell wraps correctly.
- When a shell exits, `poll_terminal` reports it once (`status.terminal_exited`)
  and the tab **stays**, marked `exited` in the title, with its transcript. Enter
  restarts the shell in place; any other key closes the tab
  (`status.terminal_closed`); the overlay goes with the last tab (T666).
- Opened before the first frame has been drawn (no editor area yet), the PTY
  starts at 24 x 80 rather than 1 x 1; the first frame resizes it.
- `Terminal::send_bytes` writes already-encoded input. Its callers are Cowork's
  shared terminal (T697, `spec/cowork`: keystrokes from a guest the host allowed
  to type) and mouse forwarding (below). The PTY itself never leaves this
  machine; only its rendered screen is shared.

## Multiple terminals (T631)

`App::terminals` is a `vix_terminal::Tabs<Terminal>`: an ordered set of tabs,
one active, each with a stable id that is never reused, plus whether the overlay
is shown. The pure bookkeeping (`push`, `next`/`prev`/`select`, `remove`,
`close_active`, `retain`) is generic and unit-tested without a PTY.

The **Tools → Terminal** submenu:

| Item | Action | Does |
|------|--------|------|
| Show/Hide Terminal | `tools.terminal` | Shows the overlay, opening a first terminal if none is open; hides it when shown. |
| New Terminal | `terminal.new` | Opens another terminal as a new tab, makes it active and shows the overlay. |
| Next Terminal / Previous Terminal | `terminal.next` / `terminal.prev` | Activates the next / previous tab (wrapping) and shows the overlay. |
| Copy Terminal Text | `terminal.copy` | Copies the active terminal's selection, or its whole visible view, to the clipboard. |
| Send Selection to Terminal | `terminal.send_selection` | Sends the editor's selection, else its current line, to the active terminal as a command and presses Enter (T666). |
| Restart Terminal | `terminal.restart` | Runs the active terminal's command again on a fresh PTY in place (T666). |
| Toggle Terminal Key Capture | `terminal.toggle_capture` | Chooses whether Ctrl/Alt chords reach the shell or the editor (T666). |
| Close Terminal | `terminal.close` | Stops the active terminal's shell and removes its tab; the next tab becomes active. |

With no terminal open, the last four say so (`status.terminal_none`). With more
than one terminal, the overlay's first row is a tab strip (`1`, `2`, …), the active
tab highlighted. The overlay stays where the terminal has always been drawn (over
the editor area) rather than moving into the bottom dock: the dock is a line
buffer with its own focus, scrolling and click handling, and a terminal there would
need a second key-capture path.

The OC menu's container shell (`oc.shell`) opens as a new tab beside any shells
already open.

## Scrollback (T631)

Each terminal's parser keeps the last `terminal_scrollback` lines (a setting;
default `DEFAULT_SCROLLBACK` = 2000, clamped to `MAX_SCROLLBACK` = 50 000, 0 for
none), read when the terminal opens. A `vt100` cell is about 36 bytes, so the
default at 120 columns is roughly 9 MB per terminal, and the cap bounds the worst
case. While scrolled back the title says how far (`ui.terminal_scrolled`) and the
caret is hidden; output that arrives meanwhile keeps the view on the same lines.
The full-screen (alternate) screen has no scrollback.

`Terminal::scroll_by`, `scroll_to_bottom` and `scroll_offset` are the API; the
pure half, `scroll_parser`, works on a bare `vt100::Parser` and is what the unit
tests drive. `vt100` 0.16 is required: 0.15 panicked (or, in release builds,
showed the wrong rows) when the view was scrolled back further than the screen is
tall.

## Mouse: selection, copy and forwarding (T631)

The renderer records the terminal's screen rectangle (`Layout::terminal`).

- When the child has asked for mouse reports (`vt100`'s mouse protocol mode is
  not `None`), clicks, releases, drags (in the motion modes), motion (any-motion
  mode) and the wheel over the screen go to it, encoded by `encode_mouse` in the
  encoding it chose: SGR (`ESC [ < b ; x ; y M/m`), UTF-8, or the legacy bytes,
  with Shift/Alt/Ctrl added to the button code. Holding Shift selects in Vix
  instead, as in xterm.
- Otherwise the wheel scrolls the scrollback three lines a notch, and a left drag
  selects text (reading order, end cell included; shown in reverse video). When
  the button comes up over a non-empty selection it is copied, through the open
  buffer's fallback-aware copy when there is one (else `vix-clipboard`), and the
  status says so (`status.terminal_copied`). The selection is in view
  coordinates, so scrolling, resizing and typing clear it.

## Terminal-to-editor bridge (T666)

- **Working directory (OSC 7).** The reader thread watches the shell's output for
  `ESC ] 7 ; file://host/path` (ended by BEL or `ESC \`), across read-chunk
  boundaries (`Osc7Scanner`, pure and unit tested; the host part is ignored,
  `%XX` is decoded, `file:///C:/x` becomes `C:/x`). `Terminal::cwd` is the last
  directory announced, else the directory the terminal started in. Shells
  announce it with a prompt hook (zsh/bash/fish integration, or
  `printf '\033]7;file://localhost%s\007' "$PWD"`); without one, paths resolve
  against the starting directory and then the workspace root.
- **Open from output.** Ctrl+click on terminal output (the view, so scrollback
  too) opens a location and hides the overlay. The clicked word is read as
  `path[:line[:col]]` (`word_in`, quotes/brackets/trailing punctuation trimmed);
  if that file exists (against the terminal's directory, then the workspace
  root) it opens there. Otherwise the whole row goes through the same problem-matcher
  parser as the task dock (`vix_tasks::matcher::locate`, T632 - one parser, via
  `App::output_location`, shared with `dock_line_location`), which finds rustc
  arrows, Python frames, gcc/tsc/eslint/go lines and so on. Nothing found: the
  click does nothing. When the child asked for the mouse, Ctrl+click still opens a
  location if there is one under the pointer, else goes to the child.
- **Send to Terminal.** `Terminal::send_text` wraps the text in bracketed-paste
  markers when the shell asked for them (`paste_bytes`), else sends newlines as
  carriage returns, then Enter. With no terminal open one is opened; with no
  buffer or only blank text, `status.terminal_nothing_to_send`.
- **Restart in place.** `Terminal::restart` starts the saved command (program,
  arguments and directory) on a new PTY **sharing the same parser**, so screen
  and scrollback are kept; it first leaves the alternate screen (only if the old
  shell was on it) and mouse modes, and writes a separator line
  (`ui.terminal_restarted`). It works on a live terminal too (the old child is
  killed once the new one has spawned), and the OSC 7 directory resets.
- **Key capture.** `Ctrl+\` or `terminal.toggle_capture` flips a per-terminal flag
  (default on). With capture off, keys carrying Ctrl or Alt (other than the
  overlay's own chords: `Ctrl+]`, `Ctrl+\`, `Ctrl+PgUp`/`Ctrl+PgDn`) are
  not forwarded; `terminal_key` returns `false` and they fall through to the
  editor's normal handling while the overlay stays open. Plain and Shift keys
  still go to the shell. The title says `editor keys` while it is off.

## Remote workspaces (T716)

The PTY is always this machine's; what runs on it comes from the workspace
authority (`vix_authority::Authority::terminal`). Locally that is the shell
above, in the workspace root, unchanged. In an SSH workspace it is `ssh -t` over
the workspace's shared connection (`ControlMaster`), whose far end starts the
remote user's login shell (`$SHELL -l`, else `/bin/sh -l`) in the remote
workspace directory; the local `ssh` starts in Vix's own working directory.
In an untrusted remote workspace (T714) the remote shell starts in the remote
home directory instead, so nothing the workspace defines (a shell `cd` hook,
an `.envrc`) runs by itself, and the status line says so
(`status.remote_terminal_home`). Closing the terminal kills the local `ssh`,
which ends the remote shell. `Terminal::open_command` takes any `OsStr`
program and arguments. Every new terminal tab (T631) goes through the same
authority, so each is its own remote shell over the one shared connection.

## Cowork (T697, T631)

The shared terminal is the tab that was active when the host confirmed
**Share Terminal**, pinned by its id. Switching tabs on the host never changes
what guests see, guests' keystrokes (when allowed) go only to that tab, closing
another tab leaves the share alone, and closing the shared tab ends it. With more
than one terminal the shared tab is marked in the tab strip
(`ui.terminal_shared_tab`). What is shared is always the live screen
(`Terminal::live_screen`): the host scrolling back is not sent.

## As implemented in Vix

The `vix-terminal` crate owns the PTY/child/parser, scrolling, the selection, the
key and mouse encoders and the tab set (unit tested). The host owns the actions
in `tui/src/app/terminal.rs` (`toggle_terminal`, `terminal_key`,
`terminal_mouse`, `poll_terminal`, `terminal_running`, `terminal_visible`, …),
with no-op stand-ins in `terminal_off.rs` for a build without the `terminal`
feature; `ui::draw_terminal` renders the active screen and the tab strip. The main
loop polls fast while any terminal is open, shown or not, so output appears
promptly. Integration tests (`tui/tests/integration/terminals.rs`, Unix) drive
real shells: two tabs that each get only their own keys, switching, hiding and
closing, and scrollback paging, the cap and wheel scrolling.

## Limitations

- A remote terminal needs a connection that doesn't prompt (`BatchMode`, as
  every remote operation); with connection sharing off (Windows' OpenSSH, or
  a control directory path too long for a socket) it makes a new connection.
- Tabs are numbered, not named: `vt100` 0.16 no longer exposes the window title
  the shell sets.
- OSC 7 is only seen when the shell prints it; Vix does not inject a prompt
  hook. A remote (SSH) terminal reports the remote directory, which Ctrl+click
  resolves through the workspace authority.
- Ctrl+click maps the click to a character column, so a row with wide (CJK)
  characters before the path can pick the neighboring word.
- Clicking a tab in the strip does not switch to it; use `Ctrl+PgUp`/`Ctrl+PgDn`
  or the menu.
