# Persistent Undo

Setting `persistent_undo` (on by default).

The branch-preserving undo tree is saved per file on save (under `<config>/undo/`) and restored on reopen, guarded by a content hash so it is only replayed when the file still matches.

`crate::undo_store::save` / `load` (SHA-256 hashed per path); serde on the `editor_core` history types; wired into `write_active_to_disk` and `open_path`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.

Each state carries the time it was recorded (T638, for the undo tree panel). It is a `serde(default)` field, so an undo file written before T638 still loads, with no times. The undo tree panel previews states by moving the real buffer, but the text and the history's current state always move together and the panel blocks saving, so a saved history always matches its content hash.
