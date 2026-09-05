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

## As implemented in Vix

`vix-tasks` is pure logic — detection, resolution, discovery, merging, and
history/test-at-point policy, over plain data (file contents, directory
listings, cursor positions) with no filesystem or process I/O of its own — so
every module is unit-tested without a live editor or project. The host
(`src/app.rs`) does all I/O, prompting, running, and persistence: reading
manifests and `.vix/project.toml`, the `ProjectSlot`-addressed confirm/edit
prompt, `resolve_lifecycle`/`discovered_tasks_at`, and the session-store
read/write pair described above. The `vix-menu` crate owns the **Project**
menu structure and its `C-c p c …` chords.

This crate absorbed the earlier standalone `vix-projectile` crate; there is
no separate crate by that name.
