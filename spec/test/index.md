# Testing

How Vix is tested, and which layer a new test belongs in. The rule that shapes
all of it: **editing logic is terminal-independent**, so almost everything can be
tested by building an `App`, feeding it events, and asserting on state — no TTY,
no sleeps, no screen scraping.

## The layers

| Layer | Where | What it is for | Runs in |
| ----- | ----- | -------------- | ------- |
| **Unit** | `#[cfg(test)] mod tests` beside the code | One pure function: the transform, the parser, the boundary case | `cargo test` |
| **Property** | Same, with `proptest` | Invariants over generated input — "the cursor a rewrite returns is always inside the text it returns" | `cargo test` |
| **Integration** | `tui/tests/integration/main.rs` | A real `App`: keys in, state out. Menus, actions, keymaps, overlays, files | `cargo test` |
| **Snapshot** | `tui/tests/snapshots.rs` | Golden text screens — the actual rendered frame, not just state, for scenarios where the layout itself is the thing being tested | `cargo test` |
| **Repo invariants** | `tui/tests/i18n_keys.rs`, `tui/tests/code_lints.rs`, `tui/tests/spawn_sites.rs`, `tui/tests/secret_scan.rs` | Facts about the repository itself — every `t!` key exists, every call fills exactly the `%{name}` placeholders its string declares, every catalog entry has an `en` fallback | `cargo test` |
| **Action sweep** | `tui/tests/action_sweep.rs` | Every action id the dispatch scan finds (`vix_action_catalog::dispatch_scan`) runs once on a fresh `App`, its frame drawn wide and narrow, then any prompt it opened answered (`sweep`, Enter), then poked (hover, click and scroll over a grid of points, the navigation keys), then Enter on whatever is highlighted and Esc pressed twice — no panic, no hang. Runs in a sandbox like the chaos monkey (a `PATH` holding only a link to `git` and a fake `docker` that prints canned JSON listings with shell built-ins, a throwaway `HOME` and git config, no `SHELL`) over a workspace that is a throwaway repository with history, a branch, a stash and a change but no remote, re-executing its own test binary, killed after 10 minutes with the action it was on named. It is the cheapest way to draw every overlay under test and coverage | `cargo test` |
| **Keymap sweep** | `tui/tests/action_sweep.rs` | Every keymap (`vix_keymap_model::KEYMAPS`) fed `C-x` and `C-c` with each letter (plain and Ctrl), the Space leader with each letter, plain letters in normal mode and a few Vim `:` commands, drawing after each; same sandbox, a fresh `App` after a quit | `cargo test` |
| **End to end** | `tui/tests/e2e_tmux.rs` | The real `vix` binary in a real terminal: a detached tmux session on a private server socket (`tmux -L`, never a person's own sessions), a throwaway `HOME`, keys sent with `send-keys` and the screen read with `capture-pane`. Covers what in-process tests cannot: terminal setup, the event loop, input decoding, teardown and the command line. Skipped without tmux; Unix only | `cargo test` |
| **Smoke** | `tui/tests/db_smoke.rs`, `tui/tests/lsp_smoke.rs` | Subsystems with an external dependency, skipped when it is absent | `cargo test` |
| **Fuzz** | `fuzz/fuzz_targets/` | Parsers and text transforms against input nobody thought of | `cargo +nightly fuzz run <target>` |
| **Mutation** | `.github/workflows/mutants.yml`, `.cargo/mutants.toml` | Whether the tests notice a change: `cargo mutants` rewrites an operator or a return value and expects some test to fail. Weekly in CI, advisory | `cargo mutants -p <crate>` |
| **Benchmark** | `tui/benches/` | The per-keystroke and per-frame paths, so latency regressions are visible | `cargo bench` |
| **Documentation** | `scripts/check-docs` | Links resolve, every crate owns a spec, the crate map is complete, `README.md` twins match, no tracked file keeps a merge-conflict marker (outside markdown code fences), CSpell finds no unknown word in the markdown | `scripts/check` |

`scripts/check` runs the gate the way CI does: fmt, build, clippy at pedantic
with `-D warnings`, the test suite, `cargo doc` with `-D warnings`, and the
documentation checks. Fuzzing and benchmarks are deliberately outside it — one
needs nightly, the other needs a quiet machine.

The integration layer lives in `tui/tests/integration/` (T143) but compiles
inside the `vix` library's own test build (T674: `tui/src/lib.rs` includes it as
`#[cfg(test)] mod integration`, with `extern crate self as vix` so its
`vix::…` paths still resolve). That lets the tests read `App` state that is
`pub(crate)` rather than public API. `tui/Cargo.toml` sets `autotests = false`
and lists the other files in `tui/tests/` as their own test targets. It is
split by area: `common.rs` holds the shared
fixtures/helpers (`app_at`, `key`/`ctrl`, `type_str`, `buffer_with`, …), and
each area gets its own file — `catalog.rs`, `db.rs`, `editing.rs`, `find.rs`,
`git.rs`, `keybindings.rs`, `keymaps.rs`, `lsp.rs`, `menu.rs`, `org.rs`,
`palette.rs`, `panels.rs`, `scripting.rs`, `workspace.rs` — so a change stays a
small diff and `cargo test -p vix-tui --lib integration::<area>::` targets
just that slice. The split is by rough topic, not a strict taxonomy; a test that doesn't
obviously belong anywhere lands in `editing.rs`, the catch-all.

