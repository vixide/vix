# Input

Backend-neutral keyboard and mouse input types for Vix (key codes,
modifiers, key and mouse events), mirroring crossterm's shapes, with an
optional lossless conversion from crossterm.

Types: `KeyCode`, `KeyModifiers`, `KeyEventKind`, `KeyEventState`,
`KeyEvent`, `MouseButton`, `MouseEventKind`, `MouseEvent`.

**Status:** Shipped (GUI Phase 0). Every input-handling path in Vix — the
`App` shell (`App::on_key`/`App::on_mouse`), the keymap and every
`tui/src/app/*.rs` key handler, keyboard macros (`vix-macros`), modal-editing
recording and dot-repeat, the editor widget (`vix-editor-core`'s
`editor_input` module), the integrated terminal's key encoder
(`vix-terminal`), and every edit surface (`vix-edit-*`, `vix-column-view`,
`vix-db`) — takes these types. No crate other than the `vix-tui` package's
terminal event loop (`tui/src/main.rs`) and `vix-input` itself names crossterm's
event types.

## Why

Vix's editing engine never depended on the terminal, but its input types
did: `crossterm::event::KeyEvent` was threaded through ~2,500 call sites in
the `App` shell and a dozen member crates. That made the terminal backend a
structural dependency of everything that handles a key, and was the one
blocker to a second front end (the GPUI desktop GUI, see
[`gui/spec/index.md`](../../../gui/spec/index.md)).

## Design

- **Mirror crossterm's shapes exactly** — same type names, variants, field
  names, `new`/`new_with_kind`/`new_with_kind_and_state` constructors, and
  modifier/state flag names (`bitflags`). Moving a file from crossterm to
  `vix-input` is a one-line `use` change; nothing else in the call site
  moves. This was a deliberate choice over a "cleaner" new vocabulary,
  which would have rewritten thousands of lines for no behavioral gain.
- **Same equality semantics.** `KeyEvent`'s `PartialEq`/`Hash` normalize
  case the way crossterm's do (`Char('A')` ≡ `Char('a')` + `SHIFT`), so
  keymap lookups and macro comparisons behave identically.
- **Convert at the edge.** The `crossterm` feature adds lossless `From`
  impls from crossterm's key/mouse types; the terminal loop calls
  `.into()` once per event. Keys Vix never distinguishes (media keys, bare
  modifier presses) become `KeyCode::Null`. A GUI front end supplies its own
  conversion (from GPUI keystrokes) in its own crate.
- **No dependencies by default** beyond `bitflags`, so a non-terminal front
  end does not pull crossterm in through this crate.

## Tests

Unit tests pin the case-normalized equality/hash, the kind/state
participation in equality, the `KeyCode` helpers, and (with the `crossterm`
feature) a field-by-field crossterm conversion including the `Null` fallback.
