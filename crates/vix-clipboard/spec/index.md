# Clipboard

Process-wide serialized access to the system clipboard, shared by every crate
that copies or pastes (`vix-editor-core`'s Cut/Copy/Paste actions, the Org
table rectangle clipboard, the DB result export, …).

Two guarantees:

- **One at a time.** Platform backends — notably macOS's Cocoa `NSPasteboard` —
  are not thread-safe, and concurrent `arboard` calls corrupt memory and crash
  the process. `set` and `get` hold one process-wide lock for the whole backend
  call, so all access is sequential. In the single-threaded app the lock is
  uncontended; under parallel tests it is what keeps the backend from being
  entered twice at once.
- **The platform clipboard is opt-in.** Until `use_system` is called, `set` and
  `get` read and write a thread-local in-memory clipboard (T596: so parallel tests cannot see each other's copies). `tui/src/main.rs` opts
  in once at startup, so the app behaves exactly as before; everything else —
  the test suite above all — copies and pastes in memory.

The opt-in exists because a test run used to overwrite the developer's real
clipboard: a VS Code keymap test cut the line `doomed` from a scratch buffer,
and that text landed on the macOS pasteboard, where the next paste in any app
produced it. Tests that need the platform clipboard must call `use_system`
themselves, and none do.

`is_system` reports which clipboard is in effect; `tui/tests/integration/editing.rs`
asserts it is `false` so the isolation cannot be lost by accident.

## Providers: system and OSC 52 (T625)

Over SSH there is no local platform clipboard to reach — `arboard` either
fails or copies onto the *remote* machine's pasteboard — so copying silently
went nowhere. The `clipboard_provider` setting picks how `set` and `get`
reach the clipboard once `use_system` is on:

| `clipboard_provider` | Writes | Reads |
| --- | --- | --- |
| `"auto"` (default) | OSC 52 when `SSH_TTY` or `SSH_CONNECTION` is set; otherwise `arboard`, falling back to OSC 52 if `arboard` errors | `arboard`; the internal register under SSH, or when `arboard` errors |
| `"system"` | `arboard` only; its error is returned | `arboard` only |
| `"osc52"` | OSC 52 always | the internal register |

Any other value behaves as `"auto"`.

An OSC 52 write is `ESC ] 52 ; c ; <base64 of the UTF-8 text> BEL`, written
to the terminal (stdout) — the terminal itself, wherever it runs, sets its
clipboard. Under tmux (`TMUX` set) the sequence is wrapped in tmux's DCS
passthrough, `ESC P tmux; <sequence with every ESC doubled> ESC \`, so tmux
forwards it to the outer terminal instead of swallowing it. (tmux must allow
it: `set -g allow-passthrough on`, or `set-clipboard on`.)

OSC 52 is write-only in practice — few terminals answer the query form, and
the ones that do ask the user first — so every OSC 52 write also stores the
text in the same thread-local internal register the in-memory clipboard uses,
and reads under OSC 52 return it. Copy then paste inside Vix therefore works
over SSH; pasting text copied *outside* Vix arrives through the terminal's
own bracketed paste instead.

The selection is pure so it can be tested without touching the process:
`select_backend(provider, &env)` maps a `Provider` and an `Environment` —
plain data, built from the real process by `Environment::current()` or from a
fake lookup by `Environment::from_lookup` — to a `Backend`, and
`encode_osc52(text, tmux)` builds the escape sequence. Tests never set
process environment variables (`unsafe` is forbidden, and tests run in
parallel). `configure(provider, env)` stores the choice process-wide;
`tui/src/main.rs` calls it once at startup, right after `use_system`, with
the loaded setting and `Environment::current()`. Without it the provider is
`auto` with an empty environment, i.e. plain `arboard` as before. In
in-memory mode (tests) neither `arboard` nor OSC 52 is ever reached, so a
test run cannot write escape sequences to the developer's terminal.

See `spec/index/index.md` for the project overview.
