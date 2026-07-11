# Vix Improvement Plan

Major improvements across capabilities, functionality, documentation,
tutorials, and examples. This plan is the "why and what"; the actionable
checklist lives in [`tasks.md`](tasks.md). Both are written to be executed
incrementally by an AI agent (or a human) one branch at a time.

## Current state (baseline, 2026-07-11)

- **Feature-rich core.** ~98 workspace crates under `crates/`, a thin App
  shell in `src/`. The editor already has folding, keyboard macros,
  bookmarks, multi-cursor, splits, LSP, DAP, git hunk staging, a DB
  workbench, Org-roam, an HTTP client, 15 locales, 9 keymap families, and a
  large Tools suite. The suggested-features backlogs (2026-06/07) are done.
- **Spec-driven.** One spec per crate at `crates/<crate>/spec/index.md`,
  cross-cutting specs at repo-root `spec/`. Specs and code are kept in sync.
- **Docs exist but are shallow and partial.** 112 markdown files (~8,000
  lines) across ~56 topics — roughly half the user-facing surface has a docs
  page, and pages average ~70 lines.
- **Almost no tutorials or examples.** No tutorial content at all, no
  in-editor tutor, and only 2 cargo examples (`headless_edit`,
  `list_commands`).
- **Thin quality infrastructure.** CI has only `release.yml` (no test/lint
  workflow), no benchmarks, no fuzzing, no TUI snapshot tests beyond 3
  integration test files.
- **No extensibility.** No plugin or scripting system; every feature is
  compiled in.

The theme of this plan: the *feature surface* is unusually broad for a young
editor, but the *platform around it* — extensibility, performance evidence,
CI, docs depth, onboarding — has not kept pace. The highest-leverage work is
now in that platform.

## Hard rules (apply to every task)

These come from `AGENTS.md` / `CLAUDE.md` and are non-negotiable:

- `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`,
  `#![warn(clippy::pedantic)]` in every crate; clippy kept at zero warnings
  with `cargo clippy --workspace --all-targets -- -D warnings`. No blanket
  `allow`s.
- Every user-facing string goes through `t!` with keys in `locales/app.yml`
  for all 15 languages (en es fr de cy ga gd pl pt ru ar hi bn zh ja).
- One action id, one `run_action` arm; new features get a menu item, a
  palette command, and a keybinding when a free combo exists.
- Update the relevant `spec/` when intent changes; specs are the source of
  truth.
- Per-feature branch off `main`, kept green (build + clippy + tests), merged
  `--no-ff`, branch deleted.

## Phases

Ordered so that early phases protect and de-risk later ones. Within a phase,
tasks are independent unless noted.

### Phase 0 — Safety net (quality infrastructure)

Everything after this phase is large-scale code and content change; do this
first so regressions are caught mechanically.

1. **CI workflow.** A `ci.yml` GitHub Actions workflow: build, test, clippy
   (`-D warnings`), `cargo fmt --check`, on Linux + macOS, on push and PR.
   Cache with `Swatinem/rust-cache`. Also a docs job: link-check all
   markdown (`lychee` or similar) and `cargo doc --no-deps` warnings-clean.
2. **Dependency hygiene.** `cargo-deny` config (licenses, advisories,
   duplicate versions) wired into CI.
3. **TUI snapshot testing.** A test harness crate (or `tests/` helpers)
   around ratatui's `TestBackend`: render the app at a fixed size, assert
   against golden text screens (insta or hand-rolled). Seed it with ~10
   snapshots: welcome screen, editor with file, menus open, palette, find
   bar, git panel, table edit surface, help overlay.
4. **Benchmarks.** A `benches/` suite (criterion): open a 100 MB file,
   insert/delete at scale in `vix-editor-core`, syntax highlight a large
   buffer, workspace search over a large tree, fuzzy palette scoring.
   Record baseline numbers in `docs/performance/index.md`.
5. **Fuzzing.** `cargo-fuzz` targets for the hand-written parsers:
   `vix-vcard-parser`, `vix-query`, the CSV/TSV/JSON/YAML/TOML converters,
   modeline parsing, macro token parsing. Fix whatever falls out.

### Phase 1 — Capabilities (engine and platform)

The genuinely new powers, biggest first.

