# vix-bui

The Vix desktop GUI on [BUI](https://github.com/ppmpreetham/BUI): the whole
Vix `App` — every panel, overlay, picker and dock — in a native window drawn
by Blender's user-interface toolkit, the sixth front end in `gui/` beside
`vix-gpui`, `vix-egui`, `vix-iced`, `vix-gtk4` and `vix-slint`.

**Status:** experimental, first version (T769). Builds and runs headless on
macOS; the native macOS window is BUI's own *experimental* Cocoa host and has
only been exercised through BUI's off-screen windows here (see
[Verified and unverified](#verified-and-unverified)). Not part of `dist`
releases: build it from source.

```sh
cargo run --manifest-path gui/Cargo.toml --release -p vix-bui [-- FILE…]
```

It takes the same command line as `vix` (`vix::cli::Cli`). Set
`BUI_THEME=light` for BUI's light theme and the light default palette.

## What BUI is

BUI ("Blender UI", crates `blenderui`, `blenderui-sys`, …, version 0.4.0
pinned exactly) turns Blender's widget drawing, interaction rules and look
into a standalone library: a small C core (the *kit*: node tree, layout,
input, a software raster, optional Direct3D 11 / OpenGL 3.3 GPU renderers,
FreeType text with an embedded Inter font, native Win32 / X11 / Cocoa window
hosts) with bindings for Python, JavaScript, C++, Dear ImGui and Rust. The
Rust crate `blenderui` presents a gpui-style API (entities, `Render`,
elements with Tailwind-ish styling, HTML elements, `canvas`) over the kit.
Upstream: <https://github.com/ppmpreetham/BUI>, active (commits the day this
was written), 0.x, on crates.io.

How vix-bui uses it — deliberately little:

- its **window and event loop** (`Application`, `open_window`, `UiSender`),
- its **input** (key, pointer and wheel events on one root element),
- its **`canvas` painter** (`Painter::fill`, `Painter::text`), into which the
  cell grid is painted.

It does *not* use Blender's widgets (sliders, dropdowns, panels, the file
browser): the Vix `App` already is the UI, and it draws itself into a
character grid. Only the `freetype` and `svg` BUI features are on
(`default-features = false`): no image or video codecs, no GPU renderer (macOS
has none), no file browser, so less C is compiled and linked.

### Licensing and supply chain

- BUI is `GPL-2.0-or-later` (derived from Blender's sources; its vendored
  FreeType is dual FTL/GPL, Inter is OFL-1.1, the rest MIT/Zlib/public
  domain). Its own `Cargo.toml` says GPL-2.0-or-later (its `LICENSE` file is
  the GPLv3 text; the README says v2-or-later).
- Linked with the Apache-2.0-only crates every Vix front end links (which the
  FSF holds incompatible with GPLv2), the binary is distributed under
  **GPL-3.0-only**, exactly like `vix-slint`. `Cargo.toml` and `LICENSE` say
  so, and `scripts/check-binary-licenses` (which now classifies
  `GPL-2.0-or-later`) recomputes and enforces it. See
  [`spec/license/index.md`](../../../spec/license/index.md).
- `cargo deny` runs on the root workspace only; `gui/` is a separate
  workspace and has never been in its scope (Slint's GPL would not pass its
  allow-list either). BUI adds no new advisory-relevant Rust dependency
  beyond `taffy`, `paste` and `smallvec`; it adds a C/C++ build (CMake plus a
  C11/C++20 compiler) whose `build.rs` was reviewed before it was first
  built: it only runs CMake on the vendored `csrc/`, with no downloads and no
  custom commands.
- Needs **CMake** and a C/C++ toolchain at build time; on Linux the X11
  headers (`libx11-dev`); libX11 itself is `dlopen`ed at run time.

## Architecture

