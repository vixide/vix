# Vix Improvement Tasks

Actionable checklist for [`plan.md`](plan.md). Work top to bottom unless a
task says otherwise; each task is one feature branch off `main`, kept green,
merged `--no-ff`, branch deleted.

**Every task must follow the standard recipe** (from `AGENTS.md`):

- New crate → `crates/vix-<name>/` with `#![forbid(unsafe_code)]`,
  `#![deny(missing_docs)]`, `#![warn(clippy::pedantic)]`, and a
  `spec/index.md`.
- New user-facing feature → one action id + one `run_action` arm, a menu
  item, a palette command, a keybinding if a free combo exists.
- All user-facing text via `t!` with keys added to `locales/app.yml` for all
  15 languages: en es fr de cy ga gd pl pt ru ar hi bn zh ja.
- Tests for the new behavior; `cargo build`, `cargo test`, and
  `cargo clippy --workspace --all-targets -- -D warnings` all clean.
- Update the owning `spec/index.md` (and repo-root `spec/` if
  cross-cutting); add a `CHANGELOG.md` entry for user-visible changes.

Task IDs are stable — reference them in branch names (e.g. `feat/T101-ci`).

---

## Phase 0 — Safety net

- [ ] **T001 — CI workflow.** Add `.github/workflows/ci.yml`: jobs for
  `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all --check`; matrix `ubuntu-latest` + `macos-latest`;
  triggers push + PR; `Swatinem/rust-cache`. Keep total wall time sane
  (share a build via job needs or one job with steps).
- [ ] **T002 — Docs CI job.** In `ci.yml`, add a job that link-checks all
  `*.md` (lychee, offline-links at minimum, external links non-blocking)
  and runs `cargo doc --workspace --no-deps` with warnings denied.
- [ ] **T003 — cargo-deny.** Add `deny.toml` (licenses: Apache-2.0/MIT
  compatible; advisories; bans on duplicate major versions where feasible)
  and a CI job running `cargo deny check`.
- [ ] **T004 — TUI snapshot harness.** Add `tests/snapshots.rs` (or a
  `vix-test-support` crate) that boots the App against ratatui
  `TestBackend` at 100×30, feeds scripted key events, and asserts golden
  text screens (insta `assert_snapshot!`). Document how to review/update
  snapshots in `AGENTS/conventions.md`.
- [ ] **T005 — Seed snapshots.** Using T004: welcome screen, editor with a
  Rust file, File menu open, palette open with query, find bar with
  matches, git panel, table edit surface, F1 help overlay, zen mode, a
  theme other than default. ~10 snapshots.
- [ ] **T006 — Benchmarks.** Add criterion benches (root `benches/` or in
  `vix-editor-core`): open/parse 100 MB synthetic file, 10k random inserts
  and deletes, syntax-highlight a 5 MB Rust file, workspace search over a
  generated 10k-file tree, palette fuzzy scoring over 10k candidates.
  `cargo bench` documented; record baseline numbers in a new
  `docs/performance/index.md`.
- [ ] **T007 — Fuzz targets.** Add `fuzz/` (cargo-fuzz) with targets:
  vcard parsing (`vix-vcard-parser`), query parsing (`vix-query`), each
  tabular/JSON/YAML/TOML converter round-trip, modeline parsing, macro
  token parsing (`vix-macros`). Run each locally ≥ 10 min; fix all crashes
  found; add regression tests for fixes. Fuzzing is not in CI (cost), but
  document the invocation in `AGENTS/conventions.md`.
- [ ] **T008 — Binary-size tracking.** CI step that builds
  `--release` (default features), records the stripped binary size, and
  comments/records it so growth is visible per PR.

## Phase 1 — Capabilities

### Scripting (epic — spec first, then slices)

- [ ] **T101 — Scripting spec.** Write `crates/vix-script/spec/index.md`
  before any code: Rhai as the engine (pure Rust, no unsafe); script
  discovery (`~/.config/vix/scripts/*.rhai`, project `.vix/scripts/`);
  API v1 surface (register command, bind key, buffer get/set text,
  selection get/set, prompt, message, apply-transform); error UX (script
  errors go to the message drawer, never crash); the `scripting` cargo
  feature (default on). Get the spec merged as its own branch.
- [ ] **T102 — `vix-script` core.** New crate: Rhai engine wrapper, script
  loading, the buffer/selection/message API bound to host callbacks, unit
  tests with a mock host.
