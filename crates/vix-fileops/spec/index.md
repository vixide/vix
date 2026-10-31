# File Explorer

Filesystem helpers for the explorer's copy / cut / paste / delete. The file
explorer is the **left dock**; its tree state lives in the internal
`left_dock` crate (see `crates/vix-left-dock/spec/index.md`), and the host renders it and runs
the file operations.

When the tree is taller than the dock, a vertical **scrollbar** appears in a
one-column gutter on the right edge (its thumb tracks the highlighted entry). It
honors the same **Show/Hide Scroll Bar** toggle (`show_scrollbar`) as the editor.

**Status:** Shipped — keyboard navigation (`→`/`Enter` opens a file or expands a
directory; `←` collapses an expanded directory or jumps to its parent, and never
expands), arrow-scan preview tabs, `Ctrl+B` toggle (revealing the active file),
`Ctrl+E`
focus switching, mouse support (wheel to move the selection; click a file to
preview it and click again to open it permanently; click a directory to
expand/collapse), the file clipboard (`Ctrl+C`/`Ctrl+X`/`Ctrl+V` with same-dir
copy suffixing, cut dimming, and an (o)verwrite/(s)kip/(c)ancel conflict prompt),
`Shift+Up`/`Shift+Down` multi-selection, `Delete` (with confirmation, moving to
the OS trash by default — see "Delete and trash" below), and buffers that
follow files on move and close on delete. Roadmap: per-file buffers-follow on
directory rename is covered; drag-and-drop remains a nice-to-have.

## Delete and trash

`Delete` on a selected explorer entry (or entries, with multi-selection)
prompts for confirmation, then acts per the `explorer_delete` setting:

