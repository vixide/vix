//! The editor host crate: the `CodeEditor` widget wrapper (`editor`) and the
//! split-pane layout tree (`pane_tree`). The two are mutually recursive — the
//! editor owns a `pane_tree::Pane` and the pane tree references the editor's
//! `SplitDir` — so they live together in one crate.
//!
//! `editor::Editor` is the tab list plus per-buffer state (path, dirty flag,
//! disk stamp, read-only reasons); the actual text widget is
//! `vix_editor_core`'s `CodeEditor`, re-exported here. File I/O goes through
//! the `vix_authority` slot so a non-local workspace works unchanged.
//!
//! Spec: `crates/vix-editor/spec/index.md`.

pub mod editor;
pub mod pane_tree;
