---
name: vix-maintainer-skill
description: Make and land a change to vixide/vix's own source — read/update the owning spec, implement, internationalize, test, pass the local gate, then branch/commit/merge/push across its three forges. Use whenever adding or changing behavior in the vix repo itself (a crate feature, an action, a menu item, a CI/doc file), or when asked to follow the repo's own contribution process. Not for helping someone use the Vix editor as an end user — see vix-skill for that.
---

# vix-maintainer-skill — contributing to vixide/vix

Vix is specification-driven, and **`AGENTS.md` at the repo root is
canonical** — read it if anything here seems to conflict, and follow it for
anything this skill doesn't cover (crate map, glossary, hard rules in full).
This skill covers the two halves of landing a change: making it, and
shipping it.

## What Vix is

A keyboard-friendly terminal text editor (a "Simple Terminal Rust IDE") built
on `ratatui`, organized as a **Cargo workspace** (edition 2024): a thin App
shell (root package `vix`, `src/`) over ~105 focused `vix-*` member crates
under `crates/`. `src/lib.rs` re-exports each member crate under a short
module name (`pub use vix_git as git;`), so `crate::git`, `crate::menu`, etc.
still name them.

## Part 1 — making a spec-driven change

Every crate owns its spec at `crates/<crate>/spec/index.md` (multi-topic
crates add `spec/<topic>/index.md`); the repo-root `spec/` holds only
cross-cutting/app-level specs (CI, tools menu, navigation, …). When behavior
and spec disagree, decide which is correct, then make them match — update
the spec when intent changes, update the code when the code drifted. Don't
leave the drift for later.

1. **Read the owning spec first.** Update it as part of this change if
   intent is changing, not after.
2. **Implement in the owning crate.** Keep editing/state logic out of
   `src/ui.rs` (rendering only lives there). One action id, one
   `App::run_action` arm — never a second code path for the same command.
3. **Internationalize new user-facing text.** Add the key to
   `locales/app.yml` across every language the file already carries, render
   with `t!`. Never hard-code a display string.
4. **Document every new public item.** `#![deny(missing_docs)]` is on at
   every crate root; an undocumented `pub fn`/`struct`/field fails the build.
5. **Add/extend tests** — `tests/integration.rs` or the module's own unit
   tests. Prefer terminal-independent tests (build an `App`, feed
   `KeyEvent`s, assert on state); render checks use a sized `TestBackend`.
   Never assert on translated text (locale is process-global and can race) —
   assert on state or i18n keys.
6. **Run the local gate**: `scripts/check` (or `make check`) — fmt, build,
   `clippy --workspace --all-targets -- -D warnings` (pedantic, no blanket
   `#[allow]`), `cargo test --workspace`, `cargo doc` with warnings denied,
   then `scripts/check-docs`. **Run the whole thing, not an assembled subset**
   — `cargo doc` with denied warnings and `check-docs` only run as part of
   the full script, and a change that doesn't look docs-related can still
   trip `rustdoc::private_intra_doc_links` or a spec/description drift. Fix
   everything before moving on; CI on all three forges only ever confirms
   what this said.
7. **Note user-visible changes** in `CHANGELOG.md` under `[Unreleased]`.
8. **Spelling**: prose/docs are CSpell-checked (`cspell.json`); add project
   terms to `project-words.txt` rather than rewording around them.

### Hard rules enforced by the build

- `#![forbid(unsafe_code)]` and `#![deny(missing_docs)]` on every crate —
  no `unsafe`, and every public item needs a doc comment.
- `#![warn(clippy::pedantic)]` at the crate root and repeated in every module
  file. No blanket `#![allow(clippy::pedantic)]`, no `#![allow(missing_docs)]`
  — fix findings in code. Sanctioned allows are only a few targeted ones
  (`#[allow(clippy::struct_excessive_bools)]` on genuine state structs,
  a handful of `#[allow(clippy::too_many_lines)]`/`too_many_arguments` on
  functions that resist further extraction).