1. **Scripting & plugin system (`vix-script`).** The largest capability gap:
   everything today is compiled in. Embed **Rhai** (pure-Rust, no `unsafe`,
   fits the workspace ethos) as a scripting layer:
   - Load user scripts from `~/.config/vix/scripts/*.rhai` and per-project
     `.vix/scripts/`.
   - Script API v1: register palette commands, bind keys, read/modify the
     active buffer (selection, line, whole text), prompt for input, show
     messages, run textops-style transforms.
   - Ship ~6 sample scripts under `examples/scripts/` and document the API.
   - Design doc first at `crates/vix-script/spec/index.md` (API surface,
     sandboxing, error UX), then implement in slices.
2. **Modal-editing depth audit (Vi keymap).** The Vi keymap today is a
   binding table, not a modal engine. Assess and, if the gap is real,
   build `vix-modal`: normal/insert/visual modes, operator × motion
   composition (`d w`, `c i (`), counts, registers, dot-repeat. This is a
   large feature — spec first, land in slices (modes → motions → operators
   → text objects → repeat). Explicitly a *capability* investment: it makes
   the Vi keymap honest.
3. **Performance at scale.** Use the Phase 0 benchmarks to drive: lazy /
   incremental syntax highlighting for large buffers, parallel workspace
   search (walk + search on worker threads, `ignore` crate semantics),
   startup-time budget (measure, then defer non-critical init). Target
   numbers go in the spec: e.g. open 100 MB file < 1 s, keypress-to-frame
   < 16 ms on a 10 MB buffer.
4. **LSP depth audit.** Diagnose against the LSP 3.17 surface; fill the
   real gaps (candidates: semantic-token highlighting, document formatting
   via the server, signature help polish, workspace-wide diagnostics
   panel, multiple servers per buffer). Audit first — several of these may
   partially exist; the task is "close the audit list", not "assume
   missing".
5. **AI provider abstraction.** `vix-ai-panel` / `vix-ai-diff` / DB NL→SQL
   exist. Factor a `vix-ai-core` provider trait (Anthropic, OpenAI-compat,
   local Ollama endpoint), config-driven model/key selection, and add:
   inline "edit selection with instruction", commit-message generation in
   the Git panel, and doc-comment generation. Keep every AI action
   explicit-invoke (no background calls), redact file paths on request.

### Phase 2 — Functionality (user-facing features)

Smaller, high-value features; each follows the standard per-feature recipe.

1. **Structural search & replace** — comby/ast-grep-style pattern matching
   with holes (`$X`), scoped to selection/file/workspace, preview before
   apply.
2. **Theme editor** — live-preview editor for the 16-ish theme slots,
   saving to `~/.config/vix/themes/*.json`; plus 4–6 new bundled themes
   (Solarized both, Catppuccin, Tokyo Night, one high-contrast
   accessibility theme).
3. **Keybinding editor** — view effective bindings per keymap, detect
   conflicts, rebind and persist user overrides.
4. **Snippet editor** — create/edit snippets from inside Vix (currently
   JSON-file-only), with tab-stop placeholders (`$1`, `${2:default}`) if
   not already supported.
5. **Markdown preview scroll-sync + TOC** — keep preview aligned with the
   source cursor; a table-of-contents jump list.
6. **Git history browsing** — commit log panel, file history, show a
   commit's diff, checkout a file at a revision into a read-only tab.
7. **CLI surface** — `vix --diff a b` (open diff view directly), read from
   stdin (`cmd | vix -`), `vix --version --json`; document git
   difftool/mergetool configuration.
8. **Soft-delete to trash** — file-explorer Delete moves to the platform
   trash (with a setting to hard-delete), fixing an easy data-loss
   footgun.

### Phase 3 — Documentation

1. **Docs site (mdBook).** A `book.toml` + `SUMMARY.md` that assembles the
   existing `docs/**/index.md` pages into a navigable book, published via
   CI to GitHub Pages. Restructure top-level into: Getting Started, Guides,
   Features (the existing per-topic pages), Reference, Contributing.
2. **Coverage audit.** One docs page per user-facing crate/feature — ~98
   crates vs ~56 topics today. Generate the gap list mechanically (crate
   list minus docs dirs), then write the missing pages, each with: what it
   is, how to open it, keybindings per keymap, settings, and a text-mockup
   screenshot like the README's.
3. **Generated reference.** The action catalog (~130+ action ids), palette
   commands, settings keys, and per-keymap bindings are all data — generate
   reference pages from the source of truth (an `xtask` or the
   `list_commands` example grown up) so they can never drift. CI check that
   the generated pages are current.
