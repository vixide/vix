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
default `[profile.release]` (size-optimized, `lto = "thin"`) that a plain
bench run would otherwise inherit — the release profile is tuned for a small
shipped binary, not for representative hot-path timing, and workspace-wide
LTO makes the final link slower to rerun casually.

## Baseline (measured 2026-09-15, `editor/open` re-measured after T121, `startup/*` added for T122)

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
| `startup/app_new` | `App::new` — theme scan, editor/menu/LSP-client setup, no `git` involved | 134 µs |
| `startup/refresh_git` | `refresh_git` (3 `git` subprocesses: repo?/branch/status) against an empty fixture repo | 12.2 ms |

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

## Cold start (T122)

`main.rs` timed step by step opening this repository itself (`~/git/vixide/
vix`, a real, large, ~115-crate workspace — not a synthetic fixture) with a
throwaway instrumented build: `Settings::load` 0.6–1.0 ms, `App::new`
10.1 ms, `load_scripts`/`maybe_prompt_script_trust`/`resolve_key_overrides`/
`maybe_show_welcome`/`restore_session` combined under 1 ms — and
`refresh_git` **75–82 ms**, by far the largest single cost, all of it before
`ratatui::init()` had even taken over the terminal. `refresh_git` shells out
to `git` three times (repo?, branch, status); on a small/empty repo that's
cheap (`startup/refresh_git`'s own 12.2 ms, above, against a fixture repo
with no files or history) but real subprocess spawn overhead, `git status`
included, scales with repository size — this one just happens to be a large
one, which is exactly the realistic case worth fixing for.

The fix: `main` no longer calls `refresh_git` before the first frame draws.
`App`'s git-related fields (`git_repo`, `git_branch`, `git_status`) already
default to "not a repo" — the same state opening Vix outside a repo renders
correctly today — so drawing before that first `refresh_git` call is safe;
the terminal now shows the real editor almost immediately (`Settings::load`
+ `App::new`, ~11 ms in the instrumented run above, before terminal setup's
own small cost), with the branch/status indicator catching up one frame
later rather than blocking everything that comes after it. Nothing else
measured mattered enough to move: every other pre-first-frame step was
already sub-millisecond, so — per this task's own "defer *if* measurement
says it matters" — none of them needed to move.

A smaller, unconditional fix landed alongside it: `App::new` scanned the
custom-themes directory (real filesystem I/O) **twice** — once inside
`apply_saved_theme`, once again immediately after for the View → Theme
submenu's name list — now scanned once and reused for both.

## Large File Viewer (T733)

The Large File Viewer opens files too big for a buffer without loading them
(see [`crates/vix-large-file/spec`](../../crates/vix-large-file/spec/index.md)).
Measured 2026-10-30 with `scripts/bench-large-file`, which generates a 1 GiB
and a 4 GiB log-like file (~113-byte lines), opens each headlessly in the
viewer with a 50 × 200 screen, and deletes them afterward.

Machine: Apple M4 Max (16 cores), 128 GiB RAM, internal SSD (APFS), macOS
27.0.1, `rustc 1.99.1`, `--release`. **Warm cache**: each file is measured
right after it is written, so it is in the OS page cache; a cold read from
disk is bounded by the drive instead, and only the full-index and search
rows (which read the whole file) would change much.

| Measurement | 1 GiB (9.5 M lines) | 4 GiB (38.1 M lines) |
| ----------- | ------------------- | -------------------- |
| Time to first screen (open + first 50 lines drawn) | 0.3–0.4 ms | 0.2 ms |
| Goto end (before the index has started) | 0.1 ms | 0.1 ms |
| Time to full line index (background) | 119–120 ms | 480–487 ms |
| Goto a line in the middle (index done) | 0.2 ms | 0.2 ms |
| Search the whole file for a string on its last line | 83–86 ms | 335–341 ms |
| Peak RSS of the whole process (`/usr/bin/time -l`) | 4.3 MiB | 3.6–4.6 MiB |
| Line-index checkpoints kept | 9,298 (73 KiB) | 37,191 (291 KiB) |

Ranges are two runs. The quotable number: **a 4 GiB file opens to its first
screen in under a millisecond and is fully indexed in about half a second,
in under 5 MiB of memory** (this machine, warm cache). Memory does not grow
with the file beyond the checkpoints (8 bytes per 1,024 lines); the index
and search each read 1 MiB at a time. Process start-up and terminal setup
are not included: the benchmark times the viewer, not `vix` launching.

