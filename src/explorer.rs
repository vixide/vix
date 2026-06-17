//! Left-drawer file explorer, re-exported from the internal [`crate::left_dock`]
//! crate. Rendering lives in [`crate::ui`] and file operations in
//! [`crate::fileops`]; this module only re-exports the tree state so the rest of
//! the app can refer to it as `crate::explorer::*`.

pub use crate::left_dock::{Explorer, Node};