- **One `ratatui` version** — the whole widget stack agrees on one
  `ratatui`/`crossterm` pair; don't add a widget crate on a different one.
- Built-in themes are monochrome (one fg, one bg; emphasis via dim/full
  intensity, no bold/italic; reversed video only for selection/cursor) —
  color belongs only to custom JSON themes.
- Input dispatch is keymap-aware: raw keys route through the active keymap
  in `App::on_key`, which translates into the same `run_action` calls rather
  than duplicating behavior.

### Touched docs/CI/meta files? A few extra gotchas

- **Root markdown files are link-checked two ways**: `scripts/check-docs`
  resolves every relative link/code-path (fails the build), and CI also runs
  `lychee` (offline pass blocking, `http`/`https` pass advisory-only). A
  prose sentence that just *looks* like a markdown link — `` `[title](url)` ``
  inside backticks describing the syntax itself — is still parsed as a real
  link by `check-docs`'s regex and reported broken; describe link syntax in
  prose instead of demonstrating it literally.
- **`README.md`/`index.md` twins**: a doc directory carrying both must stay
  byte-identical — make `README.md` a symlink to `index.md`
  (`ln -s index.md README.md`), never a second copy that can drift.
- **`llms.txt`/`llms.json`** (repo root) must stay under 40 KB and name the
  same set of links — `scripts/check-docs` gates both. If the curated map
  changes, also update `vixide.github.io`'s `static/` copies by hand,
  rewriting every relative link to `https://github.com/vixide/vix/blob/main/...`
  (a separate repo with no `check-docs` of its own to catch drift).
- **New crate**: `scripts/check-docs` fails if it has no `spec/index.md`, or
  if it's missing from `agents/share/crate-map.md`. Add both in the same
  change. It also checks that each crate's Cargo.toml `description` is a
  prefix of its spec's opening prose — keep the two in sync when either
  changes.
- Before claiming a check passes, actually run it — `scripts/check-docs`
  finishes in under a second and is worth running standalone after any doc
  edit, before paying for the full `scripts/check` build. A link pointing
  outside this repo can't be verified by `check-docs`; confirm by hand
  (`curl -s -o /dev/null -w '%{http_code}'` or equivalent).

### Where things live

| You want to… | Go to… |
| --- | --- |
| Add/route a command | `src/app.rs` (`run_action`), `crates/vix-menu/`, `crates/vix-palette/` |
| Change rendering | `src/ui.rs` |
| Add/translate UI text | `locales/app.yml` (+ `t!` at the call site) |
| Add a setting | `crates/vix-settings/` |
| Change the editor widget | `crates/vix-editor-core/` |
| Change theme colors/model | `crates/vix-theme/`, `crates/vix-theme-model/` |
| Change available UI languages | `crates/vix-locale-model/`, `crates/vix-i18n/` |
| Change keyboard navigation styles | `crates/vix-keymap-model/` + dispatch in `src/app.rs` |
| Change git status/diff/staging | `crates/vix-git/` + wiring in `src/app.rs`/`src/ui.rs` |
| Change find/replace | `crates/vix-find-panel/` |
| Change LSP support | `crates/vix-lsp/` (host) + `crates/vix-lsp-core/` (protocol) |
| Change the database workbench | `crates/vix-db/` |
| Add a benchmark or fuzz target | `benches/`, `fuzz/fuzz_targets/` |

See `agents/share/crate-map.md` for the full map and `agents/share/glossary.md`
for shared terms; `agents/conventions.md` and `agents/workflow.md` go deeper
on style and the spec-driven workflow than this skill does.

## Part 2 — shipping the change

`vix` pushes to **three forges** from one `origin` remote (`git remote -v`
shows three push URLs: GitHub, GitLab, Codeberg). The public site lives at
`vix/vixide.github.io/`, a monorepo subproject published separately (see
below) — never edit the sibling `../vixide.github.io` repo directly.

