# Scripting

Vix is compiled-in only today — every command is Rust shipped in the binary.
`vix-script` is the escape hatch: a user (or a project) can drop a small
[Rhai](https://rhai.rs/) script on disk and get a new palette command, a new
keybinding, or a one-off buffer transform, without a rebuild.

**Status**: the engine core is implemented (improvement plan T102) — discovery,
compiling, resource limits, and every API v1 function below, host-agnostic and
unit-tested with a mock `HostState` (no real `App`, no terminal). T103 (host
wiring) through T105 (samples + docs) are not done yet: nothing here is
called from `src/app.rs`, there is no palette entry, no startup load, no
`script.reload` action. Each remaining task should update this file if
reality and design turn out to disagree, same as anywhere else.

## Why Rhai

- **Pure Rust, no `unsafe`** — fits `#![forbid(unsafe_code)]` at every crate
  root in this workspace without an exception.
- **Embeds without an async runtime or a subprocess** — a script runs
  in-process, synchronously, inside the same single-threaded event loop as
  everything else `App::on_key` dispatches. No new concurrency model to
  reason about.
- **Closed by default** — Rhai's standard library has no file, network, or
  process API. A script's *only* capabilities are the Rust functions this
  crate registers (§ Execution model, "Sandboxing"); there is nothing to
  explicitly disable, because nothing unregistered exists to call.
- **Familiar syntax** — C-like expressions, `fn`, `let`; closer to Rust than
  Lua, without asking a scripter to learn a new language family.

## Script discovery

Two locations, both non-recursive, both `*.rhai` only:

| Scope | Path | Resolved by |
| ----- | ---- | ----------- |
| Global | `<config dir>/scripts/*.rhai` | a new `Settings::scripts_dir()`, the same `confy`-config-dir-relative pattern as `Settings::themes_dir()`/`macros_path()` |
| Project | `<root>/.vix/scripts/*.rhai` | `App::root`, the same `<root>/.vix/…` pattern `vix-tasks` already uses for `.vix/project.toml` and the `.vix/tasks.toml` fallback — no upward parent-directory search |

Both directories are optional; a missing one is not an error, just nothing
to load from there.

**Load timing**: all scripts under both paths load once at startup (after
`App::new`, before the first frame — mirrors how `vix-tasks` loads project
tasks). `script.reload` (Tools → Scripts → Reload) re-scans both directories
without restarting: every currently-registered command and binding is
dropped and discovery runs again from scratch, so a script that was deleted
or renamed doesn't leave a stale command behind.

**Name collisions**: a script is identified by its file stem (`foo.rhai` →
`foo`). If a project script and a global script share a stem, the project
one shadows the global one entirely — the global script does not load, not
"loads but its commands lose ties." This matches "project settings win over
global ones" elsewhere (`.editorconfig`, `.vix/project.toml`).

## Execution model

A script's top level runs once, at load time, and should do nothing but
call `register_command`/`bind_key` — the registration functions below. The
*handler* functions it registers run later, once per invocation (a palette
pick, a bound key), each a fresh, independent call into the script's Rhai
scope. There is no `await`, no coroutine, no way for a handler to suspend
mid-run and resume later: a handler either finishes synchronously within
its one host call, or (§ Prompting for input) ends by requesting a prompt
and is *re-invoked* — a new call, not a resumed one — as a different named
function when the user answers it.

**Sandboxing**: closed by default (§ Why Rhai). Nothing here needs an
explicit permission system for v1 — a script cannot read a file, open a
socket, or spawn a process, because no Rhai-visible function does any of
those things. If a later version wants to grant one of those deliberately
(e.g. an opt-in `read_file` for a project script that ships its own data),
that is a new, separately-specified capability, not a relaxation of this
default.

**Resource limits**: the `Engine` sets Rhai's built-in caps before running
any script — a maximum operation count per invocation (so an infinite loop
is caught deterministically, without a wall-clock timer or a second
thread), plus caps on expression depth, string size, and array/map size.
Hitting a cap ends the script the same way any other runtime error does
(§ Error handling) — Rhai's own error message names which limit was hit.
As implemented (T102): 10,000,000 operations, expression/statement depth 64,
1,000,000-character strings, 100,000-element arrays/maps — generous enough
that no reasonable script should ever notice them; revisit if one does.

## API v1

Every function below is a **Rhai-visible native function** this crate
registers on the `Engine`, called *from* script code. Buffer/selection
functions use **character offsets**, matching `vix-find-panel`'s convention
("operate on `&str` with character offsets") — not bytes, not `(line, col)`.

### Registering commands

```rhai
register_command("uppercase_selection", "Uppercase Selection", "on_uppercase");

fn on_uppercase() {
    set_selection_text(selection_text().to_upper());
}
```

- `register_command(id, label, handler)` — `id` is this script's own
  identifier for the command (unique within the script, not globally);
  `label` is the palette entry text, shown **verbatim** — script-authored
  text is not routed through `t!`/`locales/app.yml`, the same as a saved
  macro's name or a file name isn't; `handler` names a `fn` in the same
  script, called with no arguments when the command runs.
- The palette entry itself is namespaced `script:<script-stem>:<id>` (T103)
  — collisions between two scripts' own `id`s can't happen even if they
  pick the same one.

### Key bindings

- `bind_key(key_token, command_id)` — binds a key to a command this same
  script already registered. `key_token` reuses `vix-macros`' token
  grammar exactly (`C-`/`A-`/`S-` modifier prefixes, e.g. `C-c`, `S-Tab`,
  `Enter`, `a`) rather than inventing a second one — as implemented (T102),
  the token is validated with `vix_macros::decode_key` at registration
  time, so a malformed token (typo'd modifier, unknown key name) is a load
  error (§ Error handling) naming the bad token, not a binding that's
  silently recorded and never fires.
- Conflict handling is T104's job (binding against "the existing
  keymap-model override layer"), but the contract is fixed here: a
  conflicting `bind_key` is **reported, never silently clobbered** — either
  the script's bind loses and the user is told why, or both binds are
  rejected with a message; either way, a script cannot silently steal a key
  the built-in keymap or another script already owns.

### Buffer & selection

| Function | Effect |
| -------- | ------ |
| `buffer_text() -> String` | whole active-buffer text |
| `set_buffer_text(text)` | replace the whole buffer |
| `selection_text() -> String` | selected text, or `""` if there is no selection |
| `set_selection_text(text)` | replace the selection; with no selection, insert at the cursor |
| `current_line() -> String` | the line the cursor is on |
| `cursor_offset() -> int` | cursor position, as a character offset |
| `set_cursor_offset(n)` | move the cursor to character offset `n` (clamped in range) |

These are the whole surface for v1 — enough for a script to read text,
transform it in Rhai, and write it back, which is what "run textops-style
transforms" (plan.md) means here: a script *implements* its own transform
out of these primitives, rather than this crate exposing vix's internal
`vix-textops` functions as a second, parallel API.

### Prompting for input

```rhai
register_command("rename_word", "Rename Word Under Cursor", "on_rename");

fn on_rename() {
    prompt("New name:", "on_rename_answer");
}

fn on_rename_answer(answer) {
    // ... use `answer` ...
}
```

- `prompt(message, on_submit)` — opens a single-line input prompt showing
  `message`. Enter calls the script's `on_submit` function (named, like a
  handler) with the entered text as its one argument — a **new** call, not
  a resumed `on_rename`; whatever `on_rename` needs after the answer has to
  live in script-global state or be re-derived, since Rhai state is not
  captured across the two calls beyond the script's own global scope. Esc
  cancels — `on_submit` is not called at all.
- No richer prompt shapes (multi-field, a chooser list, a `y/n/!/q`
  step-through) in v1 — this is deliberately the same shape as a saved
  macro's rename prompt, not a UI toolkit.

### Messages

- `message(text)` — informational, non-blocking, goes to the message
  drawer.
- `error(text)` — same drawer, error styling. Still just a message: raising
  it does not stop the script, and it never reaches the host as a panic.

## Error handling

Two moments, two different failure shapes, neither ever reaches the host as
a Rust panic — a script is third-party input the same way a `.vcf` a user
opened is, and "never crash" is the literal bar (tasks.md T101):

- **At load** (startup or `script.reload`): a `.rhai` file that fails to
  parse, or whose top level errors while registering commands, is skipped —
  its file name and the Rhai error go to the message drawer, and every
  *other* script still loads normally. One broken script does not take
  scripting down.
- **At invocation** (a handler runs): a runtime error — a Rhai type error,
  an unhandled exception the script raised, a resource limit hit — aborts
  just that call. The error (script name, handler name, the Rhai message)
  goes to the message drawer. **This is not transactional**: whatever
  buffer mutations the handler already made before the error stay made:
  Rhai has no rollback, and this crate does not add one for v1. A script
  that must leave the buffer consistent on failure needs to structure its
  own logic that way (compute the new text fully before calling
  `set_buffer_text`/`set_selection_text`, rather than mutating
  incrementally) — worth saying explicitly in the T105 sample-script docs,
  not just here.

## Feature flag

`scripting`, **default-on** (tasks.md T101). Mirrors the existing
`lang-*`/`syntax-*` pattern for optional Tree-sitter grammars: the root
`vix` package depends on `vix-script` as `optional = true`, and the
`scripting` feature is exactly `["dep:vix-script"]`. Building with
`--no-default-features` (already how a user opts out of syntax highlighting
entirely) also drops scripting; there is no separate "everything but
scripting" toggle beyond the standard Cargo default-features mechanism.

## What's deliberately not in v1

Cut lines, not oversights — each is a bigger design question than this spec
needs to answer to unblock T102:

- **No file or network access.** Sandboxing (§ Why Rhai) depends on this
  being closed by default; opening it is a separately-specified capability,
  not a v1 feature.
- **No script-to-script communication or shared state.** Each script's Rhai
  scope is its own; nothing here lets one script call another's function or
  read its state.
- **No async, no coroutines, no true "pause mid-handler."** § Execution
  model's re-invocation-as-a-new-call design for `prompt` is the whole
  answer for v1 — it avoids needing Rhai execution to suspend across host
  events at all.
- **No richer prompt UI** (multi-field forms, choosers, a query-replace-style
  step-through) — one single-line `prompt` per § Prompting for input.
- **No workspace-search or multi-file API** — buffer/selection functions
  operate on the *active* buffer only; a script cannot iterate open tabs or
  read another file.

## Crate shape (implemented, T102)

- `engine.rs` — the Rhai `Engine` construction (resource limits, § Execution
  model) and every native function in § API v1; the types that cross the
  host boundary: `Command`, `KeyBinding`, `LoadedScript`, `LoadError`,
  `HostMessage`, `PromptRequest`, `HostState`, `InvokeOutcome`, and the
  `Runtime` struct itself (`Runtime::new`, `Runtime::load`,
  `Runtime::invoke`).
- `discovery.rs` — `discover(global_dir, project_dir) -> Vec<DiscoveredScript>`:
  non-recursive `.rhai` listing under each (both optional) with the
  project-wins stem-shadowing rule. Takes plain `&Path`s — it does not call
  `Settings::scripts_dir()` or know about `App::root` itself; resolving
  *which* directories those are is T103's job (host wiring), kept out of
  this crate so it stays host-agnostic.
- `lib.rs` — the public surface: re-exports plus `load_all(runtime,
  global_dir, project_dir) -> (Vec<LoadedScript>, Vec<LoadError>)`, which
  combines `discover` + reading each file + `Runtime::load`, skipping a
  script that fails to read or fails to load without aborting the rest.

**`Runtime`'s design**: a snapshot-in/effects-out model rather than a host
trait object or `unsafe` pointer games (`#![forbid(unsafe_code)]` is a hard
rule here too). `Runtime::invoke` takes an owned [`HostState`] built by the
caller from the real editor, seeds an `Rc<RefCell<HostState>>` the
registered native functions close over, runs the handler, and hands the
mutated `HostState` back — the host then applies whatever `*_written` flags
came back true, shows any `messages`, and opens a `prompt` if requested.
Nothing here holds a live reference into the real `App`; each `invoke` is a
self-contained value round trip. `register_command`/`bind_key` use a
separate `Rc<RefCell<Registry>>`, reset before each `Runtime::load`, since
they're load-time-only and unrelated to a running handler's state.

Every `vix-script` test drives this directly — construct a `Runtime`, `load`
an inline `.rhai` source string, `invoke` a handler with a hand-built
`HostState`, assert on the `InvokeOutcome` — no real `App`, no terminal, and
no files on disk beyond `discovery.rs`'s/`load_all`'s own filesystem tests
(a `std::env::temp_dir()`-rooted scratch directory, same pattern as
`vix-editorconfig`'s), the same terminal-independent-testing principle as
everywhere else in this repo (`spec/test/index.md`).
