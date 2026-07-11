# Undo Store

Persistent undo: save a buffer's undo tree to disk and restore it on reopen.

Each saved file gets a small JSON file under `<config>/undo/` named by a hash
of its absolute path, holding the serialized [`History`] plus a hash of the
file content it corresponds to. On open, the history is restored **only** when
the stored content hash matches the file's current content, so undo is never
replayed onto text it doesn't match.

## Sub-specs

- [persistent-undo](persistent-undo/index.md)
- [redo](redo/index.md)
- [undo](undo/index.md)
