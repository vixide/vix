# Status Bar

The status bar is the bottom band of the window. It shows, at a glance, the
active file and its state, the keymap mode, the latest status message, and — on
the right — details about the current buffer and cursor. A full-width top border
separates it from the body above.

## Show / Hide

Toggle the bar with **View → Show/Hide Bottom Status** (the `show_status_bar`
setting).

## Left Side

The left segment shows:

- the **keymap mode** indicator (for Vim or Emacs keymaps), when active;
- the **file path** of the active buffer;
- a **dirty flag** glyph when the buffer has unsaved changes;
- the latest **status message**, after an em-dash separator.

## Git Branch

When the workspace is a git repository, the bar shows the current **branch name**,
with a `•` dirty dot when the working tree has changes. **Clicking the branch
indicator opens the Git Changes panel.** See `docs/git-panel/index.md`.

## Right Side

The right segment shows buffer and cursor details:

- the buffer's **language** (omitted for a non-text tab);
- the **line ending** (LF or CRLF);
- the **encoding** (UTF-8);
- the **selection size**, rendered as `Sel {chars} ({lines}L)` when text is
  selected;
- the cursor position, as `Ln {line}:Col {col}`;
- a **calendar icon** at the far right.

## Example

Editing an unsaved Rust file on line 42, column 9, on the `main` branch with
changes, the bar reads roughly:

```
 src/app.rs ●  —  Saved        Rust  LF  UTF-8   Ln 42:Col 9   
```

Clicking the `main •` branch indicator opens the Git Changes panel.

---

Vix™ and Vix IDE™ are trademarks.