- [ ] **T103 — Host wiring.** App shell: load scripts at startup, surface
  registered commands in the palette (prefixed, e.g. `script:`), execute
  with the active editor, route errors to messages. Action ids
  `script.reload`, `script.run`; Tools → Scripts submenu (list + Reload).
- [ ] **T104 — Script keybindings.** Allow scripts to bind keys via the
  existing keymap-model override layer; conflicts reported, never silently
  clobbered.
- [ ] **T105 — Sample scripts + docs.** ~6 scripts in `examples/scripts/`
  (e.g. wrap-selection-in-markdown-link, insert-file-header,
  title-case-line, dedupe-selection, timestamp-signature, open-scratch-
  with-template); write `docs/scripting/index.md` documenting the full
  API v1 with each sample explained.

### Modal editing (epic — audit first)

- [ ] **T111 — Modal audit + spec.** Audit what the Vi keymap actually
  does today vs a real modal engine. Write `crates/vix-modal/spec/index.md`:
  modes (normal/insert/visual/visual-line), operator × motion grammar,
  counts, registers, dot-repeat; explicit v1 cut line (no ex commands, no
  macros — Vix already has macros). Merge the spec.
- [ ] **T112 — Mode engine.** `vix-modal` crate: mode state machine, key
  dispatch that intercepts before the normal keymap when the Vi keymap +
  modal setting are active, mode shown in the status bar.
- [ ] **T113 — Motions + counts.** `h j k l w b e 0 $ ^ gg G { } f/t/F/T`
  with counts, as pure functions over editor-core positions; heavy unit
  tests.
- [ ] **T114 — Operators.** `d c y p x` composing with T113 motions and
  visual selections; registers (unnamed + named a–z); tests per
  operator×motion pair for a representative grid.
- [ ] **T115 — Text objects + repeat.** `iw aw i( a( i" a"` etc. via
  editor-core's structural selection where possible; dot-repeat of the
  last change. Update `docs/for-vim-users/` to state exactly what is and
  isn't supported.

### Performance & depth

- [ ] **T121 — Perf: highlight and search.** Driven by T006 baselines:
  make syntax highlighting incremental/lazy for buffers past a size
  threshold, and parallelize workspace search. Set explicit targets in the
  relevant specs (e.g. open 100 MB < 1 s; keypress-to-frame < 16 ms at
  10 MB; workspace search 10k files < 500 ms) and prove them with the
  benches.
- [ ] **T122 — Startup budget.** Measure cold start; defer non-critical
  init (locale table build, theme scan, snippet load) off the first-frame
  path if measurement says it matters. Record before/after in
  `docs/performance/index.md`.
- [ ] **T123 — LSP depth audit.** Diff `vix-lsp`/`vix-lsp-core` against
  LSP 3.17: check semantic tokens, document formatting/range formatting,
  signature help, workspace diagnostics, multiple servers per buffer.
  Produce the gap list as a spec update, then file one follow-up task per
  real gap (append them to this file under T123a, T123b, …) and implement.
- [ ] **T124 — AI provider abstraction.** Factor `vix-ai-core`: provider
  trait + Anthropic, OpenAI-compatible, and Ollama implementations;
  config keys for endpoint/model/key (keyring-backed like the DB
  credential waterfall). Migrate `vix-ai-panel`, `vix-ai-diff`, and DB
  NL→SQL onto it with zero behavior change.
- [ ] **T125 — AI features.** On T124: "Edit selection with instruction"
  (AI menu; result as a reviewable diff via `vix-ai-diff`), commit-message
  generation in the Git panel (fills the message box, never commits), and
  "Generate doc comment" for the symbol under the cursor. All
  explicit-invoke only.

## Phase 2 — Functionality

- [ ] **T201 — Structural search & replace.** New crate
  `vix-structural-replace`: pattern syntax with holes (`$X`, `$$X` for
  multi), balanced-delimiter aware matching (reuse tree-sitter where
  loaded, fall back to bracket-balanced text matching); scope
  selection/file/workspace; preview list with per-match accept, like
  query-replace. Edit menu + palette.
- [ ] **T202 — Theme editor.** Tools (or View → Themes → Edit): list the
  theme's color slots, edit with the existing color-picker machinery, live
  preview on the real UI, save-as to `~/.config/vix/themes/<name>.json`.
