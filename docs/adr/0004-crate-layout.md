# ADR 4: A Cargo workspace of focused crates under a thin App shell

- **Status:** Accepted. This is the third layout: it supersedes the single
  crate of modules of 2026-06-18, which had replaced the first workspace of
  2026-06-09.
- **Date:** 2026-07-10 (commits 658b977a, 530ea0cf and 5cf38c98, merged as
  7fda5d8a, "split major concepts into cargo workspace crates")

## Context

The layout changed three times in five weeks:

1. **2026-06-08, one package.** The initial commit (a7b9ebaf) was one crate,
   a library with a thin binary, split into modules (`app`, `editor`,
   `explorer`, `menu`, `ui` and others; docs/architecture.md in that
   commit).
2. **2026-06-09, a first workspace.** b93e5695 "Restructure[d] Vix into a
   Cargo workspace" with internal crates holding "data/logic in the crate,
   host renders" (`vix-code-editor-panel`, `vix-theme-chooser`, and others).
3. **2026-06-18, back to modules.** d2dda06f folded "~49 separate vix-*
   subcrates" into modules under src/, keeping only `vix-editor` "so its
   Tree-sitter grammar feature flags keep working"; the same day 28523815
   folded that one too, leaving "no members", and 9092f519 rewrote the docs
   for "the single-crate architecture". The commits describe what moved but
   **not why the subcrates were folded**.
4. **2026-07-10, the workspace again.** 658b977a converted the repository
   to a Cargo workspace and added a shared `vix_i18n` crate; 530ea0cf moved
   76 leaf modules into crates; 5cf38c98 moved the mid-tier ones, leaving 98
   crates.

The 2026-07-10 commits record two reasons. First, translations: "rust_i18n's
t! embeds the 588 KB, 15-language table per crate that invokes i18n!;
routing every crate through vix_i18n compiles it in exactly once instead of
~90 times" (658b977a). Second, isolation: the library's crate docs say "The
split lets each concept build and unit-test on its own"
(`tui/src/lib.rs`). They do not record why the earlier fold was reversed
beyond that.

## Decision

Vix is a Cargo workspace: a thin App shell over focused `vix-*` member
crates.

- **Members.** The root `Cargo.toml` is a virtual workspace with
  `members = ["crates/*", "tui"]` and `default-members = ["tui"]`. There are
  147 member crates under `crates/` today
  ([`agents/share/crate-map.md`](../../agents/share/crate-map.md)).
- **The App shell** is the `vix-tui` package in `tui/`, which builds the
  `vix` command and holds the CLI, event loop, `App` state and rendering.
  It moved from the repository root into `tui/` on 2026-10-24 (90d7a449,
  T643), beside the desktop GUIs in `gui/`. `tui/src/lib.rs` re-exports the
  crates under short names (`pub use vix_git as git;`), so `crate::git`
  still resolves.
- **Every crate owns its spec** at `crates/<crate>/spec/index.md`
  ([`AGENTS.md`](../../AGENTS.md), "Source of truth").
- **A new crate has to earn its place** (T151, 2026-09-09, bfcdc68a): its
  own spec, its own tests, and a consumer other than the App shell or a clear
  reuse story; a crate failing all three folds into its sole consumer
  ([`AGENTS.md`](../../AGENTS.md), "When to add a new crate").

## Consequences

- **Some modules cannot be split.** `editor` and `pane_tree` were mutually
  recursive, so 5cf38c98 merged them into one `vix-editor` crate exposing
  both as modules. The App shell's `app` and `ui` stay in `tui/`, split
  into `tui/src/app/*.rs` and `tui/src/ui/*.rs` submodules instead (T141,
  T142 in `tasks.md`).
- **The split kept going.** T152 (2026-09-08) moved the last spec-less
  modules out of the root, calling it "the 2026-07 'crates, not modules'
  decision ... applied everywhere except here" (`tasks.md`); T151 folded the
  no-op `vix-query` back into the App shell.
- **Publishing and naming.** The crates were renamed to kebab-case on
  2026-07-11 (bd2f144d) and made publishable to crates.io (3c0b038c, 2026-07-10),
  so a release publishes many crates, each after its `path` dependencies
  ([`AI_STATEMENT.md`](../../AI_STATEMENT.md), "Publish order is
  respected"), and every crate's public API is checked against its last
  published version by `cargo semver-checks`
  ([`spec/ci/index.md`](../../spec/ci/index.md), "Semver (T602)").
- **Features forward through crates.** The Tree-sitter grammars live in
  `vix-editor-core`, and the App shell's `syntax-*` and `lang-*` features
  forward to it (5cf38c98; `tui/Cargo.toml`). See
  [ADR 5](0005-feature-flags.md).
- **Documentation must keep up.** With 147 crates, drift is checked by
  script: `scripts/check-docs` fails on a crate without a spec and on a
  crate missing from the crate map, and `tui/tests/docs_crate_graph.rs`
  checks the layer diagram in
  [`docs/architecture/index.md`](../architecture/index.md).

## Enforced by

- The root `Cargo.toml` `[workspace]` table.
- `scripts/check-docs` (crate specs and crate-map drift).
- The T151 guideline in [`AGENTS.md`](../../AGENTS.md).
