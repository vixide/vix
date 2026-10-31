# Undo Tree

Vix never throws undo history away. When you undo a few steps and then type
something new, the old steps stay in the history as a separate branch, so
the history is a tree rather than a line. **Edit → Undo Tree…** shows that
tree and takes you to any state in it.

## Using it

Open **Edit → Undo Tree…** (`edit.undo_tree`, also in the command palette
as "Undo Tree"). A panel lists every state of the active buffer, oldest
first, with each branch in its own column:

```text
o      0  original
o      1  12:00:01
|\
o |    2  12:00:04
  @    3  12:00:09
```

- `o` is a state and `@` the one you started at. The number is the state's
  sequence number, in the order the states were made, and the time is when
  it was made, on your local 24-hour clock.
- `|\` marks the point where a later edit started a new branch.

| Key                   | Effect                                              |
| --------------------- | --------------------------------------------------- |
| `↑` / `↓`             | Select the previous / next state                    |
| `PageUp` / `PageDown` | Move a page                                         |
| `Home` / `End`        | Select the original / the newest state              |
| `Enter`               | Keep the selected state                             |
| `Esc`                 | Go back to where you started                        |

The buffer changes as soon as the selection moves, so you see each state's
text in the editor behind the panel. `Enter` keeps the state you are on. It
becomes the current point in the history, and Redo then follows the branch
you came down. `Esc` puts back the text, the cursor, the selection and the
tab's modified flag exactly as they were.

**Edit → Switch Undo Branch** (`edit.undo_branch`) is still there for
choosing which branch the next Redo follows without opening the panel.

## When it is not available

- **Shared buffers.** In a Cowork shared buffer local undo is off: Undo
  there reverses only your own edits, through the collaboration session.
  The status bar says so, and the panel does not open.
- **No edits yet.** A buffer with no history shows a status message instead.
- **Read-only tabs** block it, as they block any change to the text.

## Safety

The text you preview while the panel is open is never written anywhere.
Auto-save, the hot-exit backup and the check for files changed on disk all
wait until the panel closes, and quitting with the panel open first puts the
starting state back. [Persistent undo](../undo-store/index.md) saves the
whole tree when you save; you cannot save while the panel is open, so the
saved tree always matches the saved text.

See [`crates/vix-undo-tree-panel/spec/index.md`](../../crates/vix-undo-tree-panel/spec/index.md)
for the specification.
