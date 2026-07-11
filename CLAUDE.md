# CLAUDE.md

This file orients Claude Code (and other AI agents) in the Vix repository.

**The canonical guidance is [`AGENTS.md`](AGENTS.md)** — read it first. This file
exists only so agents that look for `CLAUDE.md` find the same single source of
truth rather than a second, drifting copy.

Quick orientation:

- **Vix** is a keyboard-friendly terminal text editor built on `ratatui`,
  organized as a **Cargo workspace**: a thin App shell (root package `vix`,
  `src/`) over ~98 focused `vix-*` member crates under `crates/`.
- **Specs are the source of truth**, one per crate at
  `crates/<crate>/spec/index.md`; cross-cutting specs live at the repo-root
  `spec/`. Development is specification-driven — change the spec when intent
  changes, change the code when it drifted, keep them in sync.
- **Build/test/lint**: `cargo build`, `cargo test`,
  `cargo clippy --workspace --all-targets -- -D warnings` (kept clean).
- **Hard rules**: `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`,
  `#![warn(clippy::pedantic)]` in every crate; internationalize all user-facing
  text via `t!` + `locales/app.yml`; one action id, one `run_action` arm.

For everything else — conventions, the spec-driven workflow, the crate map, and
the glossary — see [`AGENTS.md`](AGENTS.md) and the [`AGENTS/`](AGENTS/) topic
guides.
