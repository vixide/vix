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

- [ ] **T131 — `SECURITY.md`.** Repo-root policy: supported versions,
  how to privately report a vulnerability (a maintainer contact — see
  `AI_STATEMENT.md`'s "Questions" section for the existing pattern — not
  a public issue), expected response time, and what's explicitly
  out-of-scope (e.g. the HTTP client's intentional no-SSRF-guard
  design, already documented in memory but not publicly). Link from
  `README.md`/`index.md` and `AGENTS.md`, alongside `AI_STATEMENT.md`.
  GitHub surfaces a root `SECURITY.md` in its Security tab automatically;
  no extra config needed there. GitLab/Codeberg mirror the file as-is.
- [ ] **T132 — Script trust prompt.** `vix-script` currently auto-loads
  and runs every `.rhai` file under `<root>/.vix/scripts/` at startup
  with no confirmation — cloning an untrusted repo and opening it in Vix
  silently executes its scripts (sandboxed: no file/network access per
  `crates/vix-script/spec/index.md`, but still able to read/rewrite the
  open buffer, spam messages, or plant a fake `prompt()` on first open).
  Add a one-time-per-workspace trust prompt before loading *project*
  scripts specifically (global scripts under `Settings::scripts_dir()`
  stay always-trusted — the user put them there directly), remembered in
  the session store, mirroring VS Code's Workspace Trust model. Update
  `crates/vix-script/spec/index.md`'s "Script discovery" section.
- [ ] **T133 — Persisted-file permission audit.** The 2026-07 audit added
  `write_private_temp` (0600) for temp files carrying secrets in transit
  (AI/branch-description scratch files); it never extended to files that
  persist long-term and may carry buffer content or history:
  `<config>/undo/*` (full undo trees, potentially of sensitive files),
  `session.toml` (recently-opened paths), and (once T104h lands)
  `keybindings.toml`. Audit each for whether its content is ever
  sensitive and, where it is, switch to `write_private_temp`'s 0600
  pattern (or document why 0600 isn't warranted, e.g. it's genuinely
  never sensitive).
