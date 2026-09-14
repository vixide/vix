//! An interactive, in-editor tutorial: chapters whose lessons are real Vix
//! buffers the learner edits with their own hands, plus cheap textual
//! progress checks against what they typed — not a scripted walkthrough.
//!
//! Design-only for now (improvement plan T401) — see `spec/index.md` for
//! the working-copy mechanics, the chapter/check data model, and the v1
//! cut line. The engine and chapter 1 land in T402; chapters 2-6 in T403.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]
