# vix-bui (desktop window on BUI)

`vix-bui` is the sixth desktop front end for Vix: the same editor, in a native
window drawn by [BUI](https://github.com/ppmpreetham/BUI), Blender's user
interface toolkit packaged as a library. Like the other desktop front ends
(`vix-gpui`, `vix-egui`, `vix-iced`, `vix-gtk4`, `vix-slint`) it is
**experimental**, written so the toolkits can be compared side by side.

## Run it

`vix-bui` is built from source and is not part of the release downloads. It
needs [CMake](https://cmake.org) and a C/C++ compiler (BUI's core is C), and
on Linux the X11 headers (`libx11-dev`).

```sh
cargo run --manifest-path gui/Cargo.toml --release -p vix-bui
cargo run --manifest-path gui/Cargo.toml --release -p vix-bui -- README.md
```

It takes the same command line as `vix`. Set `BUI_THEME=light` for BUI's light
theme and a light default palette; Vix's own themes (**View → Theme**) work as
usual.

## What you get

Everything in the terminal editor that fits a grid of characters: open, edit
and save files, tabs and splits, find and replace, the explorer, the command
palette (`Ctrl+P`), every menu (click, `F10`, `Alt` mnemonics), docks, pickers,
LSP, git, tasks, themes and keymaps. The window draws the unchanged editor
into a character grid, so nothing behaves differently from the terminal.

- **Keys:** typed text, named keys and `Ctrl`/`Alt`/`Cmd` chords. On macOS
  `Cmd` acts as `Ctrl` and `Option` as `Alt`.
- **Mouse:** click, drag, double-click and the wheel.
- **Clipboard:** the system clipboard through Vix's usual copy and paste keys.

## What is different

- **The font is BUI's, not a terminal font.** Text is drawn in BUI's embedded
  proportional font (Inter), each character in its own cell. Borders, scroll
  bars and separators are drawn as lines, so they join up. Icons from Nerd
  Fonts, CJK text and emoji show as boxes, and there are no separate bold or
  italic faces.
- **There is no native menu bar, file dialog or input-method (IME) support.**
  Use the in-window menus.
- **Quit with File → Quit (`Ctrl+Q`).** Closing the window with its close
  button ends the program immediately, without the unsaved-changes prompt:
  BUI cannot let an app veto it.
- **Move the pointer over the window before you type.** BUI delivers keys to
  the node under the pointer until something has been clicked.
- **Software drawing.** BUI has no GPU renderer on macOS.

The full list of limits, and what is and is not verified, is in the
[specification](../../gui/vix-bui/spec/index.md). Licensing: BUI is
GPL-2.0-or-later, so the `vix-bui` binary is distributed under the GPL-3.0
only (like `vix-slint`); see [`spec/license`](../../spec/license/index.md).

See also the [desktop GUI overview](../../gui/spec/index.md).

---

Vix™ and Vix IDE™ are trademarks.
