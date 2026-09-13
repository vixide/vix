# Clipboard

`vix-clipboard` is internal plumbing, not a feature with its own menu: it is
the process-wide, serialized access point to the system clipboard that every
other crate copies and pastes through — `vix-editor-core`'s Cut/Copy/Paste
actions, the Org table rectangle clipboard, the DB result exporter, and so
on. There is no separate clipboard UI backed by this crate; the user-facing
surface is the ordinary **Edit → Cut / Copy / Paste** menu items.

| Menu item | Action ID    | Keybinding |
| --------- | ------------ | ---------- |
| Cut       | `edit.cut`   | Ctrl+X     |
| Copy      | `edit.copy`  | Ctrl+C     |
| Paste     | `edit.paste` | Ctrl+V     |

## Two guarantees

- **One at a time.** Platform clipboard backends — notably macOS's Cocoa
  `NSPasteboard` — are not thread-safe, and concurrent calls into the
  underlying `arboard` library can corrupt memory and crash the process.
  `set` and `get` hold one process-wide lock for the whole backend call, so
  all access is sequential. In the (single-threaded) app the lock is
  uncontended; under parallel tests it is what keeps the backend from being
  entered twice at once.
- **The platform clipboard is opt-in.** Until `use_system` is called, `set`
  and `get` read and write a process-local in-memory clipboard instead of the
  real one. `src/main.rs` opts in once at startup, so the running app behaves
  exactly as you'd expect; everything else — the test suite above all — cuts
  and pastes in memory only.

The opt-in exists because of a real incident: a test run used to overwrite
the developer's actual system clipboard. A VS Code keymap test cut the line
`doomed` from a scratch buffer, and that text landed on the macOS pasteboard,
where the next paste in any other app produced `doomed`. Tests that need the
real platform clipboard must call `use_system` themselves, and none do —
`tests/integration/editing.rs` asserts `is_system()` is `false` so the
isolation can't quietly regress.

## Clipboard history

Layered on top of the plain clipboard (but implemented separately, at the app
level rather than in `vix-clipboard`) is a clipboard **history**: every copy
and cut is additionally recorded into a ring of up to 30 recent entries,
most-recent first and de-duplicated. **Edit → Paste from History…**
(`edit.paste_from_history`) opens a picker over that ring; choosing an entry
pastes it at the cursor and re-promotes it to the front. See
`crates/vix-editor/spec/clipboard-history/index.md` for that feature's own
spec.

See the crate spec at `crates/vix-clipboard/spec/index.md` for the locking
and opt-in details.

---

Vix™ and Vix IDE™ are trademarks.
