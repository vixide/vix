# Tasks

Project task running: named tasks, per-project-type lifecycle commands,
discovered tasks, subprojects, and test-at-point. It has three parts:

- **Named tasks** (`tasks.toml`): user-configured commands, merged with
  project-type defaults and tasks discovered from the project's own build
  tooling. **Tools → Tasks…** (action `tools.tasks`, also in the command
  palette) lists the merged set and runs the chosen one.
- **Project lifecycle** (the **Project** menu, `project.*`): six fixed
  command slots — configure/compile/test/install/package/run — resolved from
  the detected project type(s), a per-project override file, and a per-user
  cache, in that precedence order. Available for the workspace root and, via
  **Project → Subproject**, for any monorepo subproject.
- **Test at point** (`project.test_at_point`): finds the test enclosing or
  preceding the cursor and runs just that one, for the languages `vix-tasks`
  supports.

All of the above resolves *what command string to run*; running it (spawning
the shell, streaming output, reporting exit status) always goes through
`App::run_command`/`run_command_in`, exactly like a manual Run Command —
output streams to the bottom dock and the completion posts to the
notification panel (Info on exit 0, Error otherwise).

## Named tasks (`tasks.toml`)

Loaded from `<root>/tasks.toml`, falling back to `<root>/.vix/tasks.toml`:

```toml
[[task]]
name = "build"
command = "cargo build"

[[task]]
name = "test"
command = "cargo test"
```

Tasks with an empty `name` or `command` are dropped. If neither file exists or
parsing fails, the chooser reports `status.no_tasks`.

**Tools → Tasks…** merges three tiers into one list (`vix_tasks::task`):
user-configured (`tasks.toml`) tasks, one default task per detected project
type per lifecycle slot, and tasks discovered from the project's own build
manifests (below). On a name collision the user-configured entry wins, then
the project-type default, then the discovered task; a task promoted by a
higher-precedence tier keeps its original list position. Discovered task
names are always tool-prefixed (`npm:build`, `just:test`, …), so they cannot
collide with a user or project-type name by construction.

- The chooser is a list of `name — command` rows; `↑`/`↓` move, `Enter` (or a
  row click) runs the highlighted task, `Esc` cancels.
- Running a task calls `App::run_command`, so output streams to the bottom
  dock and the completion posts to the notification panel — exactly like a
  manual Run Command.

## Project types

`vix_tasks::project_type` recognizes a project directory's build tooling by
marker-file presence: `cargo`, `npm`/`yarn`/`pnpm`/`bun` (told apart by
lockfile — `package.json` alone means `npm`), `make`, `just`,
`python-poetry`/`python` (told apart by `pyproject.toml`'s content:
`[tool.poetry]` means `python-poetry`, otherwise a bare `pyproject.toml` is
unclassified), `go`, and `deno`. More than one type can match a directory
(e.g. `cargo` + `just`); [`default_lifecycle`](../src/lifecycle.rs) merges
their default commands, earliest-registered type winning a slot collision.
Each type declares up to six default commands, one per lifecycle slot below —
a `None` means that type has no sensible default for that slot (e.g. `deno`
has no `configure`/`compile`/`install`/`package` default).

## Project lifecycle (the Project menu)

Six command slots, each a menu item / action id / `C-c p c <letter>` chord:

| Slot        | Action              | Chord       |
| ----------- | -------------------- | ----------- |
| Configure   | `project.configure`  | `C-c p c o` |
| Compile     | `project.compile`    | `C-c p c c` |
| Test        | `project.test`       | `C-c p c t` |
| Install     | `project.install`    | `C-c p c i` |
| Package     | `project.package`    | `C-c p c p` |
| Run         | `project.run`        | `C-c p c r` |

Each opens a confirm/edit prompt seeded with the *resolved* command, then runs
the (possibly hand-edited) result. Resolution (`App::resolve_lifecycle`,
`vix_tasks::lifecycle::effective_lifecycle`) applies, lowest to highest
precedence:

