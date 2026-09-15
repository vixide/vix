# Performance

Baseline numbers for Vix's per-keystroke and per-frame hot paths, so a
regression is a number that moved, not a feeling. See
[`spec/test/index.md`](../../spec/test/index.md#benchmarking) for how the
benchmarks themselves are organized and how to read a run's console output.

## Running the benchmarks

```sh
cargo bench                            # everything
cargo bench --bench editor_ops         # one file
cargo bench -- editor/open             # one group; Criterion compares to the
                                        # last run under target/criterion/
```

`cargo bench` uses `[profile.bench]` (speed-optimized, no LTO) rather than the
default `[profile.release]` (size-optimized, `lto = true`) that a plain bench
run would otherwise inherit — the release profile is tuned for a small
shipped binary, not for representative hot-path timing, and workspace-wide
LTO makes the final link too slow to rerun casually.

## Baseline (measured 2026-09-15, `editor/open` re-measured after T121)

Apple M4 Max, macOS (Darwin 25.6.0), `rustc 1.98.0`, `cargo bench` (release,
no LTO). These are **one machine's numbers, not a promise** — the point is
the shape (which sizes are cheap, which are not) and a reference to diff a
later run against, not an absolute SLA.

Each cell is Criterion's middle (best) estimate from a 10–100 sample run; see
`target/criterion/` locally for the full confidence interval.

`editor/open` measures only `Editor::new` returning — since T121 (below),
that's no longer the same thing as "fully parsed and highlighted" for a
buffer at or above the 50 KB async-parse threshold, which is why these rows
moved so much from the original 2026-09-02 baseline: they're measuring the
same operation as before, but that operation itself got faster, not a
different one. `editor/open_until_highlighted` (new) measures the older,
"fully parsed too" quantity for the two sizes where that now differs — use
it, not `editor/open`, when the question is "how much total background CPU
does opening this file cost", not "how long does it block the user".

| Benchmark | Input | Time |
| --------- | ----- | ---- |
| `editor/open` | 200 lines (~8 KB, below the async-parse threshold) | 399 µs |
| `editor/open` | 5,000 lines (~200 KB) | 49.0 µs |
| `editor/open` | 20,000 lines (~800 KB) | 136 µs |
| `editor/open` | 125,000 lines (~5 MB, plan T006's "syntax-highlight a 5 MB Rust file") | 707 µs |
| `editor/open` | 2,500,000 lines (~100 MB, plan T006's "open/parse a 100 MB file") | 14.1 ms |
| `editor/open_until_highlighted` | 125,000 lines (~5 MB) — `editor/open`'s own number before T121 | 236 ms |
| `editor/open_until_highlighted` | 2,500,000 lines (~100 MB) — `editor/open`'s own number before T121 | 4.69 s |
| `editor/typing` | 200 lines, one keystroke | 14.3 µs |
| `editor/typing` | 5,000 lines, one keystroke | 2.27 µs |
| `editor/typing` | 250,000 lines (~10 MB, plan T121's "keypress-to-frame < 16 ms at 10 MB"), one keystroke | 3.35 µs |
| `editor/random_edits` | 10k inserts/deletes burst, plan T006's scenario | 1.60 s |
| `editor/paste_undo` | paste 50 lines | 12.4 µs |
| `editor/paste_undo` | undo | 272 µs |
| `editor/line_transforms` | sort_lines, 2,000 lines | 4.35 ms |
| `editor/line_transforms` | remove_duplicate_lines, 2,000 lines | 3.29 ms |
| `editor/line_transforms` | trim_trailing_whitespace, 2,000 lines | 4.21 ms |
| `find/matches` | plain, 1,000 lines | 190 µs |
| `find/matches` | plain, 20,000 lines | 4.50 ms |
| `find/matches` | regex, 1,000 lines | 341 µs |
| `find/matches` | regex, 20,000 lines | 3.83 ms |
| `workspace_search/rescan` | 1,000 files, plan T006's "10k-file tree" scenario | 33.4 ms |
| `workspace_search/rescan` | 10,000 files | 214 ms |
| `palette/fuzzy` | filter, 1,000 candidates | 59.6 µs |
| `palette/fuzzy` | filter, 20,000 candidates | 1.20 ms |
| `palette/fuzzy` | rank, 1,000 candidates | 149 µs |
| `palette/fuzzy` | rank, 20,000 candidates | 1.37 ms |
| `textops/cursor_rewrites` | transpose_words / transpose_sentences / delete_word / delete_paragraph / smart_toggle, 100 lines | 4.1–18.7 µs |
| `textops/cursor_rewrites` | same, 2,000 lines | 66.3–336 µs |
| `textops/whole_text` | wrap_80, to_crlf, squeeze_blank_lines, sentence_starts | 34.3–873 µs |

T121's three explicit budgets — open 100 MB < 1 s, keypress-to-frame < 16 ms
at 10 MB, workspace search 10k files < 500 ms — are now **all met**, two of
them by a wide margin:

- **Open 100 MB < 1 s**: was the one furthest from budget in the original
  2026-09-02 run (5.05 s), and the clearest incremental/lazy-highlighting
  candidate that run named. It wasn't highlighting that was slow, though —
  highlight-query execution was already lazy and viewport-scoped (only the
  visible rows' byte range is ever queried, at any buffer size); the whole
  5.05 s was one synchronous, unconditional, whole-buffer Tree-sitter parse
  in `Code::new`, which never went through the background-worker machinery
  a post-edit reparse already used for large buffers. Routing the *initial*
  parse through that same worker (the change T121 actually made — see
  `crates/vix-editor-core/spec/syntax-highlighting/index.md`) took it to
  **14.1 ms**, a 350× improvement, without touching the highlight layer at
  all.
- **Keypress-to-frame < 16 ms at 10 MB**: **3.35 µs** for the mutation +
  incremental-reparse-trigger side of a keystroke in a 250,000-line (~10 MB)
  buffer — this budget was already met before T121 (typing already used the
  async-reparse worker for any buffer over 50 KB), this run just adds a
  benchmark point at the actual target size to prove it with a number
  instead of an inference from the 200/5,000-line points.
- **Workspace search 10k files < 500 ms**: already clears this comfortably
  at 214 ms (unchanged by T121 — a separate subsystem) and scales close to
  linearly with file count (33 ms → 214 ms for 1k → 10k, roughly 10× for
  10×).

One thing this baseline still shows, *not* addressed by T121 and worth its
own investigation: `editor/random_edits`' 1.60 s for 10k random-position
edits, against `editor/typing`'s 2–14 µs steady-state single keystroke, says
random-position churn (structural edits scattered across a buffer, not
typing at one spot) is disproportionately expensive — the incremental-parse
machinery amortizes a single edit well, but a *burst* of scattered ones
still adds up; a candidate for a future task, not something T121's parse/
highlight work touches.

## Reading the numbers

Benchmark what the user waits for — a keystroke, a frame, a file open — not
what runs once at startup or is dominated by I/O the user isn't watching.
The sizes that matter are the *large-buffer* ones: an editor is judged on the
20,000-line file, not the 200-line one, and the query-compilation regression
that motivated this whole suite (opening a 200-line file cost 26 ms because
of an uncached Tree-sitter query) only stood out against the 5,000-line case.

Criterion stores each run under `target/criterion/` and reports the percentage
change from the previous run — measure, change, measure again, read the
delta. Don't chase a percentage on a benchmark whose absolute time is a few
microseconds; noise dominates there. Do chase one on `editor/open`'s largest
sizes or `editor/random_edits` — those are seconds, and a real regression
shows up as a real number of seconds.
