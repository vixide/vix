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
`BUI_THEME=light` for BUI's light theme and the light default palette, and
`VIX_GUI_FONT` to a family name to replace the platform's monospace font.

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
  cell grid is painted,
- its **HTML elements**, only for text in another face than the canvas's (see
  [Text](#text)).

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
| `scene` | The grid as a flat list of rectangle, canvas-text and styled-text `Op`s in logical pixels, in paint order, plus the cell geometry — pure, so it is tested without a window. |
| `font_choice` | The monospace family by name (`VIX_GUI_FONT`, else by platform), the fallback order, and the embedded font's cell metrics. |
| `fonts` | Finds those families (`fontique`), measures the cell and tests glyph coverage (`ttf-parser`), gives BUI the fonts. |
| `glyphs` | Box-drawing (U+2500–257F) and block (U+2580–259F) characters as rectangles. |
| `keys` | A BUI keystroke (keysym, typed text, Shift/Control/Alt) as a `vix-input` key event, through `key_rules`. |
| `key_rules` | The keystroke rules shared in shape with the other front ends (typed character vs. unmodified chord key, `Shift+Tab` as `BackTab`, …). |
| `menu_tree` | The native menu bar's contents from the `vix-menu` model; File gains **Open with System Dialog…**. |
| `menus` | (macOS) that tree as a `muda` bar, replacing BUI's default one; clicks are polled by the timer job, and the dialog is `rfd`. |
| `model` | The `App`, its grid and palette, mouse state and the tick schedule; no BUI types, so tests and the screenshot run drive it directly. |
| `view` | The BUI side: one `canvas` filling the window with an element above it per styled item, event handlers that forward to `model`, a timer thread posting `App::tick` jobs through `UiSender`. |
| `startup` | The terminal binary's startup sequence minus the terminal. |
| `screenshot` | (`screenshot` feature) the scripted render run. |

Per frame: `VixBui::render` fits the grid to the window (a 14 px font in cells
measured from it, see [Text](#text)), runs the unchanged `vix::ui::draw` into
it and builds the `Op` list; the canvas's paint callback replays that on BUI's
`Painter`, and styled items become elements beside the canvas. This happens in
`render`, not in the paint callback, because the elements must exist before
BUI's layout. Input handlers mutate the model and call `notify_all`; a 50 ms
timer thread posts a job that runs `App::tick` on the UI thread and redraws
at `App::poll_interval`.

### Text

What BUI 0.4.0 can and cannot do decides this design:

- `Painter::text` always draws in BUI's *default family*, with no family,
  weight or italic argument (the method that takes them is `pub(crate)`). Text
  in named families is only reachable through elements with `font-family`.
- `blenderui::add_font` registers bytes under a family name, and the empty
  name *is* the default family, checked before the embedded Inter. So one call
  makes the canvas draw any font.
- BUI opens a font's first face only: no collection index. Its file-name
  guessing for a family name is a short list and no `.ttc` styles.
- A run of text has exactly one face: no per-glyph fallback. Glyphs must be
  FreeType outlines: bitmap color fonts draw nothing.
- There is no public text measurement.

Measured on a full 1207 × 792 window of text in Menlo at scale 1 (142 × 44
cells, this machine, release build, whole frame with the pixel copy):

| Path | ms per frame |
| --- | --- |
| canvas, one item per character | 1.6 |
| canvas, 12-character spans | 1.6 |
| canvas, one item per row | 1.2 |
| elements, 12 spans per row (528 elements) | 2.6 |
| elements, one per character | 17–19 |

So vix-bui does **not** draw the grid as elements. It finds the family by name
through `fontique` (like `vix-egui`): `VIX_GUI_FONT`, else Menlo, Consolas or
DejaVu Sans Mono by platform. Then:

- the **regular face replaces BUI's default family**, so ordinary text stays
  on the canvas, **one item per character at its own cell**. The cell is the
  font's advance (from `ttf-parser`) rounded to whole device pixels, like a
  terminal's, so a run drawn as one string would drift off the grid; BUI's
  glyph cache makes per-character items as cheap as spans.
- **bold and italic spans** are elements in the family's real faces. A `.ttc`
  face other than the first (Menlo's bold) becomes the first by overwriting
  one entry of a copy's offset table (`fonts::standalone`); a style the family
  lacks is synthesized by BUI. Built-in themes use neither, so usually there
  are none. BUI places a span's characters by device-rounded widths once
  `letter-spacing` is non-zero, which is why such an element has a tiny one:
  it keeps the span on the grid.
- **fallback is done here, per character** (`Fonts::route`, cached): Nerd Fonts
  first for Vix's icons, then a per-platform list of broad system families
  (`font_choice::fallbacks`), each opened when first needed and given to BUI
  when first chosen. A character found only in a fallback is one element. A
  family counts only if the glyph is an outline.
- **box-drawing and block characters are filled rectangles**, so borders,
  scroll bars and the minimap join up whatever the font.
- underline and strikethrough are one-pixel rules, reverse/dim fold into
  colors, and the cursor is a two-pixel bar.

Element text and canvas text share one baseline: BUI places a canvas item's
baseline 0.72 em below its top and an element line's from fixed Inter metrics,
whatever the font, so `scene::Metrics` works out one drop for each from the
font's ascent and descent. Those constants are BUI 0.4.0 internals, safe
because the version is pinned exactly.

**No font found** (not installed, unreadable, no metrics): the first version's
path remains. Inter on the canvas in 8 × 18 cells, each character nudged
(`font_choice::nudge`) so narrow and wide letters read as centered, bold as the
glyph drawn twice half a pixel apart, no italic, no fallback.

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
- **Native menu bar** on macOS, built from the same `vix-menu` model; every
  leaf runs its action id through `App::run_action`. It replaces the small
  default bar BUI's Cocoa host installs, whose `Cmd+Q` and `Cmd+W` key
  equivalents would otherwise never reach Vix. File gains
  **Open with System Dialog…** (`menu.item.file.open_native`, through `rfd`).
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
- **No color emoji, no shaping.** BUI draws outlines only and shapes nothing:
  emoji with no outline font show the missing-glyph box (always on stock
  macOS; Apple Color Emoji is bitmaps), and scripts that need shaping or bidi
  (Arabic, Devanagari, Thai marks) draw as separate glyphs, one per cell, like
  in a basic terminal. Without a Nerd Font installed, icons are boxes too.
- **Fallback is a fixed list**, not the OS font fallback: a character in none
  of `font_choice::fallbacks`'s installed families shows the box.
- **Native menu bar and file dialog on macOS only**, like `vix-egui` and
  `vix-iced`: `muda`'s Windows setup is an `unsafe fn`, forbidden here, and on
  Linux it needs a GTK loop. `File → Open with System Dialog…` lives on that bar
  (the in-grid menus are the shared model, which has no GUI-only items), so
  Windows and Linux have neither; `Ctrl+O` and the in-grid menus work there. A
  menu click waits for the next tick, up to 50 ms. Labels are translated at
  startup.
- **No IME.** BUI has none: there is no preedit text (composed input arrives
  as typed characters if the OS delivers it as key text).
- **No GPU renderer on macOS** (BUI's own limit): drawing is BUI's software
  raster. Fine for a text grid; full-window redraws at high resolutions cost
  CPU.
- **Fractional-scale text** is BUI's. Box lines are snapped to whole logical
  pixels; at a fractional display scale they may blur slightly.
- **Window geometry** is fixed at startup (1200 × 800 logical pixels) and is
  not remembered; resizing re-fits the grid. No sibling remembers it either,
  and BUI 0.4 has no window position to read or set (`Window::bounds` has a
  fixed origin; `WindowOptions`' origin is ignored).
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
the key rules and the BUI-to-`vix-input` key mapping, cell geometry, font
collection handling and fallback order; end-to-end tests drive a
real BUI off-screen window with BUI's own input path (typing reaches the
buffer, `Ctrl+P` opens the palette and `Escape` closes it, clicks and wheel,
quit, input over a window using a real font); the scripted `screenshot` feature
renders a PNG of the real App. PNGs looked at by eye show Menlo on the grid,
Nerd Font icons, Japanese, Chinese and Korean through the fallback list, and
real bold and italic Menlo on the same baseline (bold and italic through a
temporary patch of the scene, since no built-in theme uses them).

The menu tree (every model leaf, in order, plus the dialog item) and what the
dialog does with its answer are tested without a window; `menus` and `rfd`
compile on macOS but need a live app.

Unverified: BUI's native Cocoa window (live typing, resizing, retina scale,
focus), so the device-pixel snapping of the cell at scale 2 has only run in
unit tests; the native menu bar and the file dialog, neither of which has been
seen on screen (whether `muda` clicks arrive under BUI's own event loop, and
whether `rfd`'s modal panel works from a timer job); Linux and Windows builds, so their font lists are untested
(Consolas, DejaVu Sans Mono, the fallback families); frame times at retina
resolution; and sustained use.

## Tests

`cargo test --manifest-path gui/Cargo.toml -p vix-bui`. The headless tests
(`src/headless.rs`) open BUI off-screen windows (BUI's software raster, no
display needed) around a real `App` and assert on the grid and on rendered
pixels. They never assert on translated text.

## Internationalization

vix-bui has almost no user-facing text of its own: the window title comes from
`startup::window_title` (`Vix — <workspace>`, the same as the other front
ends), the native menu is the `vix-menu` model's labels plus the existing
`menu.item.file.open_native` key, and every string in the window is the
`App`'s, already translated in all 15 core locales through `vix-i18n`. The one diagnostic it prints
(`vix-bui: …` on stderr when startup fails) matches the other front ends'.