## Writing an integration test

```rust
let mut app = app_at(Path::new("."));      // an App with a realistic viewport
buffer_with(&mut app, "hello", 0);         // content + cursor
app.on_key(ctrl('d'));                     // or app.run_action("edit.delete.word")
assert_eq!(app.editor.active_tab().unwrap().text(), "ello");
```

- **Assert on state, not on rendered text.** The locale is process-global, so a
  translated string can race with another test; assert on buffer content, action
  ids, or i18n *keys*.
- **Drive the real path.** Prefer `on_key` when testing a binding and
  `run_action` when testing behavior — that is the difference between "does
  `Ctrl+D` do this" and "does this action do this".
- **Platform-specific behavior gets a platform-aware assertion**
  (`if cfg!(target_os = "macos") { … } else { … }`), so the test still runs, and
  still means something, on the CI that is not that platform.
- **Nothing may touch the machine.** The system clipboard is opt-in
  (`vix_clipboard::use_system`, called only by `main`), so a test run cannot
  overwrite what the developer copied; files go under a temp directory keyed by
  `std::process::id()`.
- **Git is assumed; the developer's git config is not.** The suite needs `git`
  on `PATH` (the snapshot and T634 tests always have), so a test that needs a
  repository builds a throwaway one and runs by default — never `#[ignore]`,
  and never this checkout, which a source tarball does not have. Build it with
  `common.rs`'s `init_git_repo` (branch `main`, a fixed identity, and local
  `commit.gpgsign`/`tag.gpgsign` off, `core.hooksPath` at the repo's own hooks,
  `core.autocrlf` off) and talk to git only through `fixture_git` /
  `commit_all`, which point `GIT_CONFIG_GLOBAL` at the null device, set
  `GIT_CONFIG_NOSYSTEM` and `GIT_TERMINAL_PROMPT=0`, and fail on a git error.
  The app's own git calls still read global config, so the local settings are
  what keep a global `commit.gpgsign=true` or a passphrase-protected SSH key
  from signing, failing or hanging a test (T792).
