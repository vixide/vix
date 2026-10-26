# Pipe Selection Through a Command

Editor actions `edit.pipe_selection` and `edit.insert_command_output` (T659).

Both open a prompt for a shell command. The selection, or the whole buffer
when nothing is selected, goes to the command on stdin. The command runs in
the workspace root with the same platform shell as Run Command (`sh -c` on
Unix, `cmd /C` on Windows), through the one shell launcher in
`tui/src/app.rs`.

- **`edit.pipe_selection`**: the output replaces the selection (or the whole
  buffer), as one undo step. This is Emacs `M-|` with a prefix argument,
  Vim `!`, Helix `|`, and Fresh `Alt+|`.
- **`edit.insert_command_output`**: the output is inserted at the cursor and
  the input is left alone. This is Fresh `Alt+Shift+|`.

## Result

- **Success** (exit status 0): the buffer changes as described above. When
  the input had no trailing newline and the output ends in exactly one, that
  newline is dropped, so `sort` on a mid-line selection doesn't add a line
  break.
- **Failure** (non-zero exit, a spawn error, or the timeout): the buffer is
  not changed. The command line and its stderr go to the bottom dock, which
  is shown, and the status line reports the failure.
- **Timeout**: the command is killed after 10 seconds. The editor waits for
  the command, like Emacs `shell-command-on-region`, because the result
  applies to the text as it was when the command started.

Input bytes are passed through unchanged (CRLF stays CRLF). Output that isn't
valid UTF-8 is decoded lossily.

## Access

- **Edit → Lines → Pipe Through Command…** and **Insert Command Output…**,
  and the command palette.
- The Emacs keymap binds `M-|` to `edit.pipe_selection`.
- A read-only buffer refuses both, like every other edit.

## Gating

Both actions belong to the `tasks` Cargo feature (see
`spec/cargo-features/index.md`), the same as Run Command, so a build without
`tasks` leaves them out. This is a prompt, not ex-command scripting, so it
fits the `vix-modal` rules.

See `crates/vix-editor-core/spec/index.md` for the full action catalog.
