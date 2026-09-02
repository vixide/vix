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
| **Integration** | `tests/integration.rs` | A real `App`: keys in, state out. Menus, actions, keymaps, overlays, files | `cargo test` |
| **Snapshot** | `tests/snapshots.rs` | Golden text screens — the actual rendered frame, not just state, for scenarios where the layout itself is the thing being tested | `cargo test` |
| **Repo invariants** | `tests/i18n_keys.rs` | Facts about the repository itself — every `t!` key exists, every call fills exactly the `%{name}` placeholders its string declares, every catalog entry has an `en` fallback | `cargo test` |
| **Smoke** | `tests/db_smoke.rs`, `tests/lsp_smoke.rs` | Subsystems with an external dependency, skipped when it is absent | `cargo test` |
| **Fuzz** | `fuzz/fuzz_targets/` | Parsers and text transforms against input nobody thought of | `cargo +nightly fuzz run <target>` |
| **Benchmark** | `benches/` | The per-keystroke and per-frame paths, so latency regressions are visible | `cargo bench` |
| **Documentation** | `scripts/check-docs` | Links resolve, every crate owns a spec, the crate map is complete, `README.md` twins match | `scripts/check` |

`scripts/check` runs the gate the way CI does: fmt, build, clippy at pedantic
with `-D warnings`, the test suite, `cargo doc` with `-D warnings`, and the
documentation checks. Fuzzing and benchmarks are deliberately outside it — one
needs nightly, the other needs a quiet machine.

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

## Snapshot testing

`tests/snapshots.rs` renders a whole frame instead of asserting on state: it
boots an `App` the same way an integration test does, drives it with scripted
key events, draws one frame to a ratatui `TestBackend` (100×30 by default),
flattens the buffer to plain text (one line per row, trailing spaces
trimmed), and compares it to a golden file under `tests/snapshots/` with
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
`tests/integration.rs`; snapshots are for the cases where "what does the
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
  a throwaway test fixture.

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
git diff tests/snapshots/                             # read every line before committing
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
`macro_tokens`. See [`fuzz/README.md`](../../fuzz/README.md) for running them,
minimizing a crash, and adding a target.

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
The corpus is not committed; the regression is.

## Benchmarking

`benches/` holds Criterion benchmarks, one file per area:

| Bench | Covers |
| ----- | ------ |
| `text_ops` | The pure transforms, at 100 and 2,000 lines — they rebuild their unit ranges from the whole buffer on every keystroke |
| `editor_ops` | Opening a file (parse + highlight, up to a ~100 MB synthetic file), typing, a 10k-operation burst of random inserts/deletes, pasting, undo, whole-buffer line transforms |
| `search_and_palette` | The three per-keystroke search paths: find-in-buffer, workspace search (up to a generated 10k-file tree), and palette fuzzy filtering |

```sh
cargo bench                                  # everything
cargo bench --bench editor_ops               # one file
cargo bench -- editor/open                   # one group; Criterion compares to the last run
```

`cargo bench` runs under `[profile.bench]` (speed-optimized, no LTO) rather
than letting it inherit `[profile.release]` (`opt-level = "z"`, `lto = true`
— tuned for a small shipped binary, not representative hot-path timing, and
too slow to relink for a benchmark someone reruns often).

Criterion stores each run under `target/criterion/` and reports the change from
the previous one, so the workflow is: measure, change, measure again, and read
the percentage. That is how the query-compilation cost was found — opening a
*200-line* file cost 26 ms, and the fixed cost stood out against the 5,000-line
case. Caching the compiled Tree-sitter query took it to 0.6 ms.

Benchmark what the user waits for: a keystroke, a frame, a file open. Do not
benchmark what runs once at startup or what is dominated by I/O. See
[`docs/performance/index.md`](../../docs/performance/index.md) for one
machine's baseline numbers.

## Repository invariants

Some rules are about the repository rather than any one function, and they are
tested like anything else:

- **Every `t!` key exists in `locales/app.yml`.** A missing key does not fail the
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