- [ ] **T203 — New bundled themes.** Solarized Dark, Solarized Light,
  Catppuccin Mocha, Tokyo Night, and one WCAG-AA high-contrast theme.
  Snapshot test each (T004 harness) so slots can't silently regress.
- [ ] **T204 — Keybinding editor.** Help (or Settings) → Keybindings:
  searchable table of effective bindings for the active keymap (reuse
  `vix-keyboard-shortcut-panel` data), conflict detection, rebind → saved
  to a user-overrides file layered over the keymap; Reset to default.
- [ ] **T205 — Snippet editor + tab stops.** Audit whether `$1`/`${2:def}`
  tab stops exist in snippet expansion; implement if not. Add a snippet
  create/edit dialog writing to the user snippets scope; New Snippet from
  Selection.
- [ ] **T206 — Markdown preview sync + TOC.** Scroll-sync preview to the
  source cursor line; TOC jump list over the headings (reuse outline
  machinery if possible).
- [ ] **T207 — Git history.** Git menu: Log (commit list panel → select
  shows the commit diff in a tab), File History for the active file, and
  Open File at Revision (read-only tab titled `file @ abbrev-sha`).
- [ ] **T208 — CLI surface.** `vix --diff a b` opens the diff view
  directly; `vix -` reads stdin into a scratch buffer; `vix --version
  --json` for tooling. Update `--help`, README, and add
  `docs/cli/index.md` including git difftool/mergetool config snippets.
- [ ] **T209 — Trash on delete.** File-explorer Delete moves to the OS
  trash (`trash` crate) with setting `explorer.delete = "trash" | "hard"`
  (default trash); the confirm prompt says which will happen.

## Phase 3 — Documentation

- [ ] **T301 — mdBook site.** Add `book.toml` + `docs/SUMMARY.md`
  organizing existing pages into: Getting Started / Guides / Features /
  Reference / Contributing. `mdbook build` clean; CI job builds and
  deploys to GitHub Pages on `main`. Do not move files unless mdBook
  forces it — prefer SUMMARY links into the existing layout.
- [ ] **T302 — Docs coverage audit.** Script (in `scripts/`) that lists
  user-facing crates/features lacking a `docs/<topic>/index.md`; check its
  output into `docs/coverage.md`. Merge the audit before writing pages.
- [ ] **T303 — Fill missing docs pages (batch 1: panels & tools).** From
  T302's list, write pages for the undocumented panels and Tools-menu
  tools. Template per page: what it is, how to open (menu, palette,
  keybinding per major keymap), settings, a text-mockup screenshot, links
  to the crate spec.
- [ ] **T304 — Fill missing docs pages (batch 2: everything else).**
  Remainder of T302's list, same template. Target: coverage.md shows zero
  gaps.
- [ ] **T305 — Generated reference.** Grow `examples/list_commands.rs`
  into an `xtask` (or `scripts/`) generator that emits
  `docs/reference/actions.md` (all action ids + descriptions),
  `docs/reference/settings.md` (every settings key, type, default), and
  `docs/reference/keybindings-<keymap>.md` per keymap from the keymap
  data. CI check: regenerate and `git diff --exit-code`.
- [ ] **T306 — Getting-started guide.** `docs/getting-started/index.md`:
  install (source, and the debian/homebrew paths per `spec/debian`,
  `spec/homebrew-tap-token` once real), first launch, the 10 essentials
  (palette, explorer, find, save, splits, git, help). Link from README
  top.
- [ ] **T307 — Man page.** Generate `vix.1` with `clap_mangen` at build
  or via xtask; include in release artifacts (`release.yml`); document.
- [ ] **T308 — Migration guides.** Add `docs/for-vscode-users/` and
  `docs/for-helix-users/` in the style of the existing for-vim/for-emacs
  pages; refresh `docs/comparison/` into a feature-parity matrix
  (Vix / Vim / Helix / Micro / Zed-ish columns, honest ✓/✗).
- [ ] **T309 — CHANGELOG discipline.** Backfill `CHANGELOG.md` top section
  from git history since the last entry; add the "changelog entry per
  user-visible change" rule to `AGENTS/conventions.md` (already implied by
  this file — make it explicit there).

## Phase 4 — Tutorials

