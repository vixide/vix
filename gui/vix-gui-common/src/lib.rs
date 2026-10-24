//! The toolkit-agnostic core of the Vix desktop GUIs (`vix-gpui`,
//! `vix-egui`): everything about turning the Vix `App` into a window that
//! does not depend on which GUI toolkit draws it.
//!
//! - [`grid`]: a ratatui `Backend` drawing into an in-memory cell grid, so
//!   the App's unchanged `ui::draw` renders every panel and overlay.
//! - [`render`]: colors and the per-row background runs / text spans to paint.
//! - [`keys`]: the rules turning a keystroke into a `vix-input` key event
//!   shaped like the one crossterm delivers in the terminal.
//! - [`menu`]: the native menu bar's tree, from the `vix-menu` model.
//! - [`fonts`]: the primary monospace family and Nerd Font fallbacks.
//! - [`startup`]: settings, locale, theme and `App` startup, plus the
//!   scripted screenshot run.

#[macro_use]
extern crate vix_i18n;
vix_i18n::surface!();

pub mod fonts;
pub mod grid;
pub mod keys;
pub mod menu;
pub mod render;
pub mod startup;