Like every front end here, vix-bui is self-contained (no code shared with a
sibling; see [`gui/spec/index.md`](../../spec/index.md), "Independent
implementations") and consumes the shared crates the same way: the `App`
shell (`vix-tui` as `vix`), `vix-input`, `vix-i18n`. Everything the editor
does — the shared editor core (`vix-editor-core`, `vix-editor`), buffers,
save, themes, keymaps (`vix-keymap-model`), the explorer, search, git, LSP —
comes with the `App`; vix-bui adds none of it and re-implements none of it.

```
BUI window ── key / mouse / wheel events ──▶ keys, model ──▶ App::on_key / on_mouse
    ▲                                                          │
    │ canvas paint                                             ▼
view (replay Ops onto Painter) ◀── scene (grid → Ops) ◀── ui::draw → GridBackend
```

| Module | Role |
| --- | --- |
| `grid` | A ratatui `Backend` drawing into an in-memory cell grid. |
| `render` | Color resolution; per-row background runs and text spans. |
| `scene` | The grid as a flat list of rectangle and character `Op`s in logical pixels, in paint order — pure, so it is tested without a window. |
| `glyphs` | Box-drawing (U+2500–257F) and block (U+2580–259F) characters as rectangles. |
| `keys` | A BUI keystroke (keysym, typed text, Shift/Control/Alt) as a `vix-input` key event, through `key_rules`. |
| `key_rules` | The keystroke rules shared in shape with the other front ends (typed character vs. unmodified chord key, `Shift+Tab` as `BackTab`, …). |
| `model` | The `App`, its grid and palette, mouse state and the tick schedule; no BUI types, so tests and the screenshot run drive it directly. |
| `view` | The BUI side: one `canvas` filling the window, event handlers that forward to `model`, a timer thread posting `App::tick` jobs through `UiSender`. |
| `startup` | The terminal binary's startup sequence minus the terminal. |
| `screenshot` | (`screenshot` feature) the scripted render run. |

Per frame: the canvas's paint callback fits the grid to the canvas size
(cells of 8 × 18 logical pixels at a 14 px font), runs the unchanged
`vix::ui::draw` into it, builds the `Op` list and replays it on BUI's
`Painter`. Input handlers mutate the model and call `notify_all`; a 50 ms
timer thread posts a job that runs `App::tick` on the UI thread and redraws
at `App::poll_interval`.

### Text

BUI's text path draws strings in its one embedded proportional font (Inter);
the canvas `Painter` offers no family choice, no bold/italic face and no text
measurement, and BUI does no per-glyph font fallback. vix-bui therefore:

- places **every printable character at its own cell column**, so proportional
  glyphs cannot drift off the grid, with a small per-character nudge
  (`font_choice::nudge`) so narrow letters (`i`, `l`) and wide ones (`m`, `W`)
  read as centered in the cell;
- draws **box-drawing and block characters as filled rectangles**, since Inter
  has none (borders, scroll bars, separators and the minimap work);
- draws bold as the glyph drawn twice half a pixel apart, underline and
  strikethrough as one-pixel rules, reverse/dim by folding into colors, and
  the cursor as a two-pixel bar;
- draws any other non-ASCII symbol (accented Latin, Greek, Cyrillic: Inter has
  them) as text in its cell.

## What the first version does

Everything the terminal editor does that fits a character grid, in a native
window:

- **Open, edit and save files** with the shared editor core: file arguments,
  `--diff`, stdin (`-`), session restore, `Ctrl+O` / `Ctrl+S` / `Ctrl+W`, all
  tabs, splits, find and replace, LSP, git, tasks, docks.
- **Themes:** every Vix theme and `theme = "auto"` (the light/dark default
  follows `BUI_THEME`, since BUI cannot read the OS appearance).
- **Keybindings:** every keymap preset and custom binding, as typed
  characters, named keys and `Ctrl`/`Alt`/`Cmd` chords (`Cmd` arrives as
  `Ctrl` — BUI's Cocoa host folds it — which is what Vix does with it on
  macOS; `Alt`/Option is Meta, so `Alt+letter` mnemonics work).
- **File explorer** (left dock), command palette, menus (the in-grid bar:
  every menu, `F10`, `Alt` mnemonics), pickers, overlays — all of them,
  because the unchanged `ui::draw` draws them.
- **Mouse:** click, drag, double-click, wheel (whole lines), reported per cell
  like a terminal.
- **Clipboard:** `Ctrl/Cmd+X/C/V` reach Vix, which uses the real system
  clipboard.
- **Quit** through `File → Quit` / `Ctrl+Q` runs Vix's own flow (unsaved
  changes prompt, session save).

