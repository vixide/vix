//! Right-drawer message browser, re-exported from the internal [`crate::right_dock`]
//! crate. Rendering lives in [`crate::ui`]; this module only re-exports the state
//! so the rest of the app can refer to it as `crate::messages::*`.

pub use crate::right_dock::{Level, Message, Messages};
