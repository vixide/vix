# CLAUDE.md

This file orients Claude Code (and other AI agents) in the Vix repository.

**The canonical guidance is [`AGENTS.md`](AGENTS.md)** — read it first. This file
exists only so agents that look for `CLAUDE.md` find the same single source of
truth rather than a second, drifting copy.

Quick orientation:

- **Vix** is a keyboard-friendly terminal text editor built on `ratatui`,
  organized as a **Cargo workspace**: a thin App shell (package `vix-tui` in `tui/`,
  building the `vix` command) over ~124 focused `vix-*` member crates under
  `crates/`; the desktop GUIs (`vix-gpui`, `vix-egui`) live beside it in `gui/`.
- **Specs are the source of truth**, one per crate at
  `crates/<crate>/spec/index.md`; cross-cutting specs live at the repo-root
  `spec/`. Development is specification-driven — change the spec when intent
  changes, change the code when it drifted, keep them in sync.
- **Build/test/lint**: `cargo build`, `cargo test`,
  `cargo clippy --workspace --all-targets -- -D warnings` (kept clean).
- **Hard rules**: `unsafe_code = "forbid"`, `missing_docs = "deny"`,
  `clippy::pedantic` for every crate via the root `[workspace.lints]` (each
  crate has `[lints] workspace = true`); internationalize all user-facing
  text via `t!` + the right `locales/*.yml` file; one action id, one
  `run_action` arm.

For everything else — conventions, the spec-driven workflow, the crate map, and
the glossary — see [`AGENTS.md`](AGENTS.md) and the [`agents/`](agents/) topic
guides.
