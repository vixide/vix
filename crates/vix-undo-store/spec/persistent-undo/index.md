# Persistent Undo

Setting `persistent_undo` (on by default).

The branch-preserving undo tree is saved per file on save (under `<config>/undo/`) and restored on reopen, guarded by a content hash so it is only replayed when the file still matches.

`crate::undo_store::save` / `load` (SHA-256 hashed per path); serde on the `editor_core` history types; wired into `write_active_to_disk` and `open_path`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
