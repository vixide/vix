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
- Every user-facing string goes through `t!` with keys in `locales/`
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
6. **Security & hardening (added 2026-09-03).** The 2026-07-12 audit
   covered the surface that existed then; the capabilities above open new
   ground it never scoped. A public `SECURITY.md`/disclosure policy, a
   trust prompt before auto-running a project's `.vix/scripts/*.rhai`
   (real today — T102/T103 already ship without one), a permission audit
   of persisted (not just temp) files, and a focused re-audit once
   scripting and AI actually ship. See `tasks.md`'s "Security & hardening"
   subsection (T131–T134) for the concrete list.

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

### Comparator scan — ttt (2026-10-09)

[ttt](https://github.com/eugenioenko/ttt) (Go, MIT, "a TUI that feels like
a GUI") ships every two weeks; v1.5.0 → v1.7.1 (2026-09-10 → 2026-10-09,
~150 merged PRs) was read in full, plus its open issues. Every candidate
was checked against Vix's actual code before being kept. The ones Vix
already has were dropped: inline images (`ratatui_image`), git-status
colors in the explorer, reveal-active-file, trim trailing whitespace,
transparent background, `vix -` stdin buffers, `llms.txt`, LSP `file://`
percent-encoding, and dash-safe `git grep -e`. What's left, as **Run K**
in `tasks.md` (T569–T581):