- [ ] **T401 — vixtutor spec.** `crates/vix-tutor/spec/index.md`: launch
  via `vix --tutor` and Help → Tutorial; opens a working copy (temp dir)
  of lesson buffers so the user edits freely; chapter navigation
  (next/prev lesson actions); cheap progress checks where possible
  ("delete this line", "change this word" verified against the buffer);
  content localized via the standard `t!` pipeline or per-locale lesson
  files — decide in the spec. Merge spec first.
- [ ] **T402 — vixtutor engine + chapter 1.** `vix-tutor` crate + host
  wiring per the recipe; chapter 1 "Moving around" complete with checks.
- [ ] **T403 — vixtutor chapters 2–6.** Editing basics; find & replace;
  multi-cursor & selection; files, tabs & palette; git basics. Each
  chapter is a small self-contained lesson file.
- [ ] **T404 — Written tutorials 01–05.** `docs/tutorials/`: 01 your first
  session, 02 editing power techniques, 03 find/replace & multi-cursor,
  04 the git workflow, 05 setting up LSP (rust-analyzer, pyright,
  typescript-language-server with real config). Each runs against the
  demo workspace (T501 — do that first).
- [ ] **T405 — Written tutorials 06–10.** 06 Org mode & roam, 07 the DB
  workbench (uses the seeded SQLite db), 08 HTTP client & Tools suite,
  09 make Vix yours (themes/keymaps/snippets/settings), 10 debugging with
  DAP (real debugpy or codelldb walkthrough).
- [ ] **T406 — VHS demo tapes.** `docs/demos/*.tape` (charm VHS) for ~8
  marquee features: overview tour, palette, multi-cursor, git hunks, DB
  workbench, org-roam, edit surfaces, themes. A `scripts/render-demos.sh`
  regenerates GIFs; embed the overview GIF in README. Tapes run against
  the demo workspace.

## Phase 5 — Examples

- [ ] **T501 — Demo workspace.** `examples/demo-workspace/`: a small
  realistic project — Rust + Python + Markdown sources with intentional
  TODO/FIXME tags, `tasks.toml`, an `.http` file against
  httpbin-style endpoints, `org/` with a few roam-linked notes and a
  dailies entry, `data/*.csv|tsv`, a seeded `demo.sqlite` (with the seed
  SQL checked in and a script to regenerate), and a README explaining the
  tour. Keep it a few hundred KB max; exclude from the workspace build.
  **Do this before T404–T406.**
- [ ] **T502 — Cargo examples batch 1 (editor as a library).**
  `render_frame` (TestBackend → print the screen as text),
  `theme_roundtrip` (load bundled theme, tweak, save, reload),
  `textops_pipeline` (sort/dedupe/case a file from the CLI),
  `macro_replay` (parse a macros.toml and replay onto a buffer). Each
  ≤ ~100 lines, heavily commented, listed in README.
- [ ] **T503 — Cargo examples batch 2 (services & formats).**
  `query_search` (vix-query over a directory), `org_export` (org →
  Markdown/HTML), `vcard_parse`, `lsp_headless` (spawn a server via
  vix-lsp-core, open a doc, print diagnostics), `i18n_lookup` (one key in
  all 15 locales), `calculator_eval`.
- [ ] **T504 — Config examples.** `examples/config/`: fully-annotated
  `config.toml` covering every settings key (cross-check against T305's
  generated settings reference), a custom theme JSON, custom user
  snippets, a `macros.toml`, and sample Rhai scripts (after T105).
- [ ] **T505 — Examples in CI.** Extend `ci.yml`: `cargo build --examples`
  and execute the headless examples (`render_frame`, `textops_pipeline`,
  `query_search`, `list_commands`, `headless_edit`) so examples can't rot.

---

## Suggested execution order (batched for agent runs)

1. **Run A (infrastructure):** T001–T008.
2. **Run B (big rocks kickoff):** T101, T111 (specs only), then T102–T105
   and T112–T115 as follow-on runs.
3. **Run C (features):** T201–T209 in any order, one branch each.
4. **Run D (docs):** T301, T302, T305 first; then T303, T304, T306–T309.
5. **Run E (demo + tutorials):** T501, then T401–T406, T404/T405 last.
6. **Run F (examples):** T502–T505.
7. **Deferred/audit-driven:** T121–T125 whenever their prerequisite data
   (benches, audits) exists.

When a task is finished: check its box here, note the branch/merge commit,
and record anything learned that changes later tasks.
