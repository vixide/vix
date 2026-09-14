//! An interactive, in-editor tutorial: chapters whose lessons are real Vix
//! buffers the learner edits with their own hands, plus cheap textual
//! progress checks against what they typed — not a scripted walkthrough.
//! See `spec/index.md` for the full design; this crate is the pure data/
//! logic half (chapter metadata + bundled lesson text, progress checks).
//! The host wiring — `vix --tutor`, **Help → Tutorial**, the working-copy
//! temp dir, chapter navigation — lives in the App shell's `tutor.rs` slice
//! (T402).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

pub mod chapter;
pub mod check;

pub use chapter::{CHAPTERS, Chapter, by_ordinal, chapter as find_chapter};
pub use check::{Progress, progress};
