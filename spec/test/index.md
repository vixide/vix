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

## Fuzzing

`fuzz/` is a separate workspace (nightly + sanitizer instrumentation), so the
root `cargo build` never touches it. Targets cover the code that parses input
Vix did not write: `textops`, `org_table`, `lsp_frame`, `vcard`, `conflict`,
`http_request`. See [`fuzz/README.md`](../../fuzz/README.md) for running them,
minimizing a crash, and adding a target.

A fuzz target asserts **invariants**, not merely the absence of a panic — "a
returned cursor is a valid offset into the returned text", "render then parse
keeps every row". Those assertions are what turn a fuzzer into a bug-finder
rather than a crash-detector: the hline-only-table bug (`align` deleting a `|-`
line instead of squaring it up) was caught by the round-trip assertion, not by a
panic.

When a target finds something: minimize it (`cargo +nightly fuzz tmin`), then
write the minimized input as a **unit test in the crate that owns the code**.
The corpus is not committed; the regression is.

## Benchmarking

`benches/` holds Criterion benchmarks, one file per area:

| Bench | Covers |
| ----- | ------ |
| `text_ops` | The pure transforms, at 100 and 2,000 lines — they rebuild their unit ranges from the whole buffer on every keystroke |
| `editor_ops` | Opening a file (parse + highlight), typing, pasting, undo, whole-buffer line transforms |
| `search_and_palette` | The two per-keystroke search paths: find-in-buffer and palette fuzzy filtering, at 1,000 and 20,000 items |

```sh
cargo bench                                  # everything
cargo bench --bench editor_ops               # one file
cargo bench -- editor/open                   # one group; Criterion compares to the last run
```

Criterion stores each run under `target/criterion/` and reports the change from
the previous one, so the workflow is: measure, change, measure again, and read
the percentage. That is how the query-compilation cost was found — opening a
*200-line* file cost 26 ms, and the fixed cost stood out against the 5,000-line
case. Caching the compiled Tree-sitter query took it to 0.6 ms.

Benchmark what the user waits for: a keystroke, a frame, a file open. Do not
benchmark what runs once at startup or what is dominated by I/O.

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
