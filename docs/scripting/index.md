# Scripting

Vix ships every command compiled in — there's no plugin marketplace. Rhai
scripting is the escape hatch: drop a small [Rhai](https://rhai.rs/) `.rhai`
file on disk and get a new command palette entry, a new key binding, or a
one-off buffer transform, without a rebuild.

## Where scripts live

| Scope | Path | Notes |
| ----- | ---- | ----- |
| Global | `<config dir>/scripts/*.rhai` | Applies to every workspace you open. |
| Project | `<workspace root>/.vix/scripts/*.rhai` | Just this project. |

Both are optional and **not** searched recursively. A project script shadows
a global script with the same file stem entirely — the global one simply
doesn't load, the same way a project's `.editorconfig` or `.vix/project.toml`
wins over a global default.

Scripts load once at startup. **Tools → Scripts → Reload** (or the
`script.reload` action) re-scans both directories without restarting: every
registered command and key binding is dropped and rediscovered from scratch,
so a script you deleted or renamed doesn't leave anything stale behind.

A script that fails to parse, or whose top level raises an error, is skipped
— its file name and the error go to the message drawer, and every *other*
script still loads normally. One broken script never takes scripting down.

## Writing a script

A script's top level runs once, at load time, and should do nothing but
register itself:

```rhai
register_command("uppercase_selection", "Uppercase Selection", "on_uppercase");

fn on_uppercase() {
    set_selection_text(selection_text().to_upper());
}
```

- `register_command(id, label, handler)` adds an entry to **Tools → Scripts
  → Run…** and the command palette (search for the label). `id` only needs
  to be unique within this one script; `handler` names a `fn` in the same
  file, called with no arguments when the command runs.
- `bind_key(key_token, command_id)` binds a key to a command this script
  already registered. `key_token` uses the same grammar as saved macros —
  `C-`/`A-`/`S-` prefixes plus the key, e.g. `C-c`, `S-Tab`, `C-S-l`, `a`. A
  malformed token (typo'd modifier, unknown key name) is a load error, not a
  binding that silently never fires.

Handler functions run later, once per invocation — a palette pick, a bound
key — each a fresh call into the script. There's no `await` and no way for a
handler to pause mid-run; the one exception is `prompt` (below), which
resumes as a **new**, separate call rather than continuing the one that
asked for input.

### Reading and writing the buffer

| Function | Effect |
| -------- | ------ |
| `buffer_text() -> String` | whole active-buffer text |
| `set_buffer_text(text)` | replace the whole buffer, one undo step |
| `selection_text() -> String` | selected text, `""` if there is no selection |
| `set_selection_text(text)` | replace the selection; with no selection, insert at the cursor |
| `current_line() -> String` | the line the cursor is on (read-only — there's no `set_current_line`) |
| `cursor_offset() -> int` | cursor position, as a character offset |
| `set_cursor_offset(n)` | move the cursor to character offset `n`, clamped in range |

Positions are **character offsets**, never bytes and never `(line, col)`.
These primitives are the whole surface for v1: a script reads text,
transforms it with plain Rhai, and writes the result back — there's no
separate "run this built-in transform" API.

Both setters are blocked on a read-only buffer (reported in the message
drawer); `set_cursor_offset` alone still works even then, since moving the
cursor isn't an edit.

**Write once, not incrementally.** A handler that raises a runtime error
partway through is not rolled back — whatever it already wrote stays
written. Compute the whole new text in a local variable first, then call
`set_buffer_text`/`set_selection_text` once at the end, the way every sample
below does, rather than mutating the buffer across several calls.

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

`prompt(message, on_submit)` opens a single-line input box showing `message`.
Enter calls `on_submit` with the typed text as its one argument — a **new**
call, not a resumed `on_rename`. Whatever `on_rename` needs afterward has to
live in the buffer/selection itself or be re-read fresh (`selection_text()`
again, for instance) — Rhai doesn't carry local variables across the two
calls. Esc cancels; `on_submit` is never called.

There's no richer prompt shape in v1 — one line, no chooser, no multi-field
form.

### Messages

- `message(text)` — informational, non-blocking, goes to the message
  drawer.
- `error(text)` — same drawer, styled as an error. Raising it does **not**
  stop the script; it's just a message, not an exception the host catches.

### The clock

- `now() -> String` — today's local date, `YYYY-MM-DD`. That's the only
  time-related function — no time-of-day, no time zone.

## What scripts can't do

- **No file or network access, no subprocess.** Rhai's standard library has
  none of these; nothing here registers them either. A script's *only*
  capabilities are exactly the functions on this page.
- **No other buffers.** Every function above operates on the *active*
  buffer. A script can't list open tabs, open a new one, or read a file that
  isn't the one you're looking at.
- **No script-to-script calls.** Each script's Rhai state is its own.

## Errors never crash the editor

A script is untrusted input, the same as a `.vcf` file you open — Vix never
lets one panic:

- **At load** (startup, or a reload): a parse error or a top-level error is
  reported, that one script is skipped, every other script still loads.
- **At invocation**: a runtime error — a type mismatch, an unhandled
  exception, hitting a resource limit (Rhai caps operations, recursion
  depth, string/array size per call, generous enough that no reasonable
  script should ever notice) — aborts just that one call and is reported.
  Whatever the handler already wrote before the error stays written (see
  "Write once, not incrementally" above).

## Key bindings and conflicts

`bind_key` requests are resolved the same way a hand-edited
`keybindings.toml` override is (see [Keybindings](../keybindings/index.md)):
against the active keymap's built-ins, and against every other script's and
the user's own requests. Two things claiming the same key — two scripts, or
a script and `keybindings.toml` — are **both rejected**, reported once,
naming every source; a script that simply claims a key a built-in binding
already used still wins outright, but is reported once so you're not
surprised later when that built-in stops firing.

## Sample scripts

Six working examples live in `examples/scripts/` (linked individually
below) — copy one into `.vix/scripts/` (or `<config dir>/scripts/`) and
reload to try it. `tests/example_scripts.rs` loads and runs every one of
them, so they stay correct as the API evolves.

- [`wrap-selection-in-markdown-link.rhai`](../../examples/scripts/wrap-selection-in-markdown-link.rhai)
  — select some text, run the command (or `Ctrl+Shift+L`), type a URL,
  and the selection becomes a Markdown link wrapping the original text.
  Shows `register_command` + `bind_key` together, and `prompt`/`on_submit`.
- [`insert-file-header.rhai`](../../examples/scripts/insert-file-header.rhai)
  — prompts for a one-line description and prepends it as a `//` comment.
  Shows building the whole new buffer text before one `set_buffer_text` call.
- [`title-case-line.rhai`](../../examples/scripts/title-case-line.rhai) —
  title-cases the line the cursor is on ("the quick fox" → "The Quick Fox").
  There's no "replace the current line" primitive, so this finds the line's
  boundaries in `buffer_text()` using `cursor_offset()` and rewrites the
  whole buffer — the most involved sample, and the clearest example of
  "compute everything, write once."
- [`dedupe-selection.rhai`](../../examples/scripts/dedupe-selection.rhai) —
  removes duplicate lines from the selection, keeping the first occurrence
  of each. Plain Rhai arrays and object maps, nothing buffer-specific.
- [`timestamp-signature.rhai`](../../examples/scripts/timestamp-signature.rhai)
  — inserts `-- 2026-09-04`-style signature at the cursor. Shows `now()`.
- [`open-scratch-with-template.rhai`](../../examples/scripts/open-scratch-with-template.rhai)
  — fills an empty active buffer with a dated scratch-note heading. Open a
  fresh buffer first (`Ctrl+N`) — scripts can't open one themselves (see
  "What scripts can't do" above) — then run the command. Shows a simple
  guard against overwriting existing content.

---

Vix™ and Vix IDE™ are trademarks.