- [ ] **T134 — Post-scripting/AI security re-audit.** Once T105
  (scripting samples + docs) and T124/T125 (AI provider abstraction +
  features) ship, run a focused audit pass — same rigor as 2026-07-12,
  scoped to the new surface only: confirm Rhai's sandboxing actually
  holds under real usage (not just the spec's design intent), that AI
  provider keys go through the keyring like the DB credential waterfall
  (never a plaintext config value), and that T125's "redact file paths
  on request" / explicit-invoke-only promises are actually implemented,
  not just planned. File real findings as their own follow-up tasks
  (T134a, T134b, …), same pattern as T123.

### Code quality & maintainability

Grounded in a measured pass on 2026-09-04 (numbers below are from that
run; re-measure before starting any task). The headline: the discipline
is good — 8 non-test `unwrap()` + 2 `expect()` in all of `src/app.rs`, 6
real `TODO`/`FIXME` comments workspace-wide, only 4 `too_many_lines`
allows in 61k lines of crate code — so the debt is **structural**
(size, duplication, drift between hand-maintained copies), not
sloppiness. Tasks are ordered roughly by payoff; each is its own branch
and its own gate run, zero intended behavior change unless stated.

- [ ] **T141 — Carve up `src/app.rs`.** 22,449 lines, 808 `fn`s, 767
  string-literal match arms; `AGENTS.md`/`CLAUDE.md` describe "a thin
  App shell over ~105 focused crates" and this file is the opposite.
  Staged, not one rewrite: (a) move `impl App` blocks into
  `src/app/<feature>.rs` submodules by feature — keymap dispatch (the
  ten `*_key`/`*_token` fns, `on_key`), org, git, lsp/dap, palette,
  prompts/dialogs, scripts, tools, session/settings — pure moves, one
  module per branch, the 432-test suite green after each; (b) split
  `run_action`'s giant `match` into per-namespace dispatchers
  (`run_file_action`, `run_view_action`, …) chained the way
  `run_vim_action`/`run_edit_action` already are — "one action id, one
  arm" still holds, the arm just lives next to its feature. Do T143
  first so reviews of each slice aren't buried in an 8.8k-line test
  file's diff noise.
- [ ] **T142 — Same for `src/ui.rs`.** 7,109 lines; 3 of the workspace's
  4 `too_many_lines` allows are here (`draw_ai_diff`, `draw_terminal`,
  `draw_search` — the 4th is `app.rs`'s `draw_insert`). Move each
  overlay/panel's `draw_*` next to its state (into the owning crate
  where one exists, else `src/ui/<feature>.rs`), and split the three
  100+-line drawers so the allows come out.
- [ ] **T143 — Split `tests/integration.rs`.** 8,790 lines, 432 tests in
  one file. Move to `tests/integration/main.rs` + `<area>.rs` modules
  (keymaps, org, git, palette, panels, scripting, …) sharing a
  `common.rs` for `app_at`/`key`/`ctrl`/`type_str`/`buffer_with`. Still
  one test binary, so no compile-time regression; smaller diffs and
  `cargo test --test integration keymaps::` targeting are the wins.
- [ ] **T144 — One list-navigation state instead of eighteen.**
  `ensure_visible` is defined in 18 crates, `up`/`down` in 18,
  `page_up`/`page_down` in 14, `select_index` in 11 — every
  panel/chooser reimplements the same `selected`/`scroll` bookkeeping
  (spot-checked identical in `vix-file-browser-panel`, `vix-palette`,
  `vix-git-panel`). Extract a `ListCursor { selected, scroll }` with
  those methods (a new `vix-list-state` crate, or into whichever core
  crate every panel already depends on) and migrate one panel per
  commit. Removes several hundred lines and the class of "this panel's
  page-down is off by one" bugs.
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
- [ ] **T146 — No silent keymap fallback.** `Keymap::from_id`
  (`src/app.rs`) maps any unrecognized id to `Keymap::Apple` silently;
  it let an integration test pass for months while testing the wrong
  keymap (found in T104d: `"intellij-mac"` ≠ `"intellij-macos"`). Make it
  return `Option`, report an unknown persisted id via the messages panel
  at settings load (then fall back), and add a `vix-keymap-model` test
  that every `KEYMAPS` id round-trips and a typo is rejected. Then
  consider retiring `App`'s private 9-variant `enum Keymap` in favor of
  the model's 10 string ids everywhere — the granularity mismatch
  `crates/vix-keybindings/spec/index.md` § "Why 10, not 9" already
  documents.
- [ ] **T147 — A real action catalog.** The command palette and
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
- [ ] **T148 — i18n coverage, measured and gated.** `locales/app.yml` is
  26,298 lines holding 2,240 keys, and 690 of them (31%) carry only
  `en` — every `msg.*` added since scripting landed, most menu items
  from 2026-07 on. (a) Extend `tests/i18n_keys.rs` to print a per-locale
  coverage table and fail if any locale drops below its current floor
  (ratchet, never regress); (b) backfill the 690 — a good first job for
  the T124 AI provider with human review, or a per-locale contributor
  call; (c) split the file per top-level namespace
  (`locales/menu.yml`, `msg.yml`, `help.yml`, …) — `rust-i18n` loads a
  directory — so a translation PR isn't a 26k-line diff context.
- [ ] **T149 — Replace boolean clusters with types.** Six
  `struct_excessive_bools` allows: `App`, `Settings`, `Editor` (both
  `vix-editor` and `vix-editor-core`), `SearchBar`, `WorkspaceSearch`.
  Where the bools are really one mode (e.g. an editor's
  overwrite/read-only/soft-wrap set), an enum or a small `Flags` struct
  with named methods; where they're independent persisted toggles
  (`Settings`), group them into a nested `#[serde(flatten)]` struct so
  they can be tested and documented as a unit. Each allow comes out with
  its struct.
- [ ] **T150 — Remove the two crate-level blanket allows.**
  `crates/vix-editor-core/src/multicursor.rs` and `named.rs` each open
  with `#![allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]`
  for a whole file — the one place the "pedantic, no blanket allows"
  hard rule is broken. Replace with `isize`-typed offset arithmetic or
  `usize::try_from` at the few real cast sites, or at worst a
  per-expression `#[allow]` with a one-line proof comment (the
  `multicursor.rs` comment already states the invariant; make it local).
  Sweep the 3 `cast_precision_loss` + 2 `cast_possible_truncation`
  allows elsewhere the same way.
- [ ] **T151 — Micro-crate audit.** 105 crates; `vix-query` is 37 lines,
  `vix-theme` 66, and `vix-modal`/`vix-i18n`/`vix-query`/`vix-theme` have
  no tests at all (`vix-modal` is a documented spec-only scaffold, fine;
  `vix-theme` is not). Write a minimum-viable-crate guideline into
  `AGENTS.md` (own spec, own tests, a consumer other than the App shell
  *or* a clear reuse story), fold crates that fail it into their sole
  consumer (precedent: `vix-projectile` was merged into `vix-tasks`
  the day it was built), and add unit tests to `vix-theme`. Keep the
  crate-map/spec/check-docs invariants green throughout.
- [ ] **T152 — Root `src/` modules that should be crates.**
  `column_view.rs` (966 lines), `edit_table.rs` (770), `edit_outline.rs`
  (610), `explorer.rs`, `search.rs`, `workspace_search.rs`, `messages.rs`
  live in `src/` with no spec and outside `scripts/check-docs`'s
  "every crate owns a spec" gate — the 2026-07 "crates, not modules"
  decision was applied everywhere except here. Move each to
  `crates/vix-<name>` with a `spec/index.md`, one per branch; `src/`
  should end up as `app.rs` (or `app/`, after T141), `ui.rs`, `lib.rs`,
  `main.rs`.
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
- [ ] **T154 — Keep `Cargo.toml` descriptions honest.** `vix-keybindings`'
  `description` said "9 keymaps" for a task after T104g made it 10;
  caught only because T104h happened to edit the same file. Nothing
  checks these. Add to `scripts/check-docs`: each crate's `description`
  must equal (or be a prefix of) the first sentence under its
  `spec/index.md` H1, so the spec is the single source and the manifest
  can't drift; fix any current mismatches the check turns up.

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
- [ ] **T210 — Coverage gutter.** New crate `vix-coverage`: parse LCOV
  and Cobertura XML into per-file line-hit data; a gutter overlay
  (covered/uncovered/partial, reusing the diff-gutter's color-mark
  mechanism) toggled from the Tools menu, pointed at a coverage file the
  user generates (`cargo llvm-cov`, `pytest --cov`, …) via a settings
  path or a palette "Load Coverage File…" command. No coverage
  generation built in — Vix visualizes an existing report, doesn't run
  one.
- [ ] **T211 — Editable search results ("wgrep"-style).** Workspace
  search results open as a real, editable buffer (one line per hit,
  `path:line: text`) instead of a read-only list; editing a line and
  saving applies that edit back to its source file at the recorded
  position, deleting a line skips that hit. Builds on
  `workspace_search.rs`'s existing results model; a new action
  (`search.edit_results`) and a small apply-diff-back-to-sources step
  with a confirm summary ("N files will change") before writing.

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
  user-visible change" rule to `agents/conventions.md` (already implied by
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

1. **Run A (infrastructure):** T001–T008.
2. **Run B (big rocks kickoff):** T101, T111 (specs only), then T102–T105
   and T112–T115 as follow-on runs. T104 turned out to need its own spec
   first (`crates/vix-keybindings/spec/index.md`) — its T104a–T104j are a
   further follow-on chain, one keymap conversion per task.
3. **Run C (features):** T201–T211 in any order, one branch each — T104j
   shipped 2026-09-04, so T204 is unblocked too now (§ T204's own updated
   note); T210/T211 never had a dependency either.
4. **Run D (docs):** T301, T302, T305 first; then T303, T304, T306–T309.
5. **Run E (demo + tutorials):** T501, then T401–T406, T404/T405 last.
6. **Run F (examples):** T502–T505.
7. **Deferred/audit-driven:** T121–T125 whenever their prerequisite data
   (benches, audits) exists.
8. **Security:** T131 anytime (no dependency); T132 anytime (T102/T103
   already shipped, so it's unblocked now); T133 anytime; T134 only after
   T105 and T124/T125 ship.
9. **CI + code quality:** T009/T010 anytime (independent, small).
   T150, T153, T154, T146 anytime — each is a single short branch.
   T145 is unblocked now that T104j has shipped (the epic it refactors
   code from is done). T143 before T141, and T141 before T142 (each makes
   the next reviewable). T144, T147, T148, T149, T151, T152 are
   independent of each other and of the rest; T147 is worth doing before
   T204.

When a task is finished: check its box here, note the branch/merge commit,
and record anything learned that changes later tasks.
