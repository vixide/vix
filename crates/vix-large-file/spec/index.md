# Large File Viewer

The Large File Viewer: a read-only, chunked view of multi-gigabyte files with a
lazy line index (T733). It shows a file too big to load into a text buffer —
a multi-gigabyte log, dump, or data export — a screen at a time, without ever
reading the whole file into memory: the answer to "open this 4 GiB log" where
an editor buffer (capped at 1 GiB, T571) would refuse, or take seconds and
gigabytes.

## Opening

- **File → Open in Large File Viewer…** (`file.open_large`, also in the
  command palette) asks for a path, seeded with the active buffer's file.
- Opening a file above `editor_behavior.large_file_mb` asks first (T571); that
  confirmation now also offers **v** — view it read-only in the Large File
  Viewer — beside **y** (load it into a buffer anyway) and **n**.
- A file above the 1 GiB hard ceiling, which no buffer may hold, used to be
  refused outright; it now asks whether to view it in the Large File Viewer
  instead (`ConfirmKind::OpenLargeViewer`).
- Only regular files open (a FIFO or device is refused: it could block or
  never end).

The viewer reads **local files only**. It opens the path with `std::fs`, not
through `vix-authority` (T710), so on a remote workspace the action and the
confirmation offers are refused with a message. A ranged read on the
`Authority` trait (with an `SshAuthority` implementation over `dd`/`tail -c`)
is the natural follow-on; it was left out while T717 reworks
`vix-authority`.

## Reading without loading

- **Positioned reads.** `Source` reads fixed-size chunks at an offset —
  `pread` (`std::os::unix::fs::FileExt::read_at`) on Unix, `seek_read`
  (`std::os::windows::fs::FileExt`) on Windows. Both are safe std APIs, so
  there is no memory map and no `unsafe`. Each reader (the view, the index
  thread, a search thread) opens its own handle, so none shares a file cursor.
- The file's length is taken once at open. Bytes appended later are not
  shown; a file truncated underneath reads short rather than failing.
- **Rendering reads only what is drawn**: the visible lines, and of each only
  the bytes that fit the width (up to 4 bytes per column).

## The line index

`LineIndex` scans the file once, on a background thread, in 1 MiB chunks
(`memchr` for the newlines). It keeps only the byte offset of every 1,024th
line (`DEFAULT_STRIDE`): about 330 KB of checkpoints for a 4 GiB file of
100-byte lines. Going to line *N* reads from the nearest checkpoint, at most
one stride of lines. The view does not wait for the index: the first screen,
paging, goto-percent, goto-byte, goto-end, and search all work immediately;
only a goto-line past the indexed part waits, and is applied as soon as the
index reaches it (the status line says so). The status line shows the
cursor's line number once the index has passed it, and the total once done.

A trailing newline ends the last line rather than starting an empty one; an
empty file has one line. If a read error stops the index, the status line
says line numbers are unavailable; everything else keeps working.

## Lines as displayed

- Invalid UTF-8 shows as U+FFFD (decoded lossily per visible window).
- A CRLF ending's `\r` is not shown.
- Tabs and other control characters show as one `·` column each.
- A long line is cut to the view's width, with `›` marking more to the
  right. **←/→** scroll horizontally by 8 bytes (a `‹` marks hidden text to
  the left); after a scroll, a window never starts mid-character. **0**
  scrolls back to the left edge. A single line of hundreds of megabytes is
  shown, but stepping past it means scanning it for its end.
- A dimmed gutter shows each line's starting byte offset.

## Keys

| Key | Does |
| --- | --- |
| ↑ ↓ / `k` `j` | Move the cursor line |
| PgUp / PgDn | Page |
| Home / `g`, End / `G` | First line, last line (no index needed) |
| ← → / `h` `l`, `0` | Scroll horizontally; back to the left edge |
| `:` or Ctrl+G | Go to: `1234` a line, `50%` or `12.5%` a percentage, `@1024` or `0x400` a byte offset (`_` and `,` separators allowed) |
| `/`, `?` | Search forward, backward |
| `n`, `N` | Repeat the search the same way, the other way |
| `m` or Space | Start or end a line selection; Shift+↑/↓ extend one |
| `y`, `c`, Ctrl+C | Copy the selected lines (or the cursor's line) |
| Esc | Cancel a running search, else clear the selection, else close |
| `q` | Close |

## Search

A search streams through the file on a background thread from the cursor's
line (forward from its start, backward from its end; repeated, from just past
the last match), reading 1 MiB windows that overlap by the needle's length so
a match across a window edge is found. The status line shows its progress;
**Esc** cancels it. It is a plain substring search with smart case: a needle
with no uppercase letter matches ASCII letters of either case. It does not
wrap around. A match moves the cursor to its line, scrolls it into view
horizontally, and is highlighted.

## Copy

Copy puts the selected lines — from the first's start through the last's
newline — on the clipboard, decoded lossily. A selection larger than 16 MiB
(`COPY_LIMIT`) is refused with a message rather than read into memory.

## Not done

- **Editing.** The viewer is read-only by design; editing a multi-gigabyte
  file is out of scope.
- **Remote files** (see Opening).
- Regular-expression search, search wrap-around, and a match count.
- Following a growing file (`tail -f`).
- Mouse scrolling and clicking in the viewer.
- Character-width-aware columns: horizontal scroll counts bytes, and wide
  (CJK) characters take two cells while counting as one.

## As implemented in Vix

`vix-large-file` holds the logic, host-agnostic and terminal-free:
`Source` (positioned chunk reads and line navigation), `LineIndex` and the
pure `scan_chunk`, `Search` and the pure `find`, and `Viewer` (cursor, scroll,
selection, goto, notices) whose `handle_key` returns an `Outcome`
(`Consumed`/`Close`/`Copy`). The App shell (`tui/src/app/large_file.rs`)
opens it, routes keys, puts copies on the clipboard, and polls it each tick
(ticking every 50 ms while it indexes or searches);
`tui/src/ui/large_file.rs` draws it over the editor region.

Measured numbers (time to first screen, goto-end, full index, search, peak
memory) and how to reproduce them (`scripts/bench-large-file`) are in
[`docs/performance`](../../../docs/performance/index.md#large-file-viewer-t733).
