# Large File Viewer

The Large File Viewer opens a file too big for a text buffer — a
multi-gigabyte log, dump, or data export — read-only, a screen at a time. It
never loads the whole file: it reads only the lines on screen, and builds a
small line index in the background. On the reference machine a 4 GiB log
shows its first screen in under a millisecond, jumps to its end in under a
millisecond, and is fully indexed in about half a second, using under 5 MiB of
memory (see [Performance](../performance/index.md#large-file-viewer-t733)).

## Opening

- **File → Open in Large File Viewer…**, or *Open in Large File Viewer* in the
  command palette, asks for a path (seeded with the active file).
- Opening a file larger than your `large_file_mb` setting asks first; press
  **v** there to view it in the Large File Viewer instead of loading it.
- A file over 1 GiB is too large for any buffer; Vix offers the viewer instead.

The viewer reads files on this machine only, not on a remote (SSH) workspace.

## Moving around

- **↑ / ↓** (or **k / j**) move a line; **PgUp / PgDn** page.
- **Home** / **g** goes to the first line, **End** / **G** to the last —
  instantly, even before indexing finishes.
- **← / →** (or **h / l**) scroll sideways through long lines; **0** returns
  to the left edge. **›** and **‹** mark text beyond the edges.
- **:** (or **Ctrl+G**) goes to a line (`1234`), a percentage (`50%`), or a
  byte offset (`@1024` or `0x400`). A line the index hasn't reached yet is
  jumped to as soon as it has.

## Searching

**/** searches forward, **?** backward; **n** and **N** repeat. The search
streams through the file in the background with a progress percentage, and
**Esc** cancels it. A search in lowercase ignores case; one with a capital
letter matches case exactly.

## Copying

**m** (or **Space**) starts a selection at the cursor line; move to extend it
(**Shift+↑/↓** also extends). **y**, **c**, or **Ctrl+C** copies the selected
lines, or the cursor line if nothing is selected. Copies are limited to
16 MiB.

## What it shows

The status line shows the cursor's byte offset and position through the file,
its line number once indexed (and the total once done), and indexing and
search progress. A dimmed gutter shows each line's byte offset. Invalid UTF-8
shows as �, Windows line endings are handled, and control characters show as
`·`.

**q** or **Esc** closes the viewer. It is read-only: editing a file this size
is out of scope.

See the specification at
[`crates/vix-large-file/spec/index.md`](../../crates/vix-large-file/spec/index.md).

---

Vix™ and Vix IDE™ are trademarks.