Reproduce: `scripts/bench-large-file` (or `scripts/bench-large-file 1` for
one size). It needs about 4 GiB of free space in `$TMPDIR` at a time.

## Startup and memory vs other editors (T744)

How long Vix takes to show a file, and how much memory it holds, next to the
other terminal editors installed on the same machine, measured the same way.
Every editor that was measured is listed, including the ones that beat Vix.

Measured 2026-10-30 with `scripts/bench-editors` (commit `282b529f` plus this
script). Machine: Apple M4 Max (16 cores), 128 GiB RAM, internal SSD (APFS),
macOS 27.0.1 (Darwin 27.0.0). Vix built with `cargo build --release -p
vix-tui` (`rustc 1.99.1`, the shipped `[profile.release]`, default features).

| Editor | Version | Command (config disabled) |
| ------ | ------- | ------------------------- |
| Vix | 2.0.0 (unreleased, `main`) | `vix FILE` with a fresh `HOME`; `large_file_mb = 1024` so the 100 MB file opens as text |
| Vim | 9.2 | `vim -u NONE -i NONE -n -N FILE` |
| Neovim | 0.12.6 | `nvim --clean -n FILE` |
| Helix | 25.07.1 | `hx -c EMPTY.toml FILE` |
| GNU Emacs | 30.2 | `emacs -nw -Q --eval '(setq large-file-warning-threshold nil)' FILE` |
| micro | 2.0.15 | `micro -config-dir EMPTY_DIR FILE` |
| GNU nano | 9.3 | `nano -I FILE` |
| mg | macOS `/usr/bin/mg` (no version flag) | `mg -n FILE` |

Not installed, so not measured: Kakoune. Zed is installed but is a GUI
editor, so it is out of scope for a terminal comparison.

Each cell is the **median (min–max)** of 10 runs on the 10,000-line file
and 5 runs on the 100 MB file, after 2 discarded warm-up runs per editor and
file.

**10,000-line Rust file (~300 KB)**

| Editor | Time to first screen, ms | Memory, MiB | Idle CPU, % |
| ------ | ------------------------ | ----------- | ----------- |
| nano | 10.1 (9.3–65.9) | 8.3 (8.3–8.4) | 0 |
| Vim | 21.1 (16.8–48.0) | 12.4 (12.3–12.4) | 0 |
| Neovim | 31.2 (29.4–46.3) | 25.0 (25.0–25.2) ¹ | 0 |
| micro | 40.1 (37.8–116.5) | 28.2 (27.4–28.5) | 0 |
| **Vix** | **40.6 (39.0–52.9)** | **33.9 (33.3–34.5)** | 0 |
| mg | 43.5 (10.6–152.1) | 2.5 (2.4–2.5) | 0 |
| Emacs | 185.0 (180.3–195.2) | 53.6 (53.5–53.8) | 0 |
| Helix | 207.7 (192.4–327.4) | 59.3 (53.7–64.2) ² | 0 |

**100 MB plain-text log file (~1.1 M lines)**

| Editor | Time to first screen, ms | Memory, MiB | Idle CPU, % |
| ------ | ------------------------ | ----------- | ----------- |
| Vim | 64.6 (62.8–66.7) | 116.8 (116.8–117.0) | 0 |
| **Vix** | **99.4 (98.1–110.0)** | **1100.9 (1100.8–1102.8)** | **7.5 (7.0–8.8)** |
| Neovim | 103.2 (99.9–109.9) | 129.8 (129.8–129.8) ¹ | 0 |
| micro | 155.1 (142.5–505.4) | 351.5 (344.5–355.8) | 100.7 (93.6–101.4) |
| Emacs | 307.0 (302.6–308.7) | 148.7 (148.7–148.8) | 0 |
| nano | 490.9 (264.9–883.9) | 159.2 (159.1–159.2) | 0 |
| Helix | 554.9 (550.9–555.8) | 520.5 (509.8–526.1) | 0 |
| mg | 1329.1 (1309.5–1545.9) | 136.6 (136.6–136.6) | 0 |