1. The merged project-type default for the slot (`default_lifecycle`).
2. The shareable `<root>/.vix/project.toml` override — meant to be checked
   into the repo and shared with a team. TOML keys `configure`, `compile`,
   `test`, `install`, `package`, `run`; a missing file or unparseable TOML
   falls through to "no override" rather than erroring.
3. The current session's resolved-command cache — an edited command sticks
   for the rest of the session (and across restarts, via the session store)
   until **Project → Discard Command Cache** (`project.discard_command_cache`)
   clears it. The override file is untouched by discarding the cache.

If a slot has no command at any tier, the prompt is skipped and
`status.project_no_command` is reported instead.

Two more top-level actions round out the menu:

- **Run Task…** (`project.run_task`) — the same three-tier merged task list
  as **Tools → Tasks…**, scoped to the workspace root.
- **Repeat Last Task** (`project.repeat_last_task`, `C-c p c X`) — re-runs
  whichever project command (a lifecycle slot or a named task) ran most
  recently, or reports `status.project_no_last_command` if nothing has run
  yet this session.

Every accepted lifecycle command also pushes onto that slot's run history
(`vix_tasks::history::push_history`, `DuplicatePolicy::IgnoreConsecutive` — a
command equal to the slot's current last entry is not pushed again) and
updates the project's last-run-command-of-any-kind.

### Persistence

Two tiers, matching the precedence above:

- **Shareable**: `<root>/.vix/project.toml`, read-only to this crate (the
  host never writes it) — a team-shared override file.
- **Private, per-user**: the resolved-command cache, per-slot history, and
  last-run command, held in `App` and persisted to the session store keyed
  by workspace root, lazily loaded once per run
  (`ensure_project_session_loaded`) and written back by `save_session`. A
  run that never touches a `project.*`/`project.subproject.*` action carries
  the previous save forward unchanged rather than overwriting it with empty
  defaults.

## Task discovery

`vix_tasks::discover_*` parses seven kinds of build-tool manifest into named
tasks, each prefixed with its tool so it cannot collide with a user or
project-type task name. All are pure parsing functions (no I/O); the host
reads whichever manifests are present at a directory and merges the results:

| Source              | Manifest(s)                                         | Prefix     |
| -------------------- | --------------------------------------------------- | ---------- |
| npm/yarn/pnpm/bun    | `package.json` `scripts`                             | `npm:` etc.|
| Deno                 | `deno.json`/`deno.jsonc` `tasks` (JSONC comments stripped by a line-based heuristic) | `deno:`    |
| Composer              | `composer.json` `scripts` (string or array, array joined with `&&`) | `composer:`|
| just                  | `justfile`/`.justfile`/`Justfile` recipe headers (name at column 0, not a `name := value` assignment) | `just:`    |
| go-task               | `Taskfile.yml`/`.yaml` `tasks:` mapping (parsed as YAML, any valid shape) | `task:`    |
| Rake                  | `Rakefile` + any `*.rake` files at the root or under `rakelib/` | `rake:`    |
| Make                  | Plain named targets in a `Makefile` (`^name:` at column 0, skipping `.`-prefixed and `%`-pattern targets) | `make:`    |

Malformed input (bad JSON/YAML, a missing or wrongly-typed key) is not an
error: the affected source contributes no tasks rather than failing the
whole merge.

## Subprojects

For monorepos, `vix_tasks::subproject::find_subprojects` scans a project's own
file listing (gitignore-aware, supplied by the host) for any directory below
the root containing a project-type marker file, one `Subproject` per such
directory (deduplicated, sorted by relative root). The root itself is never a
subproject of itself.

**Project → Subproject** (`C-c p c m …`) mirrors the top-level lifecycle
family, resolved at the *nearest enclosing subproject of the active file*
(`nearest_subproject_for_active_file`) rather than the workspace root — same
six lifecycle slots plus **Find File** (`project.subproject.find_file`,
`C-c p c m f`), which opens the palette scoped to that subproject's
directory. Any subproject action reports `status.project_no_subproject` when
the active file is not inside one. The override file and command
cache/history stay workspace-root-scoped — only the project-type detection
and the run's working directory vary by subproject.

## Test at point

`project.test_at_point` (`C-c p c .`) resolves the test enclosing or
preceding the cursor in the active buffer and runs it directly — no confirm
prompt. `vix_tasks::test_at_point` uses line-based regex heuristics per
language (scan upward from the cursor for the nearest matching
test-definition line), not a syntax tree: simpler and dependency-light, right
for a cursor inside or just below a single test body, but it does not
understand real block nesting. Supported: Rust, Python, Go,
JavaScript/TypeScript, Ruby (RSpec and Minitest), and Elixir. Not
implemented: Java, Erlang, F#.

`vix_tasks::test_at_point::tests_in_file` (T726) applies the same rules to a
whole file and lists every test in file order as `(line, TestMatch)`, the line
being the one that names the test (Rust: the `fn` line after `#[test]`). The
host draws a local **Run test** code lens on each of those lines where the
language server put no lens of its own (see `spec/ide-parity`).
`knows_tests_in(ext)` says whether either function knows a test syntax for a
file extension, so the host skips copying a buffer's text for a file in which
they would find nothing — a 100 MB log, say (T749).

## Problem matchers and next-error (T632)

Every command that runs through `App::run_command_in` — a task, a lifecycle
slot, Repeat Last Task, test-at-point, a run configuration, **Tools → Test**
and a manual **Run Command** — has its output scanned for *problems* (compiler
errors, lint findings, failing tests) as it streams into the bottom dock, the
way VS Code's `problemMatcher`, Emacs's `compilation-mode` and Vim's quickfix
list do. Version-control and container commands, which run without a shell
(`run_argv`), are not scanned.

`vix_tasks::matcher` is pure: it reads output lines and returns `Problem`s
(`file` as printed, 1-based `line`, optional 1-based `column`, `Severity`
error/warning/info, a one-line `message`, and the `source` matcher). It never
touches the filesystem.

### Built-in matchers

| Name (aliases)        | Recognizes |
| --------------------- | ---------- |
| `rustc` (`cargo`)     | An `error[E…]: msg` / `warning: msg` header followed, possibly lines later, by its ` --> file:line:col` arrow (multi-line); `note:`/`help:` blocks and arrows without a pending header are ignored. A `thread '…' panicked at file:line:col:` line, its message taken from the next line (and the older `panicked at 'msg', file:line:col` form). |
| `gcc` (`clang`)       | `file:line:col: error: msg`, `fatal error:` and `warning:` (column optional). Also catches mypy's `file:line: error: msg`. Notes are ignored. |
| `tsc`                 | `file(line,col): error TS1234: msg` and the pretty `file:line:col - error TS1234: msg`. |
| `eslint`              | The default *stylish* format (a file-name line, then indented `line:col  error  msg  rule` rows — multi-line), plus the `compact` and `unix` formats. |
| `python` (`pytest`)   | A traceback: each `File "f", line N` frame, ended by the unindented `SomeError: msg` line, gives one problem at the innermost frame (multi-line); pseudo-files like `<string>` are skipped. pytest's long-format `path.py:N: SomeError` lines and `path.py:N: SomeWarning: msg` warnings (a frame line `path.py:N: in test_x` is not a problem). |
| `go`                  | `file.go:line[:col]: msg`, with the leading indentation of `go test` failure lines allowed. Stack-trace lines (`/x/y.go:12 +0x1d`) are not problems. |

Before matching, ANSI color escapes are stripped and a line is cut to its first
4,096 bytes. The same `(file, line, column, message)` is kept only once (cargo
prints a warning for every target it builds), and a run keeps at most
`MAX_PROBLEMS` (5,000) problems.

### Choosing a matcher

A `[[task]]` in `tasks.toml` may name one with `matcher`:

```toml
[[task]]
name = "lint"
command = "npx eslint src"
matcher = "eslint"
```

`MatcherSpec::parse` accepts:

- missing, empty or `"auto"` — every built-in matcher at once (the default, and
  what every command other than a `tasks.toml` task with a `matcher` uses);
- `"none"` or `"off"` — no matching;
- a built-in name or alias from the table above — only that matcher;
- anything else — a custom regular expression with the named groups `file` and
  `line` (both required), and optionally `column`, `severity` (text starting
  `err`/`fatal` is an error, `warn` a warning, anything else info; absent means
  error) and `message` (absent means the whole line). An invalid expression, or
  one missing a required group, is reported in the notification panel and the
  run falls back to `auto`.

### Paths

A problem's `file` is resolved by `matcher::resolve_path(cwd, file)` against the
**working directory of the run that printed it** (the workspace root, or a
subproject's directory for `project.subproject.*`): an absolute path is kept, a
relative one is joined to the cwd, and `.`/`..` are folded lexically. The host
falls back to the workspace root when the cwd-relative path is not a file
(cargo prints workspace-root-relative paths from a member's directory).

Matching never reads a file. A matched file is read only when the user acts —
**Go → Next/Previous Error**, `Enter` in the Problems panel, or a click in the
bottom dock — and only if it exists; the inline text below is drawn only on a
buffer that is already open. A path pointing outside the workspace (a
dependency's source, a Python standard-library frame) is listed like any other,
but nothing is opened until the user picks it.

### Results

`matcher::ProblemList` holds the last run's problems, the cwd they resolve
against, and a navigation cursor. A new scanned run clears it when it starts;
when the run ends the status line reports the count.

- **Go → Next Error** (`go.next_error`) and **Go → Previous Error**
  (`go.prev_error`) step through the list in output order across files, like
  Emacs's `next-error`, **wrapping around** at either end: the first press of
  Next visits the first problem, of Previous the last. Each opens the file at
  the problem's line and column (recorded in the jump history) and shows
  `Problem i of n` with the message in the status line; a file that does not
  exist is reported instead of opened. With no problems they report
  `status.no_task_problems`. Both belong to the `tasks` capability.
- **The Problems panel** (`lsp.diagnostics`) lists them after the language
  servers' diagnostics as a task source: `rel:line: [severity] message
  (matcher)`; `Enter` jumps.
- **Inline**: on the active buffer, problems for its file are drawn as
  end-of-line virtual text on their own layer (`VirtualTextLayer::TaskProblems`,
  after the LSP `Diagnostics` layer), colored by severity, following the same
  `inline_diagnostics` setting (`off`, `cursor-line`, `all`).
- **The bottom dock** click jumps through `matcher::locate(line)` first — every
  built-in single-line pattern plus the rustc arrow and the Python `File "…",
  line N` frame — resolved against the last run's cwd, and falls back to the
  plain `path:line[:col]` parse. `locate` is the one `path:line` parser later
  terminal work (T666) can reuse.

Repeat Last Task re-runs the command string with `auto`, since only the command
is remembered.

## As implemented in Vix

`vix-tasks` is pure logic — detection, resolution, discovery, merging, and
history/test-at-point policy, over plain data (file contents, directory
listings, cursor positions) with no filesystem or process I/O of its own — so
every module is unit-tested without a live editor or project. The host
(`tui/src/app.rs`) does all I/O, prompting, running, and persistence: reading
manifests and `.vix/project.toml`, the `ProjectSlot`-addressed confirm/edit
prompt, `resolve_lifecycle`/`discovered_tasks_at`, and the session-store
read/write pair described above. The `vix-menu` crate owns the **Project**
menu structure and its `C-c p c …` chords.

This crate absorbed the earlier standalone `vix-projectile` crate; there is
no separate crate by that name.

## Run configurations (T724)

`run_config` reads `.vix/run.toml` (`[[config]]` tables: `name`, exactly one of
`command` or `compound`, optional `cwd`, `env`, `before`). `parse` validates
(`RunConfigError::{Parse, Duplicate, Invalid}`); `plan(configs, tasks, name)`
flattens a configuration into ordered stages — `before` entries first (a name
that is not a configuration may be a `tasks.toml` task, and a shared
dependency runs once), a compound's members together in one stage — and
reports `Unknown`, `Cycle` or `NotFound`. `Plan::shell_command(Shell)` renders
one line for the host's single command pipeline: stages joined by `&&`, a
stage's members run as background jobs and `wait` on POSIX, in order on
`cmd.exe`; `cwd` and `env` are applied with quoting. Pure: no I/O besides
`load`. Documented in `docs/ide-parity/index.md`.
