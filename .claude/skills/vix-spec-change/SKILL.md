---
name: vix-spec-change
description: Make a spec-driven change in the vix repo — read/update the owning spec, implement, internationalize, test, and pass the local gate. Use whenever adding or changing behavior in vixide/vix (a crate feature, an action, a menu item, a CI/doc file), or when asked to follow the repo's own change process.
---

# vix-spec-change — the spec-driven change loop

Vix is specification-driven: **`AGENTS.md` is canonical**, and every crate owns
a spec at `crates/<crate>/spec/index.md` (repo-root `spec/` holds only
cross-cutting/app-level specs). When behavior and spec disagree, decide which
is correct, then make them match — don't leave the drift for later.

## Steps

1. **Read the owning spec first.** `crates/<crate>/spec/index.md` for a crate
   change, or the matching dir under repo-root `spec/` for a cross-cutting one
   (CI, tools menu, navigation, …). If intent is changing, update the spec as
   part of this change, not after.
2. **Implement in the owning crate.** Keep editing/state logic out of
   `src/ui.rs` (rendering only lives there — usually in one of its
   `src/ui/*.rs` submodules). One action id, one `App::run_action` arm
   (routed through `src/app.rs`'s own `src/app/*.rs` submodules) — never a
   second code path for the same command.
3. **Internationalize new user-facing text.** Add the key to the right `locales/*.yml` file
   across all languages the file already carries, render with `t!`. Never
   hard-code a display string.
4. **Document every new public item** — `#![deny(missing_docs)]` is on at
   every crate root; an undocumented `pub fn`/`struct`/field fails the build.
5. **Add/extend tests** — `tests/integration/main.rs` (split by topic into
   `tests/integration/*.rs`; see `spec/test/index.md`) or the module's own unit
   tests. Prefer terminal-independent tests (build an `App`, feed
   `KeyEvent`s, assert on state); render checks use a sized `TestBackend`.
   Never assert on translated text (locale is process-global and can race) —
   assert on state or i18n keys.
6. **Run the local gate**: `scripts/check` (or `make check`) — fmt, build,
   `clippy --workspace --all-targets -- -D warnings` (pedantic, no blanket
   `#[allow]`), `cargo test --workspace`, `cargo doc` with warnings denied,
   then `scripts/check-docs`. Fix everything before moving on; CI on all
   three forges (GitHub/GitLab/Codeberg) only ever confirms what this said.
7. **Note user-visible changes in `CHANGELOG.md`** under `[Unreleased]`.
8. **Spelling**: prose/docs are CSpell-checked (`cspell.json`); add project
   terms to `project-words.txt` rather than rewording around them.

## Touched docs/CI/meta files? A few extra gotchas

- **Root markdown files are link-checked two ways**: `scripts/check-docs`
  resolves every relative link/code-path (fails the build), and CI also runs
  `lychee` (offline pass blocking, `http`/`https` pass advisory-only — see
  `spec/ci/index.md`). A prose sentence that happens to *look* like a
  markdown link — `` `[title](url)` `` inside backticks describing the syntax
  itself — is still parsed as a real link by `check-docs`'s regex and will be
  reported broken. Don't demonstrate link syntax literally in backticks;
  describe it in prose instead.
- **`README.md`/`index.md` twins**: a documentation directory carrying both
  must keep them byte-identical — make `README.md` a symlink to `index.md`
  (`ln -s index.md README.md`), never a second copy that can drift.
- **`llms.txt`/`llms.json`** (repo root, see `spec/llms-json-and-llms-txt/index.md`)
  must stay under 40 KB and name the same set of links — `scripts/check-docs`
  gates both. If the curated map changes, also update
  `vixide.github.io`'s `static/` copies **by hand**, rewriting every relative
  link to `https://github.com/vixide/vix/blob/main/...` (they're a separate
  repo with no `check-docs` of its own to catch drift) — see `vix-ship` for
  where that repo lives and how to verify its links before pushing.
- **New crate**: `scripts/check-docs` fails if it has no `spec/index.md`, or
  if it's missing from `agents/share/crate-map.md`. Add both in the same
  change.

## Verifying, not assuming

Before claiming a check passes, actually run it — `scripts/check-docs`
finishes in under a second and is worth running standalone after any doc
edit, before paying for the full `scripts/check` build. If you added a link
that points outside this repo (e.g. into `vixide.github.io`), `check-docs`
can't verify it — confirm by hand (`curl -s -o /dev/null -w '%{http_code}'`
or equivalent) rather than assuming.