¹ Neovim runs as two processes (the terminal UI and an embedded editor
server); the figure is both together. The UI process alone peaks at 10.6
MiB. ² Helix sometimes had a second process alive while it was sampled;
the figure includes it (Helix's own peak RSS is 53.8 MiB).

What the numbers say, without spin:

- **Startup.** On a 10,000-line source file Vix shows its first screen in
  about 41 ms: tied with micro, about 10–20 ms behind Vim and Neovim, about
  30 ms behind nano, and about 4–5× faster than Emacs (185 ms) and Helix
  (208 ms) with their configurations disabled.
- **Memory on ordinary files.** Vix holds about 34 MiB with that file open:
  more than nano, Vim, Neovim and micro (8–28 MiB), less than Emacs and
  Helix (54–59 MiB).
- **Memory on a 100 MB file is Vix's weak spot.** It shows the first screen
  quickly (99 ms, second only to Vim), but holds **1.1 GiB**, about 11× the
  file's size and the most of any editor measured (Vim 117 MiB, Helix 521 MiB). It
  also keeps using about 7.5% of a core afterwards with no input. For files
  this size, use the [Large File Viewer](#large-file-viewer-t733), which
  stays under 5 MiB; the text buffer's memory use is a known gap.

### Method

- **Time to first screen** is measured from just before `fork` until two
  marker words — one on the file's first line, one on its twentieth — have
  both been written to the terminal (escape sequences stripped). That counts
  process start, loading the file and drawing the first frame; it does not
  count a terminal emulator turning those bytes into pixels.
- Each editor runs in a 120 × 40 pseudo-terminal with `TERM=xterm-256color`,
  its user configuration disabled (table above), and `HOME` and the `XDG_*`
  directories pointed at a fresh temporary directory, so no personal config,
  plugin, session or history is loaded.
- The harness answers the queries editors send at startup (device
  attributes, status report, cursor position, cell size, background color,
  mode queries) as a typical xterm-compatible terminal does. This matters:
  Vix probes the terminal for image support at startup and, in a terminal
  that does not answer the status-report query (`ESC [ 5 n`), waits about
  250 ms for it — an earlier version of the harness that did not answer it
  measured Vix at ~297 ms and Neovim at ~170 ms.
- **Memory** is the resident memory of the editor's whole process tree,
  sampled every 0.25 s for 3 s after the first screen; the cell is the
  largest sample. For single-process editors it matches the process's peak
  RSS from `wait4` (the number `/usr/bin/time -l` prints), which the script
  also records.
- **Idle CPU** is the CPU time the process tree used during the last 2 s of
  those 3 s, with no input, as a percentage of one core.
- **Warm cache only.** The binaries and input files are in the OS page cache
  (two discarded warm-up runs; the warm-up also absorbs first-launch work
  such as Vix's welcome screen). Cold-cache numbers need `sudo purge` and
  are not measured here.
- **Feature sets differ.** Vix runs with its default features (syntax
  highlighting, menus, file explorer, Git status). Of the others, Neovim
  (`--clean` keeps its built-in syntax files), Helix and micro highlight the
  Rust file; Vim with `-u NONE`, nano with `-I` and mg do not, and Emacs
  `-Q` opens it in `fundamental-mode` with no highlighting (Emacs 30 has no
  Rust mode without a grammar or package). These are each editor's own
  behavior with user configuration off, not equal workloads.
- The 100 MB file is above two editors' "ask first" limits: Vix's
  `large_file_mb` (50 MB by default) and Emacs's
  `large-file-warning-threshold` (10 MB). Both are raised so the file opens
  directly; with the defaults both editors stop at a question instead.
- One machine, one day, with no build running but other background
  activity not stopped. Spreads are min–max; isolated slow outliers (for
  example nano 65.9 ms, mg 152 ms) are scheduler noise and are left in. A
  trial run of nano during a `cargo` build took 175 ms instead of 10 ms on
  the small file, so run it on an idle machine.

Reproduce: `cargo build --release -p vix-tui && scripts/bench-editors`
(`--runs`, `--big-runs`, `--json PATH`, or a list of editor names to limit
it). It needs about 100 MB free in `$TMPDIR`, installs nothing, and deletes
its files afterwards.

## Binary size budget (T514)

CI's `binary-size` job (T008) only tracks a delta against the immediately
previous `main` build — useful for catching one PR's regression, but a slow
multi-quarter creep across many individually-small additions wouldn't show up
against a baseline that moves with it. This section is the number to compare
against over a longer horizon.

Measured 2026-09-19 (macOS/arm64, `cargo build --release`, `[profile.release]`
— `lto = "thin"`, `strip = true`, `opt-level = "z"`): the stripped `vix`
binary is **~25.4 MB**. Budget: flag it for a real look — not necessarily a
regression, could be a deliberate new bundled grammar or asset, but worth
asking why — if it crosses **35 MB** (roughly 40% headroom) on any platform
CI measures. Update this number (and re-set the budget) the next time it's
deliberately grown for a good reason, so the budget keeps meaning something.

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