1. **Security (T569–T574).** These come from ttt's own hardening PRs
   (#628 pipes/devices, #748 credential prompts, #651 flag-shaped
   queries, #698 read-only files, #673 diff memory). Following the same
   threads through Vix's code turned up one high-severity bug: **Git →
   Clone…** interpolates the prompted URL into a shell command
   *unquoted*. The others: remote VCS commands that can grab `/dev/tty`
   for a password prompt while the TUI is in raw mode; a file-open path
   with no guard against FIFOs, devices, huge files, or non-UTF-8 bytes;
   a `read_only` flag that is never set from file permissions; tests not
   proven network-free (ttt's AUR incident); and an unbudgeted gutter
   diff.
2. **Robustness (T575).** An App-level "chaos monkey": random
   key/mouse/resize streams driven through the real `App` on
   `TestBackend`, with per-iteration hang detection. Vix's `fuzz/` covers
   parsers only.
3. **Features (T576–T581).** LSP Restart Server and Show Server Log; an
   opt-in "stage everything?" step when committing with nothing staged;
   scrollbar markers for git changes, diagnostics, and search matches;
   `theme = "auto"` from the terminal's real background color; Move Line
   across collapsed folds; autoscroll while drag-selecting past the
   edge.

Larger or speculative ideas from the same scan (per-script persistent
storage and gutter-click hooks for Rhai, runtime-loadable grammars, a
right-docked bottom panel, a remote-control socket for multiplexer
integration) go to the `tasks.md` Ideas backlog rather than to tasks.

### Capability Cargo features (researched 2026-10-09)

**Goal.** Make whole capabilities, such as the **DB** menu and its
workbench (`sql`) or the **Git** menu and its integration (`git`),
optional at build time, the same way tree-sitter grammars already are
(`syntax-*`/`lang-*`, forwarded to `vix-editor-core`).

**This reverses a past decision, and the plan has to answer it.** T101
put scripting behind a `scripting` feature. T103 made it unconditional
again, reasoning that once a crate is wired into the App shell (startup,
palette, menu, prompts) it is as core as `vix-editor`. T111 made the same
call for `vix-modal`. That reasoning holds only as long as there's no
payoff. Measured today, some areas have a real payoff and some don't:

| Area | Third-party crates that disappear when off | Native C | Process/network surface removed | Coupling into the App shell |
|---|---|---|---|---|
| `sql` (vix-db, vix-edit-sql) | **48** (sqlx×6, tokio, libsqlite3-sys, tracing, …) | bundled SQLite | TCP to Postgres/MySQL, `ssh` tunnels, password command | medium: ~500 lines inline in `src/app.rs`; `vix-settings` holds `vix_db::Connection` |
| `image` | **81** (mostly from `image`'s default codecs: AV1 *encoder* rav1e/ravif, exr, tiff, …) | — | untrusted-file codec parsing | medium: `Tab.image` lives in `vix-editor` |
| `script` (vix-script) | **9**; rhai is one of the largest crates in the build | — | runs user `.rhai` | easy-medium: already isolated in `src/app/scripts.rs` |
| `terminal` (vix-terminal) | **10** (portable-pty, vt100, nix 0.28, …) | — | interactive shell in a PTY | easy |
| `http` + `ai` | **2** alone; **88** together with `sql` (rustls, **ring**, url/idna/icu, keyring) | ring | all outbound HTTP; `ai_command` shell | easy (http), medium (ai is also used by Git's commit message and DB assist) |
| `git` (+`jj`) | **0** | — | spawning `git`/`jj` | medium-hard: git feeds explorer colors, status bar, gutter, dashboard; `src/app/git.rs` also hosts the jj and DAP dispatch |
| `lsp` / `dap` | **0** | — | spawning servers/adapters | hard: completion popup reused by Org-roam/contacts; `lsp_enabled` already exists as a runtime off switch |
| `org` | **0** | — | none | hard (143 menu leaves, Emacs keymap, `vix-column-view`, settings) — **not worth flagging** |

So `sql`, `image`, `script`, and `terminal` are worth gating for build
size, compile time, and C toolchain needs. Turning off `sql`+`http`+`ai`
leaves tree-sitter as the only C in the default build, which also eases
the musl cross-build. `git`, `jj`, `lsp`, `dap`, and `ai` are worth
gating only to shrink attack surface for locked-down builds, so they come
later. `org` stays unconditional.

**Design.**

1. **Every capability feature is on by default.** `default = ["syntax-common", "sql", "git", …]`.
   Released binaries and every install channel stay byte-for-byte the
   same in content. Minimal builds are `--no-default-features --features
   …`. Whether to *ship* a minimal variant is a separate distribution
   decision; each extra variant multiplies cargo-dist artifacts and SBOMs.
2. **Gate at the root, filter at runtime. Do not scatter `cfg` through
   `vix-menu`/`vix-palette`.** One `Capability` enum in the root, with
   `Capability::enabled()` = `cfg!(feature = "…")`, and one mapping from
   action-id prefix to capability (`db.`→Sql, `git.`→Git, `jj.`→Jj,
   `ai.`→Ai, `run.`→Dap, …). `build_menus()` drops top-level menus and
   leaves whose capability is off, and so does the palette. A disabled id
   reached any other way (keymap, script `bind_key`, typed in the
   palette) gets a translated message: "*X* is not included in this
   build". That leaves `vix-menu`/`vix-palette` free of `cfg`.
   `#[cfg(feature)]` then lives only where a crate actually has to be
   absent: optional `dep:` edges, App fields, the dispatch functions,
   poll calls in `main.rs`, and the `src/ui/*` draw calls.
3. **Settings never lose data.** Before any gating, move the plain data
   types `vix-settings` borrows (`vix_db::connect::Connection`,
   `vix_dap::DebugAdapter`, `vix_org_capture::CaptureTemplate`) out of
   those crates, so they are compiled into every build.
   `Settings::save` (confy) writes back only fields it knows. If those
   fields were gated, a no-`sql` build would silently delete the user's
   `db_connections` on the next save.
4. **Fix shared plumbing first.**
   - `menu_index_for_alt` (`src/app.rs:13787`) hard-codes Alt-mnemonic
     indices (`g`→9, …). Dropping Run, AI, DB, or JJ would send every
     later mnemonic to the wrong menu, so it must look menus up by name.
   - The root re-declares sqlx/tokio/futures-util/ureq/portable-pty but
     never uses them. They have to go, or the feature will not shrink
     anything.
   - The dead `optional` root `tree-sitter-*` deps create stray implicit
     features, so they go too.
   - Do the cheap win before adding any flag: `image` is pulled in with
     default features (`image-defaults`), which compiles an AV1
     *encoder* Vix never uses. Narrowing it to png/jpeg/gif/webp may
     remove most of `image`'s 81 crates on its own.
5. **CI keeps every combination compiling.** No job builds anything but
   default features today, so gated code would rot immediately. Add a
   Linux job: `--no-default-features` build plus test, and
   `cargo hack --each-feature`. The source-text checks stay
   default-feature-only: `dispatch_scan` + `tests/action_catalog.rs`,
   menu-bar insta snapshots, and the `list_commands --write` /
   `generate_man` diffs.
6. **Specs.** A new `spec/cargo-features/index.md` holds the
   capability→crates→action-prefix table, the rules above, and the
   T103 reversal with its rationale. `crates/vix-script/spec/index.md` §
   "Packaging: not a Cargo feature" is updated to point there. The
   README/AGENTS.md feature sections say what each flag removes.

**Order** (tracked as **Run L**, T582–T594 in `tasks.md`): plumbing and
the image slimming → settings decoupling → capability framework + CI
matrix → `sql` → `image` → `script` → `terminal` → `http`/`ai` →
`git`/`jj` → (optional) `lsp`/`dap` → distribution decision. Extract
each area's inline dispatch into its own `src/app/<area>.rs` first
(DB, AI, HTTP, terminal are still inline in `src/app.rs`; jj and the DAP
`run.*` arms are misfiled in `src/app/git.rs`). That refactor is worth
doing even if a flag never lands.

### New capabilities and code quality (researched 2026-10-21)

All 594 earlier tasks are done, so this round started fresh with three
read-only research passes. Two compared Vix with the 2026 state of the art:
Helix 25.x, Zed, Neovim 0.11/0.12, VS Code, Kakoune, Fresh, ttt, and Emacs
(Magit, wdired). One pass covered language intelligence and core editing,
the other UX, workflow and platform. The third pass measured code quality
with clippy, `cargo machete`, `cargo tree -d`, a timed test run,
`cargo-mutants`, and function-length and spec-drift scripts. Each candidate
was checked against Vix's actual code before it was kept. The kept items
are **Run M** in `tasks.md` (T595–T638).

**The finding in one line:** LSP *breadth* is complete (35 methods wired,
T123 closed), but several features stop at a first version. Examples:
completion that doesn't filter as you type, no snippet expansion,
diagnostics that never show their text in the editor, full-document sync,
and a one-level debugger. The platform also has gaps that matter in daily
use: no clipboard over SSH, no live settings reload, no per-project
settings, and no crash recovery for unsaved buffers.

1. **Code quality and correctness (T595–T611).**
   - *Crash bug.* One real crash: an Org table sort by time slices a string
     by bytes (`vix-org-table/src/lib.rs:871`), the same bug class as T559,
     and 154 `string_slice` sites remain workspace-wide.
   - *Correctness.* A process-global in-memory clipboard that still makes a
     known test flaky. Spellcheck recomputed on every frame. Discarded
     `Result`s that hide failures (a script-trust decision can be lost).
   - *Build hygiene.* Lint attributes repeated in 124 crate roots, while 15
     build targets (benches, some tests, a `build.rs`) get no lints at all.
     About 27 unused root dependencies. 215 hard-coded internal version
     pins. Breaking API changes shipped in minor versions of published
     crates, with no semver check.
   - *Unwraps.* About 40 draw-path `unwrap()`s that a signature change
     removes.
   - *Tests.* nextest (75.8 s wall vs 32.6 s of actual test time), mutation
     testing, coverage, a DAP mock adapter, and overlay snapshots.
   - *Larger refactors.* One overlay priority table shared by drawing and
     key routing, and typed errors in library crates.
2. **Language intelligence and editing (T612–T624).** Completion
   filter-as-you-type and auto-trigger, with `textEdit` and
   `additionalTextEdits` (auto-import). LSP snippets through the existing
   `vix-snippets` engine. Inline diagnostics. Incremental `didChange` and
   ranged semantic tokens. Tree-sitter textobjects and function/class
   motions. Debugger depth (conditional/log breakpoints, attach, a variable
   tree). Inline ghost-text completion. Pull diagnostics. Outgoing calls and
   type hierarchy. Document colors and links. Multi-cursor depth (Kakoune
   split/select-in-selection). File-rename and watched-file notifications.
   On-type formatting.
3. **UX, workflow and platform (T625–T638).** OSC 52 clipboard over SSH and
   tmux. Live settings reload. Per-project and per-language settings, gated
   by workspace trust. Synchronized output, focus events and color-depth
   fallback. Hot-exit backups of unsaved buffers. Picker preview and resume.
   Terminal scrollback and multiple terminals. Problem matchers with
   next-error. Interactive rebase. Stash list and worktrees. An Agent Client
   Protocol (ACP) client. Oil-style directory editing. A Rhai plugin
   installer. An undo-tree visualizer.

**Sequencing.**
- T595 (crash) comes first. The cheap quality items (T596–T601, T604) go
  next, because they shorten every later task's feedback loop, nextest
  especially.
- Then the high-value, small-to-medium features: T612/T613 (completion +
  snippets), T614 (inline diagnostics), T625–T627 (OSC 52, settings reload,
  project settings).
- The large items (T618 ghost text, T635 ACP, T624 rebuilt multi-cursor
  motions, T611 overlay table) come last. Each should get a spec written
  and agreed before code.
- T616 (textobjects) should land before Run B's deferred modal work
  (T112–T115), which would bind it as `if`/`af`.

**Deliberately not proposed.** The scan found these already in Vix:
- kitty keyboard protocol, OSC 11 background detection
- jump labels, bookmarks, session restore
- wgrep, 3-way merge, project frecency, `--doctor`
- semantic tokens, inlay hints, code lens, linked editing, LSP
  restart/log, tree-sitter injections, large-file prompt

These were left out on purpose: ex-command scripting (excluded by the
`vix-modal` spec) and anything already in the Ideas backlog (remote/SSH,
collaborative editing, runtime grammars, remote-control socket). Duplicate
dependency versions all come from third-party crates, and `deny.toml`
already warns on them.

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
- **Binary size.** New grammars and demos add weight. Mitigation: keep
  the tree-sitter feature-gating pattern; track binary size in CI.
  (Rhai/scripting itself ended up a plain dependency, not a feature flag
  — see `crates/vix-script/spec/index.md`'s "Packaging" section for why
  the original default-on-feature plan changed once T103 actually wired
  it into the App shell.)
- **Locale fan-out cost.** Every string × 15 languages. Mitigation: batch
  locale updates per branch; machine-translate then flag for review, as
  established practice.
- **New capabilities widen the attack surface.** Scripting and AI both
  add ways for external input (a cloned repo's `.vix/scripts/`, an AI
  provider's response) to reach the editor. Mitigation: the Security &
  hardening tasks (T131–T134) — audit and harden each capability's real
  surface once it ships, don't assume the design doc's intent already
  covers it.

## Definition of done (per phase)

A phase is done when: all its tasks in `tasks.md` are checked; workspace
builds with zero clippy warnings; tests (including new snapshots/benches
smoke) pass; specs updated; docs/book builds without broken links; and the
CHANGELOG has an entry per user-visible change.