- **`#[ignore]` is for what CI cannot provide**, with the reason in the
  attribute: the untracked `./dictionaries` set, a real `rust-analyzer` or
  Vim, a test that only runs as another test's child process, and a known-flaky
  test in quarantine (`#[ignore = "flaky: T<n>, <reason>"]`, see "Flaky
  tests"). `tui/tests/code_lints.rs` fails on a bare `#[ignore]`. Run them by hand with
  `cargo test -p vix-tui --lib -- --ignored` (and `--test lsp_smoke`,
  `-p vix-spellcheck`).

## Snapshot testing

`tui/tests/snapshots.rs` renders a whole frame instead of asserting on state: it
boots an `App` the same way an integration test does, drives it with scripted
key events, draws one frame to a ratatui `TestBackend` (100×30 by default),
flattens the buffer to plain text (one line per row, trailing spaces
trimmed), and compares it to a golden file under `tui/tests/snapshots/` with
`insta::assert_snapshot!`.

```rust
let mut app = app_at(Path::new("."));
type_str(&mut app, "fn main() {}\n");
let screen = render_screen(&mut app, 100, 30);
insta::assert_snapshot!(screen);
```

Reach for a snapshot when the *layout* is the thing under test — a dialog's
framing, a dock's column widths, how a long line truncates — not when a plain
state assertion already says it. Most behavior still belongs in
`tui/tests/integration/main.rs`; snapshots are for the cases where "what does the
screen look like" is the actual question.

Every snapshot test pins the locale to `en`
(`rust_i18n::set_locale("en")`) before rendering — `rust_i18n::locale()` is
process-global, so leaving it to chance would make a screen's golden text
depend on test run order. Each app also opens a small synthetic fixture
tree under `/tmp` (keyed by a per-test tag plus the process id), never the
real repo root — a live checkout carries local-only untracked entries
(build output, downloaded dictionaries, …) that differ between a
workstation and a fresh CI checkout.

Two gotchas that cost real debugging time and are easy to reintroduce in a
new scenario:

- **A scenario that opens a real file embeds its full canonicalized path
  in the status bar — twice** (`Tab::display_path` and
  `t!("status.opened", path = ...)`, and `Editor::open` always
  `path.canonicalize()`s, which macOS resolves through `/private`). Two
  independent traps, both confirmed by CI failures (the second one *after*
  the first was already fixed and had passed once on CI — it takes more
  than one green run to trust a path-adjacent snapshot):
  - At a narrow viewport the renderer can truncate one or both copies
    *before* anything downstream sees them, at a cutoff that itself
    depends on the OS's path length. Root fixtures under `/tmp` rather than
    `std::env::temp_dir()` (long and session-specific via `TMPDIR` on
    macOS), and render such a scenario at a wide-enough viewport that
    neither copy ever truncates on the worst case (macOS).
  - Even fully untruncated, the status bar right-aligns trailing fields
    (language, line ending, cursor position) against the *real,
    pre-redaction* path length — so swapping the path substring for a
    same-length token still leaves behind a length-dependent amount of
    padding, which differs across machines and even across runs on the
    same machine (PID digit count alone shifts it). Don't try to
    reconstruct the "correct" padding; replace the **whole row** with a
    fixed placeholder whenever it contains the root path.
- **A fixture that needs a real git repo** (e.g. to exercise the git
  changes panel) must force the branch name explicitly (`git init -b
  <name>`, not `init.defaultBranch`, which differs by machine —
  `master`/`main`/a custom default) and set `commit.gpgsign false` locally,
  so the fixture's commit never inherits the developer's real
  `commit.gpgsign=true` and invokes their actual signing key just to build
  a throwaway test fixture (the integration tests' `init_git_repo` does all
  of this; see above).

After writing a new scenario, run the suite **twice** (a plain `cargo test
--test snapshots` after the `INSTA_UPDATE=always` pass) — a fixture keyed
by the process id will produce a *different* golden file on the second run
if anything process-id-dependent leaked into the screen, which a single
run can't catch.

**Reviewing/updating snapshots**: a snapshot test compares against the
committed `.snap` file, and it must be run and reviewed like a diff, not
regenerated and trusted blind.

```sh
cargo test --test snapshots                          # fails on any mismatch
INSTA_UPDATE=always cargo test --test snapshots       # writes the new .snap files
git diff tui/tests/snapshots/                             # read every line before committing
```

With `cargo-insta` installed (`cargo install cargo-insta`), `cargo insta
review` gives an interactive accept/reject prompt over the same `.snap.new`
files instead of a raw `git diff`. Either way: a snapshot changing is a
signal, not a formality — read *why* the screen changed before accepting it.

## Fuzzing

`fuzz/` is a separate workspace (nightly + sanitizer instrumentation), so the
root `cargo build` never touches it. Targets cover the code that parses input
Vix did not write: `textops`, `org_table`, `lsp_frame`, `vcard`, `conflict`,
`http_request`, `query_replace`, `tabular_convert`, `structured_convert`,
`macro_tokens`, `im_message`, `agent_frame`, `jupyter_wire`, `sql_split`, `edit_value`, `settings_toml`, `substitute`. See [`fuzz/README.md`](../../fuzz/README.md) for running them,
minimizing a crash, and adding a target. `.github/workflows/fuzz.yml` runs every
target for two minutes each week (see [`spec/ci`](../ci/index.md)); a new target
is added to its matrix too.

A fuzz target asserts **invariants**, not merely the absence of a panic — "a
returned cursor is a valid offset into the returned text", "render then parse
keeps every row". Those assertions are what turn a fuzzer into a bug-finder
rather than a crash-detector: the hline-only-table bug (`align` deleting a `|-`
line instead of squaring it up) was caught by the round-trip assertion, not by a
panic. Get the invariant itself right, though — `tabular_convert`'s first run
asserted CSV round-trips *exactly*, and immediately "failed" on the
formula-injection guard deliberately rewriting `@`-leading fields; the
fixed-point framing (`write(parse(write(rows))) == write(rows)`) it now uses
survives that intentional rewrite. A fuzz target that's wrong about what the
code is supposed to do is as much a false alarm as no invariant at all.

When a target finds something: minimize it (`cargo +nightly fuzz tmin`), then
write the minimized input as a **unit test in the crate that owns the code**.
The generated corpus is not committed (only a few hand-written `seed-*` inputs per target); the regression is.

## Mutation testing

`cargo mutants` changes one thing at a time (`<` to `<=`, a function body to
`Ok(())` or `Default::default()`), rebuilds, and runs the crate's tests; a
**missed** mutant is a behavior no test asserts. `.github/workflows/mutants.yml`
runs it every week (see [`spec/ci`](../ci/index.md)) in two parts (T606, T831):

- every mutant of the small pure crates, in one job;
- the core crates `vix-editor-core`, `vix-fileops`, `vix-git`, `vix-lsp` and
  `vix-db`, only in code changed in the last seven days (`--in-diff`). All of
  their mutants together are about 5,900 (2,075 in `vix-editor-core`, 2,854 in
  `vix-db`), days of runner time; one week of changes was about 1,800. Each crate
  is split into shards of at most about 200 mutants (`--shard k/n`, at most 16
  per crate) with its own test timeout, so every job stays under the runner's
  six hours. A manual run can name another starting ref, or `full` for every
  mutant.

Both parts are **advisory**: a missed mutant is a report (the job summary lists
them, `mutants.out/` is uploaded), not a failure. The core crates have never
had a full pass, so their survivors, and which of them are equivalent, are not
known yet; failing on them now would fail every week on code nobody touched
beyond a refactor. Once a full run has been triaged (a test per real gap, an
entry in `.cargo/mutants.toml` per equivalent mutant), the weekly diff run is
meant to fail on a missed mutant, since it only sees code changed that week.

`.cargo/mutants.toml` lists only mutants proven **equivalent** (no observable
difference), each with the reason; a mutant a test could kill gets the test.
Locally, run one crate at a time (`cargo mutants -p vix-list-state --timeout 60`
takes about 20 seconds; `cargo mutants --list -p <crate>` shows what would run,
and `--in-diff <file>` limits it to a diff). Output goes to `mutants.out/`
(ignored by git; the previous run moves to `mutants.out.old/`).

## Benchmarking

`tui/benches/` holds Criterion benchmarks, one file per area:

| Bench | Covers |
| ----- | ------ |
| `text_ops` | The pure transforms, at 100 and 2,000 lines — they rebuild their unit ranges from the whole buffer on every keystroke |
| `editor_ops` | Opening a file (parsing it, up to a ~100 MB synthetic file — highlighting is a separate, always-lazy, per-viewport cost that `render_ops` measures), typing, a 10k-operation burst of random inserts/deletes, pasting, undo, whole-buffer line transforms, and a keystroke plus the frame after it at 4 KiB to 64 MiB (`editor/keystroke`, T858) |
| `search_and_palette` | The three per-keystroke search paths: find-in-buffer, workspace search (up to a generated 10k-file tree), and palette fuzzy filtering |
| `startup` | Cold-start cost: `App::new` (theme scan, editor/menu/LSP-client setup) and `refresh_git` (the three `git` subprocesses a real workspace's cold start pays), benchmarked separately; and the large-checkout open-time exercise (T823, `-- open/`): a seeded 20,000-file checkout generated in a temp directory, opened with a fresh `App` per run, timing the first editable frame and the first Quick Open result locally and through the in-process `FakeAuthority`, printed as median / p90 / min / max with versions, workspace size, capabilities and load average. Its language-server half (`-- lsp/`, first semantic definition and first actionable diagnostic) runs only with `VIX_BENCH_LSP=1` and a working `rust-analyzer`; the default run starts no server. Under `cargo test --benches` (no `--bench` flag) only the Criterion smoke run happens |
| `frame_ops` | The two per-*frame* buffer scans found and fixed by T510/T511: the editor diff gutter (`vix_git::diff_marks`) and the sticky-scroll/breadcrumb declaration scan (`vix_palette::symbols`; since T858 the frame reads up from one line instead, measured by `editor_ops`' `editor/keystroke`) |
| `gutter_diff` | The git gutter diff against `HEAD` (`vix_git::diff_marks`, T574) for lightly edited and heavily rewritten buffers, so the 40 ms deadline and the 50,000-line cap stay visible |
| `render_ops` | Drawing: one viewport's syntax highlight with the cache cold (top, middle and end of 1,000 to 125,000-line files; it should stay flat as the file grows), and a whole `ui::draw` of the `App` into a 200×50 terminal, both an unchanged redraw and a redraw after a keystroke, up to a 500,000-line file |

```sh
cargo bench                                  # everything
cargo bench --bench editor_ops               # one file
cargo bench -- editor/open                   # one group; Criterion compares to the last run
```

`cargo bench` runs under `[profile.bench]` (speed-optimized, no LTO) rather
than letting it inherit `[profile.release]` (`opt-level = "z"`, `lto = "thin"`
— tuned for a small shipped binary, not representative hot-path timing, and
slower to relink for a benchmark someone reruns often).

Criterion stores each run under `target/criterion/` and reports the change from
the previous one, so the workflow is: measure, change, measure again, and read
the percentage. That is how the query-compilation cost was found — opening a
*200-line* file cost 26 ms, and the fixed cost stood out against the 5,000-line
case. Caching the compiled Tree-sitter query took it to 0.6 ms.

For a real session rather than one operation, `VIX_PROFILE=<file> vix …`
(`tui/src/profile.rs`) times every tick, frame and input in the terminal
event loop and writes percentiles, the count of inputs slower than 16 ms to
reach the screen, and the slowest inputs to that file on exit. It is off,
and times nothing, unless the variable is set.

Benchmark what the user waits for: a keystroke, a frame, a file open. Do not
benchmark what runs once at startup or what is dominated by I/O. See
[`docs/performance/index.md`](../../docs/performance/index.md) for one
machine's baseline numbers.

## Repository invariants

Some rules are about the repository rather than any one function, and they are
tested like anything else:

- **Every `t!` key exists in `locales/`.** A missing key does not fail the
  build — `rust_i18n` returns the key itself, and the UI shows `confirm.delete`
  where a sentence belongs.
- **Every call fills the placeholders its string declares**, and passes no
  argument the string has nowhere to put. `locale` is exempt: `t!` reserves it
  for choosing the target locale, so it can never be a placeholder name — a
  string that tried (`Language: %{locale}`) rendered the placeholder verbatim.
- **Every catalog entry is a locale map with an `en` fallback.**

The scanner reads `t!` calls with a paren-balanced walk of the argument list, so
`path.display()` does not end it early; a call whose arguments continue on the
next line is skipped rather than reported with half of them.

### Project-specific lints (T833)

`tui/tests/code_lints.rs` holds the rules clippy cannot express. It scans
`tui/src/` and every `crates/*/src/` as text, so it runs under `cargo test`
(and so in `scripts/check` and CI) with no extra tool:

- **No `println!`, `eprintln!`, `print!`, `eprint!` or `dbg!` in library
  code.** The TUI owns the terminal: a stray print scribbles over the frame, and
  a diagnostic belongs in the message drawer. The allowlist names each file that
  may print, with the exact number of print macros it holds and why, so a new
  print in an allowlisted file fails too: `tui/src/main.rs` (the command line:
  `--version`, `--json`, usage and start-up errors), `tui/src/app.rs`
  (`App::on_exit` runs after the terminal is restored, so stderr is the only
  channel left), `tui/src/crash.rs` (the panic hook) and `tui/src/mcp.rs`
  (`vix --mcp`: stdout is the protocol). An allowlisted file that no longer
  exists fails as well.
- **A literal given to a user-message sink goes through `t!`.** The sinks are
  the App's `messages.info`, `messages.warn`, `messages.error` and
  `messages.push`, `status = …` and `status.push_str(…)`, on `self` or
  `app`, in `tui/src/`. The expression handed to one (up to its `;` or `,`,
  or the bracket that closes around it) fails when it holds a string literal
  with a letter outside `{…}` placeholders and no `t!(` call. A literal of
  only placeholders and punctuation (`format!("{e}: {}", …)`) passes. Its
  allowlist is empty; an entry needs a reason (and a `TODO(T833)` for a real
  violation). The first run found one real violation, `Paste failed: {e}`,
  now `msg.paste_failed`.
- **Not checked: blocking I/O in async code.** The workspace has two async
  call paths, `vix-db` (`session`, `table_export`, `conn`) and the
  `vix-jupyter` worker, and each runs `block_on` on its own
  current-thread Tokio runtime on a thread of its own, one task at a time. A
  blocking call there (the table export's `std::fs` writes) stalls only that
  job, which is waiting for it anyway, so the rule would forbid nothing that
  matters. Revisit it if a shared multi-task runtime appears.

- **Every `#[ignore]` says why** (T831). Every `.rs` file under `src/` or
  `tests/` of `tui/`, `crates/*/` and `gui/*/`, test code included, is read
  for `#[ignore]` attributes: a bare one or an empty reason fails, and a
  reason starting with `flaky` must read `flaky: T<n>, <reason>` with
  `T<n>` an open (`- [ ]`) task in `tasks.md`, so ticking the task while the
  quarantine stays fails too. See "Flaky tests".

Test code is exempt from the first two rules: an item under a `cfg` that names `test`
(not `not(test)`), and every file of a module declared inside one, including
through `#[path]`. Comments and the contents of string and character literals
are never read as code, so a snippet body such as `"println!($0);"` is not a
print. Examples, benches, build scripts and `tests/` live outside `src/` and
are not scanned. The same file keeps `.github/pull_request_template.md` and
`.forgejo/pull_request_template.md` (GitHub's and Codeberg's copy of the
contributor checklist) identical.

### Contributor docs match the crate graph (T836)

`tui/tests/docs_crate_graph.rs` runs `cargo metadata --no-deps --offline` and
checks what the contributor docs say about the workspace against it:

- **The architecture diagram's arrows are real.** In every `flowchart`/`graph`
  Mermaid block of `docs/architecture/index.md`, an arrow between two nodes
  whose labels start with a workspace crate name is a direct normal or build
  dependency (dev-dependencies do not count). A subgraph titled
  `Layer N (M crates)` holds only crates of layer `N` (the longest chain of
  workspace dependencies below the crate; 0 for none), and the workspace has
  exactly `M` of them; the page's "directly on N of the M" sentence about
  `vix-tui` matches too.
- **Crate names are real.** Every `vix-…` name in backticks on that page and in
  `agents/share/crate-map.md` is a workspace member or a `gui/` crate; a name
  with `*` matches at least one member.
- **The crate map is complete and the counts agree.** Every member crate is
  named in the crate map (directly or through a `*` pattern), and every
  "N `vix-*`", "N focused `vix-*`", "N focused crates" or "N feature crates"
  count in `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `README.md`,
  `index.md`, `llms.txt`, the crate map, the architecture page and
  `docs/development.md` equals the number of member crates under `crates/`.

Adding or removing a crate therefore fails this test until the counts are
updated, which is the moment to place the crate in the crate map.

### Crate layering rules (T833)

`tui/tests/crate_layering.rs` enforces the five layering rules of
[`docs/architecture`](../../docs/architecture/index.md#layering-rules): nothing
depends on `vix-tui`, only `vix-input` and `vix-tui` depend on `crossterm`,
no `*-core`/`*-model` crate reaches a `*-panel`/`*-picker`/`*-preview`/`*-view`
crate, no GUI under `gui/` names another, and `vix-mcp-server` reaches no
such UI-surface crate (T860). It reads `cargo metadata` (through
`tui/tests/support/crate_graph.rs`, shared with `docs_crate_graph.rs`) and the
GUI manifests, and each failure names the edge (`from -> to`, and for rule 3
the whole chain). A unit test feeds each rule a small broken graph so the
messages stay right. The same module writes
`docs/reference/crate-layers.md` from `cargo run --example list_commands --
--write`, so the gate's "generated reference docs current?" step keeps it in
step with the workspace.

### Quality metrics (T833)

`docs/reference/quality-metrics.md` is the quality-metrics dashboard, written
by the same `list_commands --write` run from
`tui/tests/support/quality_metrics.rs` and so kept current by the same gate
step. It holds only numbers that are deterministic and cheap: read from the
source tree as text (comments and string-literal contents masked out, offsets
kept), with no build, test run, coverage run or network, in well under a
second. For the workspace and the desktop GUIs it gives lines of Rust under
`src/`, under `tests/` and elsewhere; `#[test]`/`#[tokio::test]` functions;
property tests (`#[test]` inside `proptest!`) and their blocks; fuzz targets;
Criterion benchmark files; `#[ignore]` attributes and how many are flaky
quarantine (`#[ignore = "flaky…"]`); `allow`/`expect` lint suppressions under
`src/`; and a table per crate. It also gives the crate count per layer (from
`crate_graph::layers`), the translation keys with each locale's count,
missing count and completeness, and the size of each written-down exception
list: `PRINT_ALLOWED` (files and print macros) and `SINK_ALLOWED` in
`code_lints.rs`, `allow` lines in `scripts/secret-patterns`, equivalent
mutants in `.cargo/mutants.toml` and ignored advisories in `deny.toml`.
Public items lacking examples are not counted: telling an item's docs apart
needs a parse of every item, not a text scan. Line coverage (`cargo tarpaulin`,
`bin/test-coverage`), mutation score (`mutants.yml`) and flakiness
(`scripts/flaky-hunt`) need a build or many runs and stay where they are
measured; the page ends with pointers to them. `tui/tests/quality_metrics.rs`
tests the counters on small inputs (masking, test/property/ignore counts,
catalog parsing, the allowlist reader, number formatting).

### Secret scanning (T832)

`tui/tests/secret_scan.rs` fails `cargo test` when a tracked file holds what
looks like a real credential (see [`spec/security`](../security/index.md) for
the patterns and the allowlist rule). It reads the working tree of every file
`git ls-files` lists, so it needs no history, no network and no extra tool, and
it is skipped with a note where `git` or the checkout is missing. Its rules
and allowlist are read from `scripts/secret-patterns`, shared with the
pre-commit hook (T833); the patterns there are spelled so the file never
matches itself, and the self-tests build their fake secrets at run time. A test
also checks that the hook still names that file. The hook's side is tested by
`scripts/test-attest`, which also checks that `tui/tests/code_lints.rs` still
compiles alone with `rustc` (no crate), as the hook runs it that way.

## Flaky tests

A test that fails now and then is a race until shown otherwise, usually in the
test, sometimes in Vix. Four that broke the gate on 2026-11-26/27 under a load
average of 50 to 150 show the usual shapes: Esc and `C-q` sent back to back
reached Vix in one read and decoded as Alt+`C-q`, which does not quit
(`tui/tests/e2e_tmux.rs`); `cat`'s own late copy of a line landed between two
echoed pieces a PTY test expected together (`vix-terminal`); a file was changed
once, before the watcher's first snapshot, so the change was never reported
(`crates/vix-authority/tests/ssh_agent.rs`); and a freed port was taken by another
process before the test bound it again (`vix-db`). Each was fixed in the test,
by handling what it had assumed could not happen (resending `C-q` until Vix
exits, dropping `cat`'s copy before comparing, changing the file until the
change is seen, accepting one rebind in five tries), not by rerunning it.

**No automatic retries.** `scripts/check`, `scripts/ci` and the forge CI run
each test once, and `.config/nextest.toml` sets `retries = 0` in both of its
profiles (it used to retry once locally and twice in CI, which passed the run
and only marked the test `FLAKY` in the log). A retry turns a race into a
green run that nobody reads, and the race still ships: hunt a suspected flake
with `scripts/flaky-hunt`, fix it, or quarantine it.

**Hunting.** `scripts/flaky-hunt` confirms or rules out a suspected flake:

```sh
scripts/flaky-hunt -n 30 -l cpus -p vix-authority --test ssh_agent
scripts/flaky-hunt -n 50 -l 8 -p vix-tui --test e2e_tmux -- esc
scripts/flaky-hunt -n 10 -c 'cargo build -p vix-tui --target-dir /tmp/load' -p vix-db
```

It builds once (`cargo test --no-run`), then runs `cargo test` with the given
arguments (`--no-fail-fast` added) `-n` times in a row, optionally while
`-l N` busy loops (`cpus`: one per CPU) or a `-c` shell command (a second
build) load the machine, and ends with each failed test, how often it failed
and in which runs, read from libtest's `test NAME ... FAILED` lines. A run
that fails naming no test (a crash, a kill) is listed as such. `-x` stops at
the first failing run; logs of a hunt with failures are kept (`-k DIR` picks
where). It exits 0 when every run passed, 1 when one failed, 2 on a usage
error and 3 when the build failed, and stops its load either way. Nothing
proves a test free of races, but 30 clean runs under `-l cpus` make a
timing flake unlikely, and a failing one gives the run's log to read. On
Linux, `cargo nextest run --stress-count N` does the same without the load.
`scripts/test-flaky-hunt` tests the script with a stub `cargo` (part of the
`push` tier).

**Quarantine.** A flaky test that cannot be fixed at once is marked
`#[ignore = "flaky: T<n>, <reason>"]`, with `T<n>` an open task in
`tasks.md` to fix it and the reason naming the race as far as it is known. The
lint above enforces the form and that the task is still open; a quarantined
test still runs with `--ignored` and under `scripts/flaky-hunt`
(`-- --ignored`). Fixing it removes the attribute in the same change that
ticks the task. Quarantine is the exception: no test is in it today.

## Manual scenarios

A few things still need a human at a terminal, because they *are* the terminal:

| Scenario | Steps | Expect |
| -------- | ----- | ------ |
| File open/close | **File → Open**, choose `hello.txt`; then **File → Close** | The text appears, then the editor is empty |
| Quit | **File → Quit** | The process exits and the terminal is restored (no stray mouse-tracking or bracketed-paste escapes) |
| Terminal paste | `Cmd+V` / middle-click a multi-line block | One undo step removes all of it; indentation is unchanged |
| Mouse | Click, drag, wheel; click tabs, dock edges, menu names | Cursor placement, selection, scrolling, resizing |
| Images | Open a PNG | It renders in a read-only tab on a graphics-capable terminal |
| Suspend | `Ctrl+Z`, then `fg` | The screen is restored, and mouse/paste modes still work |