- `"trash"` (the default) moves each path to the OS trash / Recycle Bin
  (`vix_fileops::trash_path`, the [`trash`](https://docs.rs/trash) crate) —
  undoable from the platform's own trash/recycle view.
- `"hard"` removes each path outright (`vix_fileops::remove_path`:
  `fs::remove_file`/`remove_dir_all`) — not undoable.

Any other value (including an older config file that predates this setting)
falls back to `"trash"`, the safer default, rather than silently hard-deleting
on a typo. The confirmation prompt's wording differs by mode (`confirm.delete`
vs. `confirm.delete_hard`) so it always says which will happen. Open buffers
under a deleted path are closed either way (`App::close_buffers_under`).

## Edit Directory (T636)

`explorer.edit_directory` (**File → Edit Directory…**, palette
`cmd.explorer_edit_directory`) opens one directory as an Oil/wdired-style
editable buffer. It lists the explorer's highlighted directory (or a
highlighted file's parent) when the explorer has focus, else the active
file's directory, else the workspace root; run again on the listing, it
re-reads the same directory.

Each entry is one line, `/<id> <name>`, directories first and then by name,
a directory's name ending in `/`. The id is the entry's 1-based position,
zero-padded to at least three digits; it is visible because Vix's editor has
no concealed text, and it is what lets a line keep its identity when its name
changes. Every entry is listed, hidden ones included, except a name that is
not valid Unicode (so it can't be acted on under a mangled spelling).

The buffer carries the synthetic path `*directory*`, the same pattern as
T211's editable search results: `App::save` recognizes it and diffs the text
instead of writing a file, and Save All skips it. The diff
(`vix_fileops::dir_edit::diff`) reads each non-blank line:

| Line | Meaning |
| ---- | ------- |
| a known id, same name | unchanged (the trailing `/` is optional: an entry can't change type) |
| a known id, new name | rename |
| no id | create: an empty file, or a directory when the name ends in `/` |
| an id that no longer appears | delete |

It refuses, with a status-line reason and no confirm step, a repeated id
(copying isn't supported), an unknown id, a name that isn't one entry (empty,
`.`, `..`, or containing `/` or NUL, and `\` on Windows), and two lines that
would end with the same name. A create may reuse a name that a rename or
delete frees.

`vix_fileops::dir_edit::schedule` orders the plan so no step clobbers an
entry a later one still needs: deletes, then renames, then creates. A rename
runs once its target is free; when every remaining rename waits on another
(a cycle, such as two names swapped), one source is first renamed to a
temporary `.vix-dir-edit-<pid>-<n>` name no entry uses, which breaks the
cycle; a chain (`a → b`, `b → c`) needs no temporary.

The confirm step previews one row per rename, create and delete (worded as
trash or permanent delete per `explorer_delete`); `y`/Enter applies, `n`/Esc
cancels. The host runs the steps through the authority (local: this crate's
`trash_path`, `remove_path`, `fs::rename`, `create_dir_all`), deletes honoring
`explorer_delete`. Nothing is overwritten: a rename or create whose target
exists fails (except a rename that only changes case on a case-insensitive
filesystem), and the first failure stops the rest and is reported to
Messages. Open buffers follow renames (`relocate_buffers`) and close for
deletes (`close_buffers_under`), the explorer tree is rebuilt, and the
listing is re-read so saving again does nothing.

Code: `crates/vix-fileops/src/dir_edit.rs` (`render`, `diff`, `schedule`,
unit tests including a swap cycle); host `tui/src/app/dir_edit.rs`; dialog
`tui/src/ui/dialogs.rs` (`draw_dir_edit_confirm`); integration tests
`tui/tests/integration/dir_edit.rs`.

Built-in file explorer.

Toggle Sidebar: Use Ctrl+B to show/hide the file explorer sidebar. When a nested file is active, toggling on expands the tree and reveals the file.

Focus: Use Ctrl+E to switch focus between the file explorer and editor.

Navigation: Use the arrow keys to move up and down the file tree.

## Opening Files

Enter opens the selected file and focuses the editor.

Arrow Up/Down also opens the highlighted file in a preview tab automatically as you move — so you can scan files without leaving the keyboard.

Single-click opens a file in an ephemeral preview tab — the next single-click on another file replaces it instead of piling up tabs. Any real commitment — editing the file, pressing Enter, double-clicking, clicking the tab itself, or a layout action like splitting — promotes the preview to a permanent tab.

Double-click opens the file in a permanent tab and focuses the editor.

Preview tabs are enabled by default. Turn them off via the `preview_tabs` setting (see `docs/configuration/index.md`) if you prefer every click to open a permanent tab.

## Cut / Copy / Paste and Multi-Selection

Ctrl+C / Ctrl+X / Ctrl+V — copy, cut, or paste the selection. Same-directory copy auto-appends copy / copy 2 etc. Same-directory cut is a no-op. Paste into a different directory with a name conflict prompts per-file: (o)verwrite, (O) all, (s)kip, (S) all, (c)ancel.

Cut-pending items are visually dimmed. Cancel a pending cut with Escape or by pasting back into the same directory.

Shift+Up / Shift+Down extend a multi-select range from the current anchor; all clipboard operations (and delete) act on the whole selection.

Buffers follow files — renaming or moving a file (via cut+paste) relocates any open buffers pointing at it; deleting a file closes its buffer. Renaming a directory relocates buffers for every file inside it.

## Atomic and private writes

Two crash-safe writers, both write-temp-then-rename (never a truncating
in-place write a reader could observe half-done) and both written through a
symlink rather than following it into some other file:

- `write_atomic(path, data)` — the general-purpose saver used across the
  app (editor save, workspace save, …). An *existing* file's permission
  bits are preserved exactly; a brand-new one gets the process's normal
  umask-default mode, same as a plain `fs::write` — silently narrowing
  every new file to owner-only would be a surprise for a file meant to be
  shared or version-controlled.
- `write_atomic_private(path, data)` — same mechanics, but a **new** file
  is always created owner-only (0600 on Unix) regardless of umask (T133).
  For persisted files whose content is inherently sensitive even though
  they're not secrets-in-transit the way `write_private_temp` below
  covers: `vix-undo-store`'s per-file undo histories (a full edit history
  can carry text no longer in the current buffer at all) and
  `vix-macros`' `macros.toml` (a recorded macro can carry literally-typed
  text). An existing file's mode is still left alone, exactly like
  `write_atomic`.
- `restrict_to_owner(path)` — best-effort, narrow an *already-written*
  file to owner-only on Unix. For persisted files written through
  [confy](https://docs.rs/confy) rather than this crate (`config.toml`,
  `session.toml`, `db_history.toml`, `db_queries.toml`), which give no
  hook to choose the mode as the file is created — only after. Not
  perfectly race-free at creation time, but these are single-user local
  desktop config files, not shared or network-exposed ones.
- `write_private_temp(prefix, data)` — a one-off scratch file in the OS
  temp directory (secrets briefly in transit, e.g. an AI-request payload),
  0600 and `O_EXCL` from creation, unrelated to any of the above.

`keybindings.toml` (`vix-keybindings::user_bindings`) is a deliberate
non-fix: its content (a key token paired with an action id) is never
sensitive, so it keeps its original plain, non-private write.

## Guarded text reads (T571)

`read_text_guarded(path, max_bytes)` is how a file becomes a text buffer. It refuses what
would freeze or exhaust Vix with a typed `TextReadError` (inside an `InvalidData`
`io::Error`; recover it with `TextReadError::from_io`): `NotRegular` (a FIFO, socket,
device or directory), `TooLarge` (over `max_bytes`, bounded even if the file grows after the
stat) and `Binary` (a NUL in the first 8 KiB). It returns a `TextFile` (`text`, and
`lossy`, set when no encoding decodes it losslessly and it was decoded as UTF-8 with
replacements, so the caller opens it read-only), and `encoding`: the encoding and BOM
`vix_encoding::detect` chose (a BOM, then UTF-8, then a legacy guess that round-trips byte
for byte, T661). A BOM is sniffed before the NUL-byte check, so UTF-16 opens as text; a
"UTF-16" file whose decoded text still has NULs is refused as `Binary`.
`write_text_in_file_encoding(path, text)` writes text back in the encoding the file already
has (replace across files uses it), refusing a file that only decodes lossily.
Each has a half that touches no file, for bytes read or written some other way (T712's SSH
authority): `decode_text(bytes, max_bytes)` applies the `TooLarge`/`Binary` refusals and
the encoding detection, and `encode_like(existing, text)` returns the bytes
`write_text_in_file_encoding` would write, given the guarded read of the old file.
`stream_text_guarded(path, max_bytes, sink)` is the same read for a caller that builds
its own structure from the text (the editor's rope, T749): plain UTF-8 without a BOM — the
common case — goes to `sink` in 64 KiB blocks as it is read (a character split across two
blocks is held back to the next) and the result is `TextStream::Streamed`, so no file-sized
copy is ever held. Anything else (a BOM, a NUL in the first 8 KiB, invalid UTF-8 anywhere,
or a character cut off at the end) is read again through `read_text_guarded` and returned
as `TextStream::Whole(TextFile)`, and the caller discards what the sink received. The
refusals are the same. `read_text_guarded` itself sizes its buffer from the stat, so the
read is one allocation instead of a doubling series (~264 MB of allocations for a 100 MB
file before T749).
`TextFile` is `#[non_exhaustive]`, so it can gain fields (such as an encoding) without a
breaking change. `MAX_TEXT_BYTES` (50 MiB) is the default
limit and `HARD_MAX_TEXT_BYTES` (1 GiB) the ceiling that applies even after the user
confirms. `read_to_string_guarded(path)` is the same guard for features that only want the
text (insert file, revert, compare, the TODO finder). `which_on_path` resolves a program
against `PATH` only, never the working directory.

`PathPatterns::new(&[..])` compiles a list of path globs (`~/` is the home directory, `**`
spans directories, `*` doesn't cross `/`, invalid patterns are skipped) and
`PathPatterns::matches(path)` tests one. It backs the `read_only_paths` and
`ephemeral_paths` settings (T662).

The host turns `TooLarge` and `Binary` into confirmations (`large_file_mb`; binary files
open in the byte editor, which saves the bytes back exactly) rather than errors.