### One feature branch per change

1. `git checkout -b <short-kebab-name>` off `main`.
2. Implement (Part 1 above), and confirm `scripts/check` is green **before**
   committing — don't commit red and fix in a follow-up.
3. Commit with a trailer identifying the assisting agent, e.g.
   `Co-Authored-By: <agent name> <noreply@anthropic.com>`.
   - If commits are GPG/SSH-signed, the *first* commit in a session can hang
     for up to ~2 minutes waiting on the SSH key's passphrase prompt — if
     `git commit` seems to hang, that's likely why; retry with a longer
     timeout rather than assuming it's broken. Verify with
     `git log --show-signature -1` if in doubt.
4. `git checkout main && git merge --no-ff <branch> -m "Merge branch '<branch>'"`
   — always `--no-ff`, even for a single commit, so the feature boundary
   stays visible in history.
5. `git branch -d <branch>` — delete it once merged; branches are
   disposable, `main` is the record.
6. Before pushing: this is outward-facing (public forges) — confirm with the
   maintainer first unless already told to push without asking.
7. `git push origin main` pushes to all three forges in one command. If one
   forge fails, read the error before retrying blindly:
   - An SSH `Connection reset` that repeats across retries, with the
     forge's status page green and an HTTPS `git ls-remote` working, is a
     local network/egress issue, not a forge outage or an auth problem —
     don't invent alternate credentials to route around it. Report which
     forge(s) landed and which didn't, with the exact commits each is
     missing (`git ls-remote <url> HEAD`), and let the straggler push once
     connectivity clears.
8. **Verify CI actually went green** — don't assume a push succeeded just
   because the build was green locally; new CI jobs especially can behave
   differently on real infra (network egress, forge-specific images):
   `gh run list --repo vixide/vix --limit 3` to find the run, then
   `gh run watch <id> --repo vixide/vix --exit-status`. A `continue-on-error`
   step failing (shown as an annotation) with the job still `success` is
   working as designed, not a regression — check the job's `conclusion`
   field (`gh run view <id> --json conclusion`), not just the annotation.

### `vixide.github.io/`: edit inside `vix`, publish via `git subtree push`

Changes to the site are ordinary commits under `vix/vixide.github.io/` — same
branch, same `--no-ff` merge to `main`, same process as any other part of
`vix`. Its own rule (`vixide.github.io/AGENTS.md`): `npm run build` must
prerender cleanly before publishing. If the local Node is too old for a
build-tool dependency, don't block on it — confirm the failure is
pre-existing (reproduces on `main` before your change, e.g. via
`git stash`) and that CI's pinned Node version
(`vixide.github.io/.github/workflows/deploy.yml`) is new enough.

Once merged to `vix`'s `main` (and `vix` itself is pushed — subtree push
reads from local history, not the forges), publish the subtree:

```
make github-pages
```

(equivalent to `git subtree push --prefix=vixide.github.io github-pages main`
— the `github-pages` remote points at
`git@github.com:vixide/vixide.github.io.git` and is created on first use if
missing). This should fast-forward `vixide.github.io`'s `main`; if it
doesn't, the sibling repo has a commit that didn't come from a subtree push
(someone edited it directly) — stop and reconcile rather than forcing.

This is a public GitHub Pages site — confirm before publishing here too, same
as `vix`. After pushing, verify the **deploy** workflow (not the local
build) actually goes green:
`gh run watch <id> --repo vixide/vixide.github.io --exit-status`, then spot
check the live URL(s) with `curl -s -o /dev/null -w '%{http_code}'`.

## Governance

Read `AI_STATEMENT.md` before doing anything outward-facing or hard to
reverse (a force-push, publishing a release, publishing a package) — it says
what an AI agent is and isn't pre-authorized to do here without asking first.
Found a security vulnerability? See `SECURITY.md` — report it privately, not
as a public issue.
