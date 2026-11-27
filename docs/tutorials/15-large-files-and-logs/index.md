# Tutorial 15: Large Files and Logs

Most files you open are small. Now and then one isn't: a 100 MB log, a 4 GiB
database dump, a data export that never fits on a screen. This tutorial covers
what Vix does when you open a file like that. You'll see the checks every open
goes through, the prompt for big files, the read-only **Large File Viewer** for
files too big for a buffer, and why a big buffer stays responsive while you
scroll.

You'll make your own test files with shell one-liners, so you don't need a
real multi-gigabyte log. Run the commands in a scratch directory with about
2 GiB of free space, and delete the files when you're done.

## What every open checks

Every file Vix opens as text goes through one guarded read first. That read
refuses three kinds of file before Vix tries to load them:

| The file is… | What Vix does |
| --- | --- |
| Not a regular file (a FIFO, socket, device, or directory) | Refuses it: "not a regular file (pipe, socket, device, or directory)". Reading one could block forever or never end. |
| Larger than the size limit | Asks before loading it (see below). |
| Binary (a NUL byte in the first 8 KiB) | Asks whether to open it in the byte editor instead. |

A file that starts with a byte-order mark is checked for that mark before the
NUL test. That is how UTF-16 text, which is full of NUL bytes, still opens as
text.

The size limit is the `large_file_mb` setting:

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `large_file_mb` | integer | `50` | Ask before opening a file larger than this many megabytes. `0` never asks. |

Whatever you set, a file over **500 MiB** always asks first, with a warning
that loading it may use several times that much memory, and a file over
**1 GiB**, more than a text buffer can ever hold, opens straight in the Large
File Viewer instead. Both apply when `large_file_mb` is `0` too, so setting
it above `500` has no further effect. (Vix counts a
megabyte as 1,048,576 bytes here.)

## Make a big log

Generate a log of two million lines, about 100 MB. One line near the end says
`ERROR`, so you have something to search for later:

```sh
awk 'BEGIN { for (i = 1; i <= 2000000; i++) printf "2026-11-17 12:%02d:%02d %s request %d took %d ms\n", (i/60)%60, i%60, (i == 1987654 ? "ERROR" : "INFO"), i, i % 997 }' > big.log
ls -lh big.log
```

It takes a second or two. `ls` should report about 100M.

## Open it as a buffer

Start Vix in the same directory and open the file:

```sh
vix big.log
```

