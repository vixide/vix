# Command Key (macOS)

On macOS the `Command` modifier drives Vix's `Control` bindings: `Cmd+C` does
what `Ctrl+C` does, `Cmd+S` saves, `Cmd+Shift+Z` redoes. `App::on_key` folds the
modifier before any dispatch — `Command` is removed and `Control` added, with the
rest of the chord (`Shift`, `Alt`) left in place — so every keymap, menu
mnemonic, and chord prefix sees the chord it already knows. There is no second
binding table to keep in sync.

Off macOS the key is passed through untouched: `Super` there belongs to the
window manager, not to the editor.

## What the terminal has to do

A terminal can only report `Command` when the **kitty keyboard protocol** is
enabled — crossterm sets `KeyModifiers::SUPER` only under
`KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES`. `src/main.rs` pushes that
flag at startup on macOS (popping it on exit and around suspend), alongside
mouse capture and bracketed paste. A terminal without support ignores the
request and keeps its legacy encoding; nothing else changes.

Even then, the terminal decides which `Cmd` shortcuts it forwards, and most
reserve the common ones for their own menus — `Cmd+C` copies to the system
clipboard, `Cmd+V` pastes, `Cmd+N` opens a window. Freeing a shortcut is a
terminal setting (kitty and Ghostty can unbind a `super+…` key; iTerm2 can remap
one to be sent to the app). Whatever the terminal does forward, Vix treats as
`Control`.

`App::command_as_control` is the fold; it is unit-tested on every platform, and
`tests/integration.rs` drives `Cmd+F`, `Cmd+Z`, and `Cmd+Shift+Z` through
`on_key` on macOS.

See `crates/vix-keymap-model/spec/index.md` for the keymaps themselves and
`crates/vix-editor/spec/bracketed-paste/index.md` for the other terminal-level
input Vix asks for.
