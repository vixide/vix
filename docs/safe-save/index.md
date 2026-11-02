# Safe Save

Saving never leaves a half-written file, and never changes who owns it or
splits a hard link.

## How a save works

Vix writes the new contents to a temporary file next to the target, then
renames it into place, so a crash leaves either the old file or the new one.
An existing file keeps its permission bits exactly; a brand-new file gets your
normal umask-default mode.

## Hard links and other owners

A rename replaces the file's inode, which would break a hard link or change
the owner and group of a file that belongs to someone else. So when the file
has more than one hard link, or its owner or group differs from the new
file's, Vix writes **in place** instead. The same happens when the directory
is not writable.

Before any in-place write, Vix copies the original to a recovery directory
(`$XDG_STATE_HOME/vix/recovery`, else `~/.local/state/vix/recovery`) as a
`*.vixrecover` file, and deletes the copy once the new data is flushed to
disk.

## After an interrupted save

If Vix is killed in the middle of an in-place write, the copy stays behind.
At the next start the status line reports how many such copies were found and
where. The original bytes are in that file after its two header lines (magic
and original path). A Restore / Diff / Discard prompt is not built yet.

See the specification at `crates/vix-fileops/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