You can also open it with **File → Open…** (`Ctrl+O`) or from the file
explorer. The file is over the 50 MB default, so instead of loading it Vix
asks (with the file's full path):

```text
big.log is 100 MB. Open it anyway? (v: view read-only in the Large File Viewer)
```

You have three answers:

- **y** loads it into a normal, editable buffer.
- **v** opens it read-only in the Large File Viewer instead (next section).
- **n** or **Esc** cancels. The status line says "Open cancelled".

Press **y**. The buffer opens in well under a second. Plain UTF-8 text streams
straight into the editor's text structure as it is read, so Vix never holds a
second full copy of the file. On the project's reference machine a 100 MB log
shows its first screen in about 90 ms and holds about 126 MiB of memory (see
[Performance](../../performance/index.md#startup-and-memory-vs-other-editors-t744)).

Move around the way you would in any buffer:

- **Go → File → End** jumps to the last line.
- **Go → Go to Percent…** asks for a percentage, such as `50`.
- **Go → Go to Byte…** asks for a byte offset.
- **Go → Line → Number** asks for a line number.
- **Find…** (`Ctrl+F` in the Apple keymap) searches the buffer.

Close the tab when you're done. You'll open the same file in the viewer next.

One catch: the explorer's arrow-key preview never asks. Moving the selection
onto `big.log` only puts "Open failed for …" in the status line.
Press **Enter** on it to get the prompt.

## Open it in the Large File Viewer

The Large File Viewer shows a file a screen at a time and never reads the whole
file into memory. It is read-only. Open it either way:

- Open the file as before and press **v** at the prompt.
- Use **File → Open in Large File Viewer…** (action id `file.open_large`, also
  in the command palette). It asks for a path, filled in with the active file.

The status line says "Viewing big.log read-only", and the viewer replaces the
editor area. A hint row lists the main keys. A dimmed gutter on the left shows
the byte offset where each line starts. The status line shows the cursor's
byte offset and how far through the file it is, as in "Byte 0 of 99.82 MiB
(0%)".

While the viewer builds its line index in the background, the status line
shows "Indexing N%". You don't have to wait for it:

| Key | Does |
| --- | --- |
| ↑ ↓ or `k` `j` | Move the cursor line |
| PgUp / PgDn | Page |
| Home or `g`, End or `G` | First line, last line |
| ← → or `h` `l` | Scroll sideways 8 bytes; `0` returns to the left edge |
| `:` or Ctrl+G | Go to a line, a percentage, or a byte offset |
| `/`, `?` | Search forward, backward |
| `n`, `N` | Repeat the search the same way, the other way |
| `m` or Space | Start or end a line selection; Shift+↑/↓ extends it |
| `y`, `c`, or Ctrl+C | Copy the selected lines, or the cursor's line |
| `q` | Close |
| Esc | Cancel a running search, else clear the selection, else close |

Press **End**. The last line appears at once, even before indexing finishes.
Once the index has passed the cursor, the status line also shows "Line N", and
"Line N of TOTAL" when indexing is done.

## Jump around

Press **:**. The prompt reads `Go to (line, N%, @byte):`. It accepts three
kinds of target:

- `1987654` goes to that line.
- `50%` or `12.5%` goes to that point in the file.
- `@1024` or `0x400` goes to that byte offset.

You can write `_` or `,` as digit separators, as in `1_000_000`. Percentages
and byte offsets work at once. A line number past the part already indexed
waits: the status line says "Indexing… going to line 1987654 when it is
reached", and the jump happens as soon as the index gets there. A line past
the end of the file reports "The file has only N lines".

Try `50%`, then `@0x400`, then `1987654`.

## Search

Press **Home**, then **/**, type `error`, and press **Enter**. The search
starts from the cursor and streams through the file on a background thread.
The status line shows "Searching N% (Esc cancels)". When it finds a match, the
cursor moves to that line and the match is highlighted.

Your search was lowercase, so it matched `ERROR`. A search with no capital
letter ignores the case of ASCII letters. Type a capital letter and the case
must match exactly.

Press **n** to look for the next match, or **N** for the previous one. This
log has only one `ERROR`, so **n** reports "Not found". A search does not wrap
around to the start of the file. It is a plain substring search: no regular
expressions, and no match count.

## Copy a few lines

Put the cursor on the `ERROR` line, press **m**, press **↓** twice, and press
**y**. The three lines go to the clipboard and the status line says how much
was copied. With no selection, **y** copies just the cursor's line. A
selection larger than 16 MiB is refused with a message rather than read into
memory.

Press **q** to close the viewer.

## Past the 1 GiB ceiling

A file over 1 GiB cannot be a buffer at all. Make one by repeating your log
eleven times (about 1.1 GB):

```sh
for i in 1 2 3 4 5 6 7 8 9 10 11; do cat big.log; done > huge.log
```

Open `huge.log`. This time Vix doesn't offer a buffer:

```text
huge.log is 1099 MB, too large to edit. View it read-only in the Large File Viewer?
```

Press **y** to open it in the viewer. Everything from the last few sections
works the same. On the reference machine a 4 GiB log shows its first screen in
under a millisecond, is fully indexed in about half a second, and uses under
5 MiB of memory (see
[Performance](../../performance/index.md#large-file-viewer-t733)).

## How the viewer shows text

- Invalid UTF-8 shows as `�`.
- Windows `\r\n` line endings show as ordinary line ends.
- Tabs and other control characters show as one `·` each.
- A line wider than the screen is cut, with `›` marking more text to the
  right and `‹` marking hidden text to the left.

The viewer has some limits:

- It reads local files only. On a remote (SSH) workspace it reports "The Large
  File Viewer opens local files only".
- It takes the file's length once, when it opens. Bytes appended later don't
  appear, so it can't follow a growing log the way `tail -f` does. Close it and
  open it again to see new lines.
- The mouse does nothing in it.
- Sideways scrolling counts bytes, so wide (CJK) characters may not line up.

## Binary files and pipes

Make a small binary file and a named pipe:

```sh
printf 'header\0\1\2\3 payload' > blob.bin
mkfifo log.pipe
```

Open `blob.bin`. Vix finds the NUL byte and asks:

```text
blob.bin is a binary file. Open it in the byte editor?
```

**y** opens it in the [byte editor](../../edit-bytes/index.md), which saves the
bytes back exactly. A text buffer would corrupt them. **n** cancels.

Now open `log.pipe`. Vix refuses it with "Open failed for log.pipe: not a
regular file (pipe, socket, device, or directory)". It doesn't ask, because
reading a pipe could block until something writes to it. The Large File
Viewer refuses pipes and devices too.

## Encodings and read-only opens

The same guarded read also detects the file's encoding. A byte-order mark
wins; then valid UTF-8; then a guess at a legacy encoding such as
windows-1252 or Shift_JIS. Vix uses the guess only if it decodes with no
errors *and* re-encodes to exactly the same bytes. The status bar shows the
encoding in use, and saving writes the file back in that encoding.

If no encoding decodes the file cleanly, Vix opens it anyway, replaces the
bytes it can't read with `�`, and makes the tab **read-only**, so saving can
never write those replacements over your file. Typing in it reports "Buffer is
read-only". To fix it, use **File → Reopen with Encoding…** and pick the
encoding the file really uses. See [File Encodings](../../encoding/index.md).

## Watching a log that grows

Vix has no `tail -f` mode. A log small enough for a buffer still updates on
its own, though: about once a second Vix checks whether open files changed on
disk, and reloads any buffer without unsaved edits. The status line says
"Reloaded 1 file(s) changed on disk". If you have edited the buffer, Vix keeps
your edits and warns "… changed on disk; your unsaved edits were kept".

Try it with a small log:

```sh
echo "first line" > small.log
vix small.log
```

From a second terminal, append a line:

```sh
echo "second line" >> small.log
```

Within a second the new line appears in Vix. A reload goes through the same
guarded read. If a file that opened under `large_file_mb` grows past it, the
buffer keeps its old text and Vix posts a warning instead of reloading. A
buffer you already opened past the limit with **y** keeps reloading up to the
1 GiB ceiling.

## Why a big buffer stays responsive

Syntax highlighting doesn't slow down a large source file:

- Highlighting is lazy and limited to the screen. Vix only runs highlight
  queries for the rows you can see, at any file size.
- Parsing runs in the background. A buffer of 50 KB or more is parsed by
  Tree-sitter on a worker thread, both when it opens and after each edit. The
  file appears first and colors fill in once the parse is done. A 100 MB file
  opens in about 14 ms, not the 5 seconds a full parse on the main thread
  used to take.

[Performance](../../performance/index.md) has the measured numbers.

## Tips

- **Pick the viewer for reading, the buffer for editing.** To read, search, or
  copy a few lines from a big log, press **v**: it starts at once and uses
  almost no memory. Press **y** only if you need to change the file.
- **Raise the limit if you often open big files.** Put
  `large_file_mb = 200` in `config.toml`, then run **Vix → Reload Settings**.
  See the [configuration reference](../../configuration/index.md).
- **Jump by percentage.** `:` then `90%` gets you near the end of a log faster
  than paging.
- **Clean up.** `rm big.log huge.log blob.bin log.pipe small.log`

## Where to go next

- [`docs/performance/index.md`](../../performance/index.md): the benchmark
  numbers behind this page, and `scripts/bench-large-file` to measure the
  viewer on your own machine.
- [`docs/large-file-viewer/index.md`](../../large-file-viewer/index.md): the
  Large File Viewer reference.
- [`docs/encoding/index.md`](../../encoding/index.md): encoding detection,
  and **Reopen with Encoding…** / **Save with Encoding…**.
- [`docs/edit-bytes/index.md`](../../edit-bytes/index.md): the byte editor
  binary files open in.

**Previous:** [Tutorial 14 — AI and Agents](../14-ai-and-agents/index.md)  
**Next:** [all tutorials](../index.md)

---

Vix™ and Vix IDE™ are trademarks.
