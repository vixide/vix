//! An interactive, in-editor tutorial: chapters whose lessons are real Vix
//! buffers the learner edits with their own hands, plus cheap textual
//! progress checks against what they typed — not a scripted walkthrough.
//! See `spec/index.md` for the full design; this crate is the pure data/
//! logic half (chapter metadata + bundled lesson text, progress checks).
//! The host wiring — `vix --tutor`, **Help → Tutorial**, the working-copy
//! temp dir, chapter navigation — lives in the App shell's `tutor.rs` slice
//! (T402).

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

pub mod chapter;
pub mod check;

pub use chapter::{CHAPTERS, Chapter, by_ordinal, chapter as find_chapter};
pub use check::{Progress, progress};
