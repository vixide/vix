# vix-zgui (desktop window on zgui)

`vix-zgui` is the seventh desktop front end for Vix. It is the same editor, in
a native window drawn by [zgui](https://github.com/zortax/zgui), a Rust UI
framework with fine-grained reactivity, real CSS and a GPU renderer. Like the
other desktop front ends (`vix-gpui`, `vix-egui`, `vix-iced`, `vix-gtk4`,
`vix-slint`, `vix-bui`), it is **experimental**, written so the toolkits can
be compared side by side.

## Run it

`vix-zgui` is not part of the release downloads. It builds from source in
any of three ways:

```sh
brew install vixide/tap/vix-zgui     # Homebrew: compiles the crates.io crate
cargo install --locked vix-zgui      # crates.io
cargo run --manifest-path gui/Cargo.toml --release -p vix-zgui -- README.md   # a checkout
```

Then run `vix-zgui` (or `vix-zgui FILE…`). Compiling takes a few minutes: it
builds the whole editor and zgui's GPU stack. On Linux it needs the Wayland,
xkbcommon and fontconfig development packages.

It takes the same command line as `vix`. Set `VIX_GUI_THEME=light` for a light
default palette. Vix's own themes (**View → Theme**) work as usual.

## What you get

You get everything in the terminal editor that fits a grid of characters:

- open, edit and save files;
- tabs and splits, find and replace, the explorer;
- the command palette (`Ctrl+P`);
- every menu (click, `F10`, `Alt` mnemonics), docks and pickers;
- LSP, git, tasks, themes and keymaps.

The window draws the unchanged editor into a character grid, so nothing
behaves differently from the terminal.

- **Keys:** typed text, named keys, and `Ctrl`, `Alt` and `Cmd` chords. On
  macOS `Cmd` acts as `Ctrl` and `Option` as `Alt`.
- **Mouse:** click, drag, double-click and the wheel, including trackpad
  scrolling.
- **Clipboard:** the system clipboard, through Vix's usual copy and paste
  keys.
- **Closing the window** runs **File → Quit**, so unsaved changes are asked
  about first.

## What is different

- **Fonts.** Text is in Menlo, Consolas or DejaVu Sans Mono, depending on the
  platform. Set `VIX_GUI_FONT` to a family name to change it. Icons need a Nerd
  Font installed (Symbols Nerd Font is tried first). CJK and other scripts fall
  back to system fonts automatically.
- **Menus.** On macOS the native menu bar works alongside the in-window one,
  and it has **File → Open with System Dialog…**. On other platforms, use the
  in-window menus.
- **Input methods.** Committed text from an input method is typed, but the
  text being composed is not shown in the window.
- **Light or dark** follows `VIX_GUI_THEME`, not the desktop setting.

The full list of limits, and of what is and is not verified, is in the
[specification](../../gui/vix-zgui/spec/index.md). Licensing: zgui is
Apache-2.0, so `vix-zgui` has the same license choice as the editor; see
[`spec/license`](../../spec/license/index.md).

See also the [desktop GUI overview](../../gui/spec/index.md).

---

Vix™ and Vix IDE™ are trademarks.
