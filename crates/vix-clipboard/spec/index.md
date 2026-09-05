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
  `get` read and write a process-local in-memory clipboard. `src/main.rs` opts
  in once at startup, so the app behaves exactly as before; everything else —
  the test suite above all — copies and pastes in memory.

The opt-in exists because a test run used to overwrite the developer's real
clipboard: a VS Code keymap test cut the line `doomed` from a scratch buffer,
and that text landed on the macOS pasteboard, where the next paste in any app
produced it. Tests that need the platform clipboard must call `use_system`
themselves, and none do.

`is_system` reports which clipboard is in effect; `tests/integration.rs` asserts
it is `false` so the isolation cannot be lost by accident.

See `spec/index/index.md` for the project overview.