## Honest limits

- **The close box cannot be vetoed.** BUI 0.4 only reports a window as
  closed; there is no close-request event. Closing the window with its title
  bar button ends the program without the unsaved-changes prompt (Vix's
  untitled-buffer recovery still applies; modified file-backed buffers are
  not saved). Use `File → Quit`.
- **Keys need the pointer over the window (or a click).** BUI's kit routes a
  key to the focused node, else to the node under the pointer; Rust code
  cannot focus the kit's node. Until the pointer has entered the window or a
  mouse button was pressed in it, typing is dropped. (The scripted
  screenshot run moves the pointer first for this reason.)
- **No monospace font.** Letters are Inter, placed per cell; it is readable
  but looks unlike a terminal font. No Nerd Font icons: private-use icon
  glyphs show as missing-glyph boxes (BUI has no fallback); no CJK or emoji
  either. A future version could register a system font with
  `blenderui::add_font` and draw the grid as HTML elements with a
  `font-family`, which would also give real bold and italic.
- **No native menu bar, file dialog or IME.** BUI has none; the in-grid
  menus work, `File → Open with System Dialog…` does not exist here, and
  there is no preedit text (composed input arrives as typed characters if
  the OS delivers it as key text).
- **No GPU renderer on macOS** (BUI's own limit): drawing is BUI's software
  raster. Fine for a text grid; full-window redraws at high resolutions cost
  CPU.
- **Fractional-scale text** is BUI's. Box lines are snapped to whole logical
  pixels; at a fractional display scale they may blur slightly.
- **Window geometry** is fixed at startup (1200 × 800 logical pixels) and is
  not remembered; resizing re-fits the grid.
- **macOS only has been built here.** Linux (X11, via `dlopen`) and Windows
  (Win32) are what BUI calls "works"; macOS is "experimental". The source
  carries no platform conditionals, and CI builds all three on the weekly
  GUI run.
- **Not released:** no `dist` metadata, Homebrew formula or Linux packages,
  like `vix-gtk4`.

## Verified and unverified

Verified (macOS, this machine): the crate builds (BUI's C kit compiles through
CMake); pedantic clippy with `-D warnings` and `rustfmt` are clean; unit tests
cover the grid, color and span planning, the scene builder, box-drawing glyphs,
the key rules and the BUI-to-`vix-input` key mapping; end-to-end tests drive a
real BUI off-screen window with BUI's own input path (typing reaches the
buffer, `Ctrl+P` opens the palette and `Escape` closes it, clicks and wheel,
quit); the scripted `screenshot` feature renders a PNG of the real App.

Unverified: BUI's native Cocoa window (live typing, resizing, retina scale,
focus), Linux and Windows builds, and sustained use.

## Tests

`cargo test --manifest-path gui/Cargo.toml -p vix-bui`. The headless tests
(`src/headless.rs`) open BUI off-screen windows (BUI's software raster, no
display needed) around a real `App` and assert on the grid and on rendered
pixels. They never assert on translated text.

## Internationalization

vix-bui has no user-facing text of its own: the window title comes from
`startup::window_title` (`Vix — <workspace>`, the same as the other front
ends), and every string in the window is the `App`'s, already translated in
all 15 core locales through `vix-i18n`. The one diagnostic it prints
(`vix-bui: …` on stderr when startup fails) matches the other front ends'.
