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

## Baseline (measured 2026-09-02)

Apple M4 Max, macOS (Darwin 25.6.0), `rustc 1.98.0`, `cargo bench` (release,
no LTO). These are **one machine's numbers, not a promise** — the point is
the shape (which sizes are cheap, which are not) and a reference to diff a
later run against, not an absolute SLA.

Each cell is Criterion's middle (best) estimate from a 10–100 sample run; see
`target/criterion/` locally for the full confidence interval.

| Benchmark | Input | Time |
| --------- | ----- | ---- |
| `editor/open` | 200 lines | 427 µs |
| `editor/open` | 5,000 lines | 10.9 ms |
| `editor/open` | 20,000 lines | 37.1 ms |
| `editor/open` | 125,000 lines (~5 MB, plan T006's "syntax-highlight a 5 MB Rust file") | 234 ms |
| `editor/open` | 2,500,000 lines (~100 MB, plan T006's "open/parse a 100 MB file") | 5.05 s |
| `editor/typing` | 200 lines, one keystroke | 14.3 µs |
| `editor/typing` | 5,000 lines, one keystroke | 2.45 µs |
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

A few things this baseline already shows, worth carrying into T121's targets
(that task sets explicit budgets — open 100 MB < 1 s, keypress-to-frame <
16 ms at 10 MB, workspace search 10k files < 500 ms — driven by this run):
opening the 100 MB file (5.05 s) is the one furthest from its eventual budget
and the clearest incremental/lazy-highlighting candidate; `workspace_search`
already clears the 10k-file/500 ms target comfortably at 214 ms and scales
close to linearly with file count (33 ms → 214 ms for 1k → 10k, roughly 10×
for 10×); `editor/random_edits`' 1.60 s for 10k random-position edits, against
`editor/typing`'s 2–14 µs steady-state single keystroke, says random-position
churn (structural edits scattered across a buffer, not typing at one spot) is
disproportionately expensive — a candidate worth its own investigation before
assuming T121's incremental-highlighting work alone would fix it.

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
