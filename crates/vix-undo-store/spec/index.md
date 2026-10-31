# Undo Store

Persistent undo: save a buffer's undo tree to disk and restore it on reopen.

Each saved file gets a small JSON file under `<config>/undo/` named by a hash
of its absolute path, holding the serialized [`History`] plus a hash of the
file content it corresponds to. On open, the history is restored **only** when
the stored content hash matches the file's current content, so undo is never
replayed onto text it doesn't match.

`load_with(file, content)` is `load` with the text supplied by a closure that
runs only when a saved history exists to compare it with (T749): the App
opens every file through it, so a file never edited with persistent undo on
costs no copy of its text on open (100 MB for a 100 MB file).

## Sub-specs

- [hot-exit](hot-exit/index.md): back up unsaved buffers while idle and on
  quit, and offer them back with a diff after a crash (T629)
- [persistent-undo](persistent-undo/index.md)
- [redo](redo/index.md)
- [undo](undo/index.md)

## Local History (T720)

`local_history::LocalHistory` is a second, independent store beside the undo
tree: a timestamped whole-file snapshot per save, kept under
`<config>/history/<hash of path>/<ms>.snap` (+ an optional `<ms>.label`).
`snapshot` skips a save whose text equals the newest snapshot (unless it
carries a label), refuses files over 5 MiB, and prunes past `with_keep(n)`
(the `local_history_keep` setting, default 50), dropping unlabelled snapshots
before labelled ones. `list`, `read` and `set_label` round it out. Two pure
helpers serve the host: `unified_diff(old, new)` (`@@`-grouped, 3 lines of
context) and `revert_range(current, snapshot, start, end)`, which replaces only
the differences touching 0-based lines `start..end` with the snapshot's text.
Errors are the typed `HistoryError` (`NotFound`, `Io`). The host is
`tui/src/app/ide_parity.rs`; user docs are `docs/ide-parity/index.md`.
