//! The editor host crate: the `CodeEditor` widget wrapper (`editor`) and the
//! split-pane layout tree (`pane_tree`). The two are mutually recursive — the
//! editor owns a `pane_tree::Pane` and the pane tree references the editor's
//! `SplitDir` — so they live together in one crate.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

pub mod editor;
pub mod pane_tree;
