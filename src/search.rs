//! Find / find-and-replace box state, re-exported from the internal
//! [`vix_find_panel`] crate. The box's rendering lives in [`crate::ui`] and the
//! search / replacement runs in [`crate::app`]; this module only re-exports the
//! state type so the rest of the app can refer to it as `crate::search::*`.

pub use vix_find_panel::{Field, SearchBar};
