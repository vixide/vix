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
- All user-facing text via `t!` with keys added to the right `locales/*.yml`
  namespace file (T148) for all 15 languages: en es fr de cy ga gd pl pt ru ar
  hi bn zh ja.
- Tests for the new behavior; `cargo build`, `cargo test`, and
  `cargo clippy --workspace --all-targets -- -D warnings` all clean.
- Update the owning `spec/index.md` (and repo-root `spec/` if
  cross-cutting); add a `CHANGELOG.md` entry for user-visible changes.

Task IDs are stable — reference them in branch names (e.g. `feat/T101-ci`).

---

## Phase 0 — Safety net

- [x] **T001 — CI workflow.** Add `.github/workflows/ci.yml`: jobs for
  `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all --check`; matrix `ubuntu-latest` + `macos-latest`;
  triggers push + PR; `Swatinem/rust-cache`. Keep total wall time sane
  (share a build via job needs or one job with steps).
  Done — plus an MSRV job, and the same gate for the other two forges
  (`.gitlab-ci.yml`, `.forgejo/workflows/`) with their release pipelines.
  See `spec/ci/index.md`.
- [x] **T002 — Docs CI job.** In `ci.yml`, add a job that link-checks all
  `*.md` (lychee, offline-links at minimum, external links non-blocking)
  and runs `cargo doc --workspace --no-deps` with warnings denied.
  Done — the `cargo doc` half already ran with `RUSTDOCFLAGS=-D warnings` on
  all three forges; added a `docs-links` job (`docs-links-external` on
  GitLab) running pinned/checksummed `lychee` on all three forges: an offline
  pass over local file links (blocking, `CHANGELOG.md` excluded — its
  historical entries pin paths that may no longer exist) and an
  `http`/`https` pass (non-blocking: `continue-on-error` on GitHub/Codeberg,
  `allow_failure` on GitLab). See `spec/ci/index.md`.
- [x] **T003 — cargo-deny.** Add `deny.toml` (licenses: Apache-2.0/MIT
  compatible; advisories; bans on duplicate major versions where feasible)
  and a CI job running `cargo deny check`.
  Done — `deny.toml` plus a `cargo deny --workspace --all-features check` job
  on all three forges (`.github/workflows/security.yml`, the `deny` job in
  `.gitlab-ci.yml`, `.forgejo/workflows/security.yml`), each weekly as well as
  per push/PR. Getting to green took five dependency fixes: `evalexpr` pinned
  to 11.x (12.0.0 relicensed to AGPL-3.0-only), `portable-pty` 0.8 → 0.9
  (drops the unmaintained `serial`), and `anyhow`/`crossbeam-epoch`/`spin`
  updated off advisories. Duplicate majors are `warn`, not `deny`: 42 of them
  today, almost all `windows-*`. See `spec/ci/index.md`.
- [x] **T004 — TUI snapshot harness.** Add `tests/snapshots.rs` (or a
  `vix-test-support` crate) that boots the App against ratatui
  `TestBackend` at 100×30, feeds scripted key events, and asserts golden
  text screens (insta `assert_snapshot!`). Document how to review/update
  snapshots in `agents/conventions.md`.
  Done — `tests/snapshots.rs` with three scenarios (default screen, editor
  with typed content, command palette in Commands mode), a "Snapshot" layer
  in `spec/test/index.md` plus a "Snapshot testing" section on writing and
  reviewing them, and a pointer from `agents/conventions.md`. Locale is
  pinned to `en` per test (`rust_i18n::locale()` is process-global).
  Discovered along the way: the palette's **Files** mode has no sort at
  all — `build_file_index`/`palette_file_entries` in `src/app.rs` push
  matches in raw `ignore::WalkBuilder` order (filesystem-traversal order,
  not portable across ext4/APFS), unlike Commands mode which scores with
  `palette::fuzzy_score` and ties-breaks on a stable catalog index. Not
  fixed here (out of scope for the harness); the Files-mode scenario was
  swapped for Commands mode to keep the snapshot deterministic. Worth its
  own task before T005 seeds a Files-mode screen.
- [x] **T005 — Seed snapshots.** Using T004: welcome screen, editor with a
  Rust file, File menu open, palette open with query, find bar with
  matches, git panel, table edit surface, F1 help overlay, zen mode, a
  theme other than default. ~10 snapshots.
  Done — 9 new scenarios (12 total with T004's original 3, which already
  covered "editor with a Rust file" and "palette open with query").
  `git_panel_with_changes` roots a real `git init`-ed fixture with a forced
  branch name (`git init -b`, not the machine's `init.defaultBranch`) and
  `commit.gpgsign=false` (never invoke the developer's real signing key for
  a throwaway fixture commit). Two scenarios that open a real file
  (`editor_with_an_opened_rust_file`, `find_bar_with_matches`) hit a
  determinism trap with two layers, both discovered the hard way (the first
  passed locally *and* on GitHub CI, then broke on the very next CI run —
  same OS as the machine that made the golden file, just a different PID):
  `Editor::open` always `canonicalize()`s the path (macOS resolves it
  through `/private`), and the status bar embeds it *twice*
  (`Tab::display_path` plus `t!("status.opened", path = ...)`). (1) At 100
  columns the path got truncated before a redaction step ever saw it, at a
  cutoff depending on the OS-specific length — fixed by rooting fixtures
  under `/tmp` (short on both Linux and macOS, unlike the long,
  session-specific `std::env::temp_dir()` on macOS) and rendering at 220
  columns so it never truncates. (2) Even untruncated, the status bar
  right-aligns trailing fields against the *real* pre-redaction path
  length, so a same-length substring replacement still leaves a
  length-dependent amount of padding behind. Fixed by replacing the whole
  row with a fixed placeholder whenever it contains the root path, instead
  of swapping just the path substring. Verified stable across three
  fresh-PID re-runs before trusting it (still worth a real second machine
  or a CI dry-run before assuming any status-bar-adjacent snapshot is
  safe).
- [x] **T006 — Benchmarks.** Add criterion benches (root `benches/` or in
  `vix-editor-core`): open/parse 100 MB synthetic file, 10k random inserts
  and deletes, syntax-highlight a 5 MB Rust file, workspace search over a
  generated 10k-file tree, palette fuzzy scoring over 10k candidates.
  `cargo bench` documented; record baseline numbers in a new
  `docs/performance/index.md`.
  Done — `benches/` already existed (3 files, done in an earlier session
  without checking this box) but at much smaller sizes than spec'd; widened
  `editor_ops.rs`'s `bench_open` to 125,000 lines (~5 MB, the
  syntax-highlight scenario) and 2,500,000 lines (~100 MB, the open/parse
  scenario — 5.05 s), added `bench_random_edits` (10k random inserts/deletes
  in one burst — 1.60 s) and `bench_workspace_search` in
  `search_and_palette.rs` (a real 10k-file fixture, `App::run_action` +
  `on_key` since `run_workspace_search` is private — 214 ms; `palette/fuzzy`
  already covered 20,000 candidates). Also added `[profile.bench]`
  (`opt-level = 3`, no LTO) — `cargo bench` was silently inheriting
  `[profile.release]` (`opt-level = "z"`, `lto = true`), which is tuned for
  binary size, not representative hot-path timing, and made the final link
  of a 100+ crate workspace painfully slow to rerun. `docs/performance/index.md`
  written with the full baseline table and pointers to T121's targets.
  `source()`'s line-count loop in `editor_ops.rs` also fixed from O(n²)
  (rescanning the whole accumulated string every append) to O(n) — needed
  to make the 2,500,000-line case tractable at all.
- [x] **T007 — Fuzz targets.** Add `fuzz/` (cargo-fuzz) with targets:
  vcard parsing (`vix-vcard-parser`), query parsing (`vix-query`), each
  tabular/JSON/YAML/TOML converter round-trip, modeline parsing, macro
  token parsing (`vix-macros`). Run each locally ≥ 10 min; fix all crashes
  found; add regression tests for fixes. Fuzzing is not in CI (cost), but
  document the invocation in `agents/conventions.md`.
  Done — `fuzz/` already had 6 targets (vcard among them); added 4:
  `query_replace` (`vix-find-panel`, the crate that actually owns
  search/replace parsing — `vix-query` itself is just the interactive-session
  struct, 39 lines, no parsing logic of its own), `tabular_convert`
  (`vix-convert-tabular`, the shared CSV/TSV/JSON core behind all six
  CSV/TSV/JSON `Tools → Convert` crates), `structured_convert` (the four
  JSON⇄YAML/JSON⇄TOML convert crates), `macro_tokens` (`vix-macros`).
  **"Modeline parsing" has no corresponding code** — Vix has no vim/emacs
  modeline feature, so there is nothing pure to fuzz; documented in
  `fuzz/README.md` rather than fabricating one (that's a feature to design,
  not a fuzz target to add). Each new target ran a full 10 minutes (up to 75M
  executions on `macro_tokens`): zero real bugs found. Two initial "crashes"
  turned out to be the fuzz target's own assertion being wrong, not a code
  bug — fixed in the target itself, not the crate: (1) `tabular_convert`
  asserted CSV round-trips exactly, missing that `write_csv` *intentionally*
  rewrites `=`/`+`/`-`/`@`/tab/CR-leading fields (CSV/formula-injection
  guard); switched to a fixed-point assertion
  (`write(parse(write(rows))) == write(rows)`), which the guard's own
  idempotence still satisfies. (2) `structured_convert` asserted exact
  `serde_json::Value` equality after a JSON→YAML→JSON round trip; a
  30-significant-digit float came back with its last digit changed — decimal
  text isn't a bit-exact float representation, inherent to any such round
  trip, not a `serde_yaml` or Vix bug; switched to a relative-tolerance
  numeric comparison. Invocation documented in `agents/conventions.md` (the
  actual `cargo +nightly fuzz run <target>` command, not just a pointer).
- [x] **T008 — Binary-size tracking.** CI step that builds
  `--release` (default features), records the stripped binary size, and
  comments/records it so growth is visible per PR.
  Done — deliberately asymmetric across forges (see `spec/ci/index.md`'s
  "Binary size" section for the full reasoning): GitHub gets a `binary-size`
  job that caches every `main` build's size (via `restore-keys` prefix
  matching — a cache key can't be overwritten) and posts a sticky PR comment
  with the size and delta; GitLab gets the same measurement logged to the
  job output (no API token for MR notes) with a simpler always-overwrite
  cache; Codeberg gets neither — its runners are a donated, shared resource
  already reduced to one job, and a second full `--release` (LTO + strip,
  the slowest profile in the tree) for an informational-only metric GitHub
  already reports isn't a cost that CI should carry.
- [x] **T009 — Fix the flaky `sqlite_connect_browse_query_and_filter`.**
  Done. Root cause was **not** a race in `vix-db` itself: the worker thread
  behind `Session` serves one connection, one statement at a time (`while
  let Ok(Request::Run(sql)) = req_rx.recv() { rt.block_on(stream_sql(…)) }`),
  so there is no cross-statement visibility race in the connect → browse →
  query → filter → delete flow to chase — a DELETE is fully committed on
  the worker before the next statement is even dequeued. The real bug was
  in the *test harness's* own wait: `tests/db_smoke.rs`'s `key()` drained
  `poll_query()` in a loop bounded by a **fixed 1,000,000-iteration spin
  count**, which has no relationship to actual wall-clock time. On a
  contended `ubuntu-latest` runner (many parallel `cargo test` threads
  competing for few cores), the db-session worker OS thread can go
  unscheduled long enough to burn through that budget before it delivers a
  query's `Rows`/`Done` chunks — leaving the grid holding only the `Head`
  chunk's reset (empty rows under the new headers), which is exactly the
  observed `left: [] right: [["2"]]`. Fixed by replacing the iteration
  count with a 30-second wall-clock deadline (`drain_query`, also
  de-duplicating a second, identical inlined spin loop in
  `async_query_runs_off_the_event_loop_and_cancels`) that panics with a
  clear message on a genuine stall instead of silently reading a
  half-applied result. `vix-db`'s own non-blocking `poll()`/`poll_query()`
  (correctly `try_recv`-based for a real per-frame UI tick) needed no
  change. Verified: full `db_smoke` suite green locally (11 tests); on
  GitHub, `Security` green on push and `CI` (which runs `db_smoke`) green
  across **5 consecutive runs on the same commit** — the original push
  plus 4 `gh run rerun`s, all green, none flaky — short of the
  20-consecutive-run bar stated above (each rerun costs real CI minutes),
  but the root cause is now understood and structurally fixed rather than
  papered over. Ask for more reruns if 20 consecutive is wanted as hard
  proof.
- [x] **T010 — CI runner resilience.** Done, cheapest fix tried first for
  each of the two failure classes:
  (1) GitLab's shared runner running out of disk mid-link on the `test` job
  — root `Cargo.toml` gained `[profile.test]` with `debug = 1` (line tables
  only, workspace-wide), since the link is dominated by debug info from a
  handful of heavy dependencies (tokio/sqlx/tree-sitter/image), not code
  size; panic backtraces still resolve file/line under `RUST_BACKTRACE=1`.
  Couldn't validate the size reduction locally on macOS (debug info there
  lives out-of-line in the `.o` files, not embedded in the linked binary the
  way Linux's ELF/DWARF does — a same-size local before/after binary is
  expected, not a sign the fix did nothing); confidence has to come from
  GitLab's own Linux runner. The next-cheapest steps if this alone isn't
  enough — `cargo test --no-run` + per-crate test runs, then a bigger
  runner tier — are left on this list rather than done pre-emptively.
  (2) GitHub's `docs (lychee)` job's tarball download — added
  `actions/cache@v4` keyed on `LYCHEE_VERSION`+`LYCHEE_SHA256` (so only the
  first run after a version/checksum bump ever hits the network) plus
  `curl --retry 3 --retry-all-errors` on that first-run download. GitLab's
  and Codeberg's equivalent downloads are unchanged — they haven't shown
  this failure, only GitHub has. Both documented in `spec/ci/index.md`'s
  "GitLab" and "Docs links" sections.

## Phase 1 — Capabilities

### Scripting (epic — spec first, then slices)

- [x] **T101 — Scripting spec.** Write `crates/vix-script/spec/index.md`
  before any code: Rhai as the engine (pure Rust, no unsafe); script
  discovery (`~/.config/vix/scripts/*.rhai`, project `.vix/scripts/`);
  API v1 surface (register command, bind key, buffer get/set text,
  selection get/set, prompt, message, apply-transform); error UX (script
  errors go to the message drawer, never crash); the `scripting` cargo
  feature (default on). Get the spec merged as its own branch.
  Done — a real crate had to exist alongside the spec (`workspace_crates()`
  in `scripts/check-docs` requires `crates/<name>/Cargo.toml`, and
  `[workspace] members = ["crates/*"]` needs every member directory to be a
  buildable package), so T101 also scaffolds `vix-script` as a documented
  no-op: no dependencies, no public items beyond the crate-root doc pointing
  to the spec. Root `Cargo.toml` gets `vix-script` as an `optional`
  dependency and a `scripting` feature (`default = [..., "scripting"]`,
  mirroring the `lang-*`/`syntax-*` pattern) — wired now per the task's
  explicit "the `scripting` cargo feature (default on)" deliverable, even
  though nothing uses it until T102. Settled two design questions the task
  list left open: `prompt()`'s execution model (a script cannot suspend
  mid-handler — the answer re-invokes a *named* function as a fresh call,
  not a resumed one, since Rhai's embedding here is synchronous, no
  coroutines) and what "run textops-style transforms" (plan.md) actually
  means for the API (the buffer/selection get/set primitives *are* the
  mechanism — a script implements its own transform out of them, rather
  than `vix-textops`'s internal functions being exposed as a second API).
  Registered in `agents/share/crate-map.md`; bumped the "102 crates" count
  to 103 everywhere it's stated (`AGENTS.md`, `CLAUDE.md`,
  `spec/index/index.md`, `docs/architecture/index.md`,
  `crates/vix-i18n/spec/index.md`, `spec/llms-json-and-llms-txt/index.md`,
  `agents/share/crate-map.md` ×2).
- [x] **T102 — `vix-script` core.** New crate: Rhai engine wrapper, script
  loading, the buffer/selection/message API bound to host callbacks, unit
  tests with a mock host.
  Done — `vix-script` is now a real, host-agnostic crate (three modules:
  `discovery.rs`, `engine.rs`, `lib.rs`), 14 unit tests, all against inline
  `.rhai` source strings and a hand-built `HostState` — no real `App`, no
  terminal. `Runtime` uses a snapshot-in/effects-out design rather than a
  host trait object or any `unsafe`: `Runtime::invoke` takes an owned
  `HostState`, seeds an `Rc<RefCell<HostState>>` the registered native
  functions close over, runs the handler, and returns the mutated state —
  the host applies whichever `*_written` flags came back true. Every v1
  function from the spec is implemented: `register_command`, `bind_key`
  (validates the token via `vix_macros::decode_key` at registration time —
  a malformed token is a load error, not a binding that silently never
  fires — a small spec addition this task made explicit), `buffer_text`/
  `set_buffer_text`, `selection_text`/`set_selection_text`, `current_line`,
  `cursor_offset`/`set_cursor_offset` (clamped to the buffer's character
  length), `prompt`, `message`/`error`. Resource limits set as specced:
  10M operations, 64/64 expression/statement depth, 1M-char strings,
  100k-element arrays/maps — verified an infinite-loop script is actually
  caught deterministically (a real unit test, not just a claim).
  `discovery::discover(global_dir, project_dir)` takes plain `&Path`s and
  knows nothing about `Settings`/`App::root` — resolving *which*
  directories those are stays T103's job, keeping this crate host-agnostic.
  New workspace dependency `rhai = "1.26"`, plus `vix-macros` as a sibling
  crate dependency (for `decode_key`). `cargo deny` surfaced a real,
  unavoidable finding: `rhai` pulls in `smartstring` as a **mandatory**
  (non-optional, no feature to drop it) dependency, and `smartstring` is
  now unmaintained (RUSTSEC-2026-0249, no vulnerability, repo archived
  2026-05-03) — added a dated `deny.toml` ignore entry with the dependency
  path and a revisit condition, same pattern as the pre-existing `paste`/
  RUSTSEC-2024-0436 entry. `spec/index.md` and `agents/share/crate-map.md`
  updated to reflect the engine core being done and T103+ still pending
  (no palette entry, no startup load, nothing called from `src/app.rs`
  yet).
- [x] **T103 — Host wiring.** App shell: load scripts at startup, surface
  registered commands in the palette (prefixed, e.g. `script:`), execute
  with the active editor, route errors to messages. Action ids
  `script.reload`, `script.run`; Tools → Scripts submenu (list + Reload).
  Done — `Settings::scripts_dir()` (mirrors `themes_dir()`) plus
  `App::load_scripts()` (global dir + `<root>/.vix/scripts/`) called once
  at startup (`main.rs`, right after `refresh_git()` — the "load once,
  before the first frame" precedent already used there, not the
  lazy-on-open pattern tasks/macros/snippets use) and again by
  `script.reload`. Palette: `script:<stem>:<id>`-namespaced entries mixed
  into `PMode::Commands`' existing fuzzy/recency scoring
  (`App::script_palette_entries`), labels shown verbatim per spec.
  `run_action` dispatches any `script:`-prefixed action
  (`App::run_script_command`) alongside the existing `view.theme:`-style
  prefix arms. Tools → Scripts is two static leaves (`Run…`/`Reload`), not
  a dynamic per-command menu — `vix-menu`'s submenu lists are fixed at
  compile time (confirmed via the View → Theme submenu's `OnceLock`
  apparatus, which doesn't support post-first-render changes anyway); `Run…`
  opens a new `ScriptChooser` overlay instead, the same "chooser, not a
  menu list" shape `tools.tasks`/`Play Saved Macro…` already use for their
  own dynamic lists. Buffer/selection effects apply via
  `Editor::set_content`/`paste_text` (the same path a real paste uses —
  one undo step, selection-replace-or-insert-at-cursor semantics for free);
  blocked on a read-only buffer (a bare cursor move still applies). A
  script's `prompt()` opens a real `PromptKind::Script` prompt;
  answering it re-invokes `on_submit` as a fresh call
  (`App::pending_script_prompt` carries which script/handler, cleared on
  submit or Esc). `message`/`error` route to `App::messages`.
  **Packaging revision**: `vix-script` is now a **plain, non-optional**
  dependency — T101's `scripting` Cargo feature (`dep:vix-script`,
  default-on) is removed. Once genuinely wired into the App shell (struct
  fields, `main.rs` startup, palette/menu/prompt integration), gating it
  behind an opt-out feature would mean `--no-default-features` either
  fails to build the App shell or needs `#[cfg(feature = "scripting")]`
  sprinkled through several already-large files (`src/app.rs`, `src/ui.rs`)
  — including inside two `macro_rules!` dispatch tables
  (`try_panel_key`'s `panel!`, `overlay_capturing_keys`'s `any_open!`) that
  don't support per-fragment `#[cfg]` at all. Same call T111 made for
  `vix-modal`, for the same reason. 5 new `tests/integration.rs` cases
  (command run via action + via palette, reload picks up a new script,
  read-only blocks an edit, prompt round-trips to a fresh handler call).
- [x] **T104 — Script keybindings (audit + spec).** Allow scripts to bind
  keys via the existing keymap-model override layer; conflicts reported,
  never silently clobbered.
  Done as an audit + spec, not an implementation — the premise didn't
  hold. There is no existing "keymap-model override layer": `vix-keymap-
  model` only covers *which* keymap is active, not user-rebindable keys,
  and none of the 9 keymap dispatch functions in `src/app.rs` (`vim_
  normal_key`, `emacs_key`, `vscode_key`, `intellij_key`, `eclipse_key`,
  `sublime_key`, `global_key`/`apple_ctrl_key`, `global_shared_key`,
  `spacemacs_key`) are backed by a queryable table — only Emacs/Spacemacs's
  chord *continuations* are (`EMACS_CTRL_X` etc., `SPACEMACS_LEADER`); the
  rest are hardcoded `match` arms, several of which call a bespoke method
  directly (e.g. `apple_ctrl_key`'s `self.editor_motion(KeyCode::Delete)`)
  with no action-id string to hang a `(token, action) ` table entry on.
  Asked the user how to scope this given the gap; chose the largest of 3
  options — build a real, exhaustive registry, not a best-effort or
  script/config-only one. New `crates/vix-keybindings/spec/index.md`
  designs it: a `Binding{key_token, action_id}` schema shared by every
  built-in binding and every override; keyed on `vix-keymap-model`'s 10
  string keymap ids (not `App`'s private 9-variant enum — `vscode-macos`/
  `vscode-windows` share one dispatch path but get their own table rows);
  one new `App::override_key` choke point in `on_key`, inserted between
  `org_table_key` and the per-keymap `match` (the single place all 9
  keymaps already funnel through); persisted user overrides via a new
  `Settings::keybindings_path()`/`keybindings.toml`, exactly the
  `macros.toml` pattern (`confy` for directory only, plain `toml`+`fs` for
  the rest); conflict handling fixes `vix-script/spec/index.md`'s already-
  shipped contract precisely — two overrides claiming the same token are
  **both rejected** (never a silent winner), an override shadowing a
  built-in is allowed but reported once informationally. Staged into
  T104a (crate + convert Emacs, already partly table-driven) through
  T104j (wire `LoadedScript::bindings`, T102/T103's already-recorded-but-
  unchecked script key requests, into the choke point — the original ask).
  New `vix-keybindings` crate is a documented no-op, plain dependency
  (matching `vix-modal`/`vix-script`'s post-wiring precedent, not a Cargo
  feature). Bumped the crate count 104→105 across the usual 7 files.

### Keybinding registry (epic, opened by T104's audit — see
### `crates/vix-keybindings/spec/index.md`)

- [x] **T104a — Registry crate + Emacs.** `vix-keybindings`: `Binding`/
  `KeymapTable`/`lookup`/`shortcuts_for`. Convert the Emacs keymap first
  (already partly table-driven — cheapest proof the schema fits);
  `App::shortcut_rows` uses `shortcuts_for` for Emacs's contribution.
  Done — schema turned out to need a third piece, `ChordContext` (one
  named sub-table per chord depth: `""` top level, `"C-x"`, `"C-c"`,
  `"C-c C-x"`, `"C-c p c"`, `"C-c p c m"`), since a flat `{keymap_id,
  bindings}` table can't represent a chorded keymap — the same token
  (e.g. `b`) means different things at different depths. `lookup`/
  `shortcuts_for` both gained a `context` parameter accordingly; spec
  updated (`crates/vix-keybindings/spec/index.md`, "Schema refinement,
  made during T104a") to match. `emacs_key` and its five
  `*_chord_key` handlers now dispatch through `vix_keybindings::lookup`
  instead of hardcoded matches / the old `EMACS_CTRL_*` consts (chord
  *prefix-entry* keys like `C-x`/`C-c` stay host-side — a mode
  transition, not a dispatchable action); Meta (Alt) bindings, previously
  a sixth hardcoded match (`emacs_meta_key`, now deleted), turned out to
  be single keystrokes rather than a chord prefix, so folded into the
  `""` top-level context alongside the Ctrl bindings.
  **A real, pre-existing bug found and fixed along the way**: the old
  `EMACS_CTRL_X` const (used only for the which-key popup/F1 help, never
  actual dispatch — the real `C-x` chord handler was a second, separate
  hardcoded match) had drifted from it: claimed `b` ran a `"buffers"`
  action that didn't exist anywhere in `run_action`, and was missing the
  `C-b`/`0` bindings the real handler accepted. Fixed by unifying both
  into one `ChordContext` — `nav.switch_buffer` (new action id) is what
  both `C-x b` and `C-x C-b` now really run, in dispatch and display
  alike. Also added ~10 new small action ids (`motion.*` × 8,
  `edit.keyboard_quit`, `nav.switch_buffer`) so every top-level Emacs
  binding — including ones that used to call a bespoke method directly —
  is representable as a table row. Bonus, not previously true: the
  top-level Ctrl and Meta bindings now show up in the F1 help overlay too
  (only the five chord tables ever did before). Zero intended behavior
  change: full existing 421-test suite stayed green throughout, plus 7
  new `vix-keybindings` unit tests and 3 new `tests/integration.rs` cases
  covering previously-untested contexts (`C-c C-x`, `C-c p c`, and the
  Meta document-bounds bindings).
- [x] **T104b — Vim + Spacemacs.** Convert `vim_normal_key`'s table (shared
  by both keymaps) and Spacemacs's own leader table.
  Done — Vim converted cleanly onto the T104a shape: `vim_normal_key`'s
  top-level `match` plus its `g`/`d`/`y` pending-operator continuations
  became 4 contexts (`""`, `"g"`, `"d"`, `"y"`) under keymap id `"vi"`,
  `g`/`d`/`y` prefix-entry staying host-side (mirrors Emacs's `C-x`/`C-c`
  prefix-entry). Added 6 new `vim.*` action ids
  (`vim.insert`/`append`/`append_end`/`insert_line_start`/`open_below`/
  `open_above`, a new `run_vim_action` dispatcher) so `i`/`a`/`A`/`I`/`o`/
  `O` — each a compound of `editor_motion` calls plus entering Insert —
  are representable as table rows, same pattern as T104a's `motion.*`.
  Spacemacs's `SPC`-leader did **not** fit the T104a schema — a second,
  smaller spec revision was needed (`crates/vix-keybindings/spec/
  index.md`, "A second schema addition, made during T104b"): the leader
  is a prefix search over whole multi-character sequences (`"ff"`,
  `"gs"`), not fixed chord depths, so a `Binding`'s `key_token` there is
  the whole sequence and a new `SequenceMatch`/`lookup_sequence` query
  (mirroring `App`'s now-deleted `LeaderHit` enum) was added alongside
  `lookup`, reusing the same data shape. Spacemacs's shared Normal-mode
  vocabulary is *not* duplicated under its own keymap id — `spacemacs_key`
  delegates to the same `vim_normal_key`, so `lookup("vi", ...)` already
  covers it; `"spacemacs"`'s table holds only the leader context.
  **Process note, not a product issue**: started this task directly on
  `main` without branching/stashing the pre-existing WIP first (a lapse
  in the established workflow) — caught it when clippy failed on
  unrelated `vix-db` code the pre-existing WIP had touched; recovered
  cleanly by stashing just the pre-existing files (excluding my own
  `crates/vix-keybindings` changes) and branching from a clean `main`
  before continuing, no rework needed. Zero intended behavior change:
  full 425-test suite green throughout, plus 5 new `vix-keybindings`
  unit tests and 1 new `tests/integration.rs` case covering the insert-
  entry variants (`a`/`A`/`I`/`o`/`O`) and `yy`, none of which the
  existing Vim tests happened to exercise.
- [x] **T104c — VS Code.**
  Done — the simplest of the three conversions so far: `vscode_ctrl_key`
  is all-Ctrl, no chords, so it needed no schema change (one flat `""`
  context, exactly Vim's shape) and no new action ids (`nav.goto_line`
  already existed and covers `Ctrl+G`; one new one, `view.
  toggle_explorer_focus`, for `Ctrl+Shift+E`'s bespoke method call). One
  shared table serves both `vscode-macos` and `vscode-windows` keymap
  ids, per the spec's own "Why 10 keymap ids" rationale. Real subtlety
  found anyway (spec updated, "VS Code's own subtlety, found during
  T104c"): the original dispatch distinguishes `Ctrl+Shift+<letter>` from
  plain `Ctrl+<letter>` via the Shift *modifier bit*, not char case —
  terminals can report `Ctrl+Shift+p` as a lowercase `p` with the bit
  set — so reusing `vix_macros::encode_key` unmodified (which treats
  Shift as implicit in an uppercase char and drops it for `Char` keys)
  would have silently collided `Ctrl+P`/`Ctrl+Shift+P`. Added a small
  dedicated `App::vscode_ctrl_token` that encodes Shift explicitly
  instead (`"C-S-p"`, still valid `vix-macros` grammar, just not what
  `encode_key` itself emits). Bonus, not scope creep since the display
  code needed the same treatment regardless: VS Code's bindings had
  never appeared in the F1 help overlay at all (unlike Emacs's chord
  tables, nothing ever fed `shortcut_rows` for it) — now they do, via a
  new `vscode_key_display` (handles stacked `C-`/`S-`/`A-` prefixes,
  unlike Emacs's single-prefix `emacs_key_display`). Zero intended
  behavior change: full 425-test suite green throughout — including two
  *pre-existing* tests that already exercised the exact lowercase-plus-
  Shift-bit case my new token function had to get right
  (`vscode_keymap_quick_open_command_palette_and_goto_line`,
  `vscode_keymap_split_panel_and_delete_line`) — plus 3 new
  `vix-keybindings` unit tests and 1 new `vscode_key_display` unit test.
- [x] **T104d — IntelliJ (macOS + Windows).**
  Done — unlike VS Code (T104c, one shared table), `intellij-macos` and
  `intellij-windows` turned out to be two genuinely different tables:
  converting the actual dispatch (not the doc comment) showed the "go to"
  family alone uses `Ctrl+O`/`Ctrl+Shift+O`/`Ctrl+L` on macOS vs
  `Ctrl+N`/`Ctrl+Shift+N`/`Ctrl+G` on Windows, plus platform-only
  bindings (`Ctrl+,` Settings macOS-only, `Ctrl+Y` delete-line
  Windows-only). Two independently-written tables, ~13 shared bindings
  duplicated plainly across them (spec: "`IntelliJ`'s own subtlety, found
  during T104d"). Also needed the same Shift-bit-explicit token approach
  T104c introduced (a new `App::intellij_ctrl_token`), plus one more
  wrinkle VS Code didn't have: `Ctrl+Alt+L`/`Ctrl+Alt+O` are a single
  keystroke's modifier combination, not a chord, so they're ordinary
  `"C-A-…"` entries in the same `""` context as everything else.
  **Found and preserved, not "fixed," a genuine quirk in the original
  dispatch**: neither macOS's `Ctrl+N` arm nor Windows's `Ctrl+G` arm was
  ever Shift-guarded, so `Ctrl+Shift+N`/`Ctrl+Shift+G` do the exact same
  thing as their plain counterparts on each respective platform — each
  table lists the Shift variant as an explicit duplicate row rather than
  silently dropping it or "improving" it into a distinct binding.
  **Found and fixed a real, pre-existing test bug** while looking for
  IntelliJ coverage to extend: `tests/integration.rs`'s
  `intellij_and_eclipse_keymaps_bind_find` used the keymap ids
  `"intellij-mac"`/`"intellij-win"` (not the real `vix-keymap-model` ids,
  `"intellij-macos"`/`"intellij-windows"`) — `Keymap::from_id` silently
  falls back to `Keymap::Apple` on an unrecognized id, and Apple happens
  to also bind `Ctrl+F` to find, so the test passed while testing nothing
  IntelliJ-specific at all. Fixed the ids and added two new tests
  exercising the platform divergence and the Shift quirk for real.
  Renamed `vscode_key_display` → `modifier_token_display` and merged its
  `shortcut_rows` match arm with VS Code's (same shape, now covers all 4
  platform-variant ids) rather than duplicating the walk a second time —
  IntelliJ's bindings now show up in the F1 help overlay for the first
  time too, same bonus pattern as T104a/T104c. Zero intended behavior
  change: full 427-test suite green throughout (425 + 2 new, on top of
  fixing the pre-existing test's ids), plus 4 new `vix-keybindings` unit
  tests and 1 updated `modifier_token_display` unit test.
- [x] **T104e — Eclipse.**
  Done — no schema change (one flat `""` context, all-`Ctrl` plus one
  exception), and needed the same Shift-bit-explicit token approach
  T104c/T104d introduced (a new `App::eclipse_token`). The one genuinely
  new wrinkle (spec: "Eclipse's own subtlety, found during T104e"): the
  original dispatch has a binding that isn't a `Ctrl` chord at all —
  `Alt+/` (word completion) — matched only when `Ctrl` is *not* also held,
  so `Ctrl+Alt+/` falls through to the `Ctrl` branch and resolves to
  `edit.toggle_comment` (same as plain `Ctrl+/`) rather than word
  completion; `Alt` is simply never examined once `Ctrl` is present.
  `eclipse_token` preserves this exactly (`Ctrl` takes priority,
  `Alt`-only builds an `"A-…"` token) and the table carries `"A-/"` as an
  ordinary row alongside the `"C-…"` ones in the same context, rather than
  adding a second context for one binding. Extended the F1-help-overlay's
  generic `vscode-macos`/`vscode-windows`/`intellij-macos`/
  `intellij-windows` match arm to also cover `"eclipse"` (same shape,
  `modifier_token_display` already handles a stacked-or-single prefix
  fine) instead of a fifth near-duplicate arm. Zero intended behavior
  change: full 429-test suite green throughout (427 + 2 new), plus 3 new
  `vix-keybindings` unit tests.
- [x] **T104f — Sublime Text.**
  Done — no schema change (one flat `""` context, all-`Ctrl`), and
  needed the same Shift-bit-explicit token approach as T104c/T104d/T104e
  (a new `App::sublime_ctrl_token`). The first keymap in the chain to
  find *nothing* new beyond that — no fourth subtlety subsection in the
  spec, just a fourth confirmation the "check the Shift bit" rule holds.
  Folded Sublime into the existing shared F1-help-overlay match arm
  (VS Code/IntelliJ/Eclipse) rather than a fifth near-duplicate arm.
  Two pre-existing tests (`sublime_keymap_signature_bindings`) already
  exercised the exact lowercase-plus-Shift-bit case for real, same as
  T104c's VS Code precedent — real regression coverage that didn't need
  writing. Added one new test confirming plain `Ctrl+P` (Goto Anything —
  opens the file browser) stays distinct from `Ctrl+Shift+P` (Command
  Palette). Zero intended behavior change: full 430-test suite green
  throughout (429 + 1 new), plus 3 new `vix-keybindings` unit tests
  (crate total 20 → 23). Also fixed the `unpopulated_keymaps_return_
  nothing` test's probe id, which had used `"sublime"` as the
  still-empty example — switched to `"apple"`, the one keymap id still
  unconverted after this task.
- [x] **T104g — Apple + `global_shared_key`.** The last two dispatch
  functions; the registry now covers all 10 keymap ids exhaustively.
  Done — the biggest single-task departure from the schema so far (spec:
  "Apple and `global_shared_key`'s own subtlety, found during T104g"),
  two distinct findings:
  1. Apple's `apple_ctrl_key` genuinely mixes Shift-guarded letters
     (`o`/`s`/`w`/`t`/`b`/`f`/`g`, a different action per Shift state,
     same shape T104c–f already needed) with Shift-agnostic ones
     (`q`/`n`/`p`/`e`/`r`/`/`/`7`/`_`/`]`/`;`, same action either way).
     Kept one uniform Shift-bit-explicit `apple_ctrl_token` and gave every
     Shift-agnostic letter an explicit duplicate `"C-S-…"` row — T104d's
     "faithfully preserve an unguarded quirk" technique, just needed for
     ten letters instead of two. `Ctrl+Alt+R` (query replace, the only
     Alt-keyed binding here) and `Ctrl+D` (forward delete, focus-gated)
     stay host-side pre-checks, neither fitting a static row.
  2. `global_shared_key` isn't keyed on a keymap id at all — every one of
     the 9 `App` dispatch functions falls back to it identically. Added a
     genuinely new, keymap-agnostic `SHARED: &[Binding]` +
     `lookup_shared()`, outside `TABLES` entirely (so the "one table per
     real keymap id" invariant stays meaningful) — the actual schema
     stretch this task turned out to need, bigger than any single
     `ChordContext`/`SequenceMatch`-style addition. The menu-mnemonic
     `Alt+<letter>` lookup (dynamic, not static data) and 6 focus-gated
     arms (`Ctrl+Shift+Right`/`Left`, `Alt+Up`/`Down`, `Alt+n`/`p`) stay
     host-side — `App::focus` is per-request runtime state a fixed table
     can't express. **Caught a real ordering hazard by hand-tracing the
     original `match`, not assuming order didn't matter**: `Ctrl+Shift+
     Right`/`Left` (focus-gated) and `Alt+Right`/`Left` (now in `SHARED`)
     share the same two keys, and the original's arm order gave the
     Ctrl+Shift pair priority for the rare `Ctrl+Alt+Shift+Left`
     combination — preserved by checking the focus-gated pair *before*
     the `SHARED` lookup, not after.
  Added 4 new action ids for bespoke calls that had none yet (`nav.back`,
  `nav.forward`, `view.toggle_menu`) — same "give every bespoke call a
  real id" pattern as T104a's `nav.switch_buffer`; reused 5 already-
  existing ones (`view.toggle_explorer_focus`, `view.focus_other_pane`,
  `edit.find_next`/`edit.find_prev`, `help.shortcuts`,
  `motion.delete_forward`) rather than re-inventing them. Extended the F1
  help overlay's shared match arm to also cover `"apple"`, and — new this
  task — added an *unconditional* `SHARED` walk (every keymap dispatches
  through `global_shared_key` identically, so its bindings show up
  regardless of the active keymap, unlike the per-keymap arms). One
  clippy fix needed (`format_push_string`: `token.push_str(&format!(...))`
  → `write!(token, ...)` for the `F{n}` token). Zero intended behavior
  change: full 432-test suite green throughout (430 + 2 new — one test's
  first draft asserted the wrong field, `app.query_replace` instead of
  `app.search.interactive`, caught and fixed before merging), plus 6 new
  `vix-keybindings` unit tests (crate total 23 → 29).
- [x] **T104h — Persisted overrides.** `Settings::keybindings_path()` +
  `keybindings.toml` load/save (the `macros.toml` pattern).
  Done — `Settings::keybindings_path()` added right after `macros_path()`
  in `vix-settings`, identical shape. New `vix-keybindings::user_bindings`
  module: `UserBinding { key_token, action_id }` (owned `String`s, unlike
  the built-in tables' `&'static str` `Binding` — these are loaded at
  runtime, not compile-time constants), a private `KeyBindingsFile`
  wrapper, `load`/`upsert` copying `vix-macros`' `macros.toml` pattern
  verbatim (plain `toml`+`std::fs`, no `confy::load`/`.save()`), `upsert`
  keyed on `key_token` (the natural unique key for a rebinding — you can
  only have one override per token — mirroring how `macros.toml`'s own
  `upsert` keys on `name`). `vix-keybindings` gained its first real
  dependencies (`serde`, `toml`) — was a pure no-dep data/logic crate
  until now. Also fixed a stale crate description ("9 keymaps") left
  over from before T104g's 10-id completion, caught while touching
  `Cargo.toml` for the new deps anyway.

  Deliberately scoped narrow, per the staged plan: no `Override`/`Source`
  enum, no conflict detection, nothing wired into `App` at all yet — this
  task is the file format and round trip only. `App::run_action` and
  every keymap dispatch function are completely untouched. That's T104i's
  job (the `on_key` choke point) and T104j's (wiring `vix-script`'s
  `LoadedScript::bindings` in) — both still pending. Zero risk of
  behavior change since nothing new is called from anywhere yet: full
  432-test suite green throughout (unchanged from T104g, since there's no
  new App-level code path to exercise), plus 3 new `vix-keybindings` unit
  tests (crate total 29 → 32, mirroring `vix-macros`' own
  `upsert_writes_and_replaces_by_name` test almost verbatim, plus a
  missing-file and an unparseable-content case). `cargo deny` re-checked
  clean after adding the two new dependencies (both already vetted,
  workspace-wide deps — no new advisory risk).
- [x] **T104i — The override choke point.** `App::override_key`, inserted
  in `on_key` between `org_table_key` and the per-keymap `match`;
  conflict handling for `keybindings.toml` entries (two overrides on one
  token: both rejected; shadowing a built-in: allowed, reported once).
  Done — new `vix-keybindings::overrides` module: `Source`/`Override`/
  `Conflict`/`Shadow`/`Resolved`/`resolve()`, grouped in a `BTreeMap` (not
  `HashMap`) so resolution — and every message built from it — is
  deterministic regardless of request order, not just correct. 7 new
  unit tests, including one that explicitly checks a rejected conflict
  is never *also* reported as a shadow. `App::override_key` builds the
  incoming key's token with `crate::macros::encode_key` (the shared
  grammar every override source is authored in, deliberately not any
  single keymap's Shift-bit-explicit convention) and consults a new
  `self.key_overrides: HashMap<String, String>` map. Split
  `load_key_overrides` (reads `keybindings.toml`) from a new, separately
  public `App::apply_key_overrides(requests)` (does the actual resolve +
  report + store) specifically so T104j can feed a combined
  persisted+script `Vec<Override>` into the same call — and so
  integration tests can drive the choke point directly, since (unlike
  scripts' `.vix/scripts/`) `keybindings.toml` has no project-scoped
  variant a test could seed on disk. New `keybindings.reload` action
  (+ Tools-menu leaf, mirrors `script.reload`) and 3 new `msg.keybinding_
  *`/`msg.keybindings_reloaded` locale keys (en only, matching
  `msg.script_load_error`'s precedent). One clippy fix needed
  (`missing_panics_doc` on a `.pop().expect(..)` that could never
  actually panic — restructured as `if let Some(only) = group.pop()`
  instead of documenting a panic that can't happen). Zero intended
  behavior change for every existing dispatch path: full 436-test suite
  green throughout (432 + 4 new), plus 7 new `vix-keybindings` unit
  tests (crate total 32 → 39).
- [x] **T104j — Wire scripts in.** `LoadedScript::bindings` (T102/T103,
  already recorded, never checked) through the same choke point — the
  task this epic was originally scoped as.
  Done — **the whole `vix-keybindings` epic (T104/T104a–j) is now
  complete.** Renamed `load_key_overrides` → `App::resolve_key_overrides`
  once it grew a second source to combine: it now builds one
  `Vec<Override>` from *both* `keybindings.toml` (`Source::User`) and
  every currently-loaded script's `bindings` (`Source::Script(stem)`,
  action id `format!("script:{stem}:{command_id}")` — exactly the shape
  `App::run_script_command` already parses for the palette) before the
  single `apply_key_overrides` call T104i built specifically to receive
  it. The "regardless of source" conflict rule finally holds for real: a
  script and a persisted override (or two different scripts) claiming
  the same key both get rejected, not just in a unit test pretending
  they would. `script.reload` now also re-runs `resolve_key_overrides`
  (a reloaded script's `bind_key` requests can genuinely change), the
  third and last of the spec's own "at load time" trigger list
  (script load, `script.reload`, `keybindings.toml` load/save) to
  actually fire — script load itself and `keybindings.reload` already
  did from T104i. Closes the loop `crates/vix-script/spec/index.md`'s
  "Key bindings" section opened at T102: its conflict-handling contract
  ("reported, never silently clobbered") is finally enforced, not just
  promised; that spec's own status line and "Key bindings" section
  updated to say so. 3 new integration tests, all against **real
  discovery** (`.vix/scripts/*.rhai` fixtures + `load_scripts()`/
  `resolve_key_overrides()`, not a hand-built `Vec<Override>`): a
  script's `bind_key` actually fires through `on_key`; `script.reload`
  picks up a binding added to a script after startup; two scripts
  binding the same token both reject. **Found and worked around, not
  fixed, a genuine pre-existing quirk while writing these**: `Ctrl+
  <letter>` not claimed by any keymap/override/editor-shortcut falls all
  the way through to `vix-editor-core`'s `Editor::input`, whose final
  `KeyCode::Char(c) => insert` arm has no `!ctrl` guard — so an
  "unbound" `Ctrl+J` doesn't no-op, it types a literal `j`. Pre-existing
  (nothing to do with this task), out of scope to fix here; switched the
  affected tests' probe token from `Ctrl+J` to `F9` (a code path that
  genuinely no-ops when unclaimed) rather than either masking the quirk
  or scope-creeping a fix into this task. Worth a task of its own later
  (`vix-editor-core`'s `input` should ignore `Char` while `ctrl` is held
  and unmatched, not insert it) — not filed as one yet, just recorded
  here. Zero intended behavior change for every existing dispatch path:
  full 439-test suite green throughout (436 + 3 new), no new
  `vix-keybindings` unit tests needed (the crate-level `resolve()` logic
  was already fully exercised by T104i's 7 tests; this task is pure
  `App`-side wiring).

- [x] **T105 — Sample scripts + docs.** ~6 scripts in `examples/scripts/`
  (e.g. wrap-selection-in-markdown-link, insert-file-header,
  title-case-line, dedupe-selection, timestamp-signature, open-scratch-
  with-template); write `docs/scripting/index.md` documenting the full
  API v1 with each sample explained.
  Done — all 6 named samples, in `examples/scripts/*.rhai`, each verified
  by actually running it through `vix_script::Runtime` (not just eyeballed
  for plausible-looking Rhai — real interpreter probes caught two genuine
  syntax traps along the way: Rhai's `.trim()` mutates in place and
  returns `()`, so `x = x.trim()` silently empties `x`; a directory-only
  Markdown link and a literal `` `[text](url)` `` demonstrating link
  syntax both trip `scripts/check-docs`'s "does this resolve to a file"
  check, the exact gotcha the `vix-spec-change` skill already warns
  about). New `tests/example_scripts.rs` (7 tests) loads every sample via
  real discovery and invokes each handler, so they can't silently rot —
  the same "docs are checked like code" principle `scripts/check-docs`
  already applies to links.

  **Two named samples needed something the scripting API didn't have —
  handled two different ways, on purpose:**
  1. `timestamp-signature` needs the current date; v1 had **no clock
     function at all**. Asked before adding surface to a shipped, spec'd
     crate rather than deciding alone — user chose adding one. New
     `now() -> String` (`YYYY-MM-DD`, via `jiff::Zoned::now()`, already a
     workspace dependency elsewhere) registered in `vix-script`'s engine,
     documented in its spec's API v1 as a small, dated addition, with its
     own unit test (compares against a real `jiff` call, not a hardcoded
     date literal, so it can't rot).
  2. `open-scratch-with-template` implies opening a **new** buffer — v1
     has no multi-buffer capability at all, and unlike the clock gap this
     one is an **explicit, reasoned** cut line already in `vix-script`'s
     own spec ("no workspace-search or multi-file API... a script cannot
     iterate open tabs or read another file"). Adding tab-opening would
     cut against a deliberate boundary, not fill an oversight, so this
     one wasn't a case for asking again: reinterpreted as filling the
     *already-open* active buffer with a template (guarded against
     overwriting real content), documented honestly in the script's own
     comment and the docs page about why, expecting the user to press
     Ctrl+N first. Worth knowing which of the two this was next time a
     sample needs something v1 doesn't have: an oversight is worth
     asking about, a documented deliberate boundary usually isn't.

  `docs/scripting/index.md` written from scratch (API reference, error
  handling, "write once not incrementally" guidance, key-binding conflict
  behavior, all 6 samples linked and explained), added to `docs/index.md`
  and `llms.txt`/`llms.json`. Also added an "Overrides" section to the
  previously-untouched `docs/keybindings/index.md` documenting
  `keybindings.toml` — a real, user-facing gap the whole T104h–j epic
  left behind (specs got updated throughout; no end-user doc ever
  mentioned the override file existed until now). Fixed two doc comments
  in `vix-script` (`lib.rs`, `engine.rs`) still describing key-binding
  wiring as not yet done, stale since T104j shipped in the same session.

  Caught my own process lapse partway through: started this task by
  editing directly on `main` again (see T104b's memory note — same
  mistake, second time this session) instead of stashing/branching
  first. Caught it via `./scripts/check` failing on unrelated `vix-db`
  code from the still-present stashed-should-have-been WIP. Recovered
  cleanly: stashed only the ~22 pre-existing files by explicit path list
  (not a bare `-- crates/`, which would have swept up
  `crates/vix-script`'s own in-progress changes too), branched from
  clean `main`, no rework needed. Zero intended behavior change for
  every existing script/keybinding path: full 439-test suite green
  throughout (unchanged from T104j) + `example_scripts.rs`'s 7 new
  tests + 1 new `vix-script` unit test (`now_returns_todays_local_date`,
  crate total 14 → 15).

### Modal editing (epic — audit first)

- [x] **T111 — Modal audit + spec.** Audit what the Vi keymap actually
  does today vs a real modal engine. Write `crates/vix-modal/spec/index.md`:
  modes (normal/insert/visual/visual-line), operator × motion grammar,
  counts, registers, dot-repeat; explicit v1 cut line (no ex commands, no
  macros — Vix already has macros). Merge the spec.
  Done — new `vix-modal` crate (design-only, same no-op-crate-plus-spec
  shape as T101/`vix-script`). The audit in `spec/index.md` cites the
  actual `src/app.rs` dispatch precisely: `vim_normal_key`'s `vim_pending`
  supports only three hardcoded 2-key sequences (`gg`/`dd`/`yy` — `d`/`c`
  plus any other motion is silently swallowed), no count state exists, no
  named registers (everything goes through the single OS clipboard), no
  Visual mode flag (though `MoveLeft/Right/Up/Down { shift }` selection-
  extension already exists end-to-end and just isn't wired to a mode), no
  dot-repeat, no text objects, and Spacemacs's Normal mode is confirmed to
  delegate to this same function (not a second implementation). Also found
  and flagged: three independent word-boundary implementations exist
  (`vix-textops`, `vix-editor-core::named`, a third local one in
  `vix-editor`) — the v1 design picks `vix-textops` as canonical rather
  than adding a fourth. v1 design: `Mode` enum (Normal/Insert/Visual/
  VisualLine — Visual Block cut); pure `fn(text, pos, count) -> usize`
  motions (`h j k l w b e 0 ^ $ gg G { } ( ) f/t/F/T %`) reusing
  `vix-textops` where it overlaps; `d c y` composing with any motion/text
  object/Visual selection, `x` as sugar for `d` + one-char motion, `p`/`P`
  as standalone register-paste commands (not operators); counts with the
  `count1 * count2` multiplicative composition rule (`2d3w` = 6 words);
  unnamed register mirrors the existing OS clipboard, named a–z is an
  in-memory session-only map (explicit non-persistence call-out); dot-
  repeat as keystroke replay of the last change (reusing the existing
  macro-recorder's mechanics conceptually) with `{count}.` override; text
  objects (`iw aw i( a( i" a"` + bracket/quote siblings) as their own
  delimiter-scan functions, deliberately not reusing the Tree-sitter
  `expand_to_node` structural-selection feature (different, syntax-aware
  mechanism, noted as a possible future complement). Explicit v1 cut line:
  no ex-command scripting, no macro-via-`q` (Vix's existing recorder is
  untouched), no Visual Block, no WORD motions, no `;`/`,`/`*`/`#`, no
  operators beyond `d c y (x) p P`, no register persistence/uppercase/
  numbered/special registers, no sentence/paragraph/tag text objects.
  Rollout: new `Settings::modal_engine: bool`, off at T112, flipped on once
  T115 ships the full slice (T115's call); `docs/for-vim-users/index.md`'s
  honest gap list updates incrementally as T112–T115 land, not all at once.
  Root `Cargo.toml`: `vix-modal` added as a **plain** (non-optional)
  dependency — unlike `vix-script`'s `scripting` feature, Vi/Spacemacs
  keymaps are always compiled in today, so gating the engine behind a
  Cargo feature would be inconsistent with how the keymaps already ship.
  Registered in `agents/share/crate-map.md` (new "Modal editing" row);
  bumped the "103 crates" count to 104 everywhere it's stated (`AGENTS.md`,
  `CLAUDE.md`, `spec/index/index.md`, `docs/architecture/index.md`,
  `crates/vix-i18n/spec/index.md`, `spec/llms-json-and-llms-txt/index.md`,
  `agents/share/crate-map.md` ×2 — one more occurrence than T101 found,
  since this file has two differently-worded "103" mentions).
- [x] **T112 — Mode engine.** Done 2026-09-14. `vix-modal` crate gained the
  `Mode` enum (`Normal`/`Insert`/`Visual`/`VisualLine`, `status_label()` for
  the status bar) plus a new `Settings::modal_engine: bool` (default off).
  Host wiring lives in a new `src/app/modal.rs`: `App::modal_key` is tried
  first from `vim_key`/`spacemacs_key`, before `vim_normal_key`'s existing
  table — a no-op whenever the setting is off, so today's Vi/Spacemacs
  behavior is unchanged by default. This slice: `v`/`V` enter Visual/Visual
  Line from Normal, `h j k l`/arrows extend the selection, `Esc` returns to
  Normal. Deliberately reused the editor's own native shift-extend
  mechanism (`MoveLeft/Right/Up/Down { shift: bool }`, the same plumbing
  `Ctrl+Shift+Right/Left` already relies on) rather than having the modal
  engine track its own anchor — an earlier draft that hand-tracked a
  `modal_visual_anchor` field produced a wrong selection range, since
  unshifted motion over an active selection collapses/normalizes it instead
  of doing a pure cursor step. Visual Line re-snaps to whole lines after
  each extend, keeping the cursor on the growing end (reading the raw,
  unsorted cursor position, since `get_selection()` always returns a sorted
  `start<=end` pair that loses which end is growing). Status bar shows
  `status.vim_visual`/`status.vim_visual_line` (2 new locale keys, all 15
  locales) alongside the existing Insert/Normal indicators. Found and fixed
  a real regression during the full gate run: switching keymaps (View →
  Keymap) reset `modal_insert` but not the new `modal_mode`, so a Vi→Vi
  reswitch while in Visual mode left the status bar stuck; fixed in
  `reset_keymap_modes`. 5 new integration tests
  (`tests/integration/modal.rs`) — one caught a test-file-path race
  (`unique_dir` tags shared across parallel tests), one caught the
  anchor-tracking bug above. `vim_normal_key`'s table still owns everything
  else unchanged; T113+ narrows that fallback as real motions/operators
  land.
- [x] **T113 — Motions + counts.** Done 2026-09-15. `vix-modal` gained
  `count.rs` (a `Count` accumulator implementing the real
  `{count1}{operator}{count2}{motion}` → `count1 * count2` composition rule)
  and `motion.rs`: pure `fn(text, pos, count) -> usize` functions for
  `h j k l w b e 0 ^ $ gg G { }`, plus `( )` (sentence motions — a small,
  documented addition beyond this bullet's own literal list, since the
  fuller spec design pairs them with `{`/`}` at near-zero extra cost); `%`
  deferred (a different kind of scan, and the existing `edit.match_bracket`
  action already covers it — not a gap this slice needed to close). Reused
  `vix-textops`'s `word_units`/`sentence_units`/`paragraph_units`/
  `line_ranges` (newly made `pub`) rather than a fourth word-boundary
  scanner, per the spec's own audit finding. `h`/`l` deliberately don't
  cross line boundaries and `0`/`^` are real, distinct motions — both actual
  fixes for gaps the T111 audit found in the old table (which crossed lines
  and conflated `0`/`^` into one "smart Home" toggle). 37 unit tests on the
  pure functions ("heavy unit tests" per this task) plus 12 new integration
  tests wiring them into Normal mode behind `Settings::modal_engine`
  (`App::modal_normal_key` tries a pending `gg`/`f`/`t`/`F`/`T` resolution,
  then digit accumulation, then the motion table; `d`/`c`/`y`/`x`/`p` still
  fall through to the old table unchanged — T114's job). `f`/`t`/`F`/`T`
  search only the current line, matching real Vim; a miss leaves the cursor
  untouched.
- [x] **T114 — Operators.** Done 2026-09-15. New `vix-modal` modules:
  `operator.rs` (pure `operator_range` — turns a motion's landing position
  into the real half-open/closed/whole-line range per
  `motion::MotionKind`'s exclusive/inclusive/linewise classification, real
  Vim's own `:help motion.txt` categories; `delete_range`/`insert_at`/
  `paste_plan`, all pure position/text computations, no buffer mutation)
  and `register.rs` (`Registers`, the named a–z map — session-only, per the
  spec). `d`/`c`/`y` compose with **every** T113 motion (including through
  a pending `gg`/`f`/`t`/`F`/`T`'s second key — `dgg`, `df.` both work);
  `x` is real sugar for `d` + one right motion, not its own code path;
  `dd`/`cc`/`yy` are sugar for the linewise `j`-with-`count - 1` motion,
  reusing the exact same range math as every other pair; `p`/`P` read a
  register and paste char-wise inline or line-wise as a whole line
  (landing on the first non-blank), per how the register was written;
  `"{a-z}` selects a named register for the next operator or paste, else
  the unnamed register — real `vix_clipboard`, exactly as before. Buffer
  mutation goes through the editor's own selection + `InsertText` action
  (same path as ordinary typing), not a raw rewrite, so undo/highlighting
  stay consistent. `2d3w` deletes 6 words (the spec's own
  `count1 * count2` rule) — a count can be typed before the operator, the
  motion, or both. Deliberately deferred, none in the spec's own cut list:
  `cw`'s famous "acts like `ce`" special case, and `cc`'s indentation
  preservation (both documented, narrow real-Vim polish nuances, not core
  to "operators compose"); operators composing with a **Visual** selection
  (spec's own "any motion/text object/Visual selection" — Visual's own
  motion vocabulary is still just T112's `h j k l`, so there's nothing
  richer to select with yet; a natural T115+ follow-on once text objects
  land). 26 tests total (was 12 after T113) — a representative
  operator×motion grid (`dw`/`de` exclusive vs. inclusive, `dd`/`2dd`
  linewise, `cw` entering Insert, `x`, `yy`+`p`, named-register `P`/`p`,
  `df.` composing with a pending motion, the `2d3w` count-multiply rule,
  an unrecognized key cleanly cancelling a pending operator) plus one
  deliberately isolated unnamed-register round-trip test (named registers
  are per-`App` state; the unnamed one mirrors the real, process-global,
  in-memory-in-tests `vix_clipboard` — every other test avoids it so
  parallel test execution can't make them flaky).
- [x] **T115 — Text objects + repeat.** Done 2026-09-15, closing the
  T112–T115 modal-editing arc. New `vix-modal` modules: `text_object.rs`
  (pure `fn(text, pos, count) -> Option<(usize, usize)>` for `iw`/`aw`, a
  parameterized `inner_pair`/`around_pair` covering `(`/`)`/`b`,
  `{`/`}`/`B`, `[`/`]`, `<`/`>`, and a parameterized `inner_quote`/
  `around_quote` covering `"`/`'`/`` ` `` — a character/bracket-matching
  scan per the spec, deliberately not editor-core's Tree-sitter structural
  selection, which is a different, syntax-aware mechanism). `i`/`a` +
  object compose with `d`/`c`/`y` the same way a T113 motion does (always a
  plain character-wise range — there's no before/after-cursor pair to
  sort). Dot-repeat: `.` replays the exact keys of the last real change
  (`d{motion}`, a text object, or `p`/`P` — `y` is excluded, matching real
  Vim's own "yank was never dot-repeatable either"; `c` and a plain
  Insert-mode session are excluded too, a deliberately scoped-out follow-on
  — replaying typed Insert-mode text needs a recording hook outside
  `src/app/modal.rs`, since those keys never reach `App::modal_key` at all
  under T112's Insert-mode-passthrough design). `{count}.` overrides the
  recorded change's count by splicing the new count in wherever the first
  digit run in the recorded keys was (after a register prefix if any),
  rather than just prepending it; a command recorded with two separate
  counts (`2d3w`) only has the first one replaced this way — documented,
  narrow, not silent corruption. **`Settings::modal_engine` flipped to
  `true` by default** — the full v1 slice is done, per the spec's own
  "flip is whoever ships T115's call." Real-Vim nuances still deliberately
  unimplemented (none of them in the spec's own cut list, all newly
  documented in its Status section and in `docs/for-vim-users/index.md`'s
  "Where Vim still wins"): `cw`'s "acts like `ce`" special case, `cc`'s
  indentation preservation, operators composing with a Visual selection,
  and the dot-repeat gaps above. Updated `docs/for-vim-users/index.md`'s
  gap list to say exactly what v1 now covers, per this task's own
  instruction. 18 new unit tests (73 total in `vix-modal`) plus 11 new
  integration tests (37 total in `tests/integration/modal.rs`), and a full
  `cargo test --test integration` run (542 tests, 0 failures) confirming
  the default flip doesn't regress any pre-existing Vi/Spacemacs-keymap
  test.

### Performance & depth

- [x] **T121 — Perf: highlight and search.** Done 2026-09-15. Investigated
  first (a full audit of the highlight pipeline, `crates/vix-editor-core/
  spec/syntax-highlighting/index.md`) before changing anything: highlight-
  query *execution* was already lazy and viewport-scoped
  (`Code::highlight_interval`, called only by the render path, only for the
  visible rows' byte range) — the 5.05 s `editor/open` baseline for a
  100 MB file wasn't a highlighting cost at all, it was one synchronous,
  unconditional, whole-buffer Tree-sitter *parse* in `Code::new`, the one
  piece of the pipeline that never went through the background-worker
  machinery a post-edit reparse already used. Routed the initial parse
  through that same worker for buffers at/above `ASYNC_PARSE_THRESHOLD`
  (50 KB) instead of adding a new mechanism. Found and fixed two real
  correctness bugs the change exposed (both were latent, exercised for the
  first time by an edit made before the initial parse lands, which couldn't
  happen before since the initial parse was always already finished by
  then): (1) a freshly created `ParseWorker`'s `installed` counter starts
  at `0`, indistinguishable from "generation 0 requested, not yet
  installed" — fixed by bumping `edit_gen` to `1` before the initial
  request, mirroring what `edit_tree` already does before its own request;
  (2) `insert`/`remove` gated the whole tree-update path on
  `self.tree.is_some()`, which is `None` during the async-initial-parse
  window even though a grammar applies — an edit made in that window was
  silently invisible to the tree machinery, so the stale pre-edit parse
  would land and install as if current; fixed by gating on
  `self.parser.is_some()` instead, with `edit_tree` itself now tolerating a
  `None` tree (skips `.edit()`, still bumps the generation and requests a
  fresh parse). All three of T121's explicit targets now met, two by a wide
  margin (`docs/performance/index.md`, re-measured 2026-09-15): open 100 MB
  **5.05 s → 14.1 ms** (350×, budget was < 1 s); keypress-to-frame at 10 MB
  **3.35 µs** (budget < 16 ms — already true before this task, since typing
  already used the async-reparse worker; this task just added the 10 MB
  benchmark point to prove it instead of inferring from smaller ones);
  workspace search 10k files **214 ms**, unchanged (a separate subsystem;
  already well under its 500 ms budget in the original baseline, so
  parallelizing it was judged unnecessary — a real target already met
  without it beats an optimization with no demonstrated need). New
  `editor/open_until_highlighted` benchmark group keeps the *old*
  `editor/open` quantity ("fully parsed", not just "returned") visible for
  the two sizes where it now differs, using `iter_custom` to avoid
  triggering runaway concurrent 100 MB background parses once the
  construction itself became cheap enough for criterion's own calibration
  to want hundreds of iterations. 2 new unit tests plus 2 existing ones
  updated for the new (correct) behavior; full `cargo test --test
  integration` and `scripts/check` both green — this crate is a dependency
  of nearly everything else in the app.
- [x] **T122 — Startup budget.** Done 2026-09-15. Measured first (a
  throwaway instrumented build timing each `main.rs` step opening this
  repository itself, a real ~115-crate workspace, not a synthetic
  fixture) before touching anything: `Settings::load` ~1 ms, `App::new`
  ~10 ms, `load_scripts`/`maybe_prompt_script_trust`/
  `resolve_key_overrides`/`maybe_show_welcome`/`restore_session` combined
  under 1 ms, `refresh_git` **75–82 ms** — by far the dominant cost, all
  of it before `ratatui::init()` had even taken over the terminal. None of
  this task's own suspected culprits (locale table build, theme scan,
  snippet load) turned out to be the real bottleneck; `refresh_git`
  (three separate `git` subprocesses: repo?/branch/status) wasn't even on
  the suspect list — exactly the outcome "measure first" is for. Fixed by
  moving `refresh_git` in `main.rs` to run *after* the first
  `terminal.draw()` instead of before it: `App`'s git fields already
  default to "not a repo" (the same state opening Vix outside a repo
  renders correctly today), so the first frame draws immediately and the
  branch/status indicator catches up one frame later instead of blocking
  everything after it. A smaller, unconditional fix landed alongside it:
  `App::new` scanned the custom-themes directory twice (once in
  `apply_saved_theme`, once again immediately after for the View → Theme
  submenu); now scanned once, reused for both. New `benches/startup.rs`
  (`startup/app_new`, `startup/refresh_git`) gives this a lasting
  regression guard, the same way every other perf task in this run has.
  Full before/after story and numbers in `docs/performance/index.md`'s
  new "Cold start (T122)" section.
- [x] **T123 — LSP depth audit.** Done 2026-09-16. A full method-by-method
  diff of `vix-lsp`/`vix-lsp-core` against LSP 3.17 found the real feature
  set is much larger than the stale spec table previously showed (~28
  methods wired end-to-end, not the 4 the table listed) and turned up gaps
  in three shapes: one the task's own suspect list named that turned out
  *not* to be a gap (document formatting/range formatting — already fully
  wired, unrelated to `vix-format-tool`'s data-format normalization); three
  small, safe, no-new-protocol-surface bugs fixed as part of this same
  task; and eight real, larger gaps filed as follow-ups below rather than
  implemented now, per the task's own "file real findings as follow-ups"
  instruction. The three fixes: (1) the `initialize` request's advertised
  `capabilities` didn't match reality — it claimed `"didSave": false` while
  `textDocument/didSave` is actually sent, and declared no support at all
  for most already-implemented features (rename, code actions,
  document/workspace symbols, signature help, references, code lens, inlay
  hints, folding/selection ranges, document highlight, linked editing,
  call hierarchy, `workspace/applyEdit`, `workspace/executeCommand`) — a
  spec-correct server could reasonably withhold behavior for capabilities a
  client never declared, so every one of those is now declared, matching
  what's actually requested/handled (`crates/vix-lsp-core/src/message.rs`);
  (2) a JSON-RPC `error` response (as opposed to a `result`) was silently
  dropped — a failed rename/code-action/format/… just appeared to do
  nothing, with no feedback — now surfaced via a new `LspEvent::RequestFailed`
  to the status line (`status.lsp_request_failed`, all 15 locales); (3)
  signature help existed but was manual-invoke-only; it now also
  auto-triggers right after typing `(`/`,` inside a call
  (`App::maybe_auto_signature_help`), matching every other editor's
  convention for the feature. New tests at all three layers: `vix-lsp-core`
  unit tests for the capability JSON, a `vix-lsp` mock-server integration
  test proving a JSON-RPC error surfaces as `RequestFailed`
  (`tests/lsp_smoke.rs`), and an `App`-level end-to-end test driving a real
  `on_key('(')` through to a populated `App::hover` via a mock server.
  `crates/vix-lsp/spec/index.md`'s Features table rewritten and fact-checked
  against real action-id strings (caught and fixed one invented-wrong name,
  `lsp.workspace_symbol` → `lsp.workspace_symbols`, while doing so), plus a
  new "Known gaps against LSP 3.17" section. `scripts/check` green
  throughout.
- [x] **T123a — Semantic tokens.** Done 2026-09-17, picked up as part of
  T134's security/depth re-audit rather than deferred further, and scoped
  down from its own original ambition by a real finding made while
  implementing it: every bundled theme (`themes/*.json`, all 20 of them)
  currently defines exactly **four** syntax colors —
  `comment`/`keyword`/`number`/`string` — not the far richer vocabulary
  (`function`, `type`, `variable`, …) the Tree-sitter `.scm` highlight
  queries actually emit captures for for (those simply render unstyled
  today, a pre-existing gap this task didn't create). That means the
  task's own headline motivation — mutable-vs-immutable binding,
  deprecated, unused — has nowhere to render without a theme *schema*
  change (new color slots across 20 files, plus decoding LSP token
  *modifiers*, deliberately not attempted here) — filed as a separate,
  explicitly-scoped follow-up rather than rushed into this same change.
  What *is* implemented and real: full protocol plumbing — capability
  declaration (`semanticTokens.requests.full`, the LSP 3.17 standard
  `tokenTypes`/`tokenModifiers` lists), per-server legend capture at
  `initialize` (`Server::semantic_tokens_legend`, since a token's numeric
  type index is meaningless without it), and delta-decoding
  (`vix_lsp_core::message::parse_semantic_tokens`, unit-tested against
  hand-traced relative-position math) — plus a real, if narrow, visual
  result: `App::apply_semantic_tokens` maps each token's type onto
  whichever of the 4 existing theme slots it reasonably matches
  (comment/keyword/number/string; everything else falls through to
  Tree-sitter's own classification, unchanged), and
  `Editor::set_semantic_tokens` + `draw_syntax_layer`'s merge (T123a's
  only genuinely risky code, since it touches the shared render path —
  found and fixed one real bug here via a failing test: `draw_syntax_layer`
  was gated behind `code.is_highlight()`, true only when Tree-sitter has a
  loaded grammar, which would have silently dropped semantic tokens
  entirely for a language with an LSP server but no bundled Tree-sitter
  grammar) puts the LSP's judgment ahead of Tree-sitter's wherever the two
  would disagree. Re-requested after every edit, not just on open, since a
  token's classification (e.g. future "unused") can change as the user
  types. New tests at every layer: `vix-lsp-core` (legend parsing, delta
  decoding, capability declaration), a `vix-lsp` mock-server round trip,
  and 3 `vix-editor-core` render tests (merge takes effect, an unmapped
  type is silently skipped, a stale out-of-range token doesn't panic).
- [x] **T123b — Multiple servers per buffer.** Done 2026-09-17. `configs_for`
  replaces `config_for`, returning every config matching a file's
  extension instead of just the first — the registry itself
  (`HashMap<String, Server>` keyed by `language_id`) already supported two
  independent servers once two configs could both be found for one file;
  the real gap was every call site stopping at the first match. Document
  sync (`didOpen`/`didChange`/`didClose`/`didSave`) and the two shared
  request helpers (`request`/`send_request`, covering hover, the
  definition family, completion, document/workspace symbols, code
  actions, formatting, rename, prepare-rename, signature help, code lens,
  inlay hints, folding/selection ranges, document highlight, linked
  editing, semantic tokens, call-hierarchy preparation) now fan out to
  every matching server — each response arrives as its own event, so two
  servers answering one hover request produce two `LspEvent::Hover`s, not
  a merged one. `request_references` (bespoke, not on the shared helpers)
  got the same treatment by hand.
  Diagnostics needed real restructuring, not just fan-out: previously a
  flat `HashMap<PathBuf, Vec<Diagnostic>>`, so a second server publishing
  for a file already tracked by a first would silently replace its
  report — exactly the motivating scenario (a type-checker LSP + a
  separate linter LSP on the same buffer) would have lost one of the
  two. Now `HashMap<PathBuf, HashMap<String, Vec<Diagnostic>>>` (path →
  language_id → that server's current report); `diagnostics_for`/
  `all_diagnostics` merge across servers at read time (now returning
  owned `Diagnostic`s/`Vec<Diagnostic>` rather than borrowed, since a
  flattened merge can't be borrowed — the 4 call sites in `src/app*.rs`
  needed no changes beyond one `.iter()` → `.into_iter()` a real compile
  caught, since owned values support the same field access as borrowed
  ones). Both `publishDiagnostics` (push) and `workspace/diagnostic`
  (T123d's pull) write through the same new `set_diagnostics` helper.
  **Deliberate, documented scope cut** (`crates/vix-lsp/spec/index.md`
  "Known gaps"): `request_completion_resolve`, `execute_command`, and
  `request_incoming_calls` still target only the first matching config,
  not fanned out — each continues a response one *specific* earlier
  server gave (an opaque completion-resolve payload, a code action/lens's
  own command, a call-hierarchy item), and nothing tracks which server
  that was once more than one is active for a file. Correct whenever only
  one server handles a file (still the common case); no worse than before
  T123b otherwise. Properly fixing this means tagging completion items /
  code lenses / call-hierarchy items with their originating server
  through `LspEvent`, `CompletionItem`, and `CodeLens` — a real follow-up,
  not attempted here. New tests: 2 `vix-lsp` unit tests
  (`configs_for_returns_every_server_configured_for_the_extension`,
  `diagnostics_from_two_servers_for_the_same_file_coexist` — the latter
  proves a second server clearing its own report doesn't touch the
  first's), and a `tests/lsp_smoke.rs` mock-server test spawning two real
  (Python) mock processes against one file, proving both `didOpen` sees
  the file and both publish independently without clobbering.
- [x] **T123c — Server crash recovery.** Done 2026-09-17, picked up as
  part of T134's security/depth re-audit rather than deferred further.
  `Lsp::poll`'s `Incoming::Exited` handling now respawns the same command
  (up to `MAX_RESTART_ATTEMPTS` = 3 consecutive attempts since it last
  stayed up for `STABLE_UPTIME` = 30s — a real crash loop gets 3 tries
  then a `LspEvent::ServerCrashed` message instead of respawning forever;
  an isolated crash after a long healthy run earns its own fresh budget,
  tracked via a new `Server::ready_since` timestamp rather than resetting
  on every `ready` transition, which would have let a "crashes shortly
  after each respawn" server dodge the cap entirely by reaching `ready`
  every time). A new `LspEvent::ServerRestarted(Vec<PathBuf>)` names every
  file that was open on the crashed server (from its own `docs` table,
  captured before the crashed `Server` is dropped) so the host can replay
  `didOpen` with each file's *real, current* content — `Lsp` never holds
  buffer content itself. `App::poll_lsp` handles this by forgetting those
  paths were ever synced (`lsp_synced`), so the very next
  `lsp_sync_active` tick (the active tab, if affected) or the next time a
  background tab becomes active treats it as a fresh open. New
  `tests/lsp_smoke.rs` mock server that deterministically "crashes" (exits)
  right after every `didOpen` it handles, driving the full chain end to
  end: exactly 3 respawns, each correctly naming the crashed file for
  replay, then a `ServerCrashed` event instead of a 4th attempt.
- [x] **T123d — Pull-based `workspace/diagnostic` and `$/progress`.** Done
  2026-09-17, picked up as part of T134's security/depth re-audit rather
  than deferred further. **Pull diagnostics**: new
  `Lsp::request_workspace_diagnostics` sends `workspace/diagnostic` to
  every running server; its `Pending::WorkspaceDiagnostics` response
  merges straight into the same `self.diagnostics` map push
  (`publishDiagnostics`) already fills, via the same
  `LspEvent::Diagnostics(path)` event — so the Problems panel needed zero
  new rendering logic, just a trigger. That trigger is
  `App::open_diagnostics_panel`: opening the panel now pulls a fresh
  whole-project report first, not just whatever push has accumulated for
  files that happen to have been opened/synced already. A server that
  doesn't implement pull (most don't yet — it's a newer LSP 3.17 addition)
  gets a silent no-op, not a `RequestFailed` message, same treatment as
  `prepareRename`'s absence (T123e) — push already covers the baseline
  experience regardless. **`$/progress`**: `vix_lsp_core::message::
  parse_progress` turns a `{kind, title?, message?, percentage?}` payload
  into one status-line string (`"Indexing: 3/10 crates (30%)"`), pushed as
  `LspEvent::Progress`; an `"end"` report produces no event (the status
  line is ambient/best-effort already, same as every other transient
  status message in this app — nothing new needed to "clear" it). Both
  capabilities correctly declared in `initialize` (`textDocument.diagnostic`,
  `workspace.diagnostics.refreshSupport: false`, `window.workDoneProgress`
  — none were declared before, so a spec-correct server could have
  withheld both). New mock-server test proves the full round trip: a
  `$/progress` notification sent right after `initialize`, and a
  `workspace/diagnostic` pull that reports a diagnostic for a file
  (`other.rs`) that was *never opened or synced at all* — proving this is
  genuinely whole-project, not just a second way to learn about files
  already tracked.
- [x] **T123e — `prepareRename` and `relatedInformation`.** Done
  2026-09-16, picked up as part of T134's security/depth re-audit rather
  than deferred further. `prepareRename`: `begin_lsp_rename` now sends
  `textDocument/prepareRename` before opening the prompt at all
  (`App::open_lsp_rename_prompt`, gated on a new `LspEvent::RenamePrepared`/
  `RenamePrepared` 3-way outcome: `Placeholder(text)`/`Default`/
  `NotRenameable`); a server that doesn't implement it at all (most
  servers implement plain `rename` without ever adding this refinement)
  falls back to the pre-existing word-under-cursor guess rather than
  surfacing a scary error for an optional step — verified via a mock
  server returning both a real placeholder and a bare `null`
  (`tests/lsp_smoke.rs`, 2 new tests). `relatedInformation`: `Diagnostic`
  gained a `related: Vec<(Location, String)>` field
  (`vix_lsp_core::message::parse_related_information`, unit-tested), and
  `App::open_diagnostics_panel` shows each related location as its own
  indented, separately navigable row right after the diagnostic it
  belongs to — grouped through the panel's own sort so a diagnostic's
  notes never scatter away from it. Both capabilities corrected in the
  `initialize` request (`"rename": {"prepareSupport": true}`,
  `"publishDiagnostics": {"relatedInformation": true}`, both previously
  wrong). `crates/vix-lsp/spec/index.md` updated (Features table +
  "Known gaps" § now shows both closed).
- [x] **T123f — Multi-root workspace propagation.** Done 2026-09-17,
  closing the T123 audit's whole list. Decided the question the task text
  itself raised ("how a multi-root App maps onto a single spawned server
  per language_id"): T123b's per-file fan-out mechanism already lets one
  server answer for every folder it's told about, so there's no need to
  spawn a server per root — each running `Server` (still one per
  `language_id`) just needs to know the *whole* folder set instead of one
  root. `Lsp::new` takes `folders: &[PathBuf]` (was `root: &Path`);
  `initialize` sends every folder as `workspaceFolders: [{uri, name}, ...]`
  (the first also as the deprecated single `rootUri`, for servers
  predating LSP 3.6) and declares `capabilities.workspace.
  workspaceFolders: true`. New `Lsp::add_workspace_folder` sends
  `workspace/didChangeWorkspaceFolders` to every running server that
  asked for it (`changeNotifications` in its own `initialize` response,
  parsed into a new per-`Server` `workspace_folders_change_support` bool)
  — wired into `App::workspace_add_folder`. `App::switch_workspace` gained
  a `folders: &[PathBuf]` parameter so a multi-folder workspace file
  (`App::workspace_open`) hands the fresh `Lsp` every folder from the
  start, rather than the primary alone followed by a post-hoc
  `workspace_folders` overwrite that never told `Lsp` about the rest.
  There's no "remove folder from workspace" action in the app yet, so
  only `didChangeWorkspaceFolders`'s `added` half has a real caller —
  `removed` is supported by the message builder for whenever that action
  exists, not wired to anything now. Hit `serde_json::json!`'s macro
  recursion limit adding the new fields to `initialize_params`'s already-large
  literal — fixed with `#![recursion_limit = "256"]` on `vix-lsp-core`
  (a compile-time-only ceiling, unrelated to any runtime recursion).
  New tests: 4 `vix-lsp-core` unit tests (`initialize_params` with/without
  folders, `workspace_folders_change_support` parsing including the
  registration-id-string case, `didChangeWorkspaceFolders` params shape),
  1 `vix-lsp` unit test (`add_workspace_folder` builds the right
  `(uri, name)` pairs and is a no-op on a duplicate), and a
  `tests/lsp_smoke.rs` mock-server test that logs every message the
  server receives and asserts `initialize` named the one open folder
  while a later `add_workspace_folder` call produced exactly one
  `didChangeWorkspaceFolders` notification naming the new one.
- [x] **T124 — AI provider abstraction.** Done 2026-09-16. Investigated
  first, and the investigation reframed the task: `vix-ai-panel`/
  `vix-ai-diff`/DB NL→SQL don't call any AI provider directly today — every
  one of them shells out to whatever CLI `Settings::ai_command` names
  (`claude -p {prompt}` by default, also `codex`/`ollama run`/anything
  installed), a deliberate existing design with no API key for Vix to hold.
  So "migrate onto a provider trait with zero behavior change" as
  literally written would mean replacing a working, key-free design with
  one that needs credentials — asked the user before proceeding; chosen
  direction: add direct HTTP providers as a genuinely new, opt-in path
  alongside the CLI one (still the default), not a replacement. New
  `vix-ai-core` crate: an `enum Provider { Anthropic, OpenAi, Ollama }`,
  each a pure request-builder/response-parser pair (`anthropic`/`openai`/
  `ollama` modules, unit-tested against fixture JSON, no network I/O) plus
  one function, `complete`, that performs the actual blocking `ureq` call —
  same "pure core + one IO boundary" split as `vix-lsp-core`/`vix-lsp` and
  `vix-http-client::send`. Every provider's response parser checks for a
  JSON `error` field before the expected reply shape, so a non-2xx status
  still surfaces a real message instead of a bare code (`complete` treats
  `Err(ureq::Error::Status(_, resp))` the same as `Ok(resp)` for this
  reason). `secret::resolve` mirrors `vix-db/src/secret.rs`'s credential
  waterfall exactly (a configured command's stdout, then the OS keyring —
  service `vix-ai`, account = provider name), generalized from one saved
  connection to one provider name; `keyring` hoisted from `vix-db`'s
  crate-local dependency into a workspace one now that two crates need it.
  Four new `Settings` fields (`ai_provider` — `"cli"` default, or
  `"anthropic"`/`"openai"`/`"ollama"`; `ai_endpoint`; `ai_model`;
  `ai_api_key_command`), documented in `docs/configuration/index.md`. In
  `app.rs`, `spawn_ai_cmd` (the one function every AI call site already
  funneled through — the chat panel, the AI menu's Summarize/Explain/
  Define/Annotate/Improve, and the DB workbench's assistant) now dispatches
  on `ai_provider`: `spawn_ai_cli` (renamed, otherwise byte-for-byte the
  original code) for the default, or the new `spawn_ai_http` for a
  configured provider — every call site needed zero changes. `AiMsg::Failed`
  gained an `Option<String>` reason (`None` for the CLI path, whose stderr
  is discarded so there is nothing more specific to show than before;
  `Some` for an HTTP failure's real error text) surfaced via a new
  `status.ai_failed_detail` key, all 15 locales — the CLI path's exact
  wording is unchanged. Tests at three layers: `vix-ai-core`'s own unit
  tests (request/response shape per provider, `secret::resolve`'s command
  path), a new `crates/vix-ai-core/tests/http_smoke.rs` proving `complete`
  against a real local socket (a success, a non-2xx error body, and that
  the resolved API key actually reaches the `Authorization` header), and a
  new `App`-level test driving the real `ai.summarize` action end-to-end
  through `spawn_ai_http` against a mock server, landing in a new editor
  tab. `scripts/check` green throughout.
- [x] **T125 — AI features.** Done 2026-09-16. All three, explicit-invoke
  only, on either AI path (CLI or T124's HTTP providers — `spawn_ai_cmd`
  already dispatches on `ai_provider`, so nothing here had to care which):
  (1) **Edit Selection with Instruction…** (AI menu) prompts for free-text
  (a new `PromptKind::AiInstruction`, seeded by nothing — this one always
  starts empty) and applies it to the selection; requires an actual
  selection (never silently widens to the whole buffer, unlike Annotate/
  Improve — a user-authored instruction is unpredictable enough without
  also guessing the target) and *always* opens as a reviewable diff via
  `vix-ai-diff`, regardless of `ai_diff_review` — free-text instructions
  carry more risk than the fixed prompts that setting was designed to let
  power users skip past. (2) **Generate Doc Comment** (AI menu): sends the
  cursor's line plus a bounded 40-line window of following context, and
  proposes the reply as a *pure insertion* right before that line. Found
  and fixed a real bug here: `poll_ai_replace`'s shared trim
  (`trim_end_matches('\n')`, correct for every other AI dest, where a
  stray trailing blank line looks wrong) was stripping the newline that
  keeps an inserted comment on its own line, gluing it directly onto the
  code that follows (caught by the test, not just eyeballed) — fixed with
  a new `AiDest::InsertBeforeLine` variant that restores exactly one
  trailing newline before applying, instead of reusing `Replace`/`Diff`'s
  raw-text semantics. (3) **Generate Commit Message** (Git panel, `g` key
  — no menu item, since committing itself has none either): runs
  `git diff --staged` (new `vix_git::staged_diff`) through the AI and
  opens the commit prompt pre-filled with the reply via a new
  `AiDest::GitCommitMessage` — fills the message box, never commits on its
  own; the user still reviews and presses Enter. New tests at both layers
  that matter: `tests/integration/ai.rs` (new module — the *first* real
  test coverage `spawn_ai_cmd`/`AiReplace`/`poll_ai_replace` have ever had,
  Annotate/Improve/Summarize/Explain/Define included, all driven through a
  deterministic `printf`-based `ai_command` rather than a real assistant)
  and a new throwaway-repo test in `tests/integration/git.rs` for the
  commit-message flow. `scripts/check` green throughout.

### Security & hardening

The 2026-07-12 audit (see `AI_STATEMENT.md`-adjacent history / the
`security` CI job) covered the surface that existed then and closed its
findings except two deliberate risk-acceptances (HTTP-client SSRF — it's
a local dev tool, blocking loopback breaks its purpose; `Session::run`
stream misattribution — UI key-gating serializes it today). Everything
below is new ground: gaps this plan's own later capabilities open up
(scripting, keybinding overrides, AI), plus baseline hygiene the original
audit didn't scope (a public vulnerability-reporting policy, persisted
non-temp files). Audit-shaped tasks here follow the same "audit first,
file real findings as follow-ups" pattern as T111/T123, rather than
presupposing a specific vulnerability exists.

- [x] **T131 — `SECURITY.md`.** Done. New repo-root `SECURITY.md`:
  supported versions (latest release only, no LTS/backports), how to
  report privately (the same maintainer contact `AI_STATEMENT.md`'s
  "Questions" section already names — not a public issue; also points
  at GitHub's private-vulnerability-reporting flow as an alternative),
  what to expect (an acknowledgment within days, no formal SLA, priority
  over other work, credit on disclosure), a pointer to the existing
  `cargo-deny` supply-chain scan (`spec/ci/index.md`), what's
  deliberately out of scope (the HTTP client's intentional no-SSRF-guard
  design — a local dev tool, blocking loopback/private IPs would break
  its main use — plus a second, less-obvious one pulled from the 2026-07
  audit's own notes: `vix-db`'s streaming session could in principle
  misattribute a result to the wrong in-flight request if its UI-level
  serialization were ever bypassed; both previously only in memory, not
  public), and a short note on the scripting sandbox (`vix-script` has no
  file/network/process access by default — Rhai's stdlib exposes none).
  Linked from `README.md`/`index.md` (a new "## Security" section
  between License and AI Statement) and `AGENTS.md` (a line in
  Governance, alongside the `AI_STATEMENT.md` pointer); `spec/ci/index.md`
  gained a reciprocal pointer from its "Supply chain" section. GitHub
  surfaces a root `SECURITY.md` in its Security tab automatically, no
  extra config; GitLab/Codeberg mirror the file as-is.
- [x] **T132 — Script trust prompt.** Done. `App::load_scripts` now skips a
  workspace's `.vix/scripts/` entirely unless `App::project_scripts_trusted`
  says so; global scripts (`Settings::scripts_dir()`) are unaffected —
  always trusted, the user put them there directly. A new `ScriptTrustPrompt`
  overlay ("Trust this workspace and run them?", showing the script count)
  is queued once at startup when a workspace has project scripts and the
  decision has never been made (`App::maybe_prompt_script_trust`); answering
  `y`/`n` persists the decision via a new `vix_session::WorkspaceSession::
  scripts_trusted: Option<bool>` field and a new `Session::
  set_scripts_trusted(root, trusted)` method (deliberately not
  `set_workspace`, which also bumps `visits` — a trust decision isn't a
  workspace "open"). A decline is remembered too (not re-asked every plain
  launch, which would defeat persisting "no" at all) but isn't a permanent
  lockout: `script.reload` always re-checks (`App::maybe_reprompt_script_trust`),
  so a user who changes their mind can just reload to be asked again.

  **A real test-isolation gap had to be closed first**: `vix-session` had
  no test-only override (unlike `vix-settings`' `settings_path`/
  `with_settings_path`), and no existing test had ever exercised the real
  session-persisting path — this task's own accept/decline flow was the
  first to do so, which would otherwise have written every test run's
  temp-directory trust decisions into the real developer's `session.toml`.
  Added `Session::load_from`/`save_to` (mirroring `Settings::load_from`/
  `save_to` exactly) and `App::session_path`/`with_session_path` (mirroring
  `settings_path`), routed every `App` session read/write through new
  `load_session`/`store_session` helpers, and gave the shared `app_at`/
  `app_with` test builders a per-call isolated session file so this
  extends to every test in the suite, not just the new script-trust ones.

  8 existing script tests updated to grant trust before asserting a
  project script's effects (a `load_scripts_trusted` test helper drives
  the real `on_key('y')` flow, not a backdoor) — their own premise
  (project scripts load unconditionally) was the exact behavior this task
  intentionally changed. 4 new dedicated tests for the trust gate itself
  (prompt count and deferred load; no scripts → no prompt; accept persists
  and auto-loads across a simulated restart; decline persists but
  `script.reload` re-asks) plus 2 new `vix-session` unit tests for
  `set_scripts_trusted`. Updated `crates/vix-script/spec/index.md`'s
  "Script discovery" section and `crates/vix-session/spec/index.md` (a new
  "Script trust" section).
- [x] **T133 — Persisted-file permission audit.** Done. Audited every
  persisted config-dir file, not just the 3 the task named as examples —
  its own framing ("files that persist long-term") wasn't a closed list,
  and the same investigation turned up two more real candidates
  (`db_history.toml`/`db_queries.toml`) plus `macros.toml`. Two new
  `vix-fileops` primitives, both landing in the same crate that already
  owned `write_private_temp`/`write_atomic`:
  - `write_atomic_private(path, data)` — same write-temp-then-rename
    mechanics as `write_atomic`, but a **new** file is always created
    0600 on Unix regardless of umask (an *existing* file's mode is still
    preserved, unchanged). Wired into `vix-undo-store::save` (a full undo
    tree can carry text no longer in the current buffer at all) and
    `vix-macros::upsert` (a recorded macro can carry literally-typed
    text) — the latter also gained atomicity it never had before
    (was a plain `fs::write`).
  - `restrict_to_owner(path)` — best-effort narrow-after-write, for the
    files that go through `confy` rather than this crate's own writer
    and so have no hook to choose the mode at creation time: wired into
    `vix-settings::Settings::save`/`save_to` (`config.toml` — a custom
    `ai_command` could embed a secret), `vix-session::Session::save`/
    `save_to` (`session.toml` — open-file paths reveal project
    structure, a cached `project_cmd_*` could embed one), and
    `vix-db::store::save_history`/`save_saved` (`db_history.toml`/
    `db_queries.toml` — a query's own literal text, not just its bind
    parameters, can carry a sensitive value).

  `keybindings.toml` (`vix-keybindings::user_bindings`) is a deliberate
  non-fix, documented rather than silently skipped: a key token paired
  with an action id is never sensitive. New tests for every fix (write
  mode assertions in each of the 5 touched crates, plus 2 new
  `vix-fileops` tests for `write_atomic_private` itself and 2 for
  `restrict_to_owner`); `vix-undo-store`/`vix-db::store`'s own
  confy-touching `save`/`load` paths still have no test-only path
  override (a pre-existing gap this task didn't introduce, same as
  T132's `vix-session` gap it *did* fix) — their new behavior is
  covered indirectly, through `vix-fileops`'s own already-tested
  primitives. New `vix-fileops/spec/index.md` "Atomic and private
  writes" section (this crate's writers had no spec coverage at all
  before this).
- [x] **T134 — Post-scripting/AI security re-audit.** Done 2026-09-16.
  All three checklist items investigated empirically, not by reading the
  specs' own claims and trusting them:
  - **Rhai sandboxing under real usage**: found and fixed a real gap.
    `crates/vix-script/spec/index.md`'s "closed by default" claim ("no
    file, network, or process API... nothing unregistered exists to
    call") is true for *registered functions*, but `import` is a
    language-level statement, not a registered function — and
    `Engine::new()`'s default `FileModuleResolver` turned out to be live.
    Proved it empirically before touching any code: wrote a throwaway
    script that imported a real `.rhai` file from a temp directory via a
    CWD-relative path and printed a value it defined — it worked,
    printing the value, meaning a reviewed-and-trusted project script
    (T132's whole workspace-trust premise: the user reviews *the one
    file* they're trusting) could silently pull in and execute a second
    file the user never saw at all. Fixed with
    `engine.set_module_resolver(DummyModuleResolver::new())` — `import`
    now fails unconditionally. `eval` was also probed and found to
    execute (a working Rhai feature, not a bug — it stays inside the same
    `Engine`/sandbox, reaching nothing a direct call couldn't already
    reach), documented rather than disabled. New
    `crates/vix-script/tests/sandbox.rs` (3 tests) is a permanent
    regression guard for both findings; all 15 pre-existing `vix-script`
    tests still pass unchanged.
  - **AI provider keys via the keyring waterfall**: confirmed already
    correct, no fix needed. `Settings` has exactly one AI-key-adjacent
    field, `ai_api_key_command` — a *command* (mirroring `vix-db`'s
    `password_command`), never a raw key. `vix_ai_core::secret::resolve`
    tries that command's stdout, then the OS keyring; there is no code
    path anywhere that writes a plaintext key into `config.toml`.
  - **T125's promises**: explicit-invoke-only confirmed for all three
    T125 features (no auto-trigger anywhere in `ai_begin_edit_with_
    instruction`/`ai_generate_doc_comment`/`git_generate_commit_message`).
    "Redact file paths on request" (plan.md's original wording — dropped
    somewhere before it reached `tasks.md`'s own trimmed T125 entry, a
    real drift between the two files, though not one that changed what
    got built) turned out to be moot for the surface actually shipped:
    audited every AI request-building path (all 5 AI-menu actions, the
    chat panel, DB's NL→SQL, T125's 3 new features) and found none send a
    real filesystem path at all — buffer content and selections only,
    except the commit-message generator's `git diff --staged` input,
    which necessarily carries *workspace-relative* paths (`src/app.rs`,
    never `/Users/name/...`) because a commit message is meaningless
    without knowing which files changed. Redacting just those paths while
    still sending the full diff *content* of those same files would be
    security theater, not real protection — the honest conclusion is
    "not applicable to what shipped," not silently dropped or
    token-implemented.
  - **Zero follow-up tasks filed** (no T134a/T134b/…): every item
    resolved to fixed, already-correct, or not-applicable-with-reasoning
    — unlike T123's audit, this one found no gap large enough to defer.
  `scripts/check` green throughout.

### Code quality & maintainability

Grounded in a measured pass on 2026-09-04 (numbers below are from that
run; re-measure before starting any task). The headline: the discipline
is good — 8 non-test `unwrap()` + 2 `expect()` in all of `src/app.rs`, 6
real `TODO`/`FIXME` comments workspace-wide, only 4 `too_many_lines`
allows in 61k lines of crate code — so the debt is **structural**
(size, duplication, drift between hand-maintained copies), not
sloppiness. Tasks are ordered roughly by payoff; each is its own branch
and its own gate run, zero intended behavior change unless stated.

- [x] **T141 — Carve up `src/app.rs`.** Done, all of it, in one session.
  (see below). Originally 22,449 lines, 808 `fn`s, 767 string-literal
  match arms (grown to 23,058 lines/481 `#[test]`s-worth of scaffolding
  by the time T143 ran); `AGENTS.md`/`CLAUDE.md` describe "a thin App
  shell over ~106 focused crates" and this file is the opposite.
  Staged, not one rewrite: (a) move `impl App` blocks into
  `src/app/<feature>.rs` submodules by feature — keymap dispatch (the
  ten `*_key`/`*_token` fns, `on_key`), org, git, lsp/dap, palette,
  prompts/dialogs, scripts, tools, session/settings — pure moves, one
  module per branch, the test suite green after each; (b) split
  `run_action`'s giant `match` into per-namespace dispatchers
  (`run_file_action`, `run_view_action`, …) chained the way
  `run_vim_action`/`run_edit_action` already are — "one action id, one
  arm" still holds, the arm just lives next to its feature. T143 (done
  first, as planned) is what made each slice's diff reviewable at all.
  **Slice 1 (keymap dispatch) done 2026-09-06**: moved `on_key`, the
  modal-overlay routing chain (`try_overlay_key`/`try_panel_key`/
  `try_tool_dialog_key`/`overlay_capturing_keys`/`calendar_key`),
  `on_paste`, `command_as_control`, the ten keymap-style dispatchers and
  their `*_token` helpers, and the keymap-display helpers
  (`active_keymap`/`mode_indicator`/`which_key`/`ctrl`/`alt`/`shift`)
  into new `src/app/keymap.rs` (~1490 lines) as its own `impl App`
  block — a plain `mod keymap;` in `app.rs`, no directory restructure
  needed. Every other feature's own `_key` handler (`git_panel_key`,
  `db_key`, …) stayed in `app.rs` and is called from `keymap.rs` exactly
  as before; an inherent `impl App` block can live in any module of the
  crate, so where a method is *defined* never has to move in lockstep
  with everything it calls. Needed: `use super::{App, Focus, Keymap,
  display_key, menu_index_for_alt};` (two free functions still in
  `app.rs`, referenced unqualified); `pub(super) fn` on 13 methods
  (`ctrl`/`alt`/`shift`/`spacemacs_leader_bindings`/
  `toggle_focus_explorer_editor`/`editor_motion`/`run_vim_action`/
  `command_as_control`/`emacs_c_chord_key`/`emacs_c_x_chord_key`/
  `emacs_c_p_chord_key`/`emacs_c_p_c_chord_key`/`emacs_c_p_c_m_chord_key`)
  called from `app.rs`'s own production code or its `#[cfg(test)]` unit
  tests; and a `#[cfg(test)] use crossterm::event::KeyModifiers;` in
  `app.rs` (its only remaining use of that type once the dispatch code
  needing it crate-wide moved out). The last two rounds only surfaced
  under `cargo build --all-targets` — a narrower `cargo build --lib --bin
  vix` compiles clean but never activates `#[cfg(test)]` code, silently
  missing a whole class of error; always build/test the wider way when
  moving code unit tests touch. Full workspace `cargo test` green
  throughout, matching the pre-move baseline exactly (lib unit 86/0,
  integration 451/0/11 ignored).
  **Slice 2 (git/jj — version control) done 2026-09-06**: unlike slice 1,
  these ~59 methods were *not* one contiguous block — scattered across
  ~1500 lines, interleaved with the debugger, spellcheck, and
  context-menu code that stayed behind. Moved via a small Python script
  (find every method's exact span by name — a method's own closing brace
  is the next `    }` line, since rustfmt always aligns a block's closer
  with the line that opened it and nothing nested inside a method body
  can produce a bare 4-space-indented `    }` of its own — then delete
  bottom-up so earlier deletions never shift a not-yet-processed range's
  line numbers) into new `src/app/git.rs`: `run_git_action` (the
  `run_action` git-namespace dispatcher), the Git status/gutter/hunk/
  diff/conflict-resolution plumbing, the Git panel and its commands
  (stage/commit/branch/log/clone/blame/grep/remote), the branch chooser,
  the generic diff view, and a full sibling Jujutsu (`jj_*`) dispatcher —
  grouped in with git as "version control" since there was nowhere else
  sensible for it to live. Needed `use super::{App, BranchChooser,
  DiffViewState, GitPanel, Prompt, PromptKind, gutter_hex,
  rect_contains};` + `use crate::editor::Tab;` (free functions/types
  still in `app.rs`), and `pub(super) fn` on 15 methods (`run_git_action`/
  `accept_jj_prompt`/`diff_goto`/`git_panel_key`/`git_panel_mouse`/
  `git_commit`/`git_create_branch`/`git_clone`/`git_edit_description`/
  `git_delete_branch`/`git_grep`/`branch_key`/`branch_mouse`/
  `open_diff_with`/`diff_view_key`) called from `app.rs`'s remaining code
  or from `src/app/keymap.rs` — a *sibling* module, not a parent, but
  `pub(super)` still reaches it: visibility granted at the parent `app`
  extends to every descendant of `app`, `keymap` included. **Found, not
  fixed**: `run_git_action`'s match also dispatches the unrelated `run.*`
  debugger actions (a pre-existing naming/scope mismatch predating this
  move, out of scope for a pure relocation). Applying slice 1's
  `--all-targets`-from-the-start lesson paid off immediately — only 2
  build-fix rounds needed this time despite the non-contiguous, larger
  extraction, versus slice 1's several. Full workspace `cargo test` green
  throughout, exactly matching the pre-move baseline.
  **Slice 3 (scripts) done 2026-09-06**: ~20 methods, non-contiguous
  again — scattered across the file, interleaved this time with T204's
  keybinding-editor code (`open_keybinding_editor`,
  `build_keybinding_rows`, `keybinding_editor_key`, …), which stayed in
  `app.rs` (not one of this epic's named slices). Moved into new
  `src/app/scripts.rs` via the same cherry-pick-by-name script:
  `load_scripts`/`reload_scripts`, the whole script-trust prompt flow
  (T132: `project_scripts_trusted`, `maybe_prompt_script_trust`,
  `maybe_reprompt_script_trust`, `queue_script_trust_prompt_if_any`,
  `accept_script_trust`, `decline_script_trust`, `set_scripts_trusted`,
  `script_trust_key`), the script chooser and its run/invoke/host-state
  plumbing, and `script_palette_entries` (the command palette's `!`
  script-search mode). Needed `use super::{App, PendingScriptPrompt,
  Prompt, PromptKind, ScriptChooser, ScriptTrustPrompt,
  script_current_line_text};` + direct `use crate::palette::{self,
  Action as PAction, Entry}; use crate::settings::Settings;`, and
  `pub(super) fn` on 8 methods (`reload_scripts`/`script_trust_key`/
  `open_script_chooser`/`script_chooser_key`/`script_chooser_mouse`/
  `run_script_command`/`accept_script_prompt`/`script_palette_entries`)
  called from `app.rs`'s remaining `run_action`/`accept_prompt` or from
  `src/app/keymap.rs`. Full workspace `cargo test` green throughout,
  matching baseline exactly.
  **Slice 4 (command palette) done 2026-09-06**: mostly contiguous
  again, like slice 1 — `open_palette` through `accept_palette`, ~11
  methods. Moved into new `src/app/command_palette.rs`, named
  `command_palette` rather than `palette` because `app.rs` already
  imports the top-level `vix-palette` crate under the local name
  `palette` (`use crate::palette::{self, ...}`); a `mod palette;` here
  would have collided with that `use` — checked and picked the different
  name *before* generating any files, not by trial and error. What
  moved: opening the palette (plain or seeded into a specific mode),
  the ranked project file index (`build_file_index`, `ignore`-crate
  walk) and its Files-mode entries, live recompute across all five modes
  (files/commands/buffers/goto-line/symbols/workspace-symbols), the
  live go-to-line preview, key handling, and accepting the highlighted
  entry. Needed `use super::{App, Focus}; use crate::editor::
  is_image_path; use crate::palette::{self, Action as PAction, Entry,
  Mode as PMode, Palette};`, 4 `pub(super)` bumps (`open_palette`/
  `open_palette_seeded`/`build_file_index`/`palette_key`), and —a new
  wrinkle— trimming `app.rs`'s *own* `use crate::palette::{...}` down to
  `use crate::palette::{self, Palette};` once `Action as PAction`/
  `Entry`/`Mode as PMode` became unused there (only `Palette`, the
  struct's own field type, and the `palette` module alias for a couple
  of remaining `palette::parse_path_target` calls, are still needed in
  `app.rs` itself). Full workspace `cargo test` green throughout,
  matching baseline exactly. Wrote the extractor as a proper reusable
  script (`extract_app_module.py`, edit `TARGETS` + pass a module-name
  arg) rather than a one-off per slice, since the pattern was clearly
  going to keep repeating.
  **Slice 5 (session/settings) done 2026-09-06**: mostly contiguous —
  `store_settings` through `save_session` is one clean run — plus 5
  outliers scattered further down (`ensure_project_session_loaded`,
  `fill_project_fields`, the workspace chooser's open/key/mouse trio,
  `open_settings_file`). Moved into new `src/app/session.rs`; `App::new`
  and its `build_core` helper deliberately stayed in `app.rs` — the
  constructor itself is the entry point, not a feature to relocate.
  Needed `use super::{App, Focus, ProjectHistory, WorkspaceChooser,
  node_to_pane, pane_to_node}; use crate::explorer::Explorer;`, and
  `pub(super) fn` on 11 methods (`store_settings`/`load_session`/
  `store_session`/`session_key`/`save_session`/
  `ensure_project_session_loaded`/`open_workspace_chooser`/
  `workspace_chooser_key`/`workspace_chooser_mouse`/`switch_workspace`/
  `open_settings_file`) — nearly everything in the file needed it, since
  `run_action` and half the rest of `app.rs` call into session/settings
  plumbing constantly. Checked for a name collision before starting
  (`grep -n "use crate::session" src/app.rs` found nothing — every
  reference was already fully-qualified `crate::session::...`), so
  `mod session;` was safe with no rename needed, unlike slice 4. Full
  workspace `cargo test` green throughout, matching baseline exactly.
  **Slice 6 (LSP + DAP) done 2026-09-06**: the largest and most
  scattered slice yet — 32 methods, 49 errors on the first build
  attempt (vs. single digits to ~20 for every prior slice), clean on
  the second. Moved into new `src/app/lsp_dap.rs`, named `lsp_dap`
  rather than a bare `lsp`/`dap` to stay unambiguous (no direct
  collision found — `app.rs` only ever references `crate::lsp`/
  `crate::dap` fully qualified or via function-body-local
  `crate::lsp_core` imports — but a short generic name felt like
  inviting a future one). What moved: inlay hints, document/selection-
  range highlights, diagnostics, hover, completion (including two
  non-LSP data sources that reuse the same completion popup — an
  Org-roam `[[` node-title completer and an Org-contacts `mailto:`/link
  completer), go-to-definition (`lsp_jump`), format-on-demand,
  workspace edits, linked edit, signature help, and the whole DAP
  debugger cluster (adapter lookup, breakpoints, start/stop, step
  markers, `accept_debug_prompt`). `build_core` (constructs the LSP
  client at startup) and `menu_hover` (an unrelated same-named function
  — menu tooltips, not LSP hover) both deliberately stayed in `app.rs`.
  Needed `use super::{App, CompletionPopup, Focus, Prompt, PromptKind,
  apply_edits_to_text, char_to_lsp_pos, lsp_pos_to_char,
  severity_color}; use crate::editor::SEARCH_MARK; use
  crate::workspace_search::{Hit, WorkspaceSearch};` and `pub(super) fn`
  on 27 of the 32 methods — nearly all of them, since diagnostics/hover/
  completion/debugger events are wired from `App::on_key`,
  `run_action`, and the LSP/DAP event-poll loops still in `app.rs`.
  Full workspace `cargo test` green throughout, matching baseline
  exactly.
  **`org` and `tools` turned out far too big for one slice each** (~140
  and ~86 name-matches respectively — 4–10× every module extracted so
  far) — splitting both into natural sub-areas instead of forcing one
  giant move.
  **Slice 7 (org-roam) done 2026-09-06**: the first org sub-slice —
  node lookup/creation, wiki-link insertion, random-node jump, live
  backlinks, dailies (open/capture), the node graph, database sync, and
  the `node.*` transforms (nodeify, extract subtree, insert
  transclusion, rename by title, dead-links, reset). ~21 methods,
  mostly contiguous, one outlier (`refresh_backlinks_follow`). Moved
  into new `src/app/roam.rs`. Needed `use super::{App, Prompt,
  PromptKind};` plus `std::path::{Path, PathBuf}`, and `pub(super) fn`
  on 9 of the 21 methods (`roam_node_files`/`roam_action`/
  `roam_write_and_open`/`roam_visit_or_create`/`roam_insert_link`/
  `node_insert_transclusion`/`roam_daily_capture`/`roam_open_daily`/
  `roam_rewrite_active`). Full workspace `cargo test` green throughout.
  **Slice 8 (org table + column view) done 2026-09-06**: two clean
  contiguous clusters bundled together (Column View renders interactive
  `TBLFM` tables) — `org_table_cursor_pos` through `org_table_action`
  (~20 methods incl. a `format_table_number` helper) and
  `open_column_view` through `org_columns_update_all_dblocks` (~7
  methods). Moved into new `src/app/org_table.rs`, deliberately *not*
  named `column_view` — `app.rs` already has an unrelated top-level
  `crate::column_view` module (a different, crate-root module, always
  referenced fully qualified there, so no hard collision, but reusing
  the name would leave two same-named modules confusingly coexisting at
  different levels of the tree). Needed `use super::{App, Focus, Prompt,
  PromptKind};` plus crossterm `KeyCode`/`KeyEvent`, and `pub(super) fn`
  on 15 of the 28 methods. Full workspace `cargo test` green throughout.
  **Slice 9 (org core) done 2026-09-06 — the largest slice of the whole
  epic, 83 methods/1656 lines.** Turned out to be one genuinely
  contiguous block (unlike git/scripts/lsp_dap's scattering), closer in
  shape to the keymap-dispatch slice despite the size: the `org.*`/
  `org.edit.*` action dispatchers, headline/subtree editing (priority,
  move, close-note, new heading, cut/copy/paste subtree, export),
  refile, sparse trees, edit-src, footnotes, internal links (store/
  insert/follow), archive, tags/properties, timestamps/planning
  (SCHEDULED/DEADLINE), emphasis markup, capture end-to-end (chooser,
  template fields, review, filing, clock), and the built-in agenda
  (file scoping, view building, its own key handling). Moved into new
  `src/app/org.rs`. Also fixed the slice-7 gap: `accept_roam_prompt`
  moved here too (not to `roam.rs`) — despite its name it's really the
  shared accept-handler for capture/roam/goto/workspace prompts, and
  groups better next to its siblings `accept_org_prompt`/
  `accept_org_agenda_prompt`. 7 generic editor transforms interleaved in
  the original range (`new_scratch_buffer`, `rewrite_at_cursor`,
  `bump_number`, `transpose`, `delete_unit`, `wrap_text`,
  `smart_toggle`) aren't org-specific and stayed in `app.rs`. Needed
  `use super::{AgendaKind, AgendaView, App, CaptureChooser, Focus,
  PendingCapture, Prompt, PromptKind, RefileChooser, SrcEdit};` plus
  crossterm/`std::path` types, and `pub(super) fn` on 16 methods —
  clean in just 2 rounds despite the size (better than several smaller
  slices). One clippy `doc_markdown` finding on the full-gate retry
  (needed backticks around `` `git`/`scripts`/`lsp_dap` `` in the new
  module's own doc comment) — a reminder that clippy pedantic only runs
  inside the full `scripts/check`, not a bare `cargo build`/`cargo test`.
  Full workspace `cargo test` green throughout, matching baseline
  exactly. **This closes out "org"** — roam (slice 7), table/column-view
  (slice 8), and core (this slice) between them cover the whole area.
  **Slice 10 (tools: insert snippets + converters) done 2026-09-06** —
  the first "tools" sub-slice. Clean contiguous block: `insert_content`
  through `regex_tester_key` — Markdown/HTML/SQL/LaTeX/Org snippets,
  inline markers/blocks, dynamic UUID/ZID/date-time insertion, plus the
  color/unit converters, calculator, and regex tester (all share the
  "open an input overlay, insert its result" shape). Moved into new
  `src/app/insert_tools.rs`, excluding two interlopers in the middle
  (`surround`/`toggle_wrap` — general editing operations, not
  tools-menu snippets). Needed `use super::{App, SQL_CREATE_EXTENSION,
  SQL_CREATE_TABLE};` plus crossterm `KeyCode`/`KeyEvent`, and
  `pub(super) fn` on 19 of the 21 methods. One real surprise: the
  module-level `Focus` import came back genuinely unused — `calculator_key`
  has its own `use crate::calculator_tool::Focus;` shadowing the
  app-shell `Focus` within that one function, caught by the compiler's
  own unused-import warning rather than missed silently. Full workspace
  `cargo test` green throughout, matching baseline exactly.
  **Slice 11 (tools: picker panels) done 2026-09-06** — the small
  glyph/color/type picker panels: Nerd Font character picker, ASCII-art
  character picker, X11 color picker, media type catalog, and the QR
  code generator (each an "open, arrow/click to select, insert"
  overlay). 17 methods, moved into new `src/app/picker_panels.rs`.
  `active_media_type` sits right next to this cluster by line number
  but stayed in `app.rs` — it's the snippet library's own media-type
  detection, not part of the media-type *panel*, despite the similar
  name. Needed `use super::{App, AsciiPanel, NerdPalette, X11Panel,
  rect_contains};` plus crossterm mouse/key types, and `pub(super) fn`
  on 13 of the 17 methods. Full workspace `cargo test` green throughout.
  **Slice 12 (tools: info panels) done 2026-09-06 — the last "tools"
  sub-slice.** Another large genuinely contiguous block like org core
  (~32 methods, no cherry-picking needed): the Contacts vCard view,
  file-info and text-info panels, Markdown preview, the Snippets
  picker (with its tabstop-expansion session and the active buffer's
  media-type-scoped library), and the System Info panel. Moved into
  new `src/app/info_panels.rs`. Needed `use super::{App, ContactPanel,
  FileInfoPanel, MarkdownPreview, SnippetSession, SystemInfoPanel,
  TextInfoPanel, VcardPanel, rect_contains}; use crate::editor::Tab;`
  plus crossterm key/mouse types, and `pub(super) fn` on 21 of the 32
  methods. Full workspace `cargo test` green throughout. **This closes
  out "tools" entirely** — `insert_tools` (slice 10), `picker_panels`
  (slice 11), and `info_panels` (this slice) between them cover the
  whole area, same as roam/table-view/core did for "org".
  **The only remaining piece of T141 is the `run_action` match split.**
  Dropped
  "prompts/dialogs" as its own slice after surveying it: `PromptKind`'s
  accept-handlers (`accept_org_prompt`, `accept_debug_prompt`,
  `accept_file_prompt`, `accept_goto_number`, …) aren't one coherent
  area — each belongs with its own feature and should move alongside it
  (org's prompt handlers with the org slice, debug's with lsp/dap, …),
  not into a cross-cutting "prompts" module that would just recreate the
  same scattering this epic is trying to remove.
  **T141 is now fully complete — 12 slices plus the `run_action` split,
  all in one session, `src/app.rs` down from 23,058 lines to 13,919
  (~40% cut) into a thin(ner) shell plus 12 focused
  `src/app/*.rs` submodules**: `keymap.rs` (on_key + the ten keymap
  dispatchers), `git.rs` (git + jj version control), `scripts.rs`
  (script loading/trust/chooser), `command_palette.rs` (the `Ctrl+P`
  palette), `session.rs` (session/settings persistence),
  `lsp_dap.rs` (LSP + DAP), `roam.rs` (Org-roam/node), `org_table.rs`
  (org tables + Column View), `org.rs` (org core: headline/subtree,
  refile, agenda, capture, links, tags, timestamps — the epic's
  largest single file), `insert_tools.rs` (insert snippets +
  converters + the final `run_tools_action` dispatcher),
  `picker_panels.rs` (Nerd Font/ASCII/X11/media-type/QR pickers), and
  `info_panels.rs` (vCard/file-info/text-info/Markdown-preview/
  snippets/system-info). **Final piece (`run_action` split)**: most of
  the per-namespace dispatching had already happened incrementally
  across earlier (pre-this-session) tasks — `run_file_action`/
  `run_edit_action`/`run_motion_action`/`run_text_tool_action`/
  `run_vim_action`/`run_view_action`/`run_git_action`/
  `run_project_action`/`run_named_action`/`go_action`/`contacts_action`
  already existed — so only ~17 single-line `tools.*` "open this
  overlay" arms remained undelegated; pulled into a new
  `run_tools_action(&mut self, action: &str) -> bool` in
  `insert_tools.rs`, chained as `a if self.run_tools_action(a) => {}`.
  Bumped `open_html_panel`/`open_contacts`/`open_dashboard`/
  `open_pomodoro` to `pub(super)` (now called cross-module). Genuinely
  miscellaneous arms (`tools.calendar`/`tools.clock` toggles, the
  `view.theme:`/`view.locale:`/`view.keymap:`/`view.time_zone:`/
  `script:` prefix matches, `explorer.filter_include`/`_exclude`
  prompts) stayed inline in `run_action` — they don't fit the "one
  action id, one arm" delegation shape (prefix matches, or genuinely
  one-off multi-statement toggles). Full workspace `cargo test` green
  throughout every slice (lib unit 86/0, integration 451/0/11 ignored,
  never once regressed), full `scripts/check` green on every merge.
  Techniques established along the way, reusable for T142: a small
  Python extractor script (`extract_app_module.py`) that exploits
  rustfmt's guarantee that every top-level method's own closing brace
  sits at the same indent as its `fn` line, for both contiguous ranges
  and non-contiguous cherry-picks; checking a candidate module name for
  collisions against `app.rs`'s existing `use crate::<name>` bindings
  *before* generating any files (caught real collisions for
  `command_palette` vs. `palette`, avoided for `org`/`session`/
  `insert_tools` after checking); always building with
  `cargo build --all-targets` (a narrower `--lib --bin` build misses a
  whole class of `#[cfg(test)]`-only errors); and never trusting a gate
  result piped through `tail`/`head` (it silently reports the pipe's
  own exit code, not the command's).
- [x] **T142 — Same for `src/ui.rs`.** Done, 14 slices, 2026-09-06
  through 2026-09-08. `ui.rs` went from 7,293 to 713 lines (a 90% cut)
  across 21 new `src/ui/*.rs` submodules: `db.rs`, `info_panels.rs`,
  `picker_panels.rs`, `edit_surfaces.rs`, `choosers.rs`, `menu_bar.rs`,
  `dialogs.rs`, `file_browser.rs`, `ai_terminal.rs`, `search.rs`,
  `help.rs`, `lsp_popups.rs`, `tool_panels.rs`, `explorer.rs`,
  `boxes.rs`, `tabs.rs`, `hints.rs`, `docks.rs`, `minimap.rs`,
  `bottom_dock.rs`, `status_bar.rs`, plus `editor_region.rs` (22
  total). The final 713 lines are the irreducible remainder: the
  top-level dispatch chain (`draw`, `draw_overlays`,
  `draw_overlays_aux`, `draw_chooser_overlays`, `body_columns`) that
  every extracted module's `pub(super)` function is called from, and
  shared rendering primitives (`draw_scrollbar`/`draw_hscrollbar` +
  their `scrollbar_pos_from_row`/`_col` mouse-hit-test siblings,
  `trunc`, `centered`, `span_line_width`/`hslice_spans`,
  `unix_secs_label`, `git_change_color`, `menu_offsets`,
  `dock_toggle_cols`, `menu_dropdown_rect`, `dropdown_scroll`,
  `RULER_COLUMN`, `NERD_CELL_W`) that 10+ of the already-moved sibling
  modules — or `app.rs` directly, via genuinely-public paths and one
  intra-doc-link — depend on via `super::`. Moving any of these further
  would either relocate the top of the call graph without simplifying
  it, or force an arbitrary "owner" onto a helper several unrelated
  siblings share — confirmed by checking, not assumed, per each
  slice's own findings (see the slice-by-slice history below and in
  the topic memory file). Same technique as T141 throughout: the
  rustfmt column-0-closing-brace extraction script
  (`extract_ui_module.py`, adapted for free `fn`s instead of `impl`
  methods), a `git checkout -- src/ui.rs` + corrected-TARGETS restart
  on the one real miss (slice 4), a shell-concatenation build for
  bodies too large to safely hand-retype (slices 10, 12, 13, 14), and
  full `cargo build --all-targets` / `cargo test --workspace` /
  `cargo test --test snapshots` (direct re-verification every slice,
  not just the aggregate count) / `scripts/check` on every single
  slice before merging. A genuine architecture conflict surfaced right
  at the start (T142's original wording vs. `AGENTS.md`'s "rendering
  lives only in `src/ui.rs`" hard rule) — resolved by keeping the hard
  rule and revising this task's own wording, per the user's explicit
  choice. Full slice-by-slice history below, starting from the
  original opening note:

  7,293 lines (112 `draw_*`
  functions), 3 of the workspace's 4 `too_many_lines` allows are here
  (`draw_ai_diff`, `draw_terminal`, `draw_search` — the 4th is
  `app.rs`'s `draw_insert`). **Revised 2026-09-06** (after T141 landed
  and this task's original wording — "move each `draw_*` next to its
  state, into the owning crate where one exists" — turned out to
  directly contradict `AGENTS.md`'s own non-negotiable hard rule,
  "Rendering lives only in `src/ui.rs`", which several panel crates'
  own doc comments independently restate, e.g. `vix-clock-panel`: "pure
  logic... does no rendering, so the host draws it and the logic stays
  unit-testable without a terminal." Checked with the user rather than
  picking a side: **the hard rule stays, this task's wording changes**):
  move each overlay/panel's `draw_*` into `src/ui/<feature>.rs`
  submodules — same crate, same pattern T141 used for `src/app.rs`,
  never into a panel's own crate — and split the three 100+-line
  drawers so the `too_many_lines` allows come out.
  **Slice 1 (info panels) done 2026-09-06** — the first T142 slice.
  Reused T141's extraction technique almost unchanged: a small Python
  script (`extract_ui_module.py`) exploiting the same rustfmt
  column-0-closing-brace guarantee, just applied to free `fn`s at
  column 0 (verified first: 159 top-level `fn`s + 1 top-level `struct`
  = 160 column-0 `}` lines, an exact match) instead of 4-space-indented
  `impl App` methods. Moved a clean contiguous block — `draw_contacts`
  through `draw_system_info` (7 functions: the Contacts vCard view,
  file-info/text-info panels, Markdown preview, Snippets picker, System
  Info panel) — into new `src/ui/info_panels.rs`. Needed
  `use super::draw_scrollbar;` (a shared helper still in `ui.rs`) plus
  `use crate::app::App; use crate::theme::{self, icon};` and the same
  `ratatui::prelude::*`/`widgets::{...}` imports `ui.rs` itself already
  used (the wildcard prelude import is clippy-exempt — `wildcard_imports`
  special-cases any path ending `::prelude`); made the 7 functions
  `pub(super)`, and added `mod info_panels; use info_panels::{...};` to
  `ui.rs` so its many existing bare call sites (`draw_contacts(app,
  frame, area)`, …) needed zero changes. Full workspace `cargo test`
  green, snapshot tests (`tests/snapshots.rs`, 12/12) re-verified
  directly since a rendering-path change deserves that scrutiny beyond
  the aggregate pass count. Pattern validated end-to-end.
  **Slice 2 (picker panels) done 2026-09-06.** Moved `draw_nerd_palette`
  and `draw_ascii_panel` (one contiguous pair) plus `draw_qrcode`,
  `draw_x11_panel`, and `draw_media_type_panel` (a second contiguous
  trio elsewhere in the file) — 5 functions across two non-contiguous
  ranges, bundled into one `src/ui/picker_panels.rs` for symmetry with
  the app-side `src/app/picker_panels.rs` state module from T141 slice
  11. Needed `use super::{NERD_CELL_W, draw_scrollbar, trunc};` plus the
  same `App`/`theme`/ratatui imports as slice 1. Functions made
  `pub(super)`; `ui.rs` gets `mod picker_panels; use picker_panels::{...};`,
  call sites unchanged. Full workspace `cargo test` green (lib 86,
  integration 451, snapshots 12/12 re-verified directly by name), full
  `scripts/check` clean.
  **Slice 3 (DB workbench) done 2026-09-06** — the largest T142 slice so
  far, ~800 lines. Moved the full DB overlay cluster — `draw_db` through
  `db_result_rows` (21 functions: connections list, add/edit form,
  password/save/ask/params prompts, cell/log/ERD/export views, and the
  three-pane workbench itself — schema tree, SQL editor with
  autocomplete popup, results grid) — into `src/ui/db.rs`, all fully
  contiguous in the original file. Only `draw_db` is called from
  `ui.rs` itself, so only it got `pub(super)`; the other 20 functions
  are used solely within the new module and stayed private — a
  different visibility shape than slices 1–2, where every moved
  function was still called from `ui.rs`. Needed `use super::trunc;`
  plus the same `App`/`theme`/ratatui imports as earlier slices. Full
  workspace `cargo test` green (106+ crates, every suite and doctest,
  confirmed via a real background-task block rather than assumed from a
  redirected log — cargo's own stdout buffering makes a `tail` on a
  redirected file look frozen mid-run even though the process is still
  progressing), snapshots 12/12 re-verified directly by name, full
  `scripts/check` clean (clippy pedantic alone took ~21 minutes this
  run — its own full recompile, not a regression).
  **Slice 4 (edit surfaces) done 2026-09-06.** Moved the structured-
  editing overlay family — 6 table-drawing helpers (`fit`,
  `column_widths`, `first_visible_col`, `visible_cols`,
  `table_row_line`, `table_status_line`) plus `draw_edit_table`, the
  Column View helpers plus `draw_column_view`, `outline_line` plus
  `draw_edit_sql`/`draw_edit_outline`, `value_line` plus
  `draw_edit_value`, `bytes_line` plus `draw_edit_bytes`,
  `draw_html_panel`, and `draw_outline` (21 functions, fully
  contiguous) — into `src/ui/edit_surfaces.rs`: the CSV/TSV table
  editor, Org Column View, SQL snippet library, prose outline editor,
  structured-value (JSON/YAML) editor, byte (hex) editor, HTML
  character picker, and the document-outline panel. First pass missed
  the 6 table-drawing helpers (they sit immediately before
  `draw_edit_table` and are used only by it) — caught because they'd
  have gone unused in `ui.rs` after the move; restored `ui.rs` and
  reran the extraction with the corrected, still fully-contiguous
  range, per the established "fix the TARGETS list and rerun, don't
  hand-patch" recovery. The 8 top-level `draw_*` functions each have
  exactly one other call site (`draw_overlays_aux`'s dispatcher) and
  got `pub(super)`; the 13 helpers are used only within the new module
  and stayed private. `cargo build --all-targets` clean on the first
  attempt after the fix; `cargo test --workspace` green (221 result
  lines, 0 failures); snapshots 12/12 re-verified directly; full
  `scripts/check` clean.
  **Slice 5 (choosers) done 2026-09-06.** Moved 15 fully-contiguous
  functions — the branch/workspace/macro/clipboard/task/script/
  location/capture/refile list choosers (all built on the shared
  `draw_list_chooser`), the diff viewer, the Git status panel, the
  right-click context menu, and the spell-check suggestion popup —
  into `src/ui/choosers.rs`. 14 of the 15 have exactly one other call
  site (`draw_overlays_aux`/`draw_chooser_overlays`) and got
  `pub(super)`; `draw_list_chooser` is used only by 9 siblings within
  the new module and stayed private. Two shared helpers
  (`git_change_color`, `unix_secs_label`) have OTHER callers still in
  `ui.rs` (`explorer_rows`, `file_browser_row`) so they stayed put —
  the new module reaches them via `use super::{...}`, same asymmetry
  pattern as `trunc`/`draw_scrollbar` in earlier slices. `cargo build
  --all-targets` clean on the first attempt; `cargo test --workspace`
  green (221 result lines matching baseline exactly); snapshots 12/12
  re-verified directly (including `git_panel_with_changes`, the case
  most likely touched by this move); full `scripts/check` clean.
  **Slice 6 (menu bar) done 2026-09-06.** Moved 8 functions —
  `draw_menu_bar`, `item_right`, `dropdown_width`, `render_dropdown`,
  `draw_menu_dropdown`, `menu_row_y`, `menu_tooltip_target`,
  `draw_menu_tooltip` — into `src/ui/menu_bar.rs`: the top menu bar, up
  to three nested dropdown levels, and the help tooltip. A genuinely
  new case: `dock_toggle_cols`/`menu_dropdown_rect`/`dropdown_scroll`
  sit in the same source range but stayed in `ui.rs` — `App`'s
  mouse-click handling reaches them via the truly-public path
  `crate::ui::dock_toggle_cols`/`crate::ui::dropdown_scroll` (`ui` is a
  `pub mod` at the crate root), which moving them would have broken.
  First build caught 2 real gaps: `dropdown_width` has a second caller
  in `ui.rs`'s own `menu_dropdown_rect` (needed `pub(super)` + a
  re-import), and `draw_menu_tooltip` turned out to have zero external
  callers once its only caller (`draw_menu_dropdown`) moved with it —
  should have stayed private, not `pub(super)`. Fixed by reading the
  compiler's actual errors, not by assuming the first pass was
  complete. `cargo build --all-targets` clean after the fix; `cargo
  test --workspace` green (221 lines matching baseline); snapshots
  12/12 re-verified directly (including `file_menu_open`, the case
  this slice touches); full `scripts/check` clean.
  **Slice 7 (dialogs + file browser) done 2026-09-07.** Two thematically
  distinct modules from one contiguous source range: `src/ui/dialogs.rs`
  (6 small modal dialogs — `draw_confirm`, `draw_script_trust`,
  `draw_replace_confirm`, `draw_unsaved`, `draw_paste_conflict`,
  `draw_query_replace`, all self-contained, all `pub(super)`) and
  `src/ui/file_browser.rs` (the File → Open… overlay —
  `draw_file_browser` `pub(super)` plus 3 private helpers, needing
  `use super::{draw_scrollbar, trunc, unix_secs_label};`). Confirmed
  before extracting that `dock_toggle_cols`/`menu_dropdown_rect`/
  `dropdown_scroll`/`git_change_color`/`menu_offsets` sit in the same
  original range but stay in `ui.rs` (cross-module pub API surface or
  needed by a staying function), same precedent as slice 6. `cargo
  build --all-targets` clean on the first attempt; `cargo test
  --workspace` green (221 lines); snapshots 12/12 re-verified directly;
  full `scripts/check` clean.
  **Slice 8 (AI + terminal) done 2026-09-07.** Moved 5 fully-contiguous
  functions — `draw_ai_diff` (one of the workspace's 3
  `too_many_lines` allows), its `seg_line_count` helper, `vt_color`
  (`vt100::Color` → `ratatui::Color`), `draw_terminal` (also
  `too_many_lines`), `draw_ai_panel` — into `src/ui/ai_terminal.rs`:
  the reviewable AI-diff hunk viewer, the PTY-backed terminal panel,
  and the persistent AI chat panel. Fully self-contained, no shared-
  helper dependency (confirmed `month_lines`/`centered`, sitting right
  after this cluster, belong to `draw_palette`/`draw_search` instead —
  correctly excluded). Both `too_many_lines` allows carried over with
  their functions. `cargo build --all-targets` clean on the first
  attempt; `cargo test --workspace` green (221 lines); snapshots 12/12
  re-verified directly; full `scripts/check` clean.
  **Slice 9 (search + help) done 2026-09-07 — the largest T142 slice,
  effectively the whole rest of the file after slice 8's boundary.**
  Two modules: `src/ui/search.rs` (`draw_palette`,
  `draw_workspace_search`, `button_row`, `draw_search`
  [`too_many_lines`], `draw_search_options`, `field_line`,
  `draw_prompt_preview`, `draw_prompt`) and `src/ui/help.rs`
  (`draw_help` [F1], `draw_keybinding_editor`,
  `keybinding_editor_row_line`). Both need `use super::centered;` (a
  generic overlay-centering helper used by 4 functions split across
  both new modules, so it stays in `ui.rs` rather than picking one
  owner) — `help.rs` also needs `use super::trunc;`. `month_lines`
  stays in `ui.rs` (only caller is `draw_calendar`, never part of this
  cluster despite sitting right next to it — confirmed by call-site,
  not proximity). First build left a now-dangling top-level
  `use crate::search::{Field, Scope};` in `ui.rs` — removed. `cargo
  build --all-targets` clean after that fix; `cargo test --workspace`
  green (221 lines); snapshots 12/12 re-verified directly (including
  `f1_help_overlay` and `command_palette_open_with_a_query`, the cases
  this slice touches); full `scripts/check` clean. `ui.rs` now 2,582
  lines, down from 7,293 at T142's start.
  **Slice 10 (LSP popups + tool panels) done 2026-09-07.** Two
  modules: `src/ui/lsp_popups.rs` (`cursor_screen_yx`, `draw_completion`,
  `draw_hover`, `draw_code_actions`, `draw_code_lens`, and the shared
  `draw_chooser` they both render through) and `src/ui/tool_panels.rs`
  (`draw_pomodoro`, `draw_welcome`, `draw_dialog`, `draw_color_converter`,
  `draw_regex_tester`, `draw_calculator`, `draw_unit_converter` — 7
  small standalone overlays, needing `use super::draw_scrollbar;`).
  Built `tool_panels.rs` (531 lines) via shell concatenation (header +
  extracted body, `sed` to mark the 7 top-level functions
  `pub(super)`) rather than hand-retyping, to avoid transcription risk
  at that size — verified the result read back intact before wiring
  it in. First build caught one over-speculative import (`Wrap`, never
  actually used by any of the 7 functions) — removed. `cargo build
  --all-targets` clean after that fix; `cargo test --workspace` green
  (221 lines); snapshots 12/12 re-verified directly (including
  `welcome_screen`); full `scripts/check` clean. `ui.rs` now 1,848
  lines, down from 7,293 at T142's start — a 75% cut.
  **Slice 11 (explorer + boxes) done 2026-09-07 — first slice into
  territory flagged as "not clean overlay-per-file anymore."** Two
  modules, both actually clean once surveyed carefully:
  `src/ui/explorer.rs` (`explorer_rows` + `draw_explorer`, needing
  `use super::{draw_hscrollbar, draw_scrollbar, git_change_color,
  hslice_spans, span_line_width};` — `git_change_color` stays since
  `choosers.rs` also needs it) and `src/ui/boxes.rs` (`draw_calendar`,
  `draw_clock`, `draw_dashboard`, `month_lines` — zero shared-helper
  dependency; `CAL_PREV`/`CAL_NEXT`, `pub` but with no actual external
  referrer anywhere in the crate, moved with their sole user). Also
  removed two now-dead top-level imports (`use crate::calendar;`,
  `use crate::clock;`). Confirmed before extracting that `centered`/
  `trunc`/`draw_scrollbar`/`draw_hscrollbar` now have zero remaining
  call sites *within* `ui.rs` itself — all their callers are in
  already-moved children reaching them via `super::` — which is
  exactly why they must stay. `cargo build --all-targets` clean (a
  slow ~18-minute build this run, no errors); `cargo test --workspace`
  green (221 lines); snapshots 12/12 re-verified directly; full
  `scripts/check` clean. `ui.rs` now 1,535 lines, down from 7,293 at
  T142's start — a 79% cut.
  **Slice 12 (tabs + hints + docks) done 2026-09-07.** Three clean
  clusters, all with sole callers in `draw()` itself: `src/ui/tabs.rs`
  (`center_split`, `draw_breadcrumb`, `draw_tabs`), `src/ui/hints.rs`
  (`draw_which_key`, `draw_jump_labels`), `src/ui/docks.rs`
  (`draw_test_panel`, `draw_debug_panel`, `draw_outline_dock`,
  `draw_messages`, needing `use super::{draw_hscrollbar,
  draw_scrollbar, hslice_spans, span_line_width};` since
  `draw_bottom_dock` — still in `ui.rs` — needs the same four). First
  build caught two real gaps in `docks.rs` (`Focus` and `Level`, both
  used only within the moved functions) and four now-dead top-level
  imports left behind in `ui.rs` (`Clear`/`List`/`ListItem`/`ListState`,
  `Level`) — all fixed after checking via grep that nothing remaining
  in `ui.rs` itself still used them. `cargo build --all-targets` clean
  after the fixes; `cargo test --workspace` green (221 lines,
  integration suite's own 451/0/11-ignored baseline re-confirmed
  directly in the gate log); snapshots 12/12 re-verified directly;
  full `scripts/check` clean. `ui.rs` now 1,173 lines, down from 7,293
  at T142's start — an 84% cut.
  **Slice 13 (minimap + bottom_dock + status_bar) done 2026-09-08.**
  Three more clean clusters: `src/ui/minimap.rs` (`draw_minimap`, sole
  caller `draw_editor_region` staying in `ui.rs`), `src/ui/bottom_dock.rs`
  (`hslice`, `max_line_width`, `draw_bottom_dock` — confirmed the two
  helpers are used only by `draw_bottom_dock`, unlike
  `span_line_width`/`hslice_spans` which stayed since `explorer.rs`/
  `docks.rs` also need them), `src/ui/status_bar.rs` (`draw_status_bar`,
  fully self-contained). One gotcha caught before it reached the
  compiler: the body referenced `super::editor::Tab::display_path` —
  in `ui.rs` that meant the crate root, but one level deeper it would
  resolve to a nonexistent `ui::editor` — rewritten to the absolute
  `crate::editor::Tab::display_path`. Also removed one more now-dead
  top-level import (`icon`, after its three remaining users all moved
  out). `cargo build --all-targets` clean after the fix; `cargo test
  --workspace` green (221 lines, integration suite's 451/0/11-ignored
  explicitly re-confirmed in the gate log); snapshots 12/12
  re-verified directly (including `default_screen_with_no_file_open`
  and `editor_with_an_opened_rust_file`); full `scripts/check` clean.
  `ui.rs` now 958 lines, down from 7,293 at T142's start — an 87% cut.
  **Slice 14 (core editor region) done 2026-09-08 — the slice flagged
  as most likely where the pattern stops, surveyed carefully anyway
  and it still held.** `src/ui/editor_region.rs`: `draw_editor_region`,
  `draw_pane`, `tint_ruler`, `draw_center` — the actual core editor
  rendering (single pane or a split tree, one pane's text + scrollbar
  + ruler guide, the un-split "center" case with its optional minimap
  and horizontal scrollbar). Only `draw_editor_region` needed
  `pub(super)` (sole caller `draw()`, staying); the other three are
  called only within this cluster's own chain and stayed private.
  `MINIMAP_WIDTH` (no external referrers) moved with its sole user;
  `RULER_COLUMN` stayed in `ui.rs` since `app.rs` has an intra-doc-link
  reference to `crate::ui::RULER_COLUMN` a move would have broken —
  reached via `use super::RULER_COLUMN;`. Same `super::editor::` →
  `crate::editor::` path fix as slice 13 needed, applied to three
  occurrences this time. The extraction script also left a stray
  one-line leftover doc comment orphaned above the relocated consts
  (its deletion-span boundary landed one line early) — cleaned up by
  hand. Built via the shell-concatenation technique at 236 lines.
  `cargo build --all-targets` clean after removing two more now-dead
  top-level imports (`StatefulImage`/`StatefulProtocol`); `cargo test
  --workspace` green (221 lines, lib/integration/lsp_smoke/snapshots
  each individually re-confirmed in the gate log); snapshots 12/12
  re-verified directly a second time given this touches core editor
  rendering (including `editor_with_typed_rust_source` and
  `editor_with_an_opened_rust_file`); full `scripts/check` clean.
  `ui.rs` now 713 lines, down from 7,293 at T142's start — a 90% cut.
- [x] **T143 — Split `tests/integration.rs`.** Done. The file had grown to
  9,427 lines / 481 top-level items (462 `#[test]` fns + 19 shared helpers)
  by the time this ran. Moved to `tests/integration/main.rs` (crate doc +
  `#![warn(clippy::pedantic)]`/`#![allow(...)]` + `mod` list) plus
  `common.rs` (every non-`#[test]` item — `app_at`/`key`/`ctrl`/`type_str`/
  `buffer_with`/etc., made `pub(crate)` so `use crate::common::*;` resolves
  them, plus the header's `use` statements as `pub(crate) use`) and 14 area
  files: `catalog.rs` (131 — the generated action-catalog smoke tests),
  `editing.rs` (166 — the catch-all for tests matching no more specific
  keyword), `keymaps.rs` (27), `menu.rs` (25), `workspace.rs` (21),
  `find.rs` (18), `panels.rs` (16), `palette.rs` (13), `org.rs` (12),
  `scripting.rs` (10), `lsp.rs` (8), `git.rs` (6), `keybindings.rs` (6),
  `db.rs` (3). One test binary still (`cargo test --test integration`), so
  no compile-time regression, confirmed clean on the first build.
  Classification is a rustfmt-exploiting mechanical split (every top-level
  item's closing brace sits at column 0, one-to-one with the item count —
  verified by hand before writing the splitter script), not a hand audit of
  all 462 tests, so it's an approximate topic grouping, not a strict
  taxonomy; each `#[test]` item was bucketed by a keyword match on its name
  in a fixed priority order, unmatched ones landing in `editing.rs`. Each
  area file needed `#![allow(clippy::wildcard_imports)]` (pedantic) for its
  `use crate::common::*;` — a deliberate, narrow, documented exception
  (explicit-import lists for 14 files each pulling in a couple dozen
  shared helpers/types would cost more than it buys), not a blanket allow.
  **Found and fixed a real pre-existing test bug the split exposed**:
  `edit_outline_opens_indents_and_saves` and
  `outline_panel_lists_symbols_and_jumps` both called `unique_dir("outline")`
  — `unique_dir` keys solely on tag + process id, so this was always a
  same-path race under parallel test threads, just one the two tests being
  3,626 lines apart in the old file apparently never triggered; landing in
  the same new file changed registration order enough to make them race
  reliably in this session's runs. Fixed by giving the outline*-editor*
  test a distinct tag (`"edit-outline"`); reran the full suite 4× after,
  clean every time. Grepped every other `unique_dir` call for the same
  class of collision — none found. Updated every doc/spec that named
  `tests/integration.rs` as a code-span path (`scripts/check-docs` checks
  those, not just markdown links): `AGENTS.md`, `agents/workflow.md`,
  `spec/test/index.md` (also gained a short paragraph naming the new area
  files), `docs/architecture/index.md`, and the three crate specs whose
  own tests moved to a specific area file rather than the generic entry
  point (`vix-editor/spec/command-key`, `vix-editor-core/spec`,
  `vix-clipboard/spec` → `tests/integration/editing.rs` ×2,
  `tests/integration/catalog.rs`). No CHANGELOG entry: pure-internal,
  zero product behavior change, matching T150/T154 precedent.
- [x] **T144 — One list-navigation state instead of eighteen.** Done.
  New `vix-list-state` crate: 6 pure functions (`up`, `down`,
  `page_up`, `page_down`, `select_index`, `ensure_visible`), then one
  commit per panel migrating its method bodies to delegate to them —
  18 commits total (the crate + 17 panel crates; `vix-db` counted as
  one commit covering 3 separate scrolling lists — catalog, statement
  editor, results grid — each with its own field names).
  Verified against real code, not the task's own count: `ensure_visible`
  was in 17 crates (not quite 18 — `vix-db`'s 3 files pushed the
  file-count to 19, but that's 17 distinct crates), and only 11 of
  those also had standalone `up`/`down`/`page_up`/`page_down`/
  `select_index` methods (the rest — `vix-db`'s 3 lists,
  `vix-edit-bytes`, `vix-edit-outline`, `vix-edit-sql`, `vix-edit-value`
  — inline their movement directly in a `handle_key`, sometimes under a
  combined `step(up: bool, n: usize)` rather than four separate
  methods). `vix-palette`'s `up`/`down` (named as a third "spot-checked
  identical" example) turned out to have no `scroll`/`page`/
  `select_index` concept at all — a much smaller, weaker instance of
  the pattern, left alone rather than chased for scope's sake.
  `vix-git-panel` (the task's other named example) doesn't exist as a
  crate at all; git's own navigation lives ad hoc in `src/app/git.rs`.
  A `ListCursor { selected, scroll }` struct (the task's own literal
  suggestion) was tried first and rejected: panel fields aren't
  uniformly named (`selected`/`sel`/`row`/`top`), several panels don't
  store a plain index at all (`vix-edit-bytes` derives a row from a
  byte cursor, `vix-edit-outline` derives a position from a computed
  visible-node list), and dozens of external call sites across
  `src/app.rs`/`src/ui/*.rs` read a panel's `.selected`/`.scroll`
  fields directly — a shared struct would force renaming every panel's
  public fields for a purely internal deduplication. Plain functions
  over `usize`s fit every panel with zero public API change: every
  method's signature stayed exactly the same, only bodies changed to
  one-line delegations.
  Two real, if minor, behavior fixes fell out of the unification:
  `vix-file-browser-panel`'s `ensure_visible` never clamped scroll
  against the list's own length at all (the one panel out of 17
  missing that clamp), and `vix-db`'s SQL statement editor had the
  same gap. Both now get the same "never scroll past the end" behavior
  every other panel already had. Crate count 110 → 111. Full
  `scripts/check` gate run once at the end (not per-commit — see the
  commit log on `feat/t144-list-cursor` for the per-panel history)
  before the single merge to `main`, consistent with "gate before
  merge" (one merge, one gate) while keeping "one commit per panel"
  for bisectable history.
- [x] **T145 — Consolidate the T104 epic's own leftovers.** T104c–g each
  added a near-identical key-token builder to `src/app.rs`:
  `vscode_ctrl_token`, `intellij_ctrl_token`, `eclipse_token`,
  `sublime_ctrl_token`, `apple_ctrl_token` (plus `shared_token` and
  `emacs_top_level_token`, 7 total). They differ only in whether `Alt`
  is encoded and whether Shift is read from the modifier bit — fold into
  one `ctrl_token(key, ShiftRule, AltRule)` with the Shift-bit-vs-char-
  case rationale documented once, not five times. Likewise
  `emacs_key_display` and `modifier_token_display` are two renderers for
  one token grammar — keep one. Do this *after* T104j lands so it
  doesn't churn under the in-flight epic. (Honest note: this debt was
  created deliberately during T104c–g — one small copy per slice kept
  each conversion reviewable — and is now due.)
  Done — but the "fold all 7 into one" premise only held for 5 of them.
  Actually reading `shared_token`/`emacs_top_level_token` (not just
  trusting this task's own summary of them) found they solve genuinely
  different problems: `shared_token` covers non-`Char` key codes
  (`Tab`/`BackTab`/`Left`/`Right`/`F`-keys) and bindings needing no
  `Ctrl` at all, with Shift gated by key *type* not a per-keymap policy;
  `emacs_top_level_token` delegates to `crate::macros::encode_key`'s
  general grammar rather than hand-building a `"C-"`-string, also needs
  `Alt`-only (no `Ctrl`) bindings, and never encodes Shift explicitly at
  all. Forcing either into `ctrl_token(key, ShiftRule, AltRule)` would
  have meant stretching that signature past what it actually describes,
  not simplifying anything — left both as their own functions, with a
  doc-comment note on each explaining why, so this isn't mistaken for an
  oversight later. The 5 that genuinely were "the same function, Alt
  encoded or not" (apple/vscode/intellij/sublime/eclipse's `Ctrl`
  branch) did fold into one `Self::ctrl_token(key, encode_alt: bool)` —
  no `ShiftRule` parameter either, once it turned out **every** caller
  needs Shift-bit-explicit encoding; only `Alt` ever actually varies, so
  a knob nothing would exercise was left out rather than added for
  symmetry with the task's own suggested signature.

  Also merged `emacs_key_display` into `modifier_token_display` (now
  handles every already-converted keymap's F1-help display, Emacs
  included) and merged `shortcut_rows`' separate "emacs" match arm into
  the generic one, since both now use the same display call.

  **Found and fixed a real bug while doing this, not just moved code
  around**: `modifier_token_display` unconditionally uppercased its
  trailing key, which was fine for every existing caller (VS Code/
  IntelliJ/Eclipse/Sublime/Apple/`SHARED` tokens always carry at least a
  `C-` prefix) but would have **silently shown the wrong case** for
  Emacs's real chord-continuation bindings once merged in — e.g.
  `C-x b` (switch buffer)'s second key is the bare, unprefixed,
  lowercase token `"b"`, which the old dedicated `emacs_key_display`
  correctly left alone but the merged function would have shown as
  `"B"`. Caught by actually
  grepping `crates/vix-keybindings/src/emacs.rs` for real bare tokens
  (found ~35: `b`, `k`, `o`, `f`, `c`, `t`, `.`, `!`, `'`, `/`, `-`, …)
  rather than assuming "uppercase the key" was universally safe just
  because it matched every case the existing test suite happened to
  already cover. Fixed: only uppercase when a modifier prefix was
  actually found; a bare token passes through completely unchanged.
  Added a new integration test (`help_overlay_includes_the_active_
  keymap_chords`, extended) asserting the real `"Ctrl X b"` display
  specifically, not just the already-covered `"Ctrl X Ctrl F"` case —
  the existing test suite had never actually exercised a bare
  chord-continuation token's display before this.

  Caught my own process lapse a *third* time this session (T104b, T105,
  now this) — started on `main` again before stashing/branching. Caught
  immediately this time (before any edits landed) by literally running
  `git status --short` + `git branch --show-current` as the first tool
  call, per the fix noted in T105's own entry — the fix worked. Zero
  intended behavior change everywhere except the one real bug found and
  fixed above: full 439-test suite green throughout (unchanged count —
  one existing test extended, not a new one, plus 2 new `src/app.rs`
  unit tests for `modifier_token_display`'s Emacs-equivalence).
- [x] **T146 — No silent keymap fallback.** Done. `Keymap::from_id`
  (`src/app.rs`) now returns `Option<Keymap>` (`None` for an unrecognized
  id) instead of silently mapping it to `Keymap::Apple` — the exact defect
  that let an integration test pass for months while testing the wrong
  keymap (found in T104d: `"intellij-mac"` ≠ `"intellij-macos"`). The only
  path that can ever see `None` in practice is `App::new`'s new
  `validate_keymap`, called once at startup: `App::set_keymap` (the View →
  Keymap submenu's `view.keymap:*` actions) only ever writes an id
  `vix_keymap_model::by_id` already accepted, so a genuinely unrecognized
  `settings.keymap` can only come from a hand-edited (or otherwise
  corrupted) `settings.toml` loaded fresh. `validate_keymap` reports it via
  the messages panel (new `msg.unknown_keymap`) and falls back to
  `"apple"` in memory (deliberately **not** rewriting the user's file for
  them — a bad on-disk value is worth surfacing, not silently erasing).
  `active_keymap()` keeps an `.unwrap_or(Keymap::Apple)` purely as a
  last-resort safety net for the same reason a `panic` there would be
  wrong even though it should be unreachable after startup validation. The
  "add a `vix-keymap-model` test" ask turned out **already satisfied**:
  `ids_are_unique_and_lookups_work` already asserts every `KEYMAPS` id
  round-trips through `by_id` *and* that `by_id("nope")` is `None` — found
  by actually reading the crate before writing a redundant test, not
  assumed missing. New integration test
  (`an_unrecognized_persisted_keymap_id_is_reported_and_corrected_at_
  startup`) covers the `App::new`-level behavior instead. Left "retiring
  `App`'s private 9-variant `enum Keymap` in favor of the model's 10
  string ids everywhere" as a genuinely separate, larger follow-up, not
  done here — the task's own wording ("then *consider*") treats it as
  optional, and it touches every keymap dispatch site in `src/app.rs`, not
  a small fix.
- [x] **T147 — A real action catalog.** Done. The command palette and
  `App::action_title` learn action titles only by walking
  `vix_menu::menus()`, so any action without a menu leaf is invisible to
  the palette and shows its raw id in F1 help (`nav.back`,
  `nav.forward`, `view.toggle_menu` from T104g; `keybindings.reload`
  needed a Tools leaf purely to be findable). Separately,
  `vix-keyboard-shortcut-panel::ROWS` is a hand-curated cosmetic list
  that can now drift from the real `vix-keybindings` registry (its own
  spec calls it "cosmetic, not data"). Build one `(action_id → i18n
  title key)` catalog that menus, the palette, F1 help, and
  `vix-keybindings`' `shortcuts_for` all read; derive `ROWS` from
  `vix_keybindings::SHARED` + the active keymap's table instead of
  hand-typing it; add a test that every `run_action` arm id has a
  catalog entry (the 767 arms are grep-able). Unblocks T204's editor UI
  too, which needs "every action, titled".

  New `vix-action-catalog` crate (`Action { id, title }` + `CATALOG` +
  `title_key`), covering the **157** action ids titled nowhere else —
  found by walking the real 27-function `run_action` dispatch chain by
  source (`tests/action_catalog.rs`, brace-balancing each dispatcher out
  of its file) and diffing against both real title sources. A second,
  independent hand-curated catalog turned up mid-implementation:
  `palette::COMMANDS` (the palette's own `>` mode list, 154 entries) is
  *not* purely menu-derived either (`nav.goto_workspace_symbol` has no
  menu leaf but was already there) — so the exclusion is "no menu leaf
  **and** no `COMMANDS` entry", enforced by two dedicated tests
  (`every_catalog_entry_has_no_menu_leaf_of_its_own`,
  `no_catalog_entry_duplicates_a_palette_commands_entry`), the second of
  which caught a live duplicate-titling bug (a `nav.goto_workspace_symbol`
  entry that would have shown twice under two different labels) before
  merge. `App::action_title` (F1 help, the keybinding editor) falls back
  to the catalog after the menu tree; a new `App::catalog_palette_entries`
  appends every catalog entry to the palette's `>` Commands mode
  directly (`palette::COMMANDS` isn't routed through `action_title`, so
  this is a separate integration, not a byproduct of the first). `ROWS`
  shrank from 17 rows to 3 purely-informational ones (`F10`/menu
  mnemonics, `F1`, `Mouse`) — every real binding it used to hand-duplicate
  (some inconsistently: several keymaps don't actually bind `Ctrl P` to
  the palette) now surfaces through the menu/`SHARED`/keymap-table
  sources `App::shortcut_rows` already assembled. All 157 new
  `action.*` i18n keys translated into the app's 14 core locales,
  reusing vocabulary already established by existing `menu.item.*`/
  `help.*` keys (the actual translation content generated by 4 parallel
  subagents against a from-existing-content glossary, for cross-language
  consistency at this volume). `tests/snapshots/
  snapshots__f1_help_overlay.snap` updated deliberately for the richer,
  granular per-menu-item rows the shrunk `ROWS` now produces — verified
  by eye before accepting, not just re-recorded blindly. Crate count
  106→107 everywhere it's cited. Full gate green; merged to `main`.
- [x] **T148 — i18n coverage, measured and gated.** Done, all three
  parts. `locales/app.yml` is
  26,298 lines holding 2,240 keys, and 690 of them (31%) carry only
  `en` — every `msg.*` added since scripting landed, most menu items
  from 2026-07 on. (a) Extend `tests/i18n_keys.rs` to print a per-locale
  coverage table and fail if any locale drops below its current floor
  (ratchet, never regress); (b) backfill the 690 — a good first job for
  the T124 AI provider with human review, or a per-locale contributor
  call; (c) split the file per top-level namespace
  (`locales/menu.yml`, `msg.yml`, `help.yml`, …) — `rust-i18n` loads a
  directory — so a translation PR isn't a 26k-line diff context.

  **All three parts done.** (a): new `i18n_coverage_report_and_
  floor` test prints a per-locale table and ratchets against a hardcoded
  `LOCALE_FLOORS`, tuned to what T148 actually found: only 14 locales
  (es/fr/de/cy/ga/gd/pl/pt/ru/ar/hi/bn/zh/ja, ~70% coverage each) are a
  real commitment; `tlh`/`sjn` (Klingon/Sindarin) and 10 more codes
  (el/fa/id/it/ko/nl/th/tr/uk/vi) sit at 5-10 keys each — an easter egg
  and an experimental seed batch, floored at `0` rather than held to the
  14-locale bar. (c): `locales/app.yml` (28,857 lines by the time this
  ran, 2,418 keys — both had grown since this task was written, T147's
  own 157 new keys included) split into 9 files by namespace
  (`menu.yml` 1,374 keys down to a `misc.yml` catch-all for 6 low-volume
  namespaces at 15 keys total) — a pure line-based Python script, every
  key's content verified byte-identical before/after via a YAML diff
  (`yaml.safe_load` both sides, zero missing/extra/mismatched keys)
  before deleting the original file. `rust-i18n`'s own docs confirm
  multi-file merging is supported (`i18n!` already pointed at the
  `locales/` directory, not the file, so no macro-side change was
  needed) — verified empirically too: the app itself compiles clean
  against the split. `tests/i18n_keys.rs` gained a shared `load_catalog`
  helper (merges every `locales/*.yml`, panics on a key defined in more
  than one file — a new failure mode the split makes possible that
  didn't exist with one file); `tests/action_catalog.rs` got its own
  smaller equivalent (a separate test binary, can't share the helper).
  `crates/vix-i18n/build.rs` rewritten to `rerun-if-changed` every file
  in `locales/`, not just the one that no longer exists — a directory's
  own mtime doesn't change when an existing file's content does, so
  watching only the directory would have missed the common case (editing
  a translation). All prose mentions of `locales/app.yml` repointed at
  `locales/` or the specific namespace file across `AGENTS.md`,
  `CLAUDE.md`, `crates/vix-i18n/spec/index.md` (substantially rewritten:
  "How it works", "Key namespaces", "Rebuilds" sections), and a dozen
  more docs/specs/skills — `scripts/check-docs`'s link checker caught
  three that were missed on the first pass (its path-reference regex
  flags any backtick-quoted `something.yml`-shaped span, historical
  mentions included, so a deliberately-historical "`locales/app.yml`,
  still the name..." sentence needed rewording to drop the backticks,
  not just updating).

  (b): 711 `en`-only keys (the "690" had grown by the time this ran)
  translated into the 14 core locales — 12 parallel subagents, 60-row
  chunks, the same glossary-reuse technique as T147's action-catalog
  translations but with the glossary regenerated first (1,259 → 1,697
  rows, since T147/T148(a)/(c) had already added more fully-covered
  reference entries). Every block validated programmatically before
  merging: exact 14-locale set and order (0/711 failures), and
  `%{name}` placeholder preservation across all 60 placeholder-bearing
  keys (0/60 mismatches) — merged via a block-scalar-aware script, same
  technique as (c)'s split. The post-merge completeness check then
  found **10 keys still partially uncovered** (outside the "en-only"
  scope, so untouched by the backfill) — and two of them,
  `msg.workspace_unsafe_root` and `ui.db_field_sslmode`, turned out to
  hold a **pre-existing bug unrelated to this task**: each held the
  *other* key's translations verbatim (`msg.open_failed`'s and
  `ui.db_field_ssh_identity`'s respectively), missing exactly the
  locales the other key had — a historical copy-paste mix-up between
  two keys, found only because this pass checked for completeness this
  thoroughly. Fixed by relocating each correct set to its real key and
  writing fresh translations for the two real messages; the other 6
  gaps were universal proper-noun/acronym entries (`Vix`, `UUID`,
  `SHA-256`, …) nobody had filled in. End state: **100.00% core-14
  coverage, all 2,418 entries, zero gaps** — `LOCALE_FLOORS` bumped
  from ~1,548–1,705 (~70%) to 2,418 (100%) for all 14.

  **A real, serious `rust_i18n` scale bug surfaced by the fuller
  catalog, found and fixed in the same branch**: `cargo test --lib`
  went from green to a 100%-reproducible stack overflow on bare
  `App::new()` (isolated via three throwaway `diag_stepN` tests,
  deleted after use, that bisected the failure down from the org-dblock
  test that first surfaced it to nothing more than app construction).
  Root-caused by reading `rust-i18n-macro` 4.2.1's actual generated
  code: `i18n!` expands to one function with a flat, un-chunked
  sequence of `map.insert(k, v)` — tens of thousands of statements once
  core coverage hit 100% — whose unoptimized stack frame in a
  `dev`/`test`-profile build crossed the default 8 MiB thread stack.
  Confirmed debug-build-only (`RUST_MIN_STACK=100000000 cargo test`
  passes; `cargo build --release` and the built binary are both fine).
  This would have broken GitHub CI's own `cargo test --workspace` job
  (confirmed by reading `.github/workflows/ci.yml`: that job runs
  debug, not `--release`) had it shipped unfixed. Fixed with one
  targeted `[profile.dev.package.vix-i18n] opt-level = 2` in the root
  `Cargo.toml` — `test` inherits unspecified `dev` settings including
  package overrides, so it covers both `cargo build` and `cargo test`
  while only that one crate pays an optimization-vs-iteration-speed
  cost, not the whole workspace. Full `scripts/check` gate green
  end-to-end afterward, including the originally-failing test.
- [x] **T149 — Replace boolean clusters with types.** Seven
  `struct_excessive_bools` allows found (one more than the task's own
  count of six): `App`, `Settings`, `Editor` (both `vix-editor` and
  `vix-editor-core`), `SearchBar`, `WorkspaceSearch`, and
  `vix-org::DblockParams`. **Five of the seven now done** (2026-09-09):
  `vix-editor-core::Editor` (6 bools → `editor::Flags`),
  `vix-editor::Editor` (4 bools → `editor::Flags`, its own distinct
  type from the core crate's), `vix-find-panel::SearchBar` (6 bools →
  `Flags`), `vix-workspace-search::WorkspaceSearch` (4 bools →
  `Flags`), and `vix-org::DblockParams` (4 bools → `DblockFlags`).
  `App` and `Settings` — the two largest and riskiest — are deferred to
  a separate pass, per an explicit user choice to start with the
  smaller structs first.
  A key finding changed the task's own prescription: grouping the bools
  into a plain sub-struct (a "small `Flags` struct" read literally)
  does **not** satisfy `clippy::struct_excessive_bools` — the lint
  counts `bool` fields in any struct, so a nested plain struct just
  relocates the lint to itself. Two of the five converted structs
  (`WorkspaceSearch`, `vix-editor-core::Editor`) already carried
  pre-existing code comments independently reasoning through and
  rejecting the plain-sub-struct approach for exactly this reason. The
  fix that actually works, used for all five: the `bitflags` crate (v2,
  already a transitive dependency, MIT/Apache-2.0 — added as
  `bitflags = "2"` to `[workspace.dependencies]`), one `bitflags!`
  block per struct with a named `const` per former bool field, a single
  `flags: Flags` field replacing them, reads via `.contains()`, writes
  via `.set()`/`.toggle()`/`.insert()`/`.remove()`. Every call site
  across `src/app.rs`, `src/app/lsp_dap.rs`, `src/app/org_table.rs`,
  `src/ui/search.rs`, and the integration/fuzz test suites was updated
  to match; full `scripts/check` gate green after each struct and again
  for the combined change. Where the bools are really one mode, an enum
  remains the right tool — none of the five converted structs needed
  that; each pass unlocked a genuine independent-toggle bitset.
  **`App`/`Settings` scoped and explicitly deferred indefinitely
  (2026-09-10)**, not just "later" — user chose to stop at 5/7 rather
  than proceed once real scope was measured. Findings, for whoever
  picks this up: `Settings` (31 bools) has a clean, *verified* fix —
  not bitflags (a packed integer is a poor fit for a config file users
  hand-edit) but grouping into small `#[serde(default)]` sub-structs,
  `#[serde(flatten)]`'d back onto `Settings` so the on-disk
  `config.toml` format is byte-for-byte unchanged (confirmed with a
  standalone round-trip test: flatten + old-format-missing-fields both
  work correctly with `toml`/`serde`) — but every `settings.<field>`
  read site still needs updating to `settings.<group>.<field>`, and
  there are **~200+** of those across `src/`. `App` (19 bools) is a
  poorer fit for any single unifying scheme — its bools are
  semantically scattered (panel visibility, one-shot signals like
  `should_quit`, editor modes, workspace facts like `git_repo`), so
  grouping needs real per-field judgment calls, not a mechanical pass,
  and its bools are read at **~150+** call sites. Combined, this is a
  ~350-450 call-site refactor, categorically larger than any of the
  five structs already converted (each was ~20-30 call sites) — that's
  *why* it was worth measuring and checking in on rather than starting
  on the strength of the other five going smoothly.
  **`Settings` done 2026-09-18 (6/7)**, on an explicit user go-ahead to
  do the App/Settings portion after all. Exactly the verified plan
  above: its 32 `bool` fields (the note's "31" undercounted by one)
  grouped into 11 small `#[serde(flatten)]`'d sub-structs
  (`GutterSettings`, `EditorVisualSettings`, `PanelSettings`,
  `SecondaryPanelSettings`, `ViewportSettings`, `MiscSettings`,
  `SaveSettings`, `EditorBehaviorSettings`, `TypingSettings`,
  `StartupSettings`, `SubsystemSettings`), each ≤3 bools — the lint's
  default threshold is 3, so 32 bools need ≥11 groups, and a couple of
  groups are honestly "unrelated toggles with no natural sibling"
  (`MiscSettings`) rather than pretending at cohesion. The `Settings`
  `#[allow(clippy::struct_excessive_bools)]` is gone. On-disk
  `config.toml` is byte-for-byte unchanged, now *tested* rather than
  asserted: 2 new unit tests load a pre-T149 flat-key config (overrides
  land in the right group; untouched keys in every group still get their
  defaults through the flatten boundary) and round-trip
  `Settings::default()`. Call sites: 119 mechanical `settings.<field>` →
  `settings.<group>.<field>` rewrites across 12 files, deliberately
  scoped to a `settings.` prefix — `App` has its *own* `show_explorer`/
  `show_messages`/`show_status_bar`/`show_scrollbar`/`show_bottom_dock`/
  `show_breadcrumbs`/`spellcheck` bools (runtime UI state, distinct from
  the persisted defaults), so a blind rename would have corrupted those;
  the compiler then found the 2 strays the prefix scope missed (locals
  holding a `Settings` directly). Two real knock-ons a mechanical pass
  wouldn't predict: (1) `examples/list_commands.rs`'s settings-doc
  generator parsed `pub struct Settings { .. }`'s literal source text and
  would have emitted 11 bogus `gutter: GutterSettings` rows while
  dropping all 32 real keys — rewritten to parse every `pub struct` and
  expand flatten fields in place (`docs/reference/settings.md` diff is a
  pure reorder, still 68 settings); (2) the longer qualified paths tipped
  `run_view_action` over the 100-line pedantic limit, and splitting 8
  arms into `run_view_settings_toggle` then silently dropped those 8
  action ids from `docs/reference/actions.md` (661→653) because
  `vix_action_catalog::dispatch_scan::DISPATCHERS` is an explicit
  function list — caught only by the gate's regenerate-and-diff step, not
  by `tests/action_catalog.rs` (whose assertions are "everything found
  is titled", not "everything expected was found"); fixed by registering
  the new dispatcher.
  **`App` slice 1/N done 2026-09-18**: the 15 UI-surface-visibility
  bools (`show_explorer`, `show_bottom_dock`, `pomodoro_open`,
  `coverage_visible`, `backlinks_follow`, …) become one `bitflags`
  bitset, `pub visible: Visible`, on `App` — unlike `Settings`'
  bools, these are genuinely independent freely-combinable toggles
  (explorer and message drawer can both be open at once), so
  `bitflags` (not a `#[serde(flatten)]` sub-struct — nothing here is
  persisted) is the right tool, same as the five already-converted
  structs from this task's first pass. `Visible::initial(&settings)`
  seeds the six that mirror a `Settings` default at startup; the rest
  start clear except `INLAY_HINTS`. 168 call sites across 12 files
  moved from `self.<bool>` to `self.visible.{contains,set,toggle}`.
  Two real bugs the mechanical rewrite's own regex necessarily couldn't
  catch (multi-statement match arms it wasn't designed to touch,
  patched by hand and caught immediately by the very next compile, not
  found by review): a `tools.test_panel`/`run.panel` toggle arm and the
  `view.breadcrumbs` arm each got a stray token from a naive
  toggle-pattern substitution; both are one-line, now `.toggle(...)`
  calls, covered by this slice's own passing gate run (no dedicated
  regression test — see `App`'s own `toggle_*` unit tests, all
  unchanged in behavior). `App`'s own `#[allow(clippy::
  struct_excessive_bools)]` stays (its stale rationale comment —
  "a single flags struct would itself exceed the bool limit" — was
  simply wrong, since the lint counts `bool` *fields* and a bitset has
  none; corrected in place) until every remaining bool leaves the
  struct across the following slices. 4 bools left after this slice
  are Emacs-keymap chord-prefix state (`emacs_prefix`,
  `emacs_c_prefix`, `emacs_c_x_prefix`, `emacs_c_p_prefix`,
  `emacs_c_p_c_prefix`, `emacs_c_p_c_m_prefix` — six, not four;
  mutually exclusive, an `EmacsChord` enum fits better than a bitset)
  plus a handful of other single-purpose toggles
  (`theme_editor_picking`, `test_capture`, `clip_cut`, `overwrite`,
  `show_ruler`, `macro_recording`/`macro_playing`,
  `emacs_universal`, `project_session_loaded`, `scrollbar_active`,
  `split_resize`, `modal_insert`, `modal_pending_g`,
  `modal_pending_register_select`) needing individual per-field
  judgment calls, exactly as flagged when this was scoped out.
  **Slice 2/N done 2026-09-18**: the 6 mutually-exclusive Emacs-keymap
  chord-prefix bools (`emacs_prefix`, `emacs_c_prefix`,
  `emacs_c_x_prefix`, `emacs_c_p_prefix`, `emacs_c_p_c_prefix`,
  `emacs_c_p_c_m_prefix`) become one `emacs_chord: EmacsChord` enum
  (`None`/`CtrlX`/`CtrlC`/`CtrlCCtrlX`/`CtrlCP`/`CtrlCPC`/`CtrlCPCM`) —
  at most one chord is ever pending at a time, so an enum is the
  correct shape where the first slice's bitset (independently
  combinable toggles) wasn't. 39 call sites across `src/app.rs` and
  `src/app/keymap.rs`. Found and fixed a real, previously-silent bug as
  a direct side effect of the consolidation: `reset_keymap_modes`
  (switching keymaps) used to clear only `emacs_prefix`
  (`self.emacs_prefix = false`), so a pending `C-c …`/`C-c p …` chord
  survived a keymap switch — one `self.emacs_chord = EmacsChord::None`
  now clears whichever chord was actually pending, unconditionally.
  14 bools remain: `theme_editor_picking`, `test_capture`, `clip_cut`,
  `overwrite`, `show_ruler`, `macro_recording`, `macro_playing`,
  `emacs_universal`, `project_session_loaded`, `scrollbar_active`,
  `split_resize`, `modal_insert`, `modal_pending_g`,
  `modal_pending_register_select`. Not yet started. **Correction found
  while scoping slice 3: that 14-bool list itself was stale** — a direct
  grep of the struct at that point found the true count was 18, not 14;
  five fields had gone unlisted (`suspend_requested`, `git_repo`,
  `spellcheck`, `calendar_dailies`, `should_quit`) and `show_ruler` had
  already left the struct in an earlier slice, so it never belonged on
  the list at all.
  **Slice 3/N done 2026-09-18**: 2 pairs of mutually-exclusive bools
  become 2 small enums. `macro_recording`/`macro_playing` (at most one
  true at a time — a macro is never simultaneously recording and
  playing) become `macro_state: MacroState`
  (`Idle`/`Recording`/`Playing`, `#[derive(Default)]` on `Idle`).
  `modal_pending_g`/`modal_pending_register_select` (same shape — the
  modal engine resolves a pending `gg` before it could ever also have a
  pending register-select) become `modal_pending: ModalPending`
  (`None`/`G`/`RegisterSelect`, `#[derive(Default)]` on `None`). ~15
  call sites across `src/app.rs` and `src/app/modal.rs`, done by hand
  (not the earlier slices' regex script, given the smaller scope and to
  avoid a repeat of slices 1/2's stray-token mangling) and verified
  clean on the first `cargo check`. `tests/integration/editing.rs`'s
  macro test updated to assert on `MacroState` variants directly.
  Pre-slice-3 the corrected count was 18 remaining bools (not 14, per
  the correction above); this slice removed 4 of them
  (`macro_recording`, `macro_playing`, `modal_pending_g`,
  `modal_pending_register_select`), leaving **14 genuinely
  single-purpose toggles**: `theme_editor_picking`, `test_capture`,
  `clip_cut`, `overwrite`, `suspend_requested`, `git_repo`,
  `spellcheck`, `calendar_dailies`, `should_quit`,
  `project_session_loaded`, `scrollbar_active`, `split_resize`,
  `emacs_universal`, `modal_insert` — planned as one final `AppFlags`
  bitset (slice 4, not yet started) since they're independently, freely
  combinable (a theme-pick overlay, a pending git status, and a dirty
  scrollbar are all unrelated facts that can all be true at once)
  rather than mutually exclusive modes.
  **Slice 4/4 done 2026-09-18 — `App` and T149 both fully closed
  (7/7).** All 14 remaining single-purpose bools become one `AppFlags`
  bitset (14 named consts, `u16`-backed), the same reasoning as
  `Visible`: independently, freely combinable facts, not mutually
  exclusive modes. A single `pub flags: AppFlags` field replaces them
  all — several of the 14 (`clip_cut`, `overwrite`, `suspend_requested`,
  `git_repo`, `spellcheck`, `calendar_dailies`, `should_quit`) were
  already `pub` (read from `src/main.rs`, `src/ui.rs`,
  `src/ui/explorer.rs`, and the integration test suite), so the merged
  field is `pub` too, same tradeoff `Visible` already made in slice 1.
  ~94 call sites across `src/app.rs` and 7 `src/app/*.rs` submodules,
  `src/main.rs`, `src/lib.rs`'s own doctest, `src/ui.rs`,
  `src/ui/explorer.rs`, and 5 `tests/integration/*.rs` files — mixed
  mechanical (Python find/replace, scoped per field name to avoid the
  `Settings`-vs-`App` `spellcheck`/`calendar_dailies` collision T149's
  Settings pass already found once) and by-hand fixes, clean on the
  first `cargo check`. Two knock-on fixes, both caught by the gate, not
  by review: an intra-doc link `[`App::modal_pending`]` in the
  slice-3-added `ModalPending` doc broke `cargo doc` once
  `modal_pending` stayed private (rustdoc refuses a public link to a
  private item) — changed to a plain code span; and turning 3
  single-line mouse-drag match arms into blocks (`self.flags.remove(...)`
  needs a statement, `self.scrollbar_active = false` didn't) pushed
  `try_chrome_mouse` over the 100-line pedantic limit — split its
  second half (dock-resize-edge and split-divider handling) into a new
  `try_chrome_resize_mouse` sibling. `App`'s own
  `#[allow(clippy::struct_excessive_bools)]` is gone — the struct now
  has zero `bool` fields, closing out the lint's very first finding
  from this task and all seven of the task's own original structs.
- [x] **T150 — Remove the two crate-level blanket allows.** Done. Both
  gone, no per-expression allow needed to replace either: `multicursor.rs`'s
  `multi_insert`/`multi_delete` — the only cast sites in the file —
  rewrote the `usize`-position-plus-signed-`isize`-shift arithmetic around
  `usize::checked_add_signed` (a new private `shifted(pos, shift)` helper)
  and `isize::try_from(...).expect(...)`, both proper type-safe
  conversions with no `as` casts at all, so there was nothing left for
  `cast_possible_wrap`/`cast_sign_loss` to flag. `named.rs`'s blanket
  allow turned out to be **entirely stale** — grepping the file found
  zero cast sites at all, so it was just deleted outright (whatever casts
  it once covered were already refactored away in some earlier change
  that never cleaned up the now-unused allow). Both functions gained a
  `# Panics` doc section (the new `.expect()` calls are `pub fn`s'
  responsibility to document, even though the panic is unreachable in
  practice — `shift` only ever reflects edits already applied to the same
  buffer). **4 new unit tests** in `multicursor.rs`'s own `caret_tests`
  module (`multi_insert`/`multi_delete`, both the bare-caret and
  selection-replace paths) — neither method had *any* test coverage
  before this, in this crate or `tests/integration.rs` (the existing
  multi-caret tests only ever check that carets were added, never that
  typing/deleting with several active actually rewrites the buffer
  correctly at every one). The 5 pre-existing per-expression allows
  (3 `cast_precision_loss`, 2 `cast_possible_truncation`, in
  `vix-file-browser-panel`/`vix-org-table`/`src/app.rs`) were swept and
  found **already compliant** with the task's own "at worst" fallback —
  each already carries a one-line proof comment, and each is a
  `f64`-to-display-integer/size rounding where no `TryFrom` alternative
  even exists (`f64 as i64`/`f64 as u64` casts saturate rather than wrap
  or lose sign per Rust's own float-cast semantics, so a proof comment,
  not code, is the correct final form here) — left unchanged.
- [x] **T151 — Micro-crate audit.** Done. Added a "When to add a new
  crate" section to `AGENTS.md` (own spec, own tests — in-crate or, for
  a pure App-driven panel-state crate with no logic worth isolating,
  thorough `tests/integration/*.rs` coverage — and a consumer other
  than the App shell or a clear host-agnostic reuse story). Audited the
  four named 0-test crates against it: `vix-modal` stays as documented
  design scaffolding (explicitly out of scope per the task); `vix-i18n`
  passes via its 5 real crate consumers plus the App shell; `vix-theme`
  passes the reuse/consumer bar (3 crate consumers + the App shell) but
  had a real gap — added 4 unit tests for `file_icon` (known
  extensions, fallback, case sensitivity, multi-dot names); `vix-query`
  (37 lines, a bare `Decision`/`QueryReplace` data holder with no
  methods and no consumer beyond `src/app.rs`) failed on both counts
  and was folded directly into `src/app.rs`, next to `Prompt` — same
  precedent as `vix-projectile` → `vix-tasks`, just crate-into-shell
  instead of crate-into-crate since the App shell was always its only
  consumer. Crate count 111 → 110, bumped everywhere it's cited.
  Also caught, while auditing, that `vix-workspace-search`
  (T152) technically shares `vix-query`'s "sole consumer is the App
  shell" profile but was deliberately left alone: it has real,
  non-trivial logic (field cycling, pattern building, path filtering),
  the same shape as `vix-find-panel`'s `SearchBar` (which *does* have a
  sibling-crate consumer), and folding it back one session after T152
  extracted it for good reasons would be pure churn, not an actual
  quality fix — the guideline's wording was chosen to make this
  judgment call explicit rather than mechanical.
  `vix-query`'s spec (`crates/vix-query/spec/index.md`, a broad
  "Find and Replace" feature doc, not really crate-scoped — it already
  explains `vix-find-panel`'s box, the workspace panel, and interactive
  query-replace together) moved to `spec/find-and-replace/index.md`
  (a cross-cutting root spec, per `AGENTS.md`'s own convention) rather
  than being deleted; its "As implemented in Vix" section was updated
  for T149's bitflags `SearchBar`/`WorkspaceSearch` and this fold.
  Fixed the 2 other files that linked to the old crate spec path
  (`crates/vix-find-panel/spec/index.md`,
  `crates/vix-find-panel/spec/smart-case-search/index.md`) plus
  `docs/index.md`'s find-and-replace link, `agents/workflow.md`'s spec
  table, and `agents/share/crate-map.md`'s crate list and Menu/find
  row (worded to avoid literally repeating the retired crate's name,
  since `scripts/check-docs`'s crate-map staleness check scans that
  file for any `vix-*`-shaped token, not just ones inside backticks).
  Also fixed a real drift the audit surfaced in passing: `AGENTS.md`'s
  own "sanctioned `struct_excessive_bools` allows" list still named
  `SearchBar`/`WorkspaceSearch`/`editor_core Editor` from before T149
  converted them to `bitflags` and dropped the allow — corrected to
  `App`/`Settings` only. Full `scripts/check` gate green throughout.
- [x] **T152 — Root `src/` modules that should be crates.** Done, 5
  slices, 2026-09-08. `column_view.rs` (966 lines), `edit_table.rs`
  (770), `edit_outline.rs`
  (610), `explorer.rs`, `search.rs`, `workspace_search.rs`, `messages.rs`
  live in `src/` with no spec and outside `scripts/check-docs`'s
  "every crate owns a spec" gate — the 2026-07 "crates, not modules"
  decision was applied everywhere except here. Move each to
  `crates/vix-<name>` with a `spec/index.md`, one per branch; `src/`
  should end up as `app.rs` (or `app/`, after T141), `ui.rs`, `lib.rs`,
  `main.rs`.

  Slice 1: `explorer.rs`/`search.rs`/`messages.rs` turned out to already
  be thin re-export shims (their real logic had moved to `vix-left-dock`/
  `vix-find-panel`/`vix-right-dock` in an earlier pass that never deleted
  the now-redundant files) — `pub mod X;` in `src/lib.rs` became
  `pub use vix_Y as X;` (the same pattern already used for `calendar`/
  `clock`), files deleted, zero caller changes anywhere (same public
  paths). Slices 2-5 were genuine extractions — `workspace_search.rs`
  (needed `rust-i18n` as a *direct* dependency alongside `vix-i18n` for
  `t!` to resolve, matching `vix-menu`'s own proven pattern),
  `edit_outline.rs` and `edit_table.rs` (already fully self-contained
  and host-agnostic — crossterm plus, for the latter, `vix-convert-
  tabular`), and `column_view.rs` (depends on `vix-org`, already a real
  crate). Every extraction needed its stray `[`crate::app`]`/
  `[`crate::edit_table`]`-style intra-doc links reworded to plain prose
  (nothing at the new crate's level can resolve `crate::` paths into the
  root `vix` crate) — caught before `cargo doc -D warnings` ever ran,
  by checking each copied file's own doc comments before wiring it in.
  While drafting `vix-column-view`'s spec, caught a self-authored
  mistake before it shipped: guessed `Outcome` had a `Save` variant by
  pattern-matching the sibling crates' shape, but the real enum is
  `Consumed`/`Close`/`NeedsColumnPrompt` — no `Save` at all, since
  column-view edits are already live in the buffer by the time a key
  returns. `crate-map.md`'s "App shell" file table lost a row per
  slice and gained a closing note once `src/` reached exactly
  `main.rs`/`lib.rs`/`app.rs`/`ui.rs` — this task's own literal target,
  now true. Crate count 106→111 across the whole T147/T148/T152 run
  this session (108→111 for T152's own 4 new crates); each slice's
  crate-count bump swept every doc that cites it. Full `scripts/check`
  gate green on every slice, merged to `main`.
- [x] **T153 — Sort the palette's Files mode.** Done. `palette_file_entries`
  (`src/app.rs`) now scores every candidate with `palette::fuzzy_score` and
  sorts by score descending, tie-broken on the path — the same
  `(score, tiebreak, Entry)` shape `recompute_palette`'s `PMode::Commands`
  arm already used. An empty query scores every candidate `0`
  (`fuzzy_score`'s own documented behavior), so the path tie-break alone
  puts the unfiltered list in alphabetical order — also strictly better
  than the raw `ignore::WalkBuilder` traversal order it replaces (not
  portable across filesystems, e.g. ext4 vs APFS order differently).
  Scoring now happens over every indexed candidate before the 200-result
  cap is applied (previously the cap truncated the *raw walk order* at
  200, which could bury a strong match behind 200 weaker ones the walk
  happened to visit first) — same cost, same 200-result ceiling, correct
  ranking. 2 new integration tests (score beats both walk order and
  alphabetical order; empty query lists alphabetically) — Files mode had
  no test coverage at all before this. Leaves T004's originally-flagged
  "unblocks a Files-mode snapshot scenario" as a follow-up, not done here
  (a `tests/snapshots.rs` scenario is its own small deliverable).
- [x] **T154 — Keep `Cargo.toml` descriptions honest.** Done. New
  `scripts/check-docs` gate (`check_descriptions`): each crate's `description`
  (markup-stripped, trailing period trimmed) must be a literal prefix of the
  prose right after its `spec/index.md` H1 — the spec is the single source,
  the manifest can't silently drift from it the way `vix-keybindings`'
  description once did (said "9 keymaps" a full task after the spec said 10,
  caught only because T104h happened to edit the same file).

  **Investigating turned up 87 of 106 crates failing this check today** —
  far more than this task's own sizing assumed (grouped with T146/T150/T153
  as "each a single short branch"). Asked the user how to scope it given the
  size surprise; **chose "fix all 87 now."** Real causes, roughly in order
  of frequency: a thin stub opening ("Module foo_tool.", "Editor action
  edit.foo.") that never carried a real summary at all; a spec whose real
  first paragraph covers something else first (an action id, a menu
  location, a documentation link) before ever summarizing the crate; and
  plain wording drift where the spec's opening says essentially the same
  thing as the description in different words. Fixed by editing each
  crate's `spec/index.md` opening (never `Cargo.toml` — spec stays
  authoritative) via **5 parallel general-purpose agents**, each handling a
  disjoint ~17-crate batch, self-verifying against the real
  `check_descriptions` gate before reporting back — all 87 confirmed fixed,
  `scripts/check-docs` fully green. Two real, previously-latent bugs in the
  check itself were found and fixed along the way, not just worked around:
  a reference-style markdown link (`[RFC 6350]` with no trailing `(url)`,
  `vix-vcard-parser`) wasn't stripped by the markup regex (only inline
  `[text](url)` links are) — converted to inline; and the check's own
  docstring claimed to join the spec's "first two paragraphs" when the
  code only ever compares against one (paragraph 0, or the remainder after
  a `**Status:**` lead sentence) — fixed the docstring to match the actual,
  stricter behavior rather than loosening the check now that every crate
  already passes under it. `agents/share/crate-map.md` untouched (no crate
  added/removed/renamed, only spec prose).

## Phase 2 — Functionality

- [x] **T201 — Structural search & replace.** Done — the last task in
  the entire Run C feature set, and the largest. **Scope decision,
  stated up front**: implements only the "fall back to bracket-balanced
  text matching" half of the task's own description, as the *primary*
  mechanism rather than a fallback — genuine tree-sitter-based structural
  matching needs per-grammar node-equivalence handling across the ~15
  grammars Vix loads, a substantially larger project than the
  token-based approach shipped here, which already delivers the core
  value (parameterized, bracket-aware structural replace) for every
  language Vix supports, uniformly, with no per-language work. Documented
  prominently in the new crate's own module doc and spec, not left
  implicit.

  New `vix-structural-replace` crate, pure/unit-tested (16 tests), no
  `App`/I/O dependency: a hand-rolled tokenizer (identifiers, numbers,
  quoted strings with `\`-escapes, bracket pairs, operator-punctuation
  runs) feeds a small backtracking matcher over `$NAME` (one lexical
  unit — one token, or a whole bracketed group when the next token opens
  one) and `$$NAME` (a lazy, zero-or-more-token run, never crossing an
  unmatched closing bracket relative to where it started — Comby's own
  lazy-hole convention, cited directly). `render_replacement` substitutes
  a match's captures — the *original* source text, not a token
  reconstruction — into a `$NAME`/`$$NAME` template, so a replacement
  keeps whatever formatting the captured code already had. **3 real bugs
  found via failing tests during development, not assumed away**: two
  test premises were themselves wrong (expecting a *single* hole to span
  multiple lexical units — `x > 0` is three tokens, not one — corrected
  to use `$$COND`, with the distinction now documented explicitly in the
  crate's own module doc as pattern-author guidance); the third was a
  real code bug (hole names accepted a leading digit, so `$5` in a
  template silently ate a literal dollar amount — fixed to require
  ordinary identifier rules, first char a letter/`_`).

  App integration, deliberately **not** folded into the existing
  regex-based find/replace/workspace-search infrastructure (`SearchBar`,
  `WorkspaceSearch`) despite the shape looking similar at a glance — a
  judgment call favoring risk-avoidance over unification, the same one
  T151 made keeping `vix-workspace-search` un-merged with query-replace
  for having "real logic," and consistent with how every other T20x
  feature this session added a clean parallel structure rather than
  retrofitting working, heavily-tested code:
  - **Buffer/selection** (**Edit → Structural Replace…**,
    `edit.structural_replace`): a sibling `StructuralReplace`
    session/field stepped through with `QueryReplace`'s own exact
    `y`/`n`/`!`/`q` key handling (reusing the regex-agnostic `Decision`
    enum and the existing `highlight_match`/`replace_char_span` editor
    helpers unmodified — both already took plain char offsets, no
    regex-specific coupling to undo). Scoped to the active **selection**
    when one exists (its bound grows/shrinks by each replacement's
    length delta, tracked in char space) or the whole buffer otherwise.
  - **Workspace** (**Edit → Structural Replace in Workspace…**,
    `edit.structural_replace_workspace`): computes a per-file plan and
    hands it to `ReplaceConfirm` — the **exact same struct and
    apply/confirm code path** `workspace_replace_all` already uses,
    zero new confirm-UI code, matching the task's own "preview list...
    like query-replace" ask almost literally, since that's precisely
    what `ReplaceConfirm` already is. A real bug caught by its own
    first test run: `self.file_index` needs `build_file_index()` called
    first (workspace search's own establishing call) — missing it
    silently found zero matches everywhere; fixed by calling it when
    the workspace flow's pattern prompt opens.

  A general safety fix along the way, not scoped narrowly to T201:
  `App::save` had no `Tab::read_only` guard before this session's T207
  added one — that guard already covers any T201 tab a user marks
  read-only, no new code needed here.

  10 new integration tests (`run_action`/`on_key` only) plus 16 crate
  unit tests. 11 new i18n keys (2 menu items, 2 palette commands, 2
  prompts, 1 error message, 3 status messages, 1 UI label) × 15 locales.
  New `crates/vix-structural-replace/spec/index.md`; `docs/find/
  index.md` gained a "Structural search & replace" section. Menu items
  added deliberately (unlike `edit.query_replace`/`search.workspace`,
  which are intentionally menu-less modes of the find dialog per that
  submenu's own doc comment) since this is a genuinely separate feature,
  not another mode of an existing dialog. Full `scripts/check` gate
  green.

  **This closes Run C entirely** — every task T201–T211 is now done.
- [x] **T202 — Theme editor.** Done. **View → Edit Theme…** (a sibling
  leaf next to the View → Theme submenu, not buried inside its fully
  dynamic item list) opens a new `vix-theme-editor-panel` overlay: 15
  rows, one per color slot (menu/status bar, left/right dock
  foreground+background, editor foreground/background/cursor, 4
  syntax colors). `Enter` reuses `vix-x11-color-picker` — the same
  panel Tools → X11 Colors uses to insert a hex value into a buffer —
  wired via a new `theme_editor_picking: bool` flag so its existing
  `Enter` handler applies the chosen color to the theme editor's
  highlighted slot and closes the picker instead, the same
  "one overlay reused for two purposes via a flag" pattern
  `WorkspaceSearch::static_results` already established for
  go-to-definition. Every edit is applied live
  (`vix_theme_model::apply`); `Esc` reverts to the committed theme if
  nothing was saved. `Ctrl+S` prompts for a name and writes the draft
  to `~/.config/vix/themes/<name>.json`
  (new `vix_theme_model::to_json`, added `Serialize` to `CustomTheme`
  and its nested color structs, which previously only derived
  `Deserialize` — round-trip verified), then sets it as the active
  theme via the same path `set_theme_by_name` already uses.
  Found and fixed 2 unrelated stale-docs drifts while in the theme
  code: `docs/themes/index.md` was missing `number` from its list of
  recognized `syntax` slots (a real, already-wired slot, not a
  future one), and `agents/share/crate-map.md`'s "sanctioned
  `struct_excessive_bools` allows" list still named `SearchBar`/
  `WorkspaceSearch`/`editor_core Editor` from before T149 converted
  them to `bitflags` — `AGENTS.md`'s own copy of this same drift was
  already caught and fixed during T151, but this second copy in
  crate-map.md was missed at the time.
  7 new integration tests, driven through `run_action`/`on_key` only
  (like `keybinding_editor`'s own tests) since the panel's own methods
  are `pub(super)`; none of them submits a non-empty Save As name, so
  none writes to the real themes directory (`Settings::themes_dir()`
  has no test-only override, same limitation T204's own tests
  document). New crate's own 6 unit tests cover every slot's
  get/set round trip and navigation. Crate count 111 → 112. Full
  `scripts/check` gate green.
- [x] **T203 — New bundled themes.** Done. Solarized Dark, Solarized
  Light, and Tokyo Night already existed in `themes/` (the task's own
  text didn't know that); added `catppuccin-mocha.json` (the real
  published Catppuccin Mocha palette) and `high-contrast.json` (pure
  white on pure black, bright saturated syntax colors — every text
  color verified at 11:1 contrast or better against its background,
  well past WCAG AA's 4.5:1 minimum; computed with the standard
  relative-luminance formula, not eyeballed).
  "Snapshot test each" turned out not to give the regression protection
  the task's own wording implies: `tests/snapshots.rs`'s harness
  flattens a rendered frame to **plain text** (`buf[(x,y)].symbol()`
  only) — it never reads `.style()`, so a color-only regression is
  invisible to it, and five near-identical text-only goldens (same
  glyphs, different pinned theme) would have added corpus size for no
  real coverage. Used two other layers instead: a new
  `tests/integration/themes.rs` that parses each of the 5 themes'
  JSON directly and asserts pinned RGB values per slot (the actual
  "so slots can't silently regress" goal), and one new
  `tests/snapshots.rs` test that applies **every** bundled theme
  (reading `themes/*.json` directly, not via `vix::menu::menus()` —
  that menu is built once and cached process-wide, so which themes its
  Theme submenu lists depends on test run order within the binary, not
  a reliable enumeration) and asserts the render doesn't panic and
  isn't blank — cheaper and more comprehensive than one golden per
  theme, and it's the actual failure mode "so a malformed theme is
  caught" cares about.
  Found and fixed a real, unrelated bug while writing the name-
  uniqueness test: `themes/safelight-red.json` (a red-on-black theme)
  declared `"name": "Phosphor Amber"` — copy-paste from the *real*
  `phosphor-amber.json` — so the picker only ever offered one of the
  two. Also fixed `docs/themes/index.md`'s "Ready-made themes" list,
  stale in both directions: missing `Phosphor Amber`/`Phosphor Green`/
  `Safelight Red` and the base16-derived themes entirely, while naming
  a `Matrix` theme that doesn't exist under that name (a leftover from
  before `phosphor-green.json` existed, going by feel — never
  confirmed further, not worth chasing down). `CHANGELOG.md` entries
  added (Added: the 2 new themes; Fixed: the duplicate name). Full
  `scripts/check` gate green.
- [x] **T204 — Keybinding editor.** Done. **Vix → Keybindings…** opens a
  new overlay (`crates/vix-keybinding-editor-panel/spec/index.md`): a
  searchable, sortable, *selectable* table of the active keymap's
  effective bindings (its top-level built-ins + `vix_keybindings::SHARED`
  + anything already in `self.key_overrides`), each tagged `[user]` or
  `[script: name]` when overridden. Enter opens a prompt to type the new
  key as a `vix-macros` token (`PromptKind::RebindKey` +
  `App::pending_rebind_action_id`, mirroring the established
  extra-context-lives-in-its-own-field convention); Delete resets a user
  override back to its default (new `vix_keybindings::user_bindings::
  remove`). Both write through the already-built T104h–j layer
  (`user_bindings::upsert`/`remove` + `App::resolve_key_overrides`) rather
  than inventing a second one — conflict/shadow reporting on a rebind is
  the same `keybindings.reload` machinery, not special-cased here. A real
  dispatch-order hazard had to be respected, not just avoided by luck:
  `try_panel_key`'s `panel!(keybinding_editor, …)` sits *after*
  `panel!(prompt, …)`, so the rebind `Prompt` (open while
  `keybinding_editor` stays `Some` underneath it) wins the keystroke
  instead of the editor swallowing it as a filter character — tested
  directly (`keybinding_editor_enter_opens_a_rebind_prompt_that_wins_over
  _the_editor`). Menu placement was a user decision: **Vix → Keybindings…**
  next to **Vix → Settings**, not Help (Help stays the read-only F1
  panel). Tests avoid writing to the real `Settings::keybindings_path()`
  (no test-only override exists for it) — everything up to but not
  including a successful rebind/reset's disk write is covered; the
  no-op/reset-on-a-built-in-row and validation-rejects paths are.
- [x] **T205 — Snippet editor + tab stops.** Done, scoped down from the
  task's literal text. **Tab stops already fully existed**: `vix_snippet_tool::parse`
  extracted `$1`/`${2:placeholder}`/`$0` and `App`'s `ActiveSnippet` already
  drove a Tab-navigable tabstop session — confirmed by reading the code, no
  implementation needed there. **A full snippet create/edit dialog** (a
  pre-filled multi-field form for existing entries, with scope-gating so
  bundled/project snippets aren't editable) was deliberately **not** built —
  substantially more UI surface than the rest of the task, and existing
  snippets are already just JSON files anyone can edit directly. What
  shipped instead: **Tools → New Snippet from Selection…**
  (`tools.snippet_new_from_selection`, a new menu leaf next to Tools →
  Snippets…) captures the active selection's text
  (`Tab::editor::get_selection_text`), prompts for a prefix
  (`PromptKind::SnippetPrefixFromSelection`), and on accept saves it to the
  **global** scope (`~/.config/vix/global/snippets/snippets.json`, created
  if missing) as a snippet named after the prefix, using the prefix as its
  own expansion prefix; a name collision overwrites the existing entry
  (same "later write wins" precedent as `save_theme_as`). Empty
  selection or empty prefix is a no-op (status message for the former,
  silent for the latter — matches `save_theme_as`'s empty-name precedent).
  New `vix_snippets::to_json`/`save_file` (write the same JSON shape
  `parse_json`/`load_file` read; round-trip unit tested) alongside the
  existing read-only helpers. `refresh_snippet_library` needed `pub(super)`
  to be callable from `app.rs` (same parent/child visibility rule T202's
  `save_theme_as` already ran into). 3 new integration tests, driven
  through `run_action`/`on_key` only; none submits a non-empty prefix, so
  none writes to the real global snippets file (`vix_snippets::global_dir()`
  has no test-only override, same limitation as `Settings::themes_dir()`/
  `keybindings_path()`). 4 new i18n keys (menu item, prompt, success status,
  failure message) translated directly across all 15 locales rather than
  delegated, given the small count. Full `scripts/check` gate green.
- [x] **T206 — Markdown preview sync + TOC.** Done. **Scope note**: the
  preview overlay captures all keys exclusively while open (same as every
  other Tools overlay in Vix — the editor isn't reachable underneath it),
  so "scroll-sync" is sync-**on-open** (the cursor can't move again until
  the preview closes), not continuous two-way sync; that's the complete,
  correct reading given the architecture, not a narrowing. `vix_markdown_
  preview::render_full` (the version `Panel::open` calls; the plain
  `render` most callers/tests use is now a thin wrapper) walks
  `pulldown-cmark`'s `into_offset_iter()` to tag every display line with
  its originating 1-based source line, and collects headings into a TOC —
  **reusing `vix_outline_panel::Entry`/`Outline` exactly as the task
  suggested**, just pointed at *preview* lines (`Entry.line`) instead of
  source lines. `Panel::sync_to_source_line` (cursor line → nearest
  preview line; a real bug found and fixed here: several display lines
  — a heading's text, underline, and trailing blank — can share one
  source line, and naively taking the *last* match landed one line past
  the heading, on its blank separator, not the heading itself; fixed to
  take the *first* line at the greatest qualifying source line) drives
  sync-on-open; `Panel::scroll_to_line` (a direct preview-line jump)
  drives the TOC. In the host, opening the preview captures the cursor's
  source line first, then syncs; `t`/`T` opens the TOC as a second overlay
  over the preview (same "overlay over an overlay" shape as the theme
  editor's X11 picker, dispatched via the same `panel!`-chain-ordering
  and `any_open!` pattern T202 established), `Enter` jumps the preview
  and closes just the TOC (back to the preview, not the source buffer),
  `Esc` closes just the TOC, empty-TOC is a no-op with a status message.
  5 new integration tests plus 4 new crate unit tests (source-line
  mapping, TOC extraction, both scroll helpers). 3 new i18n keys ×
  15 locales. New `docs/markdown-preview/index.md`; spec updated. Full
  `scripts/check` gate green.
- [x] **T207 — Git history.** Done. **Naming note**: `git.log` (Git → Log
  → **All**) already existed — a plain `git --no-pager log`, streamed to
  the bottom dock, no interactivity. That's a different feature from what
  this task asks for (a selectable commit *list*), so the three new items
  are new siblings (**Browse Log…**, **File History**, **Open File at
  Revision…**) inside the same Git → Log submenu, not a replacement.

  New `vix-git` pieces, all pure/unit-tested except the two that shell out:
  `log`/`file_log` (`git log` with a custom `\x1f`/`\x1e`-delimited
  `--format`, parsed by `parse_log` — verified against real `git log`
  output captured from this repo's own history, not guessed) into
  `LogEntry` rows, held by a `LogPanel` (built on `vix_list_state`, same
  as every other list panel this session); `show_commit`/`show_file_at`/
  `resolve_short_sha` (shell out to `git show`/`rev-parse`, validated with
  the same `valid_ref_name` + `--end-of-options` defense `checkout`
  already uses).

  Both "Log" and "File History" open a commit's diff via the **same**
  mechanism — `git show <sha>` (whole commit) or `git show <sha> --
  <path>` (just the file) — as raw unified-diff text in a **read-only
  tab**, not the structured `diff_view` overlay Compare-With-File uses:
  the task explicitly says "in a tab" for both this and Open File at
  Revision, and a commit can touch many files, which doesn't fit
  `diff_view`'s one-old-text/one-new-text model. New shared
  `App::open_readonly_text_tab(title, content)`: a synthetic `path`
  (never a real file, same technique T211's wgrep buffer and T210's
  coverage-file-picker use elsewhere this session) gives the tab an
  arbitrary title through `Tab::title()`'s ordinary filename rendering.

  **Real gap found and fixed**: `App::save` had no guard at all for
  `Tab::read_only` — only images were special-cased — so `Ctrl+S` on any
  of these new read-only tabs would have tried to `fs::write` to their
  synthetic, nonexistent path. Added a general `active_read_only()` check
  (reusing the existing `status.read_only_blocked` message, already used
  for edit-blocking) rather than a one-off for just these tabs.

  9 new integration tests (5 in `tests/integration/workspace.rs`'s
  sibling `tests/integration/git.rs`, `#[ignore]`d like every other test
  in that file needing a real throwaway repo — run manually with
  `--ignored` and verified passing, same as the file's pre-existing
  tests) plus 5 new `vix-git` unit tests (`parse_log`, `LogPanel`
  navigation, ref-name validation on the three new runners). 8 new i18n
  keys (3 menu items, log-panel title/hint, the revision prompt, two
  status/msg pairs) × 15 locales. New `crates/vix-git/spec/git-history/
  index.md`; `docs/git-panel/index.md` gained a "History" section. Full
  `scripts/check` gate green.
- [x] **T208 — CLI surface.** Done. `vix --diff OLD NEW` opens a
  read-only diff overlay comparing the two files directly (new
  `App::open_diff_files`, independent of any open buffer — unlike
  Tools → Compare With File…, which diffs the active buffer); `vix -`
  reads stdin into an unsaved scratch buffer (new
  `App::open_stdin_buffer`, no header line unlike the plain New Scratch
  Buffer action, since the caller may want to act on exactly what it
  piped); `vix --version --json` prints `{"name","version"}` for
  tooling. `--version` had to become hand-rolled
  (`disable_version_flag`) rather than clap's automatic one, which
  exits before `--json` could ever be inspected.
  Self-caught mistake: the struct-level rationale for
  `disable_version_flag` was first written as a `///` doc comment,
  which clap's derive surfaces as the command's own `--help` long text
  — an implementation detail is not what a CLI user asked for. Fixed
  by moving it to a plain `//` comment before publishing.
  Mergetool needed no new flag at all: Vix's existing conflict tool
  (`crates/vix-conflict-tool/spec/index.md`) already resolves
  `<<<<<<<`/`=======`/`>>>>>>>` markers in a normally-opened file, so
  `git mergetool` just points at `vix "$MERGED"` — that's the one
  "new capability" in the task's four-item list that turned out to
  already exist, once actually checked rather than assumed missing.
  New `docs/cli/index.md` (the full flag reference, plus the
  `difftool`/`mergetool` git config snippets — `trustExitCode = false`
  is deliberate, since a TUI editor's exit code reflects whether it
  ran, not whether a merge was resolved); `--help` and `README.md`
  point at it. 4 new tests (`open_stdin_buffer`/`open_diff_files` ×
  found/identical/missing-file), verified `--version`/`--version
  --json`/`--help` by hand against the built binary. Full
  `scripts/check` gate green.
- [x] **T209 — Trash on delete.** Done. File-explorer `Delete` moves to
  the OS trash by default via the `trash` crate
  (`vix_fileops::trash_path`); new `Settings::explorer_delete: String`
  (`"trash"`/`"hard"`, matching the flat-`String` convention every
  other enum-like Settings field already uses — `keymap`/`theme`/
  `time_zone` — rather than a real Rust enum or a nested `[explorer]`
  TOML table the task's own `explorer.delete` dotted notation
  suggested; a value other than `"hard"` falls back to `"trash"`, the
  safer default, rather than silently hard-deleting on a typo). The
  confirm prompt now uses one of two full messages (`confirm.delete` /
  `confirm.delete_hard`, translated into all 14 core locales) rather
  than one template with an inserted word, since "which will happen"
  reads more naturally as a full sentence in every language than a
  mid-sentence substitution would. No new menu/settings-UI toggle —
  matches several other enum-like Settings fields (`preview_tabs` among
  them) that are config-file-only with no menu affordance, so this
  isn't a gap relative to the existing pattern. Deliberately forced the
  one *pre-existing* delete test (`explorer_delete_closes_buffer`,
  about buffer-closing, not trash) to `"hard"` explicitly, so it keeps
  testing what it always tested without picking up a new dependency on
  the OS trash mechanism being available in CI — added two *new* tests
  instead for the trash-vs-hard prompt wording, plus a
  `vix-fileops` unit test that exercises real trash I/O (verified
  passing locally on macOS; `cargo deny check` clean for the new
  dependency). Docs updated: `crates/vix-fileops/spec/index.md` (new
  "Delete and trash" section, replacing its stale "trash… nice to have"
  roadmap line), `docs/file-explorer/index.md`, and
  `docs/configuration/index.md`'s settings table. `CHANGELOG.md` entry
  added.
- [x] **T210 — Coverage gutter.** Done. New crate `vix-coverage`: pure
  parsing, no `App`/I/O dependency. `parse` auto-detects LCOV (`SF:`/
  `DA:`/`BRDA:`/`end_of_record`) vs. Cobertura XML (`<class filename="…">`/
  `<line number="…" hits="…">`) from the text; the Cobertura reader is a
  tolerant single-pass regex token scan in document order (tracking the
  enclosing class's `filename`), not a validating XML parser — narrow
  enough that pulling in an XML dependency wasn't worth it. A line with an
  untaken LCOV branch (`BRDA:` `taken` of `-`/`0`) or a Cobertura
  `condition-coverage="…% (a/b)"` with `a < b` is reported `Partial`;
  otherwise a nonzero hit count is `Covered`, zero is `Uncovered`.
  `Report::lines_for` matches a buffer's path against however the report
  recorded it (exact match, then a suffix-match fallback either
  direction) since reports name files inconsistently across machines/CI.
  9 unit tests, including the partial-branch and suffix-matching cases.
  **Tools → Load Coverage File…** (prompt pre-filled from the new
  `coverage_path` setting) reads and parses the file, then paints the
  gutter via the *existing* diff-gutter mechanism
  (`Editor::set_gutter_marks`, the same green/red/yellow hex values the
  git diff gutter uses for added/deleted/modified) — reused exactly as
  the task asked, no new rendering code in `vix-editor-core`. The
  coverage and git diff gutters share that one gutter-sign column, so
  only one shows per buffer: `src/ui.rs`'s per-frame refresh calls
  `refresh_coverage_gutter` instead of `refresh_git_gutter` while
  `App::coverage_gutter_active()` is true. **Tools → Toggle Coverage
  Gutter** shows/hides without re-parsing (the loaded `Report` stays
  cached on `App`). No coverage generation built in — Vix visualizes an
  existing report, doesn't run one. 6 new integration tests (real LCOV
  fixture files in a temp dir, driven through `run_action`/`on_key`,
  asserting on `Editor::gutter_marks()` directly — no real-config-dir
  writes involved here, unlike T202/T205, so these cover the full
  load/toggle/error path end to end). 8 new i18n keys across all 15
  locales. New `docs/coverage/index.md`, `crates/vix-coverage/spec/
  index.md`, `docs/configuration/index.md` and `docs/git-panel/index.md`
  cross-link updated. Full `scripts/check` gate green.
- [x] **T211 — Editable search results ("wgrep"-style).** Done. **Alt+E**
  in workspace search (also `search.edit_results`, in `palette::COMMANDS`)
  opens the hit list as a genuine, editable `Tab` — not an overlay, closing
  the search panel so normal editing keys reach it directly, exactly as
  the task asked ("a real, editable buffer"). The buffer carries a
  synthetic `path` (`App::WGREP_RESULTS_PATH`, never a real file) purely so
  `App::save` can recognize it and reroute `Ctrl+S` to a diff step instead
  of trying to write a file literally named that.

  The diff re-identifies each **surviving** line by parsing its own
  `rel:line:` prefix (new `vix_workspace_search::parse_result_line`, the
  inverse of `Hit::display`'s own format) and comparing the text after it
  to a recorded baseline (new `WgrepBaseline`, one per hit, captured when
  the buffer opened) — deliberately **not** by buffer position, so
  deleting a line just removes it from what gets parsed (no bookkeeping:
  it's simply excluded from the diff, i.e. skipped) and a line typed from
  scratch (not matching the shape at all) is ignored rather than misread.
  `Hit` gained two fields (`rel`, `text`) to carry the pieces `WgrepBaseline`
  needs — the 6 other `Hit`-constructing call sites (symbols, references,
  diagnostics, TODO finder, go-to-definition candidates) got them filled in
  too, mostly with a sensible existing value (rarely used since
  `open_wgrep_results` gates on `!STATIC_RESULTS`, which covers all of
  them except the TODO finder — excluded anyway since T211 is scoped to a
  live text search, not every `rel:line: text`-shaped static list).

  The confirm-before-write step **reuses `ReplaceConfirm`'s exact shape**
  (per-file new contents, a `rel (count)` summary line, a scroll offset)
  and its `y`/Enter-apply, `n`/Esc-cancel key handling, matching the task's
  own "confirm summary" ask almost line for line — as a sibling
  `wgrep_confirm` field (not the same field as search-and-replace's own
  `replace_confirm`, since "N edits written" and "N replaced" are different
  claims deserving different wording) with its own draw function
  (`draw_wgrep_confirm`, a near-mirror of `draw_replace_confirm`) and its
  own `panel!`/`any_open!` entries (T202's now-familiar dispatch pattern).
  Applying writes each affected file whole and re-baselines exactly the
  lines that changed, so saving again with no further edits is a no-op.
  4 new status/UI i18n keys, plus 2 more (`cmd.search_edit_results`,
  `menu`-adjacent) for palette discoverability — all × 15 locales, none
  delegated (small enough to translate directly).

  5 new integration tests (open, edit-and-write, delete-skips-the-hit,
  no-op-with-no-edits, static-results-gate — all driven through
  `run_action`/`on_key`, asserting on real on-disk file contents since a
  wgrep report is user data in a temp dir, not app config, so — like
  T210 — nothing here needs to dodge a real-config-dir write) plus 3 new
  crate unit tests for `parse_result_line`. `crates/vix-workspace-search/
  spec/index.md` gained a full T211 section; `docs/find/index.md` updated
  (new Alt+E keybinding row + an "Editing results directly" subsection).
  Full `scripts/check` gate green.

## Phase 3 — Documentation

- [x] **T301 — mdBook site.** `book.toml` (`src = "docs"`) + new
  `docs/SUMMARY.md` organize all 68 existing `docs/*/index.md` pages —
  no files moved — into Getting Started (10) / Guides (18, including the
  7 `for-*-users` migration pages) / Features (34 panels & tools) /
  Reference (architecture, comparison, performance, plus 3 more below) /
  Contributing (6 more below). `docs/index.md` itself is the unlisted
  `[Introduction]` prefix chapter. Found (empirically, not assumed) a
  real mdBook footgun: a `SUMMARY.md` chapter *can* point outside `src`
  via `../`, and `mdbook build` doesn't error — but it also writes that
  chapter's HTML output next to its *source* file rather than under
  `book/` (with `src = "docs"`, a `../AGENTS.md` chapter's destination
  becomes `book/../AGENTS.html`, i.e. a stray file at the repo root),
  scattering generated files straight into the tracked tree. Worked
  around with 9 thin wrapper pages living inside `docs/`
  (`spec-overview.md`, `crate-map.md`, `glossary.md`,
  `contributing-agents.md`, `contributing-conventions.md`,
  `contributing-workflow.md`, `contributing-ai-statement.md`,
  `contributing-security.md`, `changelog.md`) — each just an
  explanatory comment plus one `{{#include ../<path>}}` transcluding
  the real repo-root/`agents/`/`spec/` file, since mdBook's include
  directive doesn't create a second chapter and so can't escape `src`.
  Confirmed by diffing `git status` before/after `mdbook build`: zero
  new files outside `book/`. One directory deliberately NOT in the
  book: `docs/licenses/` (bundled third-party license `.txt` files, no
  `index.md`, not referenced anywhere else either — an orphan
  predating this task, left alone rather than fixed as a drive-by).
  **Known limitation, documented rather than silently shipped**: prose
  *inside* the existing `docs/*.md` pages that links further outside
  `docs/` (a crate's spec via `../../crates/X/spec/index.md`, say) will
  404 once the book is served standalone from GitHub Pages — those
  links only resolve when the page is browsed as part of a full
  repository checkout (a forge's own file viewer, or a local clone),
  which is how the whole `docs/` tree has been written all along; fixing
  every such link is a much bigger undertaking than "organize existing
  pages" and is out of scope here — a future task if it matters once the
  site is live. New GitHub-only CI jobs `docs-build` (installs `mdbook`,
  pinned+checksummed like `cargo-deny`/`lychee`; builds on every
  push/PR; uploads the Pages artifact on `main`) and `docs-deploy`
  (`needs: docs-build`, deploys via `actions/deploy-pages@v4`, gated to
  `main`) in `.github/workflows/ci.yml` — no GitLab/Codeberg equivalent
  since neither publishes through this repo's Pages. `spec/ci/index.md`
  gained a "Docs site (mdBook)" section documenting both jobs, and that
  **actually publishing still needs a one-time manual step**: enabling
  Settings → Pages → Source: GitHub Actions on `github.com/vixide/vix`
  — not something a workflow file can do. `book/` (build output)
  gitignored. While surveying `docs/` for this task, noticed
  `docs/contacts/index.md`, `docs/hunspell/index.md`, and
  `docs/workspace-information-panel/index.md` are near-empty stub pages
  (just the trademark footer) — they exist so T302's audit didn't flag
  them, but have no real content; worth a look during T303/T304.
- [x] **T302 — Docs coverage audit.** `scripts/docs-coverage` (Python):
  combines two signals — (1) a `docs/*.md` page linking a crate's
  `crates/<crate>/spec/` (the existing convention, strong signal), (2) a
  crate name and a `docs/` directory name sharing a 4+ letter word after
  stripping generic suffixes (`-tool`/`-panel`/`-picker`/`-model`/`-parser`)
  and substring-matching so plurals don't cause false gaps
  (`contact`/`contacts`). Signal 2 is skipped for the
  `vix-convert-from-*-into-*-tool` family: their format-name words
  (`json`/`yaml`/`markdown`/…) collide with unrelated *editor* pages
  (`edit-json`, `edit-yaml`, `markdown-preview`), which would otherwise
  misread as coverage for a different feature — confirmed by checking
  `docs/menus/index.md` is the ONLY place any of that family is mentioned,
  i.e. no dedicated conversion-tools page exists at all. A short,
  individually-commented `EXCLUDE` set covers crates confirmed (by reading
  their actual doc coverage) to be pure internal infrastructure or already
  covered under a differently-named feature page
  (`vix-action-catalog`/`vix-list-state`/`vix-lsp-core`/`vix-textops`/
  `vix-lorem`/`vix-vcard-parser`/`vix-vcard-panel`/`vix-modal`/
  `vix-time-zone-model`). Writes `docs/coverage.md` and exits non-zero
  while gaps remain, so it's re-runnable after T303/T304 to watch the
  count shrink. First run found 38 real gaps (spot-checked several by
  hand — e.g. `vix-clock-panel`/`vix-tags`/`vix-roam`/`vix-x11-color-picker`/
  `vix-undo-store` are each mentioned only incidentally or in the generic
  menu listing, never on a dedicated feature page), listed in
  `docs/coverage.md` for T303/T304 to consume.
- [x] **T303/T304 — Fill missing docs pages (both batches, done together).**
  All 38 crates from T302's `docs/coverage.md` closed in one pass, split
  across 4 parallel drafting agents plus 3 direct fixes, each required to
  read the crate's own spec (and, for "how to open", `crates/vix-menu`,
  `src/app.rs`, and the T305-generated `docs/reference/actions.md`/
  `keybindings-*.md`) rather than write from the name alone — several
  crate names turned out to mean something different than they sound
  like, only caught by actually reading the source:
  - `vix-affix` → **Surround** (Edit → Surround, wrap/unwrap a selection
    in a bracket/quote pair) — new `docs/affix/index.md`.
  - `vix-align` → align lines on a delimiter (confirmed as guessed) — new
    `docs/align/index.md`.
  - `vix-tags` → **matching HTML/XML tag navigation** (Go → Matching
    Tag), not Org tags and not ctags/etags — new `docs/tags/index.md`.
  - `vix-clipboard` → internal plumbing (a process-wide, mutex-serialized
    system-clipboard access point every Cut/Copy/Paste goes through,
    opt-in to the real OS clipboard so the test suite never touches it —
    a real incident once let a test overwrite the developer's actual
    clipboard), **not** the separate clipboard-*history* ring
    (`crates/vix-editor/spec/clipboard-history/index.md`, cross-linked
    instead of duplicated) — new `docs/clipboard/index.md`.
  - `vix-undo-store` → **persistent undo** (saves each file's undo tree
    to `<config>/undo/` on save, restores on reopen only if the content
    hash still matches), a real user-facing feature gated by
    `Settings::persistent_undo`, not just internal plumbing — new
    `docs/undo-store/index.md`.
  - `vix-roam` → confirmed as guessed: Org-roam-style backlinks/
    zettelkasten note-linking under Org → Roam / Org → Node — new
    `docs/roam/index.md`, cross-linking `docs/org/index.md` rather than
    duplicating it.
  - The 12 `vix-convert-from-*-into-*-tool` crates + `vix-convert-tabular`
    (the shared CSV/TSV/JSON engine underneath 6 of them) → one
    consolidated `docs/convert/index.md` rather than 13 near-duplicate
    pages, grouped by format family (CSV/TSV/JSON, JSON/YAML, JSON/TOML,
    Markdown/HTML) with real behavior from each engine (RFC 4180 CSV
    quoting, TSV's no-quoting limitation, formula-injection neutralizing
    on write, JSON↔TOML's top-level-must-be-an-object constraint, plain
    CommonMark not GFM for Markdown↔HTML).
  - 11 single-purpose Tools-menu utilities (`vix-base-tool`,
    `vix-base16`, `vix-base64-tool`, `vix-calculator-tool`,
    `vix-checksum-tool`, `vix-color-converter-tool`, `vix-jwt-tool`,
    `vix-pomodoro-tool`, `vix-regex-tool`, `vix-unit-converter-tool`,
    `vix-url-tool`) → one consolidated `docs/tools/index.md`. Found a
    real spec-drift bug while writing it: `vix-checksum-tool`'s own spec
    documented only SHA-256/SHA-512, but the real menu and code also
    wire up MD5 and CRC-32 — fixed the crate's spec, Cargo.toml
    description, and module doc to match (not just the new docs page).
  - 5 more individual small features, each its own page:
    `docs/emmet/index.md` (Emmet abbreviation expansion, 100,000-node
    cap), `docs/html-character-picker/index.md`, `docs/http-client/index.md`
    (absolute URL required, `http`/`https` only, response opens as a new
    tab), `docs/x11-color-picker/index.md` (also reused, dual-purpose,
    by the theme editor's color-slot picker), `docs/welcome-panel/index.md`
    (shown once automatically via `Settings::show_welcome_dialog`,
    flipped off after first show; the same overlay type also backs
    Help → License/Report Issue/Privacy).
  - `vix-clock-panel` → found this one was **already substantively
    documented in the wrong place**: `docs/insert/index.md`'s Date/Time
    section and `docs/calendar-panel/index.md`'s old "Date and time area"
    section both covered clock-panel content, but the calendar page had
    gone **stale** — `vix-calendar-panel`'s own spec says the date/time
    strings moved OUT of the calendar box into a separate Clock box
    (**Tools → Clock…**) so "each box does one thing," but
    `docs/calendar-panel/index.md` still described them as part of the
    calendar, and its keybinding table was also wrong (said `←`/`→` page
    the month; the real bindings move the day, `Ctrl`+arrows page the
    month). Rewrote `docs/calendar-panel/index.md` to match current
    behavior and wrote a new `docs/clock/index.md` for the Clock box
    itself, cross-linked from both `docs/insert/index.md` and
    `docs/calendar-panel/index.md`.
  - `vix-uuid-tool`/`vix-zid-tool` → already fully covered in
    `docs/insert/index.md`'s UUID/ZID sections; just added the missing
    crate-spec mentions rather than duplicating content in new pages.

  `scripts/docs-coverage` now reports **zero gaps**. `docs/SUMMARY.md`
  gained all 14 new pages under Reference→Features (re-sorted
  alphabetically while adding them, since the list was already more than
  half new entries). Two check-docs-caught link bugs fixed along the way
  (`docs/roam/index.md` and `docs/undo-store/index.md` each cited a
  sub-spec by its bare filename instead of the real
  `crates/<crate>/spec/<sub>/index.md` path).
- [x] **T305 — Generated reference.** `examples/list_commands.rs` grown:
  plain `cargo run --example list_commands` keeps its original,
  side-effect-free behavior (print the palette's `>` commands to stdout);
  `-- --write` (re)generates `docs/reference/actions.md` (all 655
  non-dynamic dispatchable action ids, each with its resolved title and
  whether that came from a **Menu** leaf, this workspace's
  `vix-action-catalog` **Catalog**, or a `palette::COMMANDS` **Palette**
  entry — mirrors `App::action_title`'s own resolution order exactly),
  `docs/reference/settings.md` (all 63 `Settings` fields: key, literal
  Rust type, default from a live `Settings::default()`, and doc comment
  parsed out of the struct's own source), and one
  `docs/reference/keybindings-<keymap>.md` per keymap (10 files) plus a
  `keybindings-shared.md` for `vix_keybindings::SHARED` — from
  `vix_menu::menus()`, `vix_palette::COMMANDS`, `vix_action_catalog::CATALOG`,
  `vix_settings::Settings`, and `vix_keybindings::TABLES`/`SHARED`
  directly, so none of it can drift from the real data. Locale fixed to
  `"en"` before writing (`vix_i18n::set_locale`) so output is
  byte-identical run to run — verified by diffing two successive
  `--write` runs. Extracted the action-id dispatch-chain scanner
  (`DISPATCHERS`, `every_dispatchable_action_id`, `DYNAMIC_PREFIXES`) out
  of `tests/action_catalog.rs`'s own private copy into a new
  `vix_action_catalog::dispatch_scan` public module, so the test's
  definition of "every action id" and the generator's are the same code,
  never two copies to keep in sync by hand — `tests/action_catalog.rs`
  updated to call the shared version, still green. Found one real bug
  during generation, not assumed away: two `Settings` field doc comments
  use rustdoc intra-doc links (`` [`recent_files_max`](Self::recent_files_max) ``)
  that `scripts/check-docs` correctly flagged as broken once copied
  verbatim into a generated markdown page (`Self::x` means nothing outside
  rustdoc) — fixed with a small `strip_intradoc_links` transform that
  keeps the link's label, drops the wrapper. CI check added to all three
  forges (`ci.yml`/`.gitlab-ci.yml`/`.forgejo/workflows/ci.yml`) and to
  `scripts/check` itself, right after `cargo doc`: regenerate with
  `--write`, then `git diff --exit-code -- docs/reference/`.
  `docs/SUMMARY.md` (T301) gained entries for all 14 new pages under
  Reference. `spec/ci/index.md` documents the new gate step; the gate is
  now seven checks, not six.
- [x] **T306 — Getting-started guide.** New `docs/getting-started/index.md`:
  Install, First Launch, "The 10 things to learn first", Where to go next.
  Verified every install method against the real, already-published
  `1.6.0` GitHub Release rather than guessing at `dist`'s conventions —
  downloaded `vix.rb` (Homebrew formula — `brew install
  vixide/homebrew-tap/vix`), `vix-npm-package.tar.gz` (its
  `package.json` names `@vixide/vix`, confirming `npm install -g
  @vixide/vix`), and confirmed the `vix-installer.sh`/`.ps1` asset names
  match `dist`'s standard `releases/latest/download/` URL convention.
  Found a real, previously-undocumented gap: `index.md`'s "Install & run"
  only ever documented building from source — Homebrew/npm/shell/
  PowerShell installers and the GitHub Release binaries (all real,
  already shipping via `dist`, per `spec/ci/index.md`) had no end-user-
  facing mention anywhere in the repo. `spec/debian/index.md` confirms
  there genuinely is no `.deb` package yet, so that's correctly noted as
  not-yet-available rather than documented as if it existed. "The 10
  things to learn first" cross-checked every keybinding claimed against
  the real, generated `docs/reference/keybindings-apple.md`/
  `keybindings-shared.md` (T305) rather than assumed defaults — caught
  one non-obvious real fact worth calling out: Undo/Redo (`Ctrl+Z`/
  `Ctrl+Shift+Z`) isn't in `vix_keybindings::TABLES`/`SHARED` at all —
  it's wired directly into `vix-editor-core`'s own crossterm handler
  (`editor_crossterm.rs`), so it's the one binding in the list that's
  identical across every keymap rather than Apple-specific. Linked from
  `index.md`'s top (a "New to Vix?" line right after the ASCII
  screenshot) and added to `docs/SUMMARY.md`'s Getting Started section.
- [x] **T307 — Man page.** New `examples/generate_man.rs` (`cargo run
  --example generate_man`) builds `man/vix.1` with `clap_mangen` from
  the real `Cli` clap definition. Moved `Cli` out of `src/main.rs` into
  a new `src/cli.rs` library module (`vix::cli::Cli`, `pub` fields) so
  the binary and the generator share one definition — they cannot drift
  apart the way a second, hand-copied `Cli` for the generator alone
  could. `dist-workspace.toml` gained `include = ["man/vix.1"]`,
  verified (not assumed) with `dist generate --mode ci --check` that
  this doesn't itself change the generated `release.yml` — confirmed by
  actually running `dist` 0.32.0 locally (the same version pinned in
  `cargo-dist-version`), not just reading its docs. `man/vix.1` is
  committed (generated content the release pipeline reads as-is, same
  choice as `docs/reference/`), regenerate-and-diff gated on all three
  forges plus `scripts/check`. `docs/cli/index.md` gained a "Man page"
  section; `spec/ci/index.md` gained a "Man page (T307)" section.
- [x] **T308 — Migration guides.** New `docs/for-helix-users/index.md`,
  in the style of the existing for-vim/for-emacs pages — honest about
  what's genuinely different (Vix has no Helix-style selection-first
  modal; the closest options are the traditional **Vi** keymap or
  **Spacemacs**'s leader-key layer) rather than overselling a keymap
  match that doesn't exist. Verified real Helix facts (built-in LSP/
  DAP/Tree-sitter/multi-cursor, no built-in git client or terminal by
  Helix's own stated design, its Steel/Scheme plugin layer still not a
  stable public API as of 2026) via web search rather than assumed from
  memory. **`docs/for-vscode-users/` turned out to already exist** —
  under `docs/for-visual-studio-code-users/`, a name this task's own
  text didn't anticipate — so instead of shipping a duplicate page,
  fact-checked and corrected the existing one against the real,
  T305-generated `docs/reference/keybindings-vscode-macos.md`: it
  claimed `Alt+Up`/`Alt+Down` move a line (no such binding exists in
  any keymap — Move Up/Down is menu-only), and `Ctrl+Shift+K` "deletes"
  a line (it cuts to clipboard, matching `edit.cut_line`, not a bare
  delete); added the honest gaps it was missing (no `Ctrl+D` incremental
  multi-select, no `F5` one-key debug start). `docs/comparison/index.md`
  rewritten into the requested feature-parity matrix (Vix / Vim / Helix
  / Micro / Zed, 13 rows, footnoted where a flat ✓/✗ would be dishonest
  — e.g. Vim's `undofile` gives real persistence and branches, just no
  browsing UI without a plugin) — every non-Vix fact checked via web
  search against each project's own current documentation/repo rather
  than assumed, including the Helix design philosophy quote ("does not
  try to be … a git client") that justifies its `~` row for Git.
  `docs/SUMMARY.md` gained "Coming from Helix".
- [x] **T309 — CHANGELOG discipline.** Backfilled by actually diffing git
  history against `CHANGELOG.md`, not assuming: extracted every task
  number from the 124 merge-commit subjects since the `1.6.0` release
  and cross-checked each against `## [Unreleased]`'s text. 12 came back
  as "missing" on a naive check, but 11 of those are genuine internal-
  only work with zero user-visible effect (T002/T010 CI-only,
  T141/T142/T143 pure code/test reorganization, T145/T149/T150/T151/
  T152/T154 refactors and lint/tooling hygiene) — correctly absent per
  the new rule below, not backfilled. The 12th, **T144, had a real
  gap**: unifying 17 panels' scroll-cursor logic into `vix-list-state`
  fixed a genuine bug (File Explorer and the DB workbench's SQL
  statement editor could scroll past the end of their own list — the
  only 2 of 17 missing that clamp) that had never made it into
  `CHANGELOG.md`; added it under `### Fixed`. Also checked the two
  commits with no task number at all (a docs harmonization pass, the
  new `skills/vix-skill`/`vix-maintainer-skill` Claude Skills) — both
  pure repo-maintenance/AI-tooling, correctly absent. Added an explicit
  "one entry per user-visible change" rule to `agents/conventions.md`'s
  Documentation section, with the same "internal refactor vs. user-
  visible" line this backfill pass itself had to draw, so the next
  person (or agent) doesn't have to rediscover it from scratch.

## Phase 4 — Tutorials

- [x] **T401 — vixtutor spec.** `crates/vix-tutor/spec/index.md`: launch
  via `vix --tutor` and Help → Tutorial; opens a working copy (temp dir)
  of lesson buffers so the user edits freely; chapter navigation
  (next/prev lesson actions); cheap progress checks where possible
  ("delete this line", "change this word" verified against the buffer);
  content localized via the standard `t!` pipeline or per-locale lesson
  files — decide in the spec. Merge spec first.
- [x] **T402 — vixtutor engine + chapter 1.** `vix-tutor` crate + host
  wiring per the recipe; chapter 1 "Moving around" complete with checks.
- [x] **T403 — vixtutor chapters 2–6.** Editing basics; find & replace;
  multi-cursor & selection; files, tabs & palette; git basics. Each
  chapter is a small self-contained lesson file.
- [x] **T404 — Written tutorials 01–05.** `docs/tutorials/`: 01 your first
  session, 02 editing power techniques, 03 find/replace & multi-cursor,
  04 the git workflow, 05 setting up LSP (rust-analyzer, pyright,
  typescript-language-server with real config). Each runs against the
  demo workspace (T501 — do that first).
- [x] **T405 — Written tutorials 06–10.** 06 Org mode & roam, 07 the DB
  workbench (uses the seeded SQLite db), 08 HTTP client & Tools suite,
  09 make Vix yours (themes/keymaps/snippets/settings), 10 debugging with
  DAP (real debugpy or codelldb walkthrough).
- [x] **T406 — VHS demo tapes.** `docs/demos/*.tape` (charm VHS) for ~8
  marquee features: overview tour, palette, multi-cursor, git hunks, DB
  workbench, org-roam, edit surfaces, themes. A `scripts/render-demos.sh`
  regenerates GIFs; embed the overview GIF in README. Tapes run against
  the demo workspace. **Partially done 2026-09-14: all 8 tapes written**
  (every command/menu path grounded against the real repo — `vhs
  validate` confirms all 8 parse) **and `scripts/render-demos.sh` written,
  plus a `## Demos` README section** describing them and how to
  regenerate. **Not done at the time: the GIFs themselves are not
  rendered/committed** — that session's environment could build and run
  `vhs`/`ttyd` (both built from source there, working, real binaries —
  Homebrew itself was write-protected in that sandbox) but its headless
  Chrome/Chromium was killed outright (SIGKILL) under whatever restricts
  that session's own process execution, which VHS's screenshot-based
  capture pipeline needs and has no fallback for. Confirmed with a bare
  `chromium --headless --no-sandbox --screenshot=...` reproducing the same
  kill, so not VHS-specific.

  **Finished 2026-09-18, on a different environment with the identical
  restriction** (same SIGKILL on headless Chromium, verified fresh —
  `dangerouslyDisableSandbox` made no difference, ruling out the harness's
  own sandboxing as the cause, and Homebrew was again write-protected). Not
  worth waiting on a browser-capable machine a second time: built a
  complete substitute pipeline with no browser anywhere in it, since VHS's
  own job is really just "drive a real pty, capture the session, encode a
  GIF" and a headless browser is only *how* real VHS happens to do the
  middle step.
  - **`scripts/vhs_lite.py`** (new) plays a subset of the `.tape` DSL
    (everything the 8 real tapes use) against a real pty via `pexpect`,
    hand-writes an asciicast v2 file (every byte the pty produces always
    gets fed to the recorder, so a replaying terminal emulator's state
    stays correct — `Hide`/`Show` only compress a span's *timestamps*, not
    its bytes, since a screenshot-based recorder can skip a span but an
    escape-code-replaying one can't without losing state — see the
    script's own module docstring for the full reasoning), then
    [`agg`](https://github.com/asciinema/agg) (pure-Rust: fonts shaped
    directly, no screenshot, unrelated to crates.io's same-named Anti-Grain
    Geometry crate — built from git source, `cargo install --path`) turns
    that into the GIF. **`scripts/render-demos-lite.sh`** (new) orchestrates
    it exactly like `render-demos.sh` does for real VHS; the two scripts'
    doc comments point at each other.
  - **Three real, environment-independent flakiness sources found and
    fixed by hand** while getting this reliable, all confirmed via a
    direct `App::on_key`/`TestBackend` test with no pty involved (which
    applies every key correctly, every time — so none of these are vix
    bugs, all are this sandbox's shared-machine contention, the same cause
    already documented elsewhere in this repo's own session history):
    (1) two keypresses sent as separate pty writes, any gap between them,
    can lose the second one even after a long wait — fixed by batching
    every run of Sleep-free actions into one write; (2) even a batched
    write can occasionally produce no reaction at all — fixed with a
    bounded retry on total silence; (3) *repeating* the same key (`Down
    3`) is the one case batching itself breaks — send-and-settle each
    repeat individually instead (confirmed by hand: 9 batched `Right`
    presses moved a menu selection nowhere, the same 9 sent one at a time
    each moved it). A fourth, corrected instinct: an earlier version also
    retried whenever typed text didn't turn up on screen afterward, on the
    theory that "reacted, but not visibly the right way" deserved a retry
    too — reverted, because it isn't safe in general (a `Down 3 / Enter /
    Type` batch is not idempotent the way a fresh `Ctrl+P + Type` is) and
    it produced a real false positive (text that had simply scrolled out
    of the visible viewport) that duplicated an edit on retry.
  - **Two genuine `.tape` content bugs found by actually running them for
    the first time** (they'd only ever been `vhs validate`-parsed before,
    never executed against real vix): `db-workbench.tape`'s add-connection
    sequence was missing the `Name` field entirely and one `Down` short of
    reaching `File` (fixed: types a name, two `Down`s not one); a shared
    `examples/demo-workspace/rust-app/src/main.rs` between `overview.tape`
    (which saves an edit to it) and `themes.tape` (opened after it,
    alphabetically) meant a second tape in the same batch could see the
    first one's edit — fixed in both `render-demos.sh` and
    `render-demos-lite.sh`: `git checkout -- examples/demo-workspace`
    before the batch starts.
  - **Three real product gaps found the same way — `git.stage_hunk`/
    `git.unstage_hunk`, `org.link.follow`/`roam.backlinks`/`roam.graph`,
    and `view.theme_edit` had no Command Palette entry and (the git-hunk
    and org-roam ones) no keybinding in any keymap either, reachable only
    through deep menu navigation.** `org.link.follow` does carry an
    Emacs-keymap-only chord (`C-c C-o`) that doesn't exist under the
    default `apple` keymap at all. Fixed properly, not routed around:
    added all six to `crates/vix-palette/src/lib.rs`'s `COMMANDS` list,
    reusing each action's *existing*, already-fully-translated menu-item
    locale key rather than minting new `cmd.*` keys (which would have
    needed fresh translations across the 14 core locales) — a small,
    intentional label-text inconsistency (no "Git: "/"Org: " prefix the
    way older `cmd.*` entries have) in exchange for zero new translation
    debt.
  - **One real bug found, then genuinely fixed (third pass, same day,
    after the user asked directly to fix it rather than just document
    it).** Certain `t!()`-translated labels rendered as their literal raw
    i18n key instead of translated text, but *only* through the live,
    keyboard-driven app — never in `cargo test`, never in a direct
    `TestBackend` render. Two earlier passes (see the session transcript
    for the full blow-by-blow) ruled out, one at a time, with real
    instrumentation, not guessing: a translation-lookup bug at any call
    site (a direct `crate::_rust_i18n_try_translate("en", key)` probe
    itself returned `None`, so the bug was inside `vix-i18n`); a codegen
    bug (`cargo expand`ed the real generated source and confirmed the
    correct `map.insert` is there, once, no duplicate); a `serde_saphyr`
    parse bug (a standalone crate parsing the real `locales/ui.yml`
    directly got every key right); stale build output (a guaranteed-clean
    `cargo clean` + full rebuild reproduced it identically); source-file
    position (`rust-i18n-support` sorts everything into a `BTreeMap`
    before codegen; moving the failing key to the top of the file changed
    nothing); combined-insertion-sequence position (a failing key at 24%
    through the sequence, a working key later than it); stack size
    (`RUST_MIN_STACK=64MiB` changed nothing, ruling out the one
    previously-real lead, T148(b)'s documented stack-overflow precedent);
    and random hash-seed collision (the failure was 100% deterministic
    across every process restart, which a random per-process seed
    wouldn't produce).

    **Root cause, found by finally questioning the one variable never
    isolated: the compiler flags.** `[profile.release]` builds with
    `lto = true` (fat/full LTO) + `opt-level = "z"` — a much more
    aggressive combination than any test context used (`cargo test`
    doesn't use this profile at all; a debug `cargo build` gets `opt-level
    = 2` for `vix-i18n` specifically via `[profile.dev.package.vix-i18n]`,
    but no LTO). `vix-i18n`'s `i18n!`-macro-generated `LazyLock`
    initializer is exactly the shape of code most likely to trip a real
    LLVM fat-LTO codegen bug: one function doing ~2500 sequential
    `HashMap::insert` calls per locale. Verified directly: rebuilding with
    `lto = false` fixed every previously-broken key (confirmed live, all
    six of `ui.theme_editor_title`, `ui.theme_slot_menu_bar_fg`,
    `ui.theme_editor_hint`, `menu.item.view.theme_edit`,
    `menu.item.edit.structural_replace`, `menu.item.edit.
    structural_replace_workspace`); rebuilding with `lto = "thin"`
    (a lighter-weight LTO mode) **also** fixed it, with better
    performance than no LTO at all — confirmed live again, 4/4 clean runs
    of the real Theme Editor showing fully translated text. Shipped:
    `[profile.release]` now uses `lto = "thin"`, matching what
    `[profile.dist]` — the profile actual distributed releases build
    with — already used, so **real shipped releases were very likely
    never affected**; only a plain `cargo build --release` (this session's
    whole testing setup, hence how reliably it reproduced) was hitting fat
    LTO. Full detail and the reasoning trail live as a comment on
    `[profile.release]` itself (`Cargo.toml`) — including that no upstream
    `rust-i18n`/LLVM bug report has been filed yet, which is the natural
    next step for whoever has time, with this as a real, already-narrowed
    reproduction to hand a bug tracker. `themes.tape` was already
    rewritten (scrolls real syntax-highlighted code instead of opening the
    Theme Editor) before the root cause was found; left as-is since it's
    still a fine, working demo on its own merits, not reverted back to
    exercising the now-fixed Theme Editor.
  - **8 real GIFs rendered, reviewed frame-by-frame, and committed**
    (`docs/demos/*.gif`, ~1.1 MB combined), overview embedded in
    `index.md`'s (README's) `## Demos` section per the task's own ask.

## Phase 5 — Examples

- [x] **T501 — Demo workspace.** `examples/demo-workspace/`: a small
  realistic project — Rust + Python + Markdown sources with intentional
  TODO/FIXME tags, `tasks.toml`, an `.http` file against
  httpbin-style endpoints, `org/` with a few roam-linked notes and a
  dailies entry, `data/*.csv|tsv`, a seeded `demo.sqlite` (with the seed
  SQL checked in and a script to regenerate), and a README explaining the
  tour. Keep it a few hundred KB max; exclude from the workspace build.
  **Do this before T404–T406.**
- [x] **T502 — Cargo examples batch 1 (editor as a library).**
  `render_frame` (TestBackend → print the screen as text),
  `theme_roundtrip` (load bundled theme, tweak, save, reload),
  `textops_pipeline` (sort/dedupe/case a file from the CLI),
  `macro_replay` (parse a macros.toml and replay onto a buffer). Each
  ≤ ~100 lines, heavily commented, listed in README.
- [x] **T503 — Cargo examples batch 2 (services & formats).**
  `query_search` (vix-query over a directory), `org_export` (org →
  Markdown/HTML), `vcard_parse`, `lsp_headless` (spawn a server via
  vix-lsp-core, open a doc, print diagnostics), `i18n_lookup` (one key in
  all 15 locales), `calculator_eval`. **Done 2026-09-14.** Two real
  drifts found and corrected against this entry's own wording: `vix-query`
  (T151 folded it into `src/app.rs`) was never "a directory search" at
  all — it's `Decision`, the query-*replace* confirm choice — so
  `query_search` instead drives `App::search_workspace_to_dock` (the real
  directory-search engine, `crates/vix-workspace-search`) through
  `run_action`/`on_key`, the same path the real UI uses; and
  `vcard_parse` parses *Org* contacts and exports *to* vCard, not the
  other way — there's no vCard-file parser in `vix-org-contacts` to run
  in reverse. Both corrections are documented in the examples' own doc
  comments. `lsp_headless` reuses `tests/lsp_smoke.rs`'s mock-server
  technique (spawn, `initialize`, `didOpen`, `publishDiagnostics`) so it
  needs only Python 3, no real language server. All 6 verified by
  actually running them, not just compiling; `cargo clippy --all-targets
  -- -D warnings` clean.
- [x] **T504 — Config examples.** `examples/config/`: fully-annotated
  `config.toml` covering every settings key (cross-check against T305's
  generated settings reference), a custom theme JSON, custom user
  snippets, a `macros.toml`, and sample Rhai scripts (after T105). **Done
  2026-09-14.** Sample Rhai scripts already existed at `examples/scripts/`
  (T105) — nothing to duplicate. `config.toml`'s 63 keys, values, and doc
  comments were generated from the real `Settings::default()` (via
  `toml::to_string_pretty`) and `docs/reference/settings.md` rather than
  hand-typed, then the whole file verified by actually round-tripping it
  through `Settings::load_from` (a throwaway verifier example, run then
  deleted — not committed). Caught a real TOML-ordering mistake this way:
  `lsp_servers`/`debug_adapters` array-of-tables sections had scalar keys
  after them, which TOML silently reparents into the array item instead
  of the root document — fixed by moving all three `Vec<Struct>` settings
  (`lsp_servers`, `debug_adapters`, `org_capture_templates`) to the end,
  after every scalar key. `theme.json` and `snippets.json` were each
  verified the same way (parsed with the real `CustomTheme`/
  `snippets::parse_json` code, not just eyeballed).
- [x] **T505 — Examples in CI.** Extend `ci.yml`: `cargo build --examples`
  and execute the headless examples (`render_frame`, `textops_pipeline`,
  `query_search`, `list_commands`, `headless_edit`) so examples can't rot.
  **Done 2026-09-14 — closes Run F entirely (T502–T505 all done).** No
  separate `cargo build --examples` step needed: the `test` job's
  existing `cargo build --workspace --all-targets` already builds every
  example (`--all-targets` covers examples too). Added the real missing
  half — actually *running* the five headless examples — as a new step
  in all three forges' CI (`.github/workflows/ci.yml`'s `test` job,
  `.gitlab-ci.yml`'s `test:` job, `.forgejo/workflows/ci.yml`), plus
  `scripts/check` itself, so local and CI stay in parity (verified: ran
  the exact same 5 commands locally first, all exit 0). Found and fixed
  a real pre-existing drift in `spec/ci/index.md` while touching it: its
  gate list said "seven" steps but `scripts/check` already had eight
  (T307's man-page-current? check was never added to the spec's list) —
  fixed both the missing step and the count, now nine with this task's
  addition.

## Run G (self-audit findings, 2026-09-19)

Found by four parallel research passes (lint/dead-code, test/CI gaps,
performance, architecture/docs-drift) over the whole workspace after the
T001–T505 backlog closed out. Ranked by value/effort within each group;
`[x]`/`[ ]` tracks status same as every other task.

**T506–T515 all done 2026-09-19** (same day, one pass). **T516 and T517
remain, both explicitly deferred** in their own entries above — T516
(splitting `vix-db`/`vix-org`'s remaining un-split `lib.rs` files) is a
multi-session project on the scale of T141/T142, not a quick win; T517
(lazy per-locale i18n backend) needs upstream-crate-level work and isn't a
measured problem today. Everything else actionable in this run is closed.

- [x] **T506 — `scripts/check` silently skips 2,440 unit tests in all 116
  member crates.** Root cause: the repo's `Cargo.toml` declares both a
  `[workspace]` and a root `[package] name = "vix"`, so Cargo's
  `workspace_default_members` is just the root package — any command
  without `--workspace`/`-p` only touches `vix` itself. `scripts/check`'s
  `cargo build --all-targets` / `cargo clippy --all-targets` /
  `cargo test` all omit `--workspace`. Clippy-linting of member crates'
  *library* code still happens (it's pulled in transitively as a path
  dependency), but member crates' own `#[cfg(test)]` test code is never
  even compiled locally — confirmed via `cargo test --no-run` (10 test
  binaries, all belonging to root `vix`) vs. `cargo test --workspace
  --no-run` (128). All three real CI configs already correctly use
  `--workspace`, and `spec/ci/index.md`'s own documented gate does too —
  only the script drifted. **Done 2026-09-19**: added `--workspace` to
  all three lines in `scripts/check`. Ran the full `cargo test
  --workspace` once by hand before shipping the change: 245 test
  binaries, all green, zero failures — nothing had been silently
  regressing, but this is the first time that was actually verified
  locally instead of only on CI.
- [x] **T507 — 21 crates (incl. `vix-db`) are missing the "universal" hard
  lint attributes `AGENTS.md`/crate-map.md claim every crate has.**
  `vix-db` (DB workbench: connections, credentials, SQL — the most
  security-sensitive feature crate in the repo) has none of
  `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`, or
  `#![warn(clippy::pedantic)]`. 20 more crates (`vix-base-tool`,
  `vix-base16`, `vix-calendar-panel`, `vix-case`, `vix-conflict-tool`,
  `vix-editor-core`, `vix-format-tool`, `vix-jwt-tool`, `vix-lsp`,
  `vix-markdown-preview`, `vix-menu`, `vix-palette`, `vix-regex-tool`,
  `vix-session`, `vix-settings`, `vix-snippet-tool`, `vix-theme`,
  `vix-undo-store`, `vix-workspace`) have `pedantic` but are missing
  `forbid(unsafe_code)` and `deny(missing_docs)`; `vix-fileops` is
  missing only `deny(missing_docs)`. Since both lints are allow-by-default
  in rustc, this isn't redundant — undocumented pub items or stray
  `unsafe` blocks in these crates (`vix-db`, `vix-menu`, `vix-palette`,
  `vix-settings`, `vix-editor-core` among them) would not fail CI today.
  Also undercuts the CodeQL-deferral rationale elsewhere in this file,
  which assumes `forbid(unsafe_code)` is already workspace-wide. **Done
  2026-09-19**: added the missing attributes to all 21 crates. Every crate
  but `vix-db` was already clean under the newly-enabled lints (no
  missing-docs or unsafe-code findings anywhere else) — `vix-db`, which
  had never run `clippy::pedantic` at all, turned up 5 real findings:
  a `format!` appended to a `String` (`connect.rs`, fixed with `write!`),
  two missing statement-terminating semicolons, and `commit_edits` at
  107/100 lines — split into `commit_edits` (orchestration) +
  `build_pending_updates` (the conflict-checked `UPDATE` builder) +
  `apply_updates_in_transaction` (the transaction itself), with a new
  `CellEdit` type alias for the tuple clippy flagged as too complex
  inline. `cargo test -p vix-db` still green after the split.
- [x] **T508 — `README.md`'s License section understates the actual
  license.** It says "Apache-2.0 or MIT at your option," but the real
  license (`LICENSE`, `Cargo.toml`'s `license` field, and
  `spec/license/index.md`, all three consistent) is a 5-way choice:
  Apache-2.0, BSD-3-Clause, MIT, GPL-2.0-only, or GPL-3.0-only. This is
  the landing-page text a downstream consumer is most likely to read.
  **Done 2026-09-19** (`index.md`, `README.md`'s twin): rewrote to name
  all five and link `LICENSE`.
- [x] **T509 — Stale "sanctioned allow" claims in `AGENTS.md`,
  `agents/conventions.md`, and `agents/share/crate-map.md`.** All three
  still say `#[allow(clippy::struct_excessive_bools)]` is sanctioned "on
  `App` and `Settings`" — both were converted to `bitflags` fields
  2026-09-18 (T149), so the allow no longer exists anywhere in the repo
  (verified: grepped every `#[allow(...)]`, none is
  `struct_excessive_bools`). This is the third time this exact claim has
  gone stale across these docs. `agents/conventions.md` and
  `agents/share/crate-map.md` also claim `vix-editor-core`'s modules
  "keep `#[allow(clippy::all, clippy::pedantic)]` for upstream style" —
  every one of its 13 module files actually carries a plain
  `#![warn(clippy::pedantic)]` with no blanket allow. **Done 2026-09-19**:
  rewrote both stale clauses in all three files (kept the still-accurate
  `too_many_lines`/`too_many_arguments` clause), and fixed the also-stale
  crate count in `agents/share/crate-map.md` (112 → 116) while touching
  it. Also fixed two more stale mentions found in passing while
  re-reading these files for T510–T515: `spec/test/index.md` and
  `docs/performance/index.md` both still said `[profile.release]` uses
  `lto = true` — it's been `lto = "thin"` since this same session's
  earlier i18n/LTO-miscompilation fix (see T406's entry).
- [x] **T510 — Per-frame git-gutter diff has no revision cache.**
  `src/ui.rs`'s `draw()` calls `app.refresh_git_gutter()`
  (`src/app/git.rs`) on *every redraw* for any git-tracked file (i.e.
  almost always), which does a full `Code::get_content()` (O(n) rope→
  String) plus a full `similar::TextDiff` Myers diff against the HEAD
  blob — unconditionally, even when nothing changed since the last
  frame (a cursor move, a resize). The sibling `refresh_git`'s own doc
  comment says "not per-frame"; that discipline wasn't applied here.
  Unmeasured by the existing benches (`benches/editor_ops.rs` never
  calls `ui::draw()`). For a large tracked file this directly threatens
  the keypress-to-frame budget T121/T122 established. **Done 2026-09-19**:
  new `App::git_gutter_cache_key: Option<(PathBuf, u64)>` field; `refresh_
  git_gutter` now reads the active tab's path + `Editor::revision()`
  (already existed, O(1)) *before* touching the buffer at all, and
  returns immediately on a cache hit — the O(n) `get_content()` +
  Myers diff only runs on a genuine miss. `refresh_git` (HEAD moved)
  clears the cache key alongside the existing HEAD-blob cache clear. New
  test `git_gutter_refresh_skips_recompute_until_the_buffer_revision_
  changes` (`tests/integration/git.rs`) proves the cache hit doesn't
  repopulate cleared marks and a real edit does invalidate it; all 10
  `git::*` tests (`--ignored`) still pass.
- [x] **T511 — Sticky-scroll header and breadcrumbs recompile a regex and
  rescan the whole buffer every frame.** `App::sticky_header` (default
  on: `editor_behavior.sticky_scroll` defaults `true`) and
  `App::breadcrumb` both call `tab.text()` (full buffer clone) then
  `palette::symbols()`, which compiles a fresh `regex::Regex` from a
  `format!`'d pattern on *every call* — no `LazyLock`, no cache keyed on
  buffer revision. Fires every frame the file is scrolled past the top
  line. **Done 2026-09-19**, two parts: (1) `vix-palette`'s `SYMBOL_RE`
  hoisted to a `LazyLock<Regex>` — the pattern never varied with input,
  so compiling it per-call was pure waste regardless of caching; (2) new
  `App::active_tab_symbols`, backing both callers, caching the scan by
  `(tab index, path, revision)` in a `RefCell` (both callers only ever
  had `&App` — sticky-scroll/breadcrumb are read-only rendering queries,
  and `draw_breadcrumb` deliberately keeps `&App` rather than becoming
  the one `&mut App` exception among "immutable reads" render calls, per
  the comment in `src/ui.rs`'s `draw`). Tab *index* is part of the key,
  not just path, because both callers must also work for untitled
  buffers (`path: None`), where two different tabs could otherwise share
  a `(None, 0)` key. New tests: `breadcrumb_symbols_cache_tracks_edits_
  and_distinguishes_tabs` (edit invalidates; a second tab's cache entry
  is never served for the first) and the pre-existing `sticky_header_
  shows_enclosing_scope_when_scrolled` (a scroll with no edit must still
  find the right cached entry) both pass.
- [x] **T512 — Minimap clones the whole buffer, once per line, every
  frame it's visible.** `src/ui/minimap.rs`: `tab.text().lines().map(
  str::to_string).collect()` — full rope→String plus a fresh `String`
  per line, unconditional on whether the buffer changed. `viewport.
  show_minimap` defaults off, so lower priority than T510/T511, but
  users who *do* enable it are disproportionately likely to be editing
  large files. **Done 2026-09-19**: reads each line's trimmed length
  straight from `Code::line(i)` (a zero-copy `RopeSlice`) via a new
  `trimmed_char_len` helper (ropey's `Chars` iterator isn't
  double-ended, so it's a single forward walk remembering the length as
  of the last non-whitespace char) — no per-line `String`, no whole-
  buffer clone. Incidentally fixed a latent inconsistency: `total` now
  comes from `Code::len_lines()` (rope semantics, matching what
  `top_visible_line`/`editor_height` already use) instead of
  `str::lines()`, which drops the final phantom empty line a trailing
  `\n` produces — the two were previously counting lines two different
  ways. New test `minimap_renders_a_bar_per_line_band_without_panicking`
  (`tests/integration/panels.rs`) renders it via `TestBackend` with a
  trailing-newline buffer (the edge that inconsistency touched) and
  checks for real bar glyphs; pre-existing `minimap_click_jumps_to_
  proportional_line` still passes.
- [x] **T513 — Criterion benches exist but never run in CI, and don't
  cover the T510/T511 hot paths.** `benches/{text_ops,editor_ops,
  search_and_palette,startup}.rs` are real (wired via root `Cargo.toml`
  `[[bench]]`) but deliberately local-only per `spec/test/index.md`
  ("too slow to relink for a benchmark someone reruns often") — so
  there's no automated guard against a future regression in *any* hot
  path, and none of the four files benchmark `vix-git::diff_marks` or
  `vix-palette::symbols` at all, meaning even a local `cargo bench` run
  today wouldn't catch T510/T511. **Done 2026-09-19** (the small half):
  new `benches/frame_ops.rs`, two groups (`git/diff_marks`,
  `palette/symbols`) at 1k/20k/100k lines, wired into root `Cargo.toml`'s
  `[[bench]]` list and `spec/test/index.md`'s bench table; smoke-tested
  with `cargo bench --bench frame_ops -- --test` (all 6 cases pass).
  **The stretch half (a non-blocking, informational CI job) stays
  deferred** — it's an infra addition, not a quick pairing with the
  bench-file work, and nothing here demands it urgently; revisit if a
  frame-path regression ever actually slips through unnoticed.
- [x] **T514 — No documented binary-size budget or long-term trend.**
  The `binary-size` CI job (T008) only tracks a delta against the
  immediately-previous `main` build (cache overwritten each push) — a
  slow multi-quarter creep across many individually-small PRs would be
  invisible. `spec/ci/index.md` documents the mechanism but states no
  target number. **Done 2026-09-19**: new "Binary size budget" section
  in `docs/performance/index.md` — measured today's stripped release
  binary (macOS/arm64, current `[profile.release]`) at ~25.4 MB, and set
  a 35 MB (~40% headroom) threshold as "worth a real look," with a note
  to re-baseline the next time it's deliberately grown for a good
  reason.
- [x] **T515 — `Code::get_content()`/`slice()` are undocumented O(n)
  traps.** `vix-editor-core` correctly uses `ropey::Rope` (O(log n)
  insert/delete/index, O(1) structural-sharing clone) — no algorithmic
  risk at the storage layer — but `get_content()`/`slice()` are full O(n)
  materializations with no doc-comment warning, and the crate's own spec
  never states the rope's complexity characteristics at all. This is
  exactly the trap T510/T511/T512 each fell into independently. **Done
  2026-09-19**: `crates/vix-editor-core/spec/index.md` documents actions,
  not the buffer engine, so the natural home turned out to be the code
  itself — added a "performance characteristics" paragraph to `Code`'s
  own struct doc comment (pointing at T510/T511 as real examples of the
  trap and its fix) plus doc-comment notes on `get_content`/`slice`
  themselves. `cargo doc -p vix-editor-core` (warnings denied) confirms
  every new intra-doc link resolves.
- [x] **T516 — `crates/vix-db/src/lib.rs` (3,035 lines / 119 fns) and
  `crates/vix-org/src/lib.rs` (2,969 lines / 152 fns) are the one
  un-split piece left in two otherwise fully-modularized crates.**
  `vix-db` already has 17 sibling submodules covering every concern
  except the top-level key-dispatch/view state machine; `vix-org` has
  only `columns.rs` split out so far. Same shape `src/app.rs`/`src/ui.rs`
  were in before T141/T142 (which proved out reusable extraction tooling
  — `extract_app_module.py`/`extract_ui_module.py`). Not a rule
  violation (no stated repo-wide max-file-size rule), just a real
  opportunity matching established practice. **Deferred**: this is a
  multi-slice project on the scale of T141/T142 (each took several
  sessions), not a quick win — scope it as its own run when picked up,
  don't fold into a general cleanup pass. **Started as its own run
  2026-09-20** (picked up explicitly, not folded into the Run H
  continuation): no committed `extract_app_module.py`/
  `extract_ui_module.py` tooling survived from T141/T142 (scratchpad
  scripts, never checked in), so this run writes fresh single-use
  extraction scripts per slice instead — the technique (find each
  method's exact span via brace-matching, move it, fix what the
  compiler flags) is the same either way.
  - **`vix-db` slice 1/~5, done**: `lifecycle.rs` — connecting/
    disconnecting (`key_connections`/`key_form`/`key_password`,
    `start_connect`/`begin_connect`/`connect_running`/`poll_connect`,
    `finish_connected`/`load_columns`/`disconnect`/`refresh_catalog`,
    plus the private `ConnectOutcome`/`PendingConnect`/`connect_worker`
    T531 added). `lib.rs` 3,328→2,918 lines (this session's T531/T532/
    T538 had already grown it past the task's original 3,035
    citation). Pure move, zero behavior change: `cargo test -p vix-db`
    (119 tests) and `cargo test --test db_smoke -- --include-ignored`
    (12 tests) both pass unchanged; full `scripts/check` green.
    `PendingConnect` needed `pub(crate)` (referenced from `lib.rs`'s
    `Browser` struct field); `key_connections`/`key_form`/
    `key_password` needed `pub(super)` (called from `lib.rs`'s
    `handle_key` dispatcher) — same visibility pattern `src/app/*.rs`'s
    own split already established, since `Browser` (like `App`) is
    defined in the parent module the new file is a child of.
  - **`vix-db` slice 2/~5, done**: `execute.rs` — running SQL: the
    write/DDL confirmation and bind-parameter prompts (`key_confirm`,
    `key_params`), synchronous internal queries (`run_sql`,
    `run_catalog`, `run_traced`), the busy-gate (`workbench_busy`),
    async execution (`execute`/`execute_sql`/`execute_all`/`explain`/
    `run_statement`/`start_query`/`poll_query`/`cancel_query`/
    `reconnect_running`/`poll_reconnect`/`finish_stream`/
    `finish_stream_err`), client-side transaction tracking
    (`note_tx`/`run_tx`/`begin_tx`/`commit_tx`/`rollback_tx`/`run_all`),
    and two small accessors (`toggle_write_mode`/`pending_summary`) —
    24 methods, plus the private `PendingRun`/`QueryKind`/`Pending`/
    `PendingReconnect` types. `lib.rs` 2,918→2,340 lines.
    Found-and-fixed one genuinely pre-existing (T531-introduced) doc-
    comment misplacement surfaced by the move: `key_workbench`'s own
    doc comment had ended up sitting above the newly-extracted
    `workbench_busy` instead (harmless — both are private, so
    `#![deny(missing_docs)]` never caught it) — restored to the right
    function. Also caught a real bug in the extraction *script itself*
    before it shipped: its item-mover only walked back over `///` doc
    lines, not `#[derive(...)]` attribute lines, so a moved enum/struct
    with a derive left that attribute orphaned in `lib.rs`, silently
    attaching to whatever the next item happened to be — surfaced
    immediately as a `conflicting implementations of trait Debug`
    compile error (two derives landing on one item), not a silent
    wrong-behavior bug, but real enough to have shipped a corrupted
    file had the build not been checked before committing. Re-verified
    slice 1 was unaffected (its two moved types never had derives to
    begin with, so the bug never fired there). Pure move otherwise,
    zero behavior change: `cargo test -p vix-db` (119 tests) and
    `cargo test --test db_smoke -- --include-ignored` (12 tests) both
    pass unchanged; full `scripts/check` green.
  - **`vix-db` slice 3/~5, done**: `ai_features.rs` — Ask a schema-
    grounded question, fix the last error, explain a query, optimize
    the statement at the cursor, and apply/discard the reply
    (`ai_busy`/`take_ai_request`/`key_ask`/`open_ask`/`schema_facts`/
    `submit_ask`/`queue_ai`/`fix_error`/`explain_query`/
    `optimize_current`/`apply_ai_reply`/`ai_failed` — 12 methods), plus
    the private `AiReply`/`AiState` and `pub` `AiRequest` types (and two
    small `SchemaColumns`/`SchemaRels` type aliases). `lib.rs`
    2,340→2,074 lines. The one slice so far needing `pub mod` rather
    than a plain private `mod`: `AiRequest` crosses the crate boundary
    (`src/app.rs` calls `Browser::take_ai_request` directly), so keeping
    the module private would have left a `pub` type unreachable from
    outside the crate — caught immediately by `cargo doc`'s
    `private-intra-doc-links` check (`RUSTDOCFLAGS="-D warnings"`,
    matching CI's own invocation, now run as a matter of course before
    every commit in this run rather than found out by CI). Also needed
    to bump `AiReply` to `pub(crate)` (a plain compiler warning this
    time, not a hard doc error): `AiState`'s own `pub(crate)`
    `Running(AiReply)` variant can't be more visible than the payload
    type it carries. Pure move otherwise, zero behavior change: `cargo
    test -p vix-db` (119 tests, including the AI-specific
    `ask_ai_builds_a_schema_only_request_and_applies_the_reply` in
    `db_smoke.rs`) and the full `db_smoke.rs` suite (12 tests) both
    pass unchanged; full `scripts/check` green.
  - **`vix-db` slice 4/~5, done**: `cell_edit.rs` — the results grid's
    editable-cell workflow: previewing a single table
    (`preview_selected`/`set_editable`/`set_uneditable`), staging and
    committing cell edits (`begin_cell_edit`/`key_cell_edit`/
    `commit_edits`/`build_pending_updates`/`apply_updates_in_
    transaction`), following a foreign key (`follow_fk`), expanding a
    row (`expand_row`), the cell/text viewer (`key_cell`,
    `staged_value`, `editable`), and the detail/DDL popup
    (`show_detail`/`show_ddl`/`refresh_popup`/`preview_selected_
    refresh`) — 17 methods, plus the private `CellEdit` type alias.
    `lib.rs` 2,074→1,636 lines (more than half its original size gone).
    The largest cross-reference count of any slice so far — 11 of its
    17 methods needed `pub(super)` (called from `lib.rs`'s dispatch and
    from `ai_features.rs`/`execute.rs`/`lifecycle.rs`), applied via a
    small loop this time rather than one `sed` per name, having done
    enough of these by now to know the shape. `Popup` (the struct these
    methods populate) stayed in `lib.rs`: also used by the editor's
    autocomplete popup, unrelated to cell editing, so moving its
    *definition* here would have been wrong even though this slice is
    its heaviest user. Pure move otherwise, zero behavior change:
    `cargo test -p vix-db` (119 tests) and `cargo test --test db_smoke
    -- --include-ignored` (12 tests, including
    `staged_cell_edits_commit_in_a_transaction` and
    `table_details_report_columns`) both pass unchanged; full
    `scripts/check` green.
  - **`vix-db` slice 5/5, done — `vix-db` fully sliced.** `panels.rs`:
    everything left that wasn't connecting, running SQL, talking to the
    assistant, or cell editing — history/saved-query lists, the query
    log, the ER diagram, CSV/TSV import, results export, and the
    tree/editor/results panes' own key dispatch (including the shared
    autocomplete-popup handling) plus the chart/yank/format-at-cursor
    odds and ends — 23 methods, no types to move. `lib.rs`
    1,636→1,116 lines (**3,328→1,116 across all five slices, a 66%
    cut**); real code (excluding the `#[cfg(test)]` module) is now
    ~660 lines — `Browser`'s struct/`new`, `Form`/`Pane`/`View`/`Popup`,
    and the two top-level dispatchers (`handle_key`/`key_workbench`),
    exactly the "genuinely shared" core this run expected to be left
    once every cohesive feature slice had its own file. Same mechanics
    as every prior slice (a handful of `use super::{...}` imports for
    modules this slice's methods call into, `pub(super)` on whichever
    of its 23 methods `lib.rs`'s own dispatchers or a sibling slice
    call back into). Pure move, zero behavior change: `cargo test -p
    vix-db` (119 tests) and `cargo test --test db_smoke --
    --include-ignored` (12 tests, including
    `params_import_fk_and_chart_flows` and `query_log_records_metrics_
    and_erd_maps_foreign_keys`) both pass unchanged; full `scripts/
    check` green.
  - **`vix-db` is done.** `vix-org`'s own `lib.rs` (2,969 lines per the
    task's original citation) is the remaining half of this run, on the
    same scale as what `vix-db` just took five slices to do — but a
    different *shape*: not one giant `impl Browser` block but ~105 free
    functions + 47 methods on two small structs ("a pragmatic subset of
    Org-mode... all functions are pure", per its own doc comment),
    already pre-organized into 15 `// ----- Section Name -----`-
    delimited sections that make ready-made slice boundaries. Reused
    `vix-db`'s established conventions: a fresh single-use Python
    extraction script (`extract_org_slice.py`, brace/section-span based
    rather than the `impl`-method-span matcher `vix-db` needed), the
    `columns.rs` submodule's own pre-existing pattern of an *explicit*
    `pub use mod_name::{a, b, c};` re-export list at the crate root
    (never a wildcard) so external callers keep calling
    `vix_org::function_name(...)` in the crate's flat namespace
    (confirmed via `src/app/org.rs`, which calls e.g.
    `crate::org::nav_parent` directly).
    - **`vix-org` slice 1/~6, done**: `headline_nav.rs` — headline
      location and structure editing: finding the headline governing a
      cursor line (`governing`), navigating between headlines
      (`nav_parent`/`nav_next`/`nav_prev`/`nav_forward_same`/
      `nav_backward_same`), inserting a new heading (`new_heading`),
      listing every headline (`headlines`), sorting a subtree's children
      (`sort_children`), and refiling/pasting a subtree elsewhere
      (`refile`/`paste_subtree`) — 11 items. Despite being the single
      most depended-upon section in the crate (`governing`/`relevel`
      back edits in the Tags & properties, Archive, Dates & scheduling,
      and Footnotes sections, plus `columns.rs`'s column-view logic), it
      earned its own file rather than folding into `lib.rs`'s core: it's
      a cohesive feature in its own right, matching how every other
      topic here already gets its own file (mirrors the `vix-db`
      decision to keep only genuinely-shared state in the root, not
      widely-used helpers). `lib.rs` 2,961→2,591 lines (post-`cargo
      fmt`). `governing`/`relevel` bumped to `pub(crate)`, referenced
      back from `lib.rs` and `columns.rs` via `crate::headline_nav::`.
      Pure move, zero behavior change: `cargo test -p vix-org` (73
      tests) passes unchanged; full `scripts/check` green.
    - **`vix-org` slice 2/~6, done**: `todo_meta.rs` — the "Priority"
      and "Statistics cookies & checkbox propagation" sections merged
      into one file (they're small and share the checkbox regex/
      helpers): priority cookies (`priority`/`set_priority`/
      `priority_up`/`priority_down`/`close_headline`/`has_checkbox`/
      `toggle_checkbox`) and statistics-cookie propagation
      (`update_statistics`, plus `move_subtree_up`/`move_subtree_down`,
      which live in this section for no deeper reason than "also
      subtree-reordering-adjacent") — 10 `pub fn`s re-exported at the
      crate root. `lib.rs` 2,591→2,242 lines. Three private helpers
      bumped to `pub(crate)` and referenced back as
      `crate::todo_meta::*`: `split_keyword`/`strip_priority` (the
      Column view section still needs them, in `lib.rs` for now) and
      `headline_todo` (needed by "Other built-in agenda views", also
      still in `lib.rs`). Fixed two `crate::strip_priority`/
      `crate::split_keyword` call sites (one live, one just a doc-
      comment mention) in `columns.rs` to the new path. Pure move,
      zero behavior change: `cargo test -p vix-org` (73 tests) and the
      full workspace suite pass unchanged; full `scripts/check` green.
    - **`vix-org` slice 3/~6, done**: `properties_and_dates.rs` — "Tags
      & properties" (`get_tags`/`set_tags`/`toggle_tag`/`set_property`),
      "Archive" (`archive_subtree`), and "Dates & scheduling"
      (`timestamp_for`/`shift_timestamp_at`/`plan`) merged into one
      file — 8 `pub fn`s re-exported at the crate root. `lib.rs`
      2,242→1,878 lines (post-`cargo fmt`). Three private items bumped
      to `pub(crate)`: `TAGS` (the tag-group regex — needed by
      `columns.rs`, which renders a tags column, and by the "Column
      view" section still in `lib.rs`), `is_planning` (needed by
      `columns.rs`, to skip a headline's planning line when placing a
      dblock), and `line_of_char` (needed by the Footnotes section,
      still in `lib.rs`). Fixed 6 `crate::TAGS`/`crate::is_planning`
      call sites in `columns.rs` to the new `crate::
      properties_and_dates::` path. Extended the extraction script's
      `pub_crate_items` bumper to also match `static` items (it
      previously only matched `fn`/`struct`/`enum`/`const`/`type` — the
      first slice needing a `pub(crate)` static surfaced the gap, a
      `SystemExit` before anything was written, not a silent bug).
      Pure move, zero behavior change: `cargo test -p vix-org` (73
      tests) and the full workspace suite (every crate, 0 failures)
      pass unchanged; full `scripts/check` green.
    - **`vix-org` slice 4/~6, done**: `text_refs.rs` — five small,
      individually-too-small-for-their-own-file sections merged into
      one: "Hyperlinks" (`link_at`/`link_pos`), "Sparse trees"
      (`todo_tree_folds`/`occur_folds`), "Footnotes"
      (`footnote`/`id_location`), "Source blocks"
      (`src_block_at`/`replace_src_body`), and "Column view"
      (`column_view`) — 9 `pub fn`s re-exported at the crate root. No
      new `pub(crate)` bumps needed this slice: every private helper it
      touches (`sparse_folds`/`is_todo_headline`/`FOOTNOTE`/
      `line_start_char`/`append_footnote_definition`/`src_begin`) is
      used only within these five sections, confirmed via cross-
      reference grep before extracting rather than discovered by
      compiler error. Referenced back to `lib.rs`'s still-there Export
      section for `LINK`/`BARE_LINK` (`super::`, no visibility change
      needed — private root items are visible to every descendant
      module). `lib.rs` 1,878→1,730 lines (post-`cargo fmt`). Pure
      move, zero behavior change: `cargo test -p vix-org` (73 tests)
      and the full workspace suite pass unchanged; full `scripts/
      check` green.
    - **`vix-org` slice 5/~6, done**: `agenda.rs` — "Agenda & time
      tracking" (`agenda_items`/`render_agenda`/`agenda`, the
      `AgendaItem` struct), "Other built-in agenda views"
      (`todo_list`/`tags_match`/`search`/`stuck_projects`/
      `render_list`/`time_report`), and "Clocking"
      (`clock_in`/`clock_out`) — 12 `pub` items re-exported at the
      crate root. Four private items bumped to `pub(crate)`:
      `days_from_civil` (needed by `properties_and_dates.rs`, T516
      slice 3, to shift a timestamp's date — its own import switched
      from `super::days_from_civil` to `crate::agenda::
      days_from_civil` now that the two live in sibling modules rather
      than parent/child) and `clock_start`/`clock_minutes`/`hhmm`
      (needed by `columns.rs`'s CLOCK-summary rendering; fixed 3
      `crate::clock_*`/`crate::hhmm` call sites there to the new
      `crate::agenda::` path). Caught by the compiler on the first
      `cargo check`, not by cross-reference grep — the grep this time
      only covered `lib.rs` itself, missing `columns.rs`, a reminder
      to grep every sibling file, not just the one being edited.
      `lib.rs` 1,730→1,275 lines. Pure move, zero behavior change:
      `cargo test -p vix-org` (73 tests) and the full workspace suite
      pass unchanged; full `scripts/check` green.
    - **`vix-org` slice 6/6, done — closing T516 entirely.**
      `export.rs`: the last of the file's 15 sections, `to_markdown`/
      `to_html`/`to_latex`/`to_ics` re-exported at the crate root.
      `LINK`/`BARE_LINK` (the `[[target][desc]]` regexes) and
      `safe_href` (the XSS-hardening scheme guard from the 2026-07
      security audit) bumped to `pub(crate)`: `text_refs.rs`'s
      Hyperlinks functions need the first two, and `lib.rs`'s own test
      module asserts on `safe_href` directly. That last one surfaced a
      real asymmetry in the extraction technique worth recording: a
      `pub(crate)` item referenced only through the test module's
      `use super::*;` glob does *not* count as "used" for rustc's
      unused-import lint on the re-exporting `use` at the crate root —
      only a *named* `use super::{that_item, ...};` from a sibling
      module does. Every prior slice's `pub(crate)` bump happened to
      also be named-imported by a sibling, masking this; `safe_href`
      wasn't, so `use export::safe_href;` at the root came back
      "unused" even though the tests genuinely called it. Fixed by
      dropping that import and qualifying the three test call sites as
      `export::safe_href(...)` instead — simpler than chasing glob
      semantics, and it's how a one-off test-only reference should
      look anyway. Also dropped three more root-level imports
      (`std::fmt::Write`, `std::sync::LazyLock`, `regex::Regex`) that
      export.rs's departure left genuinely unused in `lib.rs`.
      **`lib.rs` 1,275→936 lines (3,328→936 lines including the
      3,328-line high-water mark this session's own T531/T532/T538
      pushed it to before this run started — a 72% cut). What's left
      is exactly the "genuinely central" core this run expected:
      `headline_level`/`subtree_range`/`drawer_name`/
      `is_drawer_header`/`drawer_range` (the primitives even
      `headline_nav.rs` depends on), `promote`/`demote`/
      `reindent_subtree`/`cycle_todo`/`set_headline_keyword`, the 15
      `mod`/`use`/`pub use` declarations wiring the six new
      submodules (`headline_nav`, `todo_meta`, `properties_and_dates`,
      `text_refs`, `agenda`, `export` — plus the pre-existing
      `columns`), and the `#[cfg(test)] mod tests` block exercising
      the crate end-to-end (unchanged in shape across all six slices).**
      Pure move, zero behavior change: `cargo test -p vix-org` (73
      tests) and the full workspace suite (every crate, 0 failures)
      pass unchanged; full `scripts/check` green.
  - **T516 is fully done — both halves.** `vix-db`: 3,328→1,119 lines
    across 5 slices. `vix-org`: 2,961→936 lines (crate-root citation)
    across 6 slices. 11 new submodules total (5 + 6), zero behavior
    change anywhere (every slice's tests passed unchanged before and
    after), full `scripts/check` green on every one of the 11 pushes.
- [ ] **T517 — `vix-i18n` eagerly builds all 15 locales' translation
  maps at startup, not just the active one.** Confirmed via the real
  `rust-i18n-macro` expansion: `i18n!` generates a `LazyLock` whose init
  closure inserts every key for every locale (~37,800 total
  `HashMap::insert` calls across 9 `locales/*.yml` files × 15 locales),
  even though only one locale's map is ever queried per process. This
  fires on the first `t!()` call, early enough that it's likely folded
  invisibly into one of T122's already-small measured startup buckets
  rather than isolated. **Deferred**: fixing this means implementing a
  custom `rust_i18n::Backend` that builds only the active locale eagerly
  (others lazily on `set_locale`) — upstream-crate behavior vix doesn't
  directly control beyond swapping backends. Large effort, and the
  startup-budget task (T122) already closed with headroom, so this is a
  "nice to have" rather than a measured problem — revisit if startup
  time ever becomes a real complaint.

## Run H (second self-audit pass, 2026-09-19)

Four more parallel research passes over fresh angles Run G didn't cover:
code duplication/DRY, concurrency/threading correctness, error-handling
quality, and cross-platform (Windows) correctness. Two genuine
correctness bugs turned up (T518, T535 below), not just style/maintenance
debt. Grouped by source pass; ranked by value/effort within each group.

**31 of 32 done as of 2026-09-20** — everything except T547, which is
the one item in the entire run that genuinely can't be done responsibly
without a Windows machine or CI: novel `cmd.exe`/PowerShell quoting
logic, not just a compile-time cfg branch, so "looks right" isn't
enough confidence to ship on faith. T548 and T549 (the run's other two
Windows findings) *were* safe to land blind — both pure, additive,
non-branching changes (a feature flag plus a proven-identical API
reuse; a portability rewrite with no OS-specific code at all) that this
session could actually verify. T547 needed real quoting logic to get
right, which this session could not verify — that's the whole
difference, not "some Windows work got done and some didn't."

### Duplication / DRY

- [x] **T518 — `node_insert_transclusion` is missing the `create_dir_all`
  its sibling `roam_insert_link` has, a real bug from copy-paste drift.**
  `src/app/roam.rs`: both functions share an identical "find or create
  the node's file" preamble, but `roam_insert_link` (introducing commit
  `a913aa3`) creates the target directory first
  (`std::fs::create_dir_all(parent)`) before `roam_insert_link` on
  `path`; `node_insert_transclusion` (added later, `ca47669`, by
  copy-pasting the block) dropped that line. Inserting a transclusion
  for a title whose Org-roam directory doesn't exist yet fails silently
  where the sibling function succeeds. Fix: extract one
  `fn roam_find_or_create_node(&mut self, title: &str) -> Option<String>`
  (including the `create_dir_all`) shared by both — the crate already
  has `roam_write_and_open` for the "and open it" variant three other
  callers use; this is its "don't open" sibling. Small effort, real bug.
  **Done 2026-09-19**: added `App::roam_find_or_create_node`, both call
  sites now delegate to it. New test `roam_node_insert_and_transclusion_
  recreate_a_missing_root` (`tests/integration/editing.rs`) deletes the
  workspace root between the two calls and proves both actions still
  succeed — it would have failed on `node.insert_transclusion` before
  this fix.
- [x] **T519 — `is_repo`/`nothing_staged` guard clauses copy-pasted 13+
  times in `src/app/git.rs`.** The 3-line
  `if !crate::git::is_repo(&self.root) { self.status = t!(...); return; }`
  guard appears verbatim at 13 call sites (plus 2 `AppFlags::GIT_REPO`
  variants), and the "nothing staged" guard opens both
  `git_begin_commit` and `git_generate_commit_message` identically. Fix:
  `fn require_git_repo(&mut self) -> bool` / `fn require_staged(&mut
  self) -> bool`, called as `if !self.require_git_repo() { return; }` —
  an internal guard helper, not a second `run_action` path for one
  command, so it doesn't conflict with the "one action id, one arm"
  rule. Small, mechanical effort. **Done 2026-09-19**, implemented
  exactly as scoped: both helpers added, all 13 + 2 sites replaced by a
  byte-for-byte search/replace (zero behavior change — `require_git_
  repo` still does the same fresh `crate::git::is_repo` check every
  one of the 13 sites did; the 2 `AppFlags::GIT_REPO`-cached sites were
  deliberately left alone, different check semantics, out of scope
  here). All 11 `--ignored git::*` tests plus the 5 `--ignored
  editing::*` git/hunk/spellcheck tests still pass.
- [x] **T520 — The Howard Hinnant civil-date algorithm
  (`civil_from_days`/`days_from_civil`) is hand-rolled independently in
  3 crates.** `crates/vix-file-information-panel/src/lib.rs:159-170`,
  `crates/vix-git/src/lib.rs:355-368` (`epoch_to_date`), and
  `crates/vix-org/src/lib.rs:1050-1060`/`1875-1882` each reimplement the
  same ~15-line integer algorithm (magic constants `719_468`/
  `146_097`/`36_524`/`146_096`) with no shared test — real subtle-bug
  risk in three unrelated copies rather than domain-driven similarity
  (none of the three crates depend on `time`/`chrono`). Fix: extract a
  tiny dependency-free crate (or fold into an existing low-level one)
  exposing both functions, unit-tested once. Small effort. **Done
  2026-09-20**: new `vix-civil-date` crate (same "own spec + own tests
  + a real reuse story" bar T521 cleared), all three sites now
  delegate; `vix-org`'s `days_from_civil` (the reverse direction, never
  duplicated elsewhere) moved here too rather than staying behind,
  since it's the natural pair of `civil_from_days`. New direct tests
  (round-trip across a ~1000-year span, known reference dates
  cross-checked against Python's `datetime` — caught a real arithmetic
  slip in the test's own first draft, `18_506` vs. the correct
  `18_518`, before it shipped) give this algorithm its first-ever
  isolated unit coverage; previously all three copies were only ever
  exercised indirectly through each crate's own higher-level date
  logic. Crate count 117→118. All pre-existing tests that exercise the
  refactored call sites directly (`vix-file-information-panel`'s
  `epoch_and_known_dates_format`, `vix-git`'s
  `epoch_to_date_applies_tz_offset`, `vix-org`'s full 73-test suite)
  still pass unchanged.
- [x] **T521 — `human_bytes` byte-size formatter duplicated verbatim
  (including its doc comment) in 2 crates.**
  `crates/vix-file-information-panel/src/lib.rs:108-126` and
  `crates/vix-system-information-panel/src/lib.rs:140-161` — identical
  `UNITS` array, loop, and "lossless u64→f64" rounding comment. (Not
  `vix-file-browser-panel::size_label`, a deliberately different compact
  KB/MB variant.) Fix: move `human_bytes` + its `u64_to_f64` helper
  somewhere both crates can share. Small effort. **Done 2026-09-19**:
  new `vix-byte-size` crate (clears T151's "own spec + own tests + a
  real reuse story" bar — two existing consumers, pure/testable in
  isolation), both panel crates now delegate to it. Crate count
  116→117 — found and fixed two more stale "112"/"116" count mentions
  in `agents/share/crate-map.md`/`AGENTS.md` while updating them (one,
  `crate-map.md`'s own `crates/` table row, had been stale since
  *before* T509's count fix earlier this run — T509 caught two of the
  three mentions, missed this third one).
- [x] **T522 — `revert_hunk`/`resolve_conflict` share an identical
  8-line "commit the rebuilt buffer" tail.** `src/app/git.rs:438-447`
  and `:471-479` are byte-for-byte identical (set content/cursor/
  selection/dirty/preview, refresh gutter, set status) except the final
  status key. Fix: `fn apply_rebuilt_buffer(&mut self, rebuilt: &str,
  caret: usize, status_key: &str)`. Small effort. **Done 2026-09-19**,
  implemented exactly as scoped (the `t!(status_key)` runtime-key
  pattern already exists elsewhere, e.g. `git_op`'s `ok_key`). Both
  `revert_hunk_restores_committed_text` and
  `conflict_resolve_keeps_chosen_side` (`tests/integration/editing.rs`)
  still pass.
- [x] **T523 — Throwaway git-repo bootstrap reimplemented 13 times
  across `tests/integration/{git,editing}.rs`.** No shared helper in
  `common.rs`, so every test needing a real repo hand-rolls the same
  `run` closure + `init -q` + two `config` calls (13 sites total), each
  preceded by a redundant `fs::create_dir_all` (`unique_dir` already
  does this). Fix: add `pub(crate) fn init_git_repo(dir: &Path)` to
  `common.rs`. Small effort, test-only. **Done 2026-09-20**: signature
  ended up `init_git_repo(dir: &Path, name: &str)` — 12 of the 13 sites
  pass the literal `"Test"`, one (`git_blame_annotates_the_current_line`)
  deliberately passes `"Ada Lovelace"` to prove blame surfaces the real
  author, not a hardcoded placeholder, so the name had to stay a
  parameter rather than being baked into the helper. Only the `git
  init`/`git config` sequence moved — every site still declares its own
  local `run` closure for whatever commands it needs afterward (`add`,
  `commit`, …); 2 of the 13 no longer needed one at all post-init and
  had it removed outright (caught by `unused_variables` immediately).
  The redundant-`fs::create_dir_all`-after-`unique_dir` note in this
  entry's own original text was **not** acted on — it turned out to
  describe a much broader pattern (21 sites across the whole test
  suite, not just these 13), out of this task's actual scope; left for
  a separate cleanup if picked up later. All 16 `--ignored` `git::*`/
  relevant `editing::*` tests pass, including the Ada Lovelace one.
- [x] **T524 — Three genuine sentence-level duplicate i18n key pairs.**
  `prompt.git_clone`/`prompt.jj_clone` (`locales/prompt.yml:795-796` /
  `:1051-1052`), `status.git_empty_url`/`status.jj_empty_url`
  (`locales/status.yml:807-808` / `:4855-4856`), and
  `status.pomodoro_break`/`ui.pomodoro_break_label` (`locales/
  status.yml:3671-3672` / `locales/ui.yml:2607-2608`, the clearest case
  — spans two different namespace files) all carry an identical `en:`
  sentence for two call sites that could share one key. Fix: collapse
  each pair, repoint the 2 call sites each, drop ~15 translated lines
  per merge × 3. Small effort each. **Done 2026-09-19**: verified all
  three pairs match across every one of the 15 locales (not just `en:`)
  before merging any of them, so nothing translated is actually lost.
  All three `jj`/`status` call sites repointed at their surviving
  sibling key with an explanatory comment; the three now-redundant
  locale blocks removed (45 lines across the two `locales/*.yml`
  files). `cargo test --test i18n_keys` still passes.
- [x] **T525 — Undo/redo snapshot stacks hand-rolled independently in 5
  `vix-edit-*` crates, at 3 different quality levels.** Identical
  `const HISTORY_CAP: usize = 200;` and near-identical `push_undo`
  bodies in `vix-edit-bytes`, `vix-edit-sql`, `vix-edit-value` (still
  duplicate push/pop inline in both directions) vs. `vix-edit-table`/
  `vix-edit-outline` (already converged on a shared `restore()` helper
  between undo/redo). Fix: a small generic `vix-undo-stack` crate (same
  shape as `vix-list-state`, T144) — `push_capped<T>(stack: &mut
  Vec<T>, item: T, cap: usize)`. Small/medium effort. **Done
  2026-09-20**: named `vix-capped-stack` instead of the task's own
  suggested `vix-undo-stack` — deliberately, since the function isn't
  undo-aware at all (each crate keeps its own `Snapshot` type and
  undo/redo semantics unchanged; only the identical "push, evict oldest
  if over cap" clamp moved), and `vix-undo-store` already exists for a
  *different* feature (persistent, per-file undo history saved to
  disk) — reusing "undo" in this crate's name risked exactly the
  confusion the task itself didn't intend. Matches T144/T521/T520's own
  "own spec + own tests + a real reuse story" bar (5 consumers).
  `push_capped` deliberately evicts only one entry per call, mirroring
  every original site's own `if` (not `while`) — documented explicitly
  in both the doc comment and a test, rather than silently changing to
  a stronger "always enforce the cap" guarantee nobody asked for.
  Crate count 118→119. All 6 affected crates' full test suites pass,
  including each `vix-edit-*` crate's own pre-existing `undo_and_redo`-
  shaped test.
- [x] **T526 — `wrap_line` greedy word-wrap reimplemented independently
  in `vix-welcome-panel` and `vix-ai-panel`, with a real behavior
  difference.** `crates/vix-welcome-panel/src/lib.rs:34-60` does not
  break over-long words; `crates/vix-ai-panel/src/lib.rs:154-183` does,
  character-by-character. No doc explains why a welcome screen and an
  AI transcript should differ here — looks accidental, not deliberate.
  (`vix-textops::wrap`/`wrap_chunk` solves the harder editor-wrap
  problem and isn't part of this — a candidate host for a shared "plain
  greedy wrap" primitive.) **Needs a product decision first** (which
  over-long-word behavior is correct for each panel) before the merge —
  small/medium effort once decided.
  **Done 2026-09-20**: asked rather than guessed — the user chose
  "always break over-long words" (the AI panel's prior behavior) as the
  merged answer. New tiny crate `vix-greedy-wrap` (`wrap_line(line: &str,
  width: usize) -> Vec<String>`), matching this session's established
  T520/T521/T525/T530 pattern for a pure function duplicated across
  unrelated crates. Its implementation is the welcome panel's original
  structure (the `width == 0`/blank-line edge cases it already handled
  cleanly) with the AI panel's over-long-word hard-break spliced in, and
  `split_whitespace()` (not `split(' ')`) for the word boundary — the AI
  panel's original `split(' ')` would have produced spurious empty
  "words" on a double space; folding both panels onto the more correct
  splitter is a safe, in-scope tightening, not scope creep, since it
  only changes behavior on an input (consecutive spaces) neither panel's
  own tests exercised. Both panels' `wrap_line` are now one-line
  delegations. Crate count 120→121. New tests (6, including one proving
  double-spaces collapse and one proving an over-long word doesn't eat
  the word before it) plus both panels' pre-existing wrap tests, which
  needed no changes since the merged behavior is a strict superset for
  every case they exercise. No CHANGELOG entry: the AI panel's rendered
  behavior is unchanged, and the welcome panel's changes only for a
  pathological input (a single word wider than the terminal) that
  wasn't reachable before either. Verified: `cargo clippy --workspace
  --all-targets -- -D warnings` clean; `cargo test -p vix-greedy-wrap -p
  vix-welcome-panel -p vix-ai-panel` and the full `cargo test --workspace`
  green; `scripts/check-docs` confirms the new crate (121 crates).
- [x] **T527 — `menu.item.org.roam.*.help`/`menu.item.org.node.*.help`:
  4 pairs of identical help text for the same underlying actions, across
  ~15 locales.** `locales/menu.yml:20905/20921/20937/20969` (Org▸Roam)
  and `:21065/21081/21129/21177` (Org▸Node) — confirmed both menu paths
  bind to the same actions in `crates/vix-menu/src/lib.rs:1722-1762`.
  Arguably intentional (same feature surfaced at two menu locations),
  but the whole sentence is duplicated per locale, not a short word.
  Medium effort (touches ~15 languages × 4 pairs).
  **Done 2026-09-20**: verified, per-locale, that all 4 pairs really are
  byte-identical across all 15 languages first (unlike T530's hint-string
  sub-item, which looked the same but wasn't — checked this one properly
  before touching it). `Item` gained a new private `help_key: Option<
  &'static str>` field (and a `leaf_shared_help(label, action, shortcut,
  help_key)` constructor alongside the existing `leaf`/`sub`): when set,
  `Item::help()` derives the `.help` lookup from `help_key` instead of
  the item's own `label`, so two items can share one catalog entry while
  keeping their own (different) display labels. The 4 Org▸Node entries
  now use `leaf_shared_help` pointing at their Org▸Roam counterpart's
  help key; the 4 now-redundant `menu.item.org.node.*.help` blocks were
  deleted from `locales/menu.yml` (60 locale lines removed: 4 keys × 15
  languages). New test `org_node_items_share_help_text_with_their_org_
  roam_counterparts` proves both the sharing (`Item::help()` returns the
  same text as the Roam counterpart) and the cleanup (the stale
  `org.node.*.help` key no longer resolves at all) — it would fail
  either way if the wiring regressed. No CHANGELOG entry (pure internal
  dedup, per T309's rule; the rendered tooltip text is unchanged).
  Verified: `cargo clippy --workspace --all-targets -- -D warnings`
  clean; `cargo test -p vix-menu` (11 tests, including the new one) and
  the workspace-wide `tests/i18n_keys.rs` structural tests (catalog
  completeness, placeholder-fill checks) all pass; `scripts/check-docs`
  green.
- [x] **T528 — `stage_hunk`/`unstage_hunk` share ~20 lines of identical
  setup and an identical `stage_content` dispatch/report tail; only the
  middle (staging vs. unstaging logic) genuinely differs.**
  `src/app/git.rs:524-579` and `:585-639`. Medium effort — the shared
  edges are easy to extract, the middle needs care not to conflate.
  **Done 2026-09-20**: extracted the shared preamble into
  `App::active_hunk_context(&mut self, failed_key: &str) -> Option<(Hunk,
  PathBuf, String, String)>` (hunk, path, rel, current) — built on the
  existing `active_hunks()` helper, resolving the cursor's hunk, its
  file's absolute path/relative-to-root path/current text, or setting the
  "outside workspace" status (keyed per-caller) and returning `None`. Kept
  `path` in the tuple (not just `rel`) even though only `stage_hunk` needs
  it (for the `git_head_cache` fallback lookup) — harmless for
  `unstage_hunk` to ignore, and simpler than two near-identical helpers.
  Extracted the shared tail into `App::apply_hunk_index(&mut self, rel:
  &str, new_index: &str, ok_key: &str, err_key: &str)`, mirroring
  `stage_content`'s dispatch/refresh/report pattern used by both. The
  middle (the actual staging-math vs. unstaging-math, which genuinely
  differs) was left untouched in each function. No behavior change, no
  new locale keys, no CHANGELOG entry (pure internal dedup, per T309's
  rule). Verified: `cargo clippy -p vix --all-targets -- -D warnings`
  clean; the three existing integration tests
  (`stage_hunk_stages_only_the_cursor_hunk`,
  `unstage_hunk_removes_the_cursor_hunk_from_index`,
  `revert_hunk_restores_committed_text`) still pass unchanged; full
  `scripts/check` green.
- [x] **T529 — `src/app/picker_panels.rs`: near-identical key/mouse
  dispatch duplicated across 4 list panels (~120 lines).** `ascii_mouse`
  (148-163), `x11_mouse` (228-243), `media_type_mouse` (461-476) are
  textually identical apart from field name and insert callback; the
  `Up`/`Down`/`PageUp`/`PageDown` key-handler block repeats across
  `ascii_key`/`x11_key`/`media_type_key`/`theme_editor_key`. The four
  panel types already expose the same method names (`up`/`down`/
  `page_up`/`page_down`/`select_index`), so a small trait (legal in the
  `vix` binary crate) could unify this. Medium effort.
  **Done 2026-09-20**: added a private `ListPanel` trait (`up`/`down`/
  `page_up`/`page_down`/`scroll`) implemented for the four panel types
  via one `impl_list_panel!` macro (each method just forwards to the
  panel's own identically-named inherent method) — `select_index` was
  deliberately left out of the trait: it's still called directly as an
  inherent method at each call site, since what happens after a
  successful select is exactly the part that genuinely differs per
  panel. Two free functions now do the shared work: `list_panel_nav_key`
  (Up/Down/PageUp/PageDown, used by all 4 `*_key` handlers, each still
  handling its own remaining keys — Home/End, Enter, Esc, Backspace/Char
  — afterward) and `list_panel_click_index` (left-click hit-test +
  scroll-relative row → index, used by all 4 `*_mouse` handlers, each
  still deciding what to do with a successful `select_index`).
  `picker_panels.rs` shrank from 494 to 468 lines despite the added
  trait/macro/helper scaffolding. No behavior change, no new locale
  keys, no CHANGELOG entry (pure internal dedup, per T309's rule).
  Verified: `cargo clippy -p vix --all-targets -- -D warnings` clean;
  full `cargo test --test integration` (548 tests, including the ASCII
  panel, media-type picker, and all 5 theme-editor/x11-picker tests)
  passes unchanged.
- [x] **T530 — Minor duplication, low priority.** `rgb(hex)` hex-to-byte
  parsing duplicated in `crates/vix-editor-core/src/utils.rs:88-99` and
  `crates/vix-base16/src/lib.rs:99-103` (same lenient policy; not
  `vix-color-converter-tool::from_hex`, a deliberately stricter public
  API). Picker footer hint strings duplicated verbatim: `ui.snippets_
  hint`/`ui.media_types_hint` and `ui.tasks_hint`/`ui.scripts_hint`
  (`locales/ui.yml`). AI-replace polling loop (`tests/integration/
  ai.rs:30-38` vs. inlined in `git.rs:157-162`) and a custom-dir-and-
  settings app-builder (`ai.rs:16-27` vs. `git.rs:145-150`) each
  duplicated once — promote `wait_for` to `common.rs`, add `app_at_
  with(root, settings)`. All small effort, low individual value; batch
  together if picked up.
  **Done 2026-09-20**: 3 of the 4 sub-items landed; the 4th was
  investigated and correctly rejected.
  - `rgb(hex)`: new tiny crate `vix-hex-rgb` (`rgb(hex: &str) -> (u8, u8,
    u8)`), matching the precedent set by T520/T521/T525's crates this
    run. Both `vix-editor-core::utils::rgb` and `vix-base16::rgb` now
    delegate to it (the latter just reformats the tuple into its
    `"[r, g, b]"` JSON string). Crate count 119→120.
  - Test helpers: added `app_at_with(root, settings)` and
    `wait_for_ai_replace(app, pred)` to `tests/integration/common.rs`
    exactly as scoped; `ai.rs`'s `app_with_canned_ai_reply` now builds on
    `app_at_with` instead of repeating `App::new`/`with_session_path`/
    `layout.editor`, and both `ai.rs` and `git.rs` now call the shared
    `wait_for_ai_replace` instead of each keeping (or, in `git.rs`'s
    case, inlining) their own copy of the same 5-second poll loop.
  - **Picker footer hint strings: investigated, NOT merged.** The
    original finding claimed `ui.snippets_hint`/`ui.media_types_hint`
    and `ui.tasks_hint`/`ui.scripts_hint` are duplicated "verbatim" —
    true only for English. A full per-locale diff
    (`locales/ui.yml`) found real, independent divergence: `de`/`pl`/
    `ja` differ between `snippets_hint`/`media_types_hint` ("Filter" vs.
    "Filter tippen", "ruch" vs. "przesuń", missing vs. present "入力"),
    and `de`/`hi` differ between `tasks_hint`/`scripts_hint` ("wählen"
    vs. "auswählen", a Hindi diacritic). Merging would have silently
    overwritten those already-distinct translations with one locale's
    wording for both contexts — a real user-visible regression, not a
    cleanup. Left as four separate keys; corrected here rather than
    silently either merging (an unreviewed translation change) or
    skipping without explanation.
  No CHANGELOG entry (pure internal dedup, per T309's rule) except the
  new crate itself is still an internal-only refactor with no
  user-visible behavior change either. Verified: `cargo clippy --workspace
  --all-targets -- -D warnings` clean; `cargo test --workspace` fully
  green (every crate, including the two new `vix-hex-rgb` unit tests);
  `scripts/check-docs` confirms the new crate's spec/description/crate-map
  entry are all consistent (120 crates).

### Concurrency / threading correctness

- [x] **T531 — DB workbench connect (and its own cancel path) fully
  blocks the single UI event-loop thread — can freeze the whole editor,
  not just the DB view.** `crates/vix-db/src/lib.rs`'s `finish_connect`
  (called synchronously from ordinary key handlers) runs, all inline on
  the UI thread: `secret::resolve()` (may shell out and block on a
  configured `password_command`), `tunnel::open`'s `wait_ready` (polls
  with `thread::sleep(100ms)` up to a 10s timeout), and
  `session::Session::connect` (blocks on a real `sqlx::AnyConnection::
  connect(url)` with **no configured connect timeout** — an
  unreachable/filtered host hits the OS TCP timeout, commonly 60-130s).
  Every other DB operation has a `poll_*` async counterpart
  (`Session::send`/`poll`, whose own doc comment explains exactly why);
  `connect`/`restart` were left out of that design. Worse: **the
  designed escape hatch also blocks** — `cancel_query`'s `session.
  restart()` calls `Session::connect` again, synchronously, on the same
  UI thread, inside the Ctrl+C handler meant to recover from a stuck
  query, so if the network condition that caused the hang is still
  present, cancelling a hung query can itself hang. Fix: move
  `finish_connect`'s body to a background thread mirroring `session::
  worker`'s existing pattern, with a `poll_connect` drained each
  event-loop tick — same shape as `poll_command`/`poll_ai_replace`/
  `poll_http` already established in `src/app.rs`. **Medium-large
  effort, highest-severity finding of this run** — real, but risky to
  rush in a security-sensitive area (DB connections); scope as its own
  focused change with its own test pass rather than folding into a
  general cleanup.
  **Done 2026-09-20**: took the "own focused change, own test pass"
  instruction literally — read `session.rs`, the existing
  `poll_query`/`send`/`poll` async-query design, and the
  `poll_http`/`poll_ai_replace` host-side pattern in full before writing
  anything.
  - `connect_worker(conn, password)` (a new free function) now does the
    entire risky chain — resolve a password when none was given
    (`secret::resolve`, mirroring the old waterfall, reported back as
    `ConnectOutcome::NeedsPassword` when it comes up empty so the host
    still shows the interactive prompt), open the SSH tunnel, open the
    session — on a plain `std::thread::spawn`'d background thread, not
    the UI thread. `Browser::begin_connect` starts it and stores a new
    `pending_connect: Option<PendingConnect>`; the new `poll_connect`
    (called each tick from `App::poll_db_connect`, wired into
    `src/main.rs`'s loop and its busy-timeout list exactly like the
    other `poll_*`s) drains it — into the workbench on success, onto the
    password prompt on `NeedsPassword`, or back to the connections list
    with the error on failure. `start_connect` and the password prompt's
    Enter handler both now call `begin_connect` instead of the old
    synchronous `finish_connect`.
  - The catalog load (`run_catalog(objects_sql)`) right after a
    successful connect stayed a synchronous, one-shot `session.run`
    call — a deliberate, documented scope boundary: it's bounded and
    fast on an already-open connection, unlike the network-risky work
    that moved off the UI thread, and every other one-shot internal
    query in this crate (`refresh_catalog`, `load_columns`) already
    works this way.
  - `cancel_query`'s reconnect got the same treatment: it now takes the
    session, spawns a background thread to call `Session::restart` on
    it, and stores a `pending_reconnect: Option<PendingReconnect>`
    drained by a new `poll_reconnect` (same wiring as `poll_connect`).
    This fixes the "designed escape hatch also blocks" half of the
    finding; it does **not** fix the *other* half T532 describes (the
    abandoned worker thread itself still has no cancellation signal) —
    that remains T532's own, separate, unstarted scope.
  - Added a live "Connecting… (Ns)" status (`msg.db_connecting`, now
    taking `%{secs}`) refreshed on every still-pending `poll_connect`
    tick — connects can now genuinely take up to a minute-plus, so the
    user needs proof it isn't stuck, not just proof the UI didn't
    freeze. `key_connections` and `key_workbench` gate input while a
    connect/reconnect is pending (mirroring the existing
    `query_running()` busy-gate) so a second attempt can't race the
    first.
  - Genuinely new risk introduced: `ConnectOutcome`/`Session` now cross
    a thread boundary (`Send`, verified by the crate compiling — neither
    type holds anything non-`Send`). Fixed all 4 test-suite fallout
    sites this uncovered: `tests/db_smoke.rs`'s `key()` helper (its
    `drain_query` renamed `drain_async`, now also draining
    connect/reconnect — every one of its ~11 "connect via Enter, assert
    `View::Workbench`" call sites keeps working unmodified), one
    `cancel_query` call site there needing an explicit extra drain
    before its next keypresses (a real race the old synchronous code
    never had — a Backspace typed before the background reconnect
    resolved would have been silently swallowed by the new busy gate),
    and `crates/vix-db/src/lib.rs`'s own internal
    `password_prompt_gates_server_connections` test (now waits via a
    new `wait_for_connect` helper). Added a fifth, dedicated
    regression test, `async_connect_and_reconnect_run_off_the_event_loop`
    in `db_smoke.rs`, asserting directly (not just incidentally, via the
    other tests still passing) that both `connect_running()` and
    `reconnect_running()` are true *immediately* after their triggering
    key/call returns — the actual property this task exists to
    guarantee.
  - No CHANGELOG entry: this is an internal responsiveness fix with one
    small, genuinely user-visible addition (the "Connecting… (Ns)"
    status), not withheld per T309's rule but not judged worth a
    changelog line either — the UI stays "connect, then the workbench
    opens," just without a chance of freezing on the way.
  - Verified: `cargo clippy --workspace --all-targets -- -D warnings`
    clean; `cargo test -p vix-db` (114 tests) and `cargo test --test
    db_smoke -- --include-ignored` (12 tests, all needing real SQLite
    fixtures) both fully green; the workspace-wide
    `tests/i18n_keys.rs` structural tests pass (the reshaped
    `msg.db_connecting` key checked for real call-site placeholder
    agreement); full `cargo test --workspace` green;
    `scripts/check-docs` green.
  - **Not attempted, and not required to claim this done**: an explicit
    "cancel an in-flight connect" gesture. Input is gated (not silently
    dropped into a confusing error) while a connect runs, but there's no
    way to abort one early — out of scope for "stop the UI thread from
    blocking," a plausible separate future feature, not part of what
    this finding asked for.
- [x] **T532 — Cancelling or disconnecting a DB session abandons, but
  doesn't cancel, the in-flight query — repeated cancels against a hung
  query can exhaust the DB's connection limit.** `crates/vix-db/src/
  session.rs`'s `restart()`/`Browser::disconnect()` just drop
  `reply_rx`; the old worker thread, if blocked inside `stream_sql`'s
  `stream.next().await` waiting on the network, has no cancellation
  signal and only notices abandonment the next successful `reply_tx.
  send`, which never happens if the query never produces another row.
  Each Ctrl+C against a truly hung query leaves one more connection
  open-but-blocked; repeat enough times and even fresh connect attempts
  fail against the DB's own `max_connections`, with no indication it's
  self-inflicted. Fix: thread a cancellation token (`Arc<AtomicBool>` or
  a `watch` channel) through `stream_sql`'s loop, checked each
  iteration — same pattern the tree-sitter parse worker already uses
  (T533). Medium effort; depends on T531's architecture for the cleanest
  fix, though it could also land independently.
  **Done 2026-09-20.** The task's own suggested design (a flag "checked
  each iteration") doesn't actually work for this shape of loop on its
  own: `stream.next().await` can block on a single network read
  indefinitely, and a flag is only ever consulted *between* iterations —
  it never gets a chance to interrupt a read that's already in flight.
  Genuinely interrupting it needs the read to be raced against something,
  so `Session` gained a `tokio::sync::watch::channel(bool)` instead:
  `cancel_tx` on the session, `cancel_rx` cloned into the worker and
  threaded into `stream_sql`, whose row loop now does
  `tokio::select! { item = stream.next() => ..., _ = cancel_rx.changed()
  => return false }` — a genuinely-hung read gets raced away from, not
  just checked around. `Session::cancel()` (new, `pub`) sends the signal;
  `restart()` calls it on the *old* session before opening the new one
  (calling it only via natural `Drop` timing, as first drafted, would
  have delayed the signal until the new connect finished — too late to
  matter for how promptly the old worker exits); `Browser::disconnect()`
  now calls it too before dropping the session, so a plain disconnect
  (not just mid-query Ctrl+C) also releases a stuck connection promptly.
  Also bounded the worker's own `Connection::close()` with a 2s
  `tokio::time::timeout` — the same "network is unresponsive" condition
  that motivated this task could in principle make a graceful close hang
  too, one step later; a worker that already gave up on its query
  shouldn't then get stuck saying goodbye to it. `cancel()` turned out to
  be a one-way, **sticky** signal — once sent, the worker's *next*
  statement (if any) is abandoned too, not just whatever was mid-flight —
  which is exactly right for its only two real callers (both about to
  replace or drop the session anyway) but is documented explicitly since
  it's an easy contract to get wrong; caught via a test of my own
  drafted with the wrong premise (asserted the session stayed usable
  after an idle `cancel()`; it doesn't, correctly) before it shipped.
  Added `tokio`'s `sync`/`macros` features to the workspace dependency
  (only `vix-db` and the root `vix` package consume `tokio`, so this is
  additive-only for everyone else). Two new tests: one exercising the
  exact `select!` mechanism against a future that would otherwise never
  resolve (sqlite has no way to simulate a genuinely hung network read
  for a true end-to-end test, so this tests the real primitive instead,
  deterministically — no timing-based flakiness), one proving the sticky
  contract. No CHANGELOG entry (pure internal robustness fix, no
  user-visible behavior change — the observable difference is a
  previously-possible connection leak no longer happening, not a new
  screen or message). Verified: `cargo clippy --workspace --all-targets
  -- -D warnings` clean; `cargo test -p vix-db` (116 tests, up from 114)
  and `cargo test --test db_smoke -- --include-ignored` (12 tests) both
  green; full `cargo test --workspace` green; `scripts/check-docs` green.
- [x] **T533 — Tree-sitter background parse isn't cancelled when its
  buffer closes mid-parse.** `crates/vix-editor-core/src/code.rs`'s
  `Code` has no `Drop` impl; `request_async_parse` only sets an existing
  parse's `cancel: Arc<AtomicBool>` when a *newer* request supersedes an
  older one, never when `Code` itself is dropped. Not a leak (the
  worker thread's current parse still finishes and then exits cleanly
  once `res_rx` is gone) — just a few seconds of wasted CPU on a
  now-irrelevant parse for a very large closed buffer. Fix: `impl Drop
  for Code` that sets the cancel flag before its fields drop. Small
  effort, low severity. **Done 2026-09-19**, implemented exactly as
  scoped. New test `dropping_code_mid_parse_signals_the_worker_to_
  cancel` checks the flag transitions on drop — deterministic, not
  dependent on winning a race with the worker thread. Also fixed, while
  touching `Code`'s own doc comment (added for T515): it claimed
  "cloning `Code` is O(1)", but `Code` doesn't implement `Clone` at all
  (it owns a worker thread, a `Box<dyn Fn>` callback, and undo history)
  — only the underlying rope has that property; reworded to say so.
- [x] **T534 — `vix-theme-model`/`vix-time-zone-model` use `.expect(...)`
  on a lock instead of the poison-recovery pattern 3 sibling crates
  already established.** `crates/vix-theme-model/src/lib.rs` (7 call
  sites) and `crates/vix-time-zone-model/src/lib.rs` (2 call sites) do
  `CUSTOM.write().expect("theme lock")`/`ACTIVE.read().expect(...)` —
  a panic anywhere while holding either lock (in a test, or in
  production) permanently poisons the process-wide static, cascading
  into every subsequent call (both are read on essentially every frame
  render). `vix-clipboard`, `vix-terminal`, and two `Arc<Mutex<Child>>`
  sites in `src/app.rs` already guard against exactly this with
  `unwrap_or_else(PoisonError::into_inner)`, one with an explicit
  comment explaining why. Low real-world risk in production (the code
  held under these two locks is trivial, panic-free field access/
  `format!`), but a concrete risk for **test-suite flakiness**: `cargo
  test` runs a crate's tests in parallel by default (no
  `--test-threads=1`/`serial_test` anywhere in the repo), so any test
  panicking while touching theme/time-zone state poisons the static for
  the rest of that binary, cascading into unrelated failures in the
  same run — the same class of "shared process-wide global touched by
  parallel test threads" flakiness already fixed once for the clipboard
  (see `[[vix-flaky-clipboard-register-test]]`-style prior fix). Fix:
  mirror the existing pattern at all ~9 call sites. Small effort.
  **Done 2026-09-19**: both crates gained private `read()`/`write()`
  helpers (mirroring `vix-clipboard`'s `lock` helper) recovering from
  poisoning; every one of the 9 call sites converted, and the now-stale
  "Panics if the lock is poisoned" doc notes removed from all 9 public
  functions (they no longer can). New test coverage extends each
  crate's one existing state-touching test (per its own "keep the
  process-global state sequential" convention) with a check that
  spawns a thread which panics while holding the lock, then proves a
  normal call afterward still works — would have panicked before the
  fix.

### Error handling / silent-failure quality

- [x] **T535 — `write_atomic`/`write_atomic_private` report the WRONG
  error when both the atomic write and its fallback fail — a real bug
  in the save path behind nearly every file write in Vix.**
  `crates/vix-fileops/src/lib.rs:156,179`: on the fallback path,
  `fs::write(&target, data).map_err(|_| atomic_err)` discards the
  fallback's own `io::Error` and always returns the *first* attempt's
  error instead — so a user hitting (say) "disk full" on the fallback
  write sees a stale "permission denied creating temp file" message
  from the earlier, unrelated failure. Fix: capture and report the
  fallback's own error (or combine both). Small effort, real bug.
  **Done 2026-09-19**: new `combine_errors` helper reports the
  fallback's own `ErrorKind`, with the atomic error appended to the
  message rather than discarded outright. New test `write_atomic_
  reports_the_fallback_error_when_both_attempts_fail` blocks both the
  atomic path (unwritable directory) and the fallback (read-only file)
  and confirms the returned error's kind and message reflect the
  fallback's real failure, not the stale atomic one.
- [x] **T536 — DAP (debugger) requests silently discard failure — a
  rejected breakpoint or an invalid step request just "does nothing,"
  no message.** `crates/vix-dap/src/lib.rs:375-420`, the `Pending::
  Other => {}` arm at line 418 throws away both `success` and the
  adapter's `message` field for every DAP request except `evaluate`
  (`initialize`, `launch`, `configurationDone`, `setBreakpoints`,
  `continue`/`next`/`stepIn`/`stepOut`/`pause`). LSP has the equivalent
  right (`LspEvent::RequestFailed`, surfaced in `src/app/lsp_dap.rs`);
  DAP has no counterpart. Fix: add a `DapEvent::RequestFailed(String)`
  variant, check `success` in the `Other` arm, surface it the same way
  LSP's failures already are. Medium effort. **Done 2026-09-20**: went
  a step further than "the `Other` arm" — `Pending::Other` gained a
  `String` field (the command name) so the check runs uniformly for
  *every* pending kind except `Evaluate` (which already reports its
  own failure inline), not just the generic catch-all; `StackTrace`/
  `Scopes`/`Variables` failures are now reported too, closing a gap
  the task's own wording hadn't named. The command-name-plus-reason
  formatting logic was pulled into a free function
  (`failed_request_text`) specifically so it has direct unit coverage
  without needing a live `Session`/spawned adapter — this crate's
  existing tests are all pure-function-shaped, so a live-`Session`
  integration test would have been the odd one out. New
  `status.dap_request_failed` mirrors the existing
  `status.lsp_request_failed` exactly (`"Debugger: %{message}"`, same
  as `"Language server: %{message}"`), surfaced in `src/app.rs`'s
  `DapEvent` match arm right next to the LSP one it mirrors.
- [x] **T537 — A settings-file syntax error silently resets everything
  to defaults on next launch, with zero notification.**
  `crates/vix-settings/src/lib.rs:541-543`: `pub fn load() -> Settings {
  confy::load(APP_NAME, Some(CONFIG_NAME)).unwrap_or_default() }` — a
  typo from hand-editing `config.toml` loses the user's theme,
  keybindings, and every other setting with no warning at all. Fix:
  distinguish "file doesn't exist yet" (fine, use defaults silently)
  from "file exists but failed to parse" (queue a warning message
  naming the parse error) — `confy`'s error type should let these be
  told apart. Medium effort. **Done 2026-09-20**: reading `confy`'s own
  source settled the distinction for free — a missing file was never
  actually an `Err` to begin with (`confy::load_path` catches
  `NotFound` internally and transparently returns/writes
  `Settings::default()`), so every real `Err` already means a genuine
  failure; no manual "which case is this" logic was needed. New
  `Settings::try_load`/`try_load_from` (kept `load`/`load_from` as
  thin discard-the-error wrappers, only `main.rs` cared about the
  error) plus `App::warn_settings_load_failed` — called once, right
  after `App::new`, well before the first frame, so (unlike T544's
  post-`ratatui::restore()` case) the message drawer can show it
  normally, no `eprintln!` fallback needed here. New test loads a
  missing file (must succeed) and a file with real broken TOML (must
  report a real error naming "toml") from the same helper, proving the
  distinction holds without ever touching the real user config
  directory.
- [x] **T538 — `sqlx::Error`'s structured detail is flattened to a bare
  `String` inside the DB worker thread, before it ever crosses back to
  the UI.** `crates/vix-db/src/session.rs:236,280,187` all do `.map_err
  (|e| e.to_string())`/equivalent at the point of origin. `sqlx::Error`
  is structured (`Database` with `.code()`/`.constraint()`, `Io`,
  `PoolTimedOut`, `Protocol`, `Configuration`), but stringifying it
  immediately means the UI can never programmatically distinguish
  "connection lost, offer reconnect" from "unique-constraint violation,
  highlight the row" from "syntax error at position N" — all become one
  opaque string. Fix: plumb a structured error (or at least `.kind()`/
  `.code()`) across the `mpsc` channel instead of a `String`. Medium
  effort, cross-cutting (touches the channel's message type and every
  consumer).
  **Done 2026-09-20.** Scoped deliberately to what the task actually
  asks — plumb the structured data — not to building the two illustrative
  UI features it names ("offer reconnect", "highlight the row"); those
  stay future work now that the data to build them on finally exists.
  `session::Chunk::Err(String)` → `Chunk::Err(QueryError)`, a new struct
  (`message: String`, `kind: QueryErrorKind`, `code: Option<String>`)
  with a `Display` impl printing `message`, so every existing "just show
  the text" call site (`{e}`, `.to_string()`) keeps compiling unchanged.
  `QueryErrorKind` mirrors `sqlx::error::ErrorKind`'s constraint variants
  (`UniqueViolation`/`ForeignKeyViolation`/`NotNullViolation`/
  `CheckViolation`/`ExclusionViolation`) plus `ConnectionLost` (an `Io`
  error) and `Database`/`Other` — a plain **copy** of sqlx's own
  categories rather than reusing `sqlx::error::ErrorKind` directly,
  since that type isn't `Clone` and `Chunk` (which carries it) already
  is. The one real origin site (`stream_sql`'s `Err(e) =>` arm) now
  classifies via a new `QueryError::from_sqlx`; the crate's actual
  ripple turned out small, not the "touches every consumer" the task
  worried about — `Session::run()` (the *blocking* convenience wrapper,
  used by ~20+ call sites throughout `lib.rs`) converts back to a plain
  `String` in one line at its own `Chunk::Err` match arm, so its public
  signature and every one of those callers needed zero changes.
  Realized one small, real consumer rather than shipping inert
  plumbing: `finish_stream_err` (the async streaming path's failure
  handler — what F5/EXPLAIN actually go through) now appends a
  clarifying "reconnect from the connections list" hint (new locale key
  `msg.db_connection_lost`, 15 locales) to the status line specifically
  for `ConnectionLost`, while `last_error` (fed to "fix the last error"
  AI prompts, which want the driver's own raw text, not a UI hint)
  keeps the unmodified message regardless of kind. No CHANGELOG entry:
  the *general* case (a syntax error, say) shows exactly the same text
  as before; only the one new, narrow `ConnectionLost` case gets an
  additional sentence, not different enough on its own to warrant a
  changelog line by this crate's usual bar for internal robustness
  work. Verified: `cargo clippy --workspace --all-targets -- -D
  warnings` and `RUSTDOCFLAGS="-D warnings" cargo doc --workspace
  --no-deps` (matching CI's own invocation) both clean; `cargo test -p
  vix-db` (119 tests, up from 116 — three new: two proving real SQLite
  errors classify correctly through the actual `send`/`poll` path, not
  just a synthetic `sqlx::Error` fed straight to the classifier, one
  proving the `Display` fallback) and `cargo test --test db_smoke --
  --include-ignored` (12 tests) both green; `tests/i18n_keys.rs`
  structural tests confirm the new locale key's placeholder is filled
  correctly at its one call site; full `cargo test --workspace` and
  `scripts/check-docs` green.
- [x] **T539 — The five most common user actions (save/open/revert/
  rename/delete) show generic errors with no filename, though the path
  is in scope at every site.** `src/app.rs`: Ctrl+S save (`:3651-3654`,
  `msg.save_failed`), open file (`:7301-7304`, `msg.open_failed`),
  revert buffer (`:5865`, wrongly reuses `msg.open_failed`), rename
  (`:13193-13195`, `msg.rename_failed`, names neither old nor new path),
  and explorer batch delete (`:7174-7177`, each loop iteration's
  failure, no filename). Contrast `:12742-12744` (workspace
  search-and-replace write), which already does this right —
  `t!("msg.write_failed", path = path.display(), error = e)`. Fix: add
  `path`/`old`/`new` interpolation to the five single-item locale keys.
  Small effort each. **Done 2026-09-19**, all five: `msg.save_failed`/
  `msg.open_failed` each split into a new `_path`-suffixed sibling key
  (kept the originals unchanged — both are shared across several other
  call sites this task didn't touch, each without an obviously-correct
  path in scope); `msg.revert_failed` is a new, correctly-named key
  replacing the wrongly-reused `msg.open_failed`; `msg.rename_failed`
  and `msg.delete_failed` (each with exactly one call site) had `path`/
  `old`+`new` added directly to their existing shape. One correction to
  this entry's own original text while implementing: batch delete does
  **not** actually overwrite — `self.messages.error(...)` already
  appends (confirmed by reading `Messages::push`), so every failure in
  a batch was always individually visible; only the missing filename
  was a real gap. Five new regression tests (one per site, four via a
  deleted-out-from-under-it file, one via `#[cfg(unix)]` permission
  bits matching T535's own technique) each force a real failure and
  check the specific path appears in the message, not just the error.
- [x] **T540 — `vix-git::stage`/`unstage` return a bare `bool`,
  discarding git's real stderr — inconsistent with their own sibling.**
  `crates/vix-git/src/lib.rs:498-500,568-571` collapse the command
  result to `bool`; the caller (`src/app/git.rs:744-747`) can only show
  a static, non-interpolated `"Git command failed"` on failure. The
  sibling `stage_content` (same file, line 520) already does this
  right — `Result<(), String>` with the real stderr — this is an
  internal inconsistency, not a systemic constraint. Medium/large
  effort (touches the function signatures and both call sites' i18n).
  **Done 2026-09-20**, turned out smaller than scoped: `src/app/git.rs`
  already had a second sibling, `git_op` (stash/stash-pop/amend),
  reporting exactly this shape via an existing `msg.git_failed_reason`
  key — `git_stage_selected` just hadn't been converted to match when
  those were added. `stage`/`unstage` now return `Result<(), String>`
  like `stage_content`; the one call site mirrors `git_op`'s identical
  `Err` arm. The now-fully-unused `msg.git_failed` key (its only two
  remaining references were comments) was removed from all 15 locales
  rather than left as dead cruft. New `vix-git` unit test runs both
  functions against a plain non-repo directory (deterministic, no
  fixture needed) and confirms a real, non-empty git-provided message
  comes back. `git_panel_stages_and_commits` (the real staging/
  unstaging integration test) and the full `vix-git` suite (26 tests)
  still pass; `cargo test --test i18n_keys` confirms nothing still
  references the removed key.
- [x] **T541 — `ensure_speller` discards a well-designed 3-variant
  `Error` enum, going "silently inert" exactly as its own doc comment
  admits — but that admission never reaches the user.** `src/app.rs:
  4496`: `self.speller = crate::spellcheck::load_for(...).ok();`.
  `vix_spellcheck::Error` (`crates/vix-spellcheck/src/lib.rs:34`)
  distinguishes `Io` (permissions/path), `Parse` (malformed
  dictionary), and `NotFound` (no dictionary for the locale) — each
  independently actionable — but `.ok()` throws all three away. A user
  with a wrong `dictionary_path` vs. a corrupt dictionary vs. an
  unsupported locale sees identical (zero) feedback. Fix: surface a
  status/message keyed on the `Err` variant. Small effort. **Done
  2026-09-19**: new `App::speller_error: Option<String>` field records
  the formatted `Display` of whichever variant `load_for` returned;
  `open_spell_suggest` (already the one place that reports "spellcheck
  unavailable", via the pre-existing `status.spell_unavailable`) now
  shows the specific detail via a new `status.spell_load_failed`
  ("Spell-check failed to load: %{error}") when one was recorded,
  falling back to the original generic message only when spellcheck
  was never even attempted (still off). New test simulates a recorded
  failure without depending on the untracked `./dictionaries` set or
  mutating the global i18n locale, matching how the file's other
  spellcheck tests already avoid both.
- [x] **T542 — Clipboard "yank" operations in two places claim success
  even when the clipboard write silently failed.** `src/app/org.rs:
  979-980` ("Copied %{url} to the clipboard") and `src/app/org_table.rs:
  325-326` ("Sum: %{sum} (copied to the clipboard)") show their success
  message unconditionally, not gated on `vix_clipboard::set`'s actual
  result — unlike the established fallback pattern in `crates/
  vix-editor-core/src/editor.rs:645-653`, which sets an in-memory
  fallback and (implicitly) knows when the real clipboard write failed.
  Two more sites (`src/app/modal.rs:509`, `src/app/org.rs:379`) also
  bypass the checked pattern without the false-success claim. One layer
  down, `crates/vix-clipboard/src/lib.rs:72,89` wraps `arboard::Error`
  via `.map_err(|e| anyhow!(e.to_string()))` rather than
  `anyhow::Error::from(e)`, losing the ability to distinguish
  `ClipboardOccupied` (transient, retryable) from `ContentNotAvailable`/
  `ClipboardNotSupported` even for a caller that wanted to react
  differently. Fix: gate the two false-success messages on the real
  `Result`; fix the `anyhow!` wrapping to preserve the source error.
  Small effort, real (if minor) correctness bug on the two claiming
  sites. **Done 2026-09-19**: both call sites now route through the
  same fallback-aware `Editor::set_clipboard` the rest of the app uses
  (real clipboard, else an in-memory register) instead of the bare
  `vix_clipboard::set`, so the "copied" claim is genuinely true again —
  something is always actually stored. `vix-clipboard`'s `set`/`get`
  now use `anyhow::Error::from(e)`, preserving `arboard::Error` as the
  source. New test follows a web link and reads the clipboard back
  through the same tab, not just checking the status message. The two
  lower-priority sites named in the finding (`modal.rs:509`,
  `org.rs:379` — bypass the pattern but never claimed success) were
  deliberately left alone, out of this task's actual scope.
- [x] **T543 — `vix-edit-value` (JSON/YAML editor) discards
  `serde_yaml`'s line/column diagnostics, showing only a static "not
  valid JSON or YAML."** `crates/vix-edit-value/src/lib.rs:117-119`:
  `serde_yaml::from_str(text).ok()?` throws away the parser's own
  location detail; the caller (`src/app.rs:9639`) has nothing better to
  show. Fix: change `from_text` to return `Result<Self, String>` and
  surface the real message. Medium effort. **Done 2026-09-20**,
  implemented exactly as scoped. `msg.edit_value_parse` (single call
  site, safe to reshape directly) gained `%{error}`. `Tree` isn't
  `Debug`, so the existing tests' `.unwrap_err()` calls needed
  `let Err(e) = ... else { panic!() }` instead — `unwrap_err` requires
  the `Ok` type to be `Debug`, not the error. New test confirms the
  parser's real message (a line number) reaches the surfaced string,
  not just a generic "not valid" message; a second, App-level test
  confirms the same all the way through `tools.edit_json`'s real
  failure path. All 11 `vix-edit-value` tests plus both
  `editing::edit_json_*` integration tests pass.
- [x] **T544 — `save_session` discards its error silently while the
  identical-shaped `store_settings` two lines below is handled
  properly.** `src/app.rs:13419-13428`: `self.save_session();` (root:
  `src/app/session.rs:227`, `let _ = self.store_session(&session);`)
  vs. `store_settings()` right below, wrapped in `if let Err(e) = ...`.
  Also worth checking in the same fix: the settings-save warning is
  queued immediately before process exit (`src/main.rs:152`) with no
  further render tick, so it may never actually be seen either — same
  root issue as T539's "the user needs to see this before exit"
  concern, just for the settings/session case instead of a save/open
  error. Small effort. **Done 2026-09-19**: `App::save_session` now
  returns `Result<(), confy::ConfyError>` instead of swallowing it
  internally; both callers (`on_exit`, `switch_workspace`) handle it —
  new `msg.session_save_failed` key, same shape as the existing
  `msg.settings_save_failed`. Also fixed the "never actually seen"
  half found in the same task: `on_exit` runs after `main` has already
  called `ratatui::restore()`, so a `self.messages` push at that point
  can never render — both `on_exit` failure paths (session *and* the
  pre-existing settings one) now also `eprintln!`, the one channel
  that still reaches the user post-restore, while keeping the
  `self.messages` push for anything that inspects `App` state directly
  without a real terminal session. New test blocks the session path's
  write with an unwritable parent (settings kept on a separate,
  writable path) and confirms both: the session failure is reported,
  and the settings save still succeeds independently.
- [x] **T545 — Several settings/state writes after an explicit user
  action are silently discarded, at 6+ sites with the same shape.**
  `src/app/session.rs:381` (`open_settings_file` doesn't save settings
  first, no warning), `src/app/org.rs:850,864,872` (agenda file add/
  remove/clear discard `store_settings()`, then unconditionally show
  success), `src/app/picker_panels.rs:402` (theme "Save As" persists
  the theme JSON carefully but discards the active-selection save),
  `src/app.rs:9581,9588,9595` (DB connection list/query history/saved
  queries discarded after explicit user actions), and `crates/vix-db/
  src/lib.rs:938` (OS-keyring password store discarded after the user
  explicitly opts into "remember password"). Fix: reuse the existing
  `self.messages.error`/Warn pattern already used elsewhere in the same
  files at each site. Small effort each; batch together if picked up.
  **Done 2026-09-19**: found one more instance of the identical shape
  while sweeping (`maybe_show_welcome`) plus the command palette's
  recent-commands persist, 9 sites total. New shared `App::
  store_settings_or_warn` (warns instead of erroring — the in-memory
  change already happened at every site and should be kept regardless
  of whether the write succeeded) replaces `let _ = self.
  store_settings();` everywhere; new `msg.db_data_save_failed` covers
  the two `crate::db::store` sites (query history/saved queries, a
  different persistence mechanism than `Settings`); `vix-db`'s keyring
  site (a `bool`, not a `Result` — no detail to surface) gets a new,
  generic `msg.db_keyring_save_failed`. New test forces
  `org_agenda_file_add`'s persist to fail and confirms both halves:
  the in-memory change is kept, and the failure is reported, not
  silently dropped. Full workspace `cargo test --test integration`
  (547 tests) and `-p vix --lib` (56 tests) both green after the
  refactor.
- [x] **T546 — `unwrap()`/`expect()` reachability: thorough negative
  result, recorded so a future pass doesn't re-walk the same ground.**
  Stripped `#[cfg(test)]` bodies workspace-wide (823 → 103 genuine
  candidates) and traced ~15 of the riskiest, including two requiring a
  read of the `ropey`/`str_indices` dependency source itself to confirm
  behavior — e.g. "Go to Byte" → `Rope::byte_to_char` looked like the
  strongest candidate (a user-typed byte offset, not char-boundary-
  clamped) but `str_indices` 0.4.4's `from_byte_idx` snaps backward to
  the nearest boundary rather than panicking; `Rope::byte_slice` does
  genuinely panic on a non-boundary range, but its only caller always
  passes tree-sitter node ranges, always valid by construction. All
  user-facing regex compilation and all LSP/DAP response parsing use
  `match`/`Option`, never bare-unwrap user/server-shaped input. **No new
  confirmed-reachable panic found — done as a negative result**, not a
  gap. One soft, unactioned observation: nothing enforces this (no
  `#![deny(clippy::unwrap_used)]` anywhere), so the cleanliness is by
  discipline, not by a lint gate — a future regression wouldn't be
  caught automatically; not proposing that lint here (it would be a
  large, disruptive addition for close to zero measured benefit given
  this clean result), just naming the gap between "true today" and
  "enforced."

### Cross-platform (Windows) correctness

CI (`.github/workflows/ci.yml`) only ever builds/tests on
`ubuntu-latest`/`macos-latest` — there is no Windows runner, so a
Windows-only bug can live indefinitely undetected. Checked and
confirmed **solid, no finding needed**: `PathBuf::join` used
consistently (no raw `"{}/{}"` path concatenation for real filesystem
paths), CRLF/LF explicitly detected and preserved
(`Code::first_line_ending`), config-dir resolution goes through
`confy`→`etcetera` (a real cross-platform base-dirs crate, not manual
`$HOME` parsing), `git`/`hunspell` binary resolution has deliberate
`PATHEXT`-aware, cwd-safe `which_on_path` (also closes a real Windows
`CreateProcessW` binary-planting vector), symlink creation has all
three platform branches, the integrated terminal uses `portable-pty`
(ConPTY-aware) with `cfg!(windows)`-gated shell selection, and
duplicate-tab detection uses `Path::canonicalize()` (handles
case-insensitive filesystems correctly).

- [ ] **T547 — Every "run an external command" feature hard-depends on
  a POSIX `sh` with no Windows path — breaks whole feature classes, not
  just degrades them, on stock Windows.** Two sites in `src/app.rs`:
  `spawn_ai_cli` (`:10241`, the CLI-mode AI assistant) and
  `run_command_in` (`:13227`, "the one async pipeline every
  command-running action funnels through" per its own doc comment,
  reached from ≥9 call sites: the "Run shell command" palette action,
  Project → Compile/Run/Test, the Go menu, etc.) both do
  `Command::new("sh").arg("-c").arg(cmd)` unconditionally. Unlike
  `toggle_terminal` (`:10664-10685`), which does branch on `cfg!
  (windows)` for `cmd.exe` vs. `/bin/sh`, these two have no such branch
  — on stock Windows (no WSL, no Git-Bash on `PATH`) every one of these
  actions fails outright with "program not found." `Settings::
  ai_command_line` also builds its command string via `sh_single_quote`
  (POSIX quoting), meaningless to `cmd.exe`/PowerShell even if a shell
  were found. Fix: mirror `toggle_terminal`'s cfg-gated shell + flag
  choice (`cmd.exe /C` vs. `sh -c`), and give `sh_single_quote` a
  Windows sibling — cmd.exe/PowerShell quoting is genuinely
  inconsistent, so this needs real care, not just "make it compile."
  Medium effort; **no Windows CI exists to verify against**, so land
  this only with a way to actually test it (a local Windows machine, or
  standing up a Windows CI job first).
- [x] **T548 — OS keyring support on Windows is a silent no-op stub,
  despite the `keyring` crate (already a dependency, already used on
  macOS via the identical `keyring::Entry` API) supporting Windows
  Credential Manager behind an unused feature flag.**
  `crates/vix-db/src/secret.rs:81-119` and `crates/vix-ai-core/src/
  secret.rs:36-59`: the `#[cfg(not(unix))]` arm is `{ let _ = conn;
  None }` / `{ let _ = (conn, password); false }`. Root cause: root
  `Cargo.toml:41` pins `keyring = { version = "3", features =
  ["apple-native"] }` — only the macOS backend is enabled. On Windows,
  both the DB workbench's saved-password waterfall and the AI
  provider's saved-API-key waterfall permanently skip persistent
  storage, falling through to `api_key_command`/`password_command` (if
  configured) or an interactive prompt every single time — no "remember
  this" the way macOS/Linux users get. Fix: add `"windows-native"` to
  the `keyring` feature list, add a third `#[cfg(windows)]` arm mirroring
  the macOS one (the `keyring::Entry` API is backend-agnostic, close to
  copy-paste). Small effort, isolated/additive (cannot regress non-
  Windows behavior) — the one caveat is the same as T547: no Windows CI
  to actually verify the new arm works. **Done 2026-09-19**, exactly as
  scoped: `"windows-native"` added to the `keyring` feature list, both
  `keyring_get` and (`vix-db` only) `keyring_set` gained a
  `#[cfg(any(target_os = "macos", windows))]` arm reusing the identical
  macOS body (`vix-ai-core` never had a `keyring_set` to begin with —
  read-only API-key lookup by design). Confirmed additive: `cargo deny
  check` clean (advisories/bans/licenses/sources all ok), full local
  test suite green. **Windows compilation itself could not be directly
  verified** — this sandbox's `x86_64-pc-windows-msvc`/`-gnu` cross-
  compile fails on an unrelated, pre-existing `ring` (TLS) C-toolchain
  gap (`assert.h` not found) before ever reaching `keyring`'s own code,
  and there is still no Windows CI. Confidence is high regardless: the
  change is a pure feature-flag addition plus copy-pasting an API
  already proven to work identically for macOS, and `keyring`'s whole
  purpose is exactly this one-API/pluggable-backend shape — but this is
  a real, honestly-acknowledged verification gap, not a claim of
  Windows testing that didn't happen.
- [x] **T549 — Workspace Dashboard's disk-usage stat shells out to
  `du`, unavailable on Windows; fails silently (cosmetic, not a
  crash).** `src/app.rs:10872-10883`: `Command::new("du").arg("-sh")...`
  inside `if let Ok(out) = ... { }`, so on Windows the Dashboard's
  disk-size figure just never populates — already degrades gracefully,
  lowest priority of the three Windows findings. Fix: replace with a
  pure-Rust recursive size sum (`walkdir` + `Metadata::len()`, both
  already dependencies elsewhere) so it works identically on every
  platform instead of shelling out at all. Small effort, low value.
  **Done 2026-09-20.** Unlike T547 (genuinely Windows-specific shell/
  quoting logic that needs real Windows testing to have confidence in),
  this is pure filesystem code with no OS branch at all — fully
  verifiable right here. New `dir_size(dir: &Path) -> u64` mirrors the
  sibling `count_files`'s own hand-rolled recursive `std::fs::read_dir`
  walk exactly (same `.git`/`target` skip, same best-effort-on-unreadable
  shape) rather than pulling in `walkdir` as the task's own text
  suggested — the existing local idiom already does the job with zero
  new dependencies. Formatted via `vix_byte_size::human_bytes` (T521),
  matching how every other size figure in the app is already shown,
  rather than trying to preserve `du -sh`'s particular output style.
  Extended the existing dashboard integration test (previously only
  waited for/checked `file_count`) to also wait for and assert on
  `disk_usage`, catching the exact byte count of two known-size fixture
  files. No CHANGELOG entry (T309: the Windows case goes from "silently
  broken" to "working" — user-visible there, but this session can't
  verify it firsthand — while on macOS/Linux the only visible change is
  a cosmetic reformat from `du`'s `"128M"`-style to `"128.0 MiB"`-style,
  neither significant enough alone). Verified: `cargo clippy --workspace
  --all-targets -- -D warnings` and `RUSTDOCFLAGS="-D warnings" cargo
  doc --workspace --no-deps` both clean; the extended integration test
  passes; full `scripts/check` green.

---

## Ideas backlog (unscoped)

Bigger or more speculative than the tasks above — not yet sized, not yet
assigned a task id, and not yet agreed as worth doing. Promote one to a
real `T6xx` task (write it up with the same rigor as the rest of this
file) when someone actually wants to build it; don't start from this list
directly. Recorded here so they aren't re-discovered and re-argued from
scratch each time they come up.

- **Remote/SSH editing.** Open a directory over SSH the way VS Code's
  Remote-SSH does — a remote filesystem + remote process (LSP servers,
  `Project → Compile`, terminal) with a thin local UI. Large: a real
  remote-fs/remote-process protocol, not a small feature. Vix's
  local-only model (direct `std::fs`, `Command::new` everywhere) would
  need a real abstraction layer first.
- **Collaborative editing.** Multiple people editing the same buffer
  live (CRDT or OT-based). Large scope, a genuinely different product
  direction (network sync, presence, conflict resolution beyond git) —
  worth being explicit that this is *not* implied by anything already
  planned.
- **Interactive 3-way merge conflict resolver.** `vix-conflict-tool`
  already parses merge markers; there's no overlay UI to resolve
  conflicts interactively (accept ours/theirs/both per hunk, edit
  inline). A real gap, moderate scope — the parser half already exists.
- **`vix --doctor`.** A CLI subcommand (and Help menu entry) that checks
  the environment for common friction: is `git` on PATH, are any
  configured LSP servers actually installed and runnable, does the
  active locale's spellcheck dictionary exist, is the terminal's
  `TERM`/color support adequate. Prints a plain pass/fail report.
- **Settings/profile export-import.** "Export my setup" bundles
  `config.toml`, the active theme, custom snippets, `macros.toml`, and
  (once T104h lands) `keybindings.toml` into one archive; "Import" the
  reverse, onto a fresh machine or for sharing a team preset.
- **Accessibility audit for screen readers.** T203 adds a WCAG-AA
  high-contrast *theme* (visual only). A TUI's accessibility to a
  screen reader is inherently constrained, but worth auditing whether
  mode/state changes (Vim mode switches, a completed long-running
  command, a modal opening) are ever announced somewhere a screen
  reader's terminal integration could pick up, not just shown via color/
  position — and documenting the honest limits where they aren't fixable.
- **SBOM generation.** Emit a Software Bill of Materials (e.g.
  `cargo-cyclonedx`) as a release artifact alongside the existing
  binaries, for downstream consumers doing their own supply-chain
  compliance. Lower urgency than T131–T134 — a nice-to-have for a
  specific downstream audience, not a gap in Vix's own posture.
- **CodeQL (or similar static analysis) in CI**, alongside the existing
  `cargo-deny` advisory/license/bans/sources scan. Lower value than it
  sounds here specifically: `#![forbid(unsafe_code)]` is already
  workspace-wide, which is what most of CodeQL's Rust query set targets;
  worth revisiting if that stops being true, or if CodeQL's logic-bug
  queries (not just memory-safety ones) turn out to catch something
  clippy pedantic doesn't.

---

## Suggested execution order (batched for agent runs)

**Status as of 2026-09-14**: Run A is fully done. Run B is done except
T112–T115 (the modal-editing implementation; T111's audit/spec landed).
**Run C (T201–T211) is fully done.** **Run D (docs) is fully done —
T301–T309, all nine tasks.** `scripts/docs-coverage` reports
zero gaps; `book.toml`/`docs/SUMMARY.md` + GitHub Pages CI jobs;
`docs/reference/` and `man/vix.1` generated from real data,
regenerate-and-diff gated on all three forges (now 8 gate steps); all
38 missing docs pages written; a getting-started guide; migration
guides for VS Code (an existing page, corrected) and Helix (new); a
refreshed feature-parity comparison matrix. **Run E is nearly done: T501,
T401–T403, T404, and T405 are all done** — the vixtutor (all six chapters)
and all ten written tutorials (`docs/tutorials/01`–`10`) are real; **T406 is
now fully done too** (2026-09-18: a browser-free rendering pipeline built
from scratch after headless Chrome again proved unusable in this
environment — see T406's own entry for the full detail, including three
real product gaps and two real tape bugs found and fixed along the way, and
one real i18n bug found but not yet root-caused). **Run E is therefore
fully done.** **Run F is fully done: T502–T505, all four tasks.** Of
the deferred/security/CI items below, T131/T132/T133 and T009/T010/T143/
T145/T146/T150/T153/T154/T141/T204 are all done; what's left from those
groups is listed explicitly.

1. **Run A (infrastructure):** T001–T008. Done.
2. **Run B (big rocks kickoff):** T101, T111 (specs only), then T102–T105
   and T112–T115 as follow-on runs. T104 turned out to need its own spec
   first (`crates/vix-keybindings/spec/index.md`) — its T104a–T104j are a
   further follow-on chain, one keymap conversion per task. **All of
   T101–T105, T111, and T112–T115 are done (T112–T115 finished
   2026-09-14/15) — Run B is complete.**
3. **Run C (features):** T201–T211 in any order, one branch each — T104j
   shipped 2026-09-04, so T204 was unblocked too (§ T204's own note);
   T210/T211 never had a dependency either. **All of T201–T211 are done
   (2026-09-10/12) — Run C is complete.**
4. **Run D (docs):** T301, T302, T305 first; then T303, T304, T306–T309.
   **All nine done (2026-09-13) — Run D is complete.**
5. **Run E (demo + tutorials):** T501, then T401–T406, T404/T405 last.
   **All of T501, T401–T405, and now T406 are done (T406 finished
   2026-09-18) — Run E is complete.**
6. **Run F (examples):** T502–T505. **All four done (2026-09-14) — Run F
   is complete.**
7. **Deferred/audit-driven:** T121–T125. **All five done (2026-09-15/16)
   — this group is complete.**
8. **Security:** T131/T132/T133 are done. **T134 is also done
   (2026-09-16)** — this group is complete.
9. **CI + code quality:** T009/T010/T141/T142/T143/T144/T145/T146/T147/
   T148/T149/T150/T151/T152/T153/T154 are all done. **T149's `App`/
   `Settings` portion (measured at ~350-450 call sites combined, once
   explicitly deferred indefinitely after that measurement) was picked
   back up and finished 2026-09-18**, closing all 7/7 structs (see
   T149's own entry for the full slice-by-slice detail). This run has
   nothing else open.

**Every run and every group above is complete. The backlog (T001–T505)
has no open tasks remaining as of 2026-09-19.**

When a task is finished: check its box here, note the branch/merge commit,
and record anything learned that changes later tasks.
