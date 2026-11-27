# ADR 3: Forbid `unsafe` code

- **Status:** Accepted
- **Date:** 2026-06-09 (commit b93e5695, "Add i18n, themes, config & CLI;
  split into internal crates")

## Context

The initial commit (a7b9ebaf, 2026-06-08) had no lint attributes in its
src/lib.rs or src/main.rs. The next day, b93e5695 added
`#![forbid(unsafe_code)]` and `#![deny(missing_docs)]` to src/lib.rs, and a
new spec, spec/main-rs-and-lib-rs-boilerplate.md (now
[`spec/main-rs-and-lib-rs-boilerplate/index.md`](../../spec/main-rs-and-lib-rs-boilerplate/index.md)),
required every main.rs and lib.rs to start with them under the comment
"Always start with high quality coding conventions."

That comment is the only reason the commit gives. **The repository does not
record the alternatives considered** (for example allowing `unsafe` in
reviewed modules), nor a longer rationale. Later files describe the rule as a
security property: [`SECURITY.md`](../../SECURITY.md) lists "No unsafe code"
first under Hardening, and [`docs/why-vix/index.md`](../why-vix/index.md)
lists it under "Private and careful by default".

## Decision

No first-party crate may contain `unsafe` code. Since 2026-10-23 (e3754514,
T599) the rule is one line in the root `Cargo.toml`:

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
```

and every crate opts in with `[lints] workspace = true`, so it covers
libraries, binaries, tests, benches, examples and build scripts
([`AGENTS.md`](../../AGENTS.md), "Hard rules enforced by the build").
`forbid`, unlike `deny`, cannot be overridden by an `#[allow]` further in.

## Consequences

- **It survived every layout change.** When the subcrates were folded into
  modules on 2026-06-18 (d2dda06f), the commit notes that
  "forbid(unsafe_code) and deny(missing_docs) still apply crate-wide"; the
  workspace split of 2026-07-10 ([ADR 4](0004-crate-layout.md)) repeated the
  attributes in every crate root, until T599 replaced the 124 copies with
  `[workspace.lints]` (`tasks.md`, T599).
- **Features are designed around it.** The Large File Viewer reads only the
  lines on screen with positioned reads: "there is no memory map and no
  `unsafe`" ([`crates/vix-large-file/spec/index.md`](../../crates/vix-large-file/spec/index.md)).
- **It covers first-party code only.** Dependencies may use `unsafe`
  internally; they are vetted by `cargo deny` (advisories, licenses,
  sources) instead ([`SECURITY.md`](../../SECURITY.md),
  [`deny.toml`](../../deny.toml)).
- **The separate workspaces carry it too.** `gui/Cargo.toml` and
  `matrix/Cargo.toml` set `unsafe_code = "forbid"` in their own lint tables.

## Enforced by

- `Cargo.toml` `[workspace.lints.rust]`, and `[lints] workspace = true` in
  every crate manifest.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo build`
  in `scripts/check` and CI ([`spec/ci/index.md`](../../spec/ci/index.md)).
- [`agents/conventions.md`](../../agents/conventions.md), "Lints".
