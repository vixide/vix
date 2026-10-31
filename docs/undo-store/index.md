# Persistent Undo

`vix-undo-store` is not just internal plumbing behind Ctrl+Z — it backs a
real, user-facing feature: **persistent undo**. A buffer's undo tree survives
closing and reopening the file, across separate runs of Vix.

## What it does

Vix's undo is a branch-preserving *tree*, not a simple linear stack (see
**Edit → Switch Undo Branch**, `edit.undo_branch`, for moving between
branches, and [**Edit → Undo Tree…**](../undo-tree/index.md) to see the
tree and jump to any state in it). Persistent undo saves that whole tree to disk and restores it the
next time the same file is opened, so an undo history you built up in a
previous session is still there — you can undo past the point where you last
saved, even after quitting and relaunching Vix.

## When it runs

- **On save.** Whenever a file is written to disk and the
  `persistent_undo` setting is on, its undo tree is serialized to a small
  JSON file under `<config>/undo/`, named by a hash of the file's absolute
  path.
- **On open.** When a file is freshly opened (not already open in another
  tab) and `persistent_undo` is on, Vix looks for that file's saved undo
  history and restores it — but **only if the stored content hash matches
  the file's current content**. Each saved history carries a hash of the
  file content it was saved against; if the file has changed since (edited
  outside Vix, checked out to a different revision, …), the mismatched
  history is discarded rather than replayed onto text it doesn't describe.
  The undo tree then simply starts fresh, as if persistence were off for
  that file.

This all happens automatically — there is no menu item or action id for
"restore undo history"; it is a side effect of the ordinary open/save flow.

## Setting

| Setting           | Type   | Default | Description                                                                                   |
| ------------------ | ------ | ------- | ----------------------------------------------------------------------------------------------- |
| `persistent_undo` | `bool` | `true`  | Persist each file's undo tree across sessions (restored on reopen when the file content still matches). |

See `docs/reference/settings.md` for the setting alongside every other one.
Turn it off to keep undo history purely in-memory, cleared as soon as a
buffer or the whole app closes.

## Storage and limits

- One JSON file per saved file, under `<config>/undo/`, holding the
  serialized undo history (`editor_core`'s history types via `serde`) plus a
  hash of the matching file content.
- Restoration is all-or-nothing per file: either the content hash matches and
  the whole tree comes back, or it doesn't and the file opens with no undo
  history at all. There is no partial replay.
- The store only ever helps a file you've *saved* through Vix at least once
  with the setting on — a buffer that was only ever edited and never
  written, or edited with the setting off, has nothing to restore.

## Implementation

`crate::undo_store::save`/`load` do the hashing (SHA-256 of the path for the
filename, and of the content for the guard) and JSON (de)serialization; they
are wired into `write_active_to_disk` (save) and `open_path` via
`App::restore_persistent_undo` (open).

See the crate spec at `crates/vix-undo-store/spec/index.md`, and its
sub-specs `crates/vix-undo-store/spec/persistent-undo/index.md`,
`crates/vix-undo-store/spec/undo/index.md`, and
`crates/vix-undo-store/spec/redo/index.md` for the plain undo/redo actions
this builds on.

---

Vix™ and Vix IDE™ are trademarks.

## Local History

A second store, [Local History](../ide-parity/index.md), keeps timestamped
snapshots on every save, with labels, diff and restore. It is controlled by the
`local_history_keep` setting (default 50, `0` = off) and is separate from
`persistent_undo`.

## Hot exit

The same crate keeps **hot-exit backups** of unsaved buffers (T629), so a
crash, a closed terminal or a forced quit never loses typed text. While Vix is
idle (every couple of seconds, writing only buffers that changed since their
last backup) and again on quit, each unsaved buffer is copied to
`<config>/backup/`. Untitled buffers are kept too, under a session id.

When you next open a file that has a backup, Vix asks whether to recover it:

| Key | Does |
| --- | --- |
| `r` | Put the unsaved text back into the buffer. It stays unsaved until you save. |
| `d` | Show the file on disk against the backup, then ask again. |
| `x` | Delete the backup. |
| `n` / `Esc` | Decide later: the backup is offered again next time. |

Untitled buffers are offered together when Vix starts in the same workspace.
A backup is never applied without asking, and saving the buffer deletes it.
Buffers in a remote workspace are backed up on this machine, keyed by their
remote name. Turn the feature off with `hot_exit = false`.

Full behavior: `crates/vix-undo-store/spec/hot-exit/index.md`.