4. **Getting-started guide + man page.** A single narrative
   `docs/getting-started/index.md` (install, first launch, the 10 things to
   learn first), and a generated man page (`clap_mangen`) shipped in
   release artifacts.
5. **Comparison + migration pages refresh.** `docs/for-vim-users`,
   `for-emacs-users`, `comparison` exist — extend with for-vscode-users and
   for-helix-users, and a feature-parity matrix.

### Phase 4 — Tutorials

1. **`vixtutor` — in-editor interactive tutorial.** The signature
   onboarding feature, like `vimtutor`: `vix --tutor` (and Help → Tutorial)
   opens a guided buffer in a scratch copy; lessons are markdown chapters
   with exercises performed in place; progress detection where cheap
   (e.g. "delete this line" checks the buffer). Chapters: moving around,
   editing, find/replace, multi-cursor, files & palette, git basics.
   Localized like everything else.
2. **Written tutorial series** at `docs/tutorials/`, each a task-oriented
   walkthrough against the demo workspace (see Phase 5):
   01 first session, 02 editing power techniques, 03 find/replace &
   multi-cursor, 04 the git workflow, 05 setting up LSP (rust-analyzer,
   pyright, typescript-language-server), 06 Org mode & roam, 07 the DB
   workbench, 08 HTTP client & the Tools suite, 09 make Vix yours (themes,
   keymaps, snippets, settings), 10 debugging with DAP.
3. **Recorded demos.** VHS (`charm vhs`) tape files checked into
   `docs/demos/` so terminal GIFs are reproducible; embed in README and
   the book. One tape per marquee feature (~8 tapes).

### Phase 5 — Examples

1. **Demo workspace** at `examples/demo-workspace/`: a small realistic
   project containing source files in several languages, a `tasks.toml`,
   an `.http` file, an Org directory with roam notes, CSV/TSV data, a
   seeded SQLite database, and intentional TODO/FIXME comments — the shared
   fixture all tutorials and VHS tapes run against. `vix --demo` (or
   documented `cargo run -- examples/demo-workspace`) opens it.
2. **Cargo examples** (grow from 2 to ~12), each a small documented program
   using the workspace crates as libraries: render a frame to text with
   `TestBackend`, theme load/modify/save round-trip, run a `vix-query`
   search over a directory, textops pipeline (sort/dedupe/case), org →
   Markdown/HTML export, record & replay a keyboard macro, drive an LSP
   server headless via `vix-lsp-core`, evaluate the calculator, parse a
   vCard, i18n lookup across locales.
3. **Config examples** at `examples/config/`: annotated `config.toml`
   showing every settings key, a custom theme JSON, custom snippets, a
   `macros.toml`, and (post-Phase-1) sample Rhai scripts.
4. **Example CI check.** `cargo build --examples` and run the headless
   examples in CI so they can never rot.

## Sequencing & dependencies

- Phase 0 first, in full — it is cheap (~days) and everything else leans on
  it.
- Phase 1.1 (scripting) and 1.2 (modal) are the two big rocks; run them as
  spec-first, multi-branch epics. They can proceed in parallel with Phases
  2–5, which are many small independent branches.
- Phase 5.1 (demo workspace) before Phase 4 (tutorials reference it) and
  before the VHS tapes.
- Phase 3.3 (generated reference) before writing docs pages that would
  duplicate the data by hand.

## Risks & mitigations

- **Scope creep on scripting/modal.** Both are editors-within-the-editor.
  Mitigation: spec-first with an explicit v1 cut line; land vertical slices
  that are each shippable.
- **Docs drift.** 98 crates × hand-written pages will rot. Mitigation:
  generate what is data (Phase 3.3), link-check in CI, and keep the
  spec-per-crate discipline as the deep source with docs as the friendly
  layer.
- **Binary size.** Rhai, new grammars, and demos all add weight.
  Mitigation: keep the tree-sitter feature-gating pattern; put Rhai behind
  a default-on `scripting` feature; track binary size in CI.
- **Locale fan-out cost.** Every string × 15 languages. Mitigation: batch
  locale updates per branch; machine-translate then flag for review, as
  established practice.

## Definition of done (per phase)

A phase is done when: all its tasks in `tasks.md` are checked; workspace
builds with zero clippy warnings; tests (including new snapshots/benches
smoke) pass; specs updated; docs/book builds without broken links; and the
CHANGELOG has an entry per user-visible change.
