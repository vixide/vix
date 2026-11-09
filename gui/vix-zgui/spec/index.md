# vix-zgui

The Vix desktop GUI on [zgui](https://github.com/zortax/zgui): the whole Vix
`App` — every panel, overlay, picker and dock — in a native window drawn by
zgui's retained wgpu renderer. It is the seventh front end in `gui/`, beside
`vix-gpui`, `vix-egui`, `vix-iced`, `vix-gtk4`, `vix-slint` and `vix-bui`.

**Status:** experimental, first version (T791). It builds and passes its tests
on macOS, and its window opens and runs there. Its pixels have not been
checked yet, and no screenshot smoke test exists (see
[Verified and unverified](#verified-and-unverified)). It is not part of `dist`
releases. Unlike its siblings it is on crates.io, and the Homebrew tap has a
`vix-zgui` formula that compiles that crate:

```sh
cargo run --manifest-path gui/Cargo.toml --release -p vix-zgui [-- FILE…]
cargo install --locked vix-zgui
brew install vixide/tap/vix-zgui
```

It takes the same command line as `vix` (`vix::cli::Cli`). Set
`VIX_GUI_THEME=light` for the light default palette. Set `VIX_GUI_FONT` to a
family name to replace the platform's monospace font.

## What zgui is

zgui (crate `zgui`, version 0.1.0, pinned exactly) is a native Rust UI
framework. It combines:

- Leptos-style fine-grained reactivity (`reactive_graph` signals; components
  run once);
- real CSS through Servo's Stylo (selectors, the cascade, flexbox, grid);
- an incremental layout engine;
- parley/fontique text;
- a retained GPU renderer on wgpu and vello.

Windows are native: winit on macOS, Windows and X11, plus its own Wayland
backend. There is no webview, HTML or JavaScript.

zgui 0.1.0 was published to crates.io on 2026-10-15 and is Apache-2.0. It is
0.x and changes often, so the pin is exact. Upstream:
<https://github.com/zortax/zgui>.

vix-zgui uses little of it:

- **The application and its window:** `zgui::app()`, `with_menu`,
  `on_close_request`, `set_interval`, `use_windows().quit()`.
- **One custom element** (`zgui::custom`). It is zgui's middle ground between
  composing elements and bringing a whole renderer. The element measures
  itself, keeps its own state, and paints through `ScenePainter`, on the
  engine's own quad and glyph pipelines. The cell grid is that element.
- **The window's font system:** `Fonts::shaper` and `Shaper::shape_line`.
  `LineRequest` has `ligatures: false` for fixed cells, and
  `Shaper::line_metrics` gives the cell advance.
- **Listeners on that element** for keys, IME commits, the pointer and the
  wheel, and a window shortcut so keys reach it before anything has focus.

It does not use zgui's components (`zgui-ui`), its `text` or `editor`
elements, or its styling beyond a two-rule style sheet. The Vix `App` already
is the UI, and it draws itself into a character grid.

### Licensing and supply chain

- zgui and its crates are Apache-2.0. Its graph adds Servo's Stylo, which is
  MPL-2.0. MPL-2.0 is file-level copyleft and allows every one of Vix's binary
  licenses.
- `scripts/check-binary-licenses` computes `Apache-2.0 OR BSD-3-Clause OR MIT
  OR GPL-3.0-only` for the binary, the same as `vix-egui`, so `Cargo.toml`
  keeps the workspace license. See
  [`spec/license/index.md`](../../../spec/license/index.md).
- `cargo deny` runs on the root workspace only, and `gui/` is outside its
  scope, as for every front end.
- The dependency graph is large: about 320 crates for a bare zgui app. That
  includes Stylo, wgpu 29 and vello 0.9. A release build of a bare app takes
  about 45 s on a recent Mac. No C toolchain is needed on macOS or Windows. On
  Linux the Wayland backend links libwayland and xkbcommon through pkg-config,
  as `vix-gpui` and the winit front ends already need.

## Architecture

Like every front end here, vix-zgui is self-contained: it shares no code with
a sibling (see [`gui/spec/index.md`](../../spec/index.md), "Independent
implementations"). It consumes the shared crates the same way: the `App` shell
(`vix-tui` as `vix`), `vix-input` and `vix-i18n`. Everything the editor does
stays in the shared crates.

| Module | Role |
| --- | --- |
| `main` | parse the CLI, `startup`, then `view::run` |
| `startup` | the terminal binary's startup minus the terminal (copied verbatim, like every front end's) |
| `grid` | the ratatui `Backend` that draws into an in-memory cell grid (verbatim copy) |
| `render` | toolkit-independent span planning: colors, background runs, styled text spans (verbatim copy) |
| `key_rules` | GUI keystroke → `vix-input` rules, matching crossterm (verbatim copy) |
| `keys` | zgui `Key` / `Modifiers` → the rules' names and flags |
| `font_choice` | the font size, line height, primary family and Nerd Font fallbacks |
| `model` | the `App`, the grid, cell geometry and input → `App` calls; no zgui in it |
| `paint` | `GridElement`, the zgui custom element: fit, render, paint |
| `view` | the window: the element, its listeners, the tick timer, close box, menu clicks |
| `menu_tree`, `menus` | the native macOS menu bar from the `vix-menu` model (`menu_tree` is a verbatim copy) |
| `headless` (tests) | the model driven through `keys::key_event` with zgui key values |

### One frame

1. Input or the timer mutates the shared `Model` (`Rc<RefCell<Model>>`, all
   on the UI thread). The handler then either quits or calls
   `CustomHandle::repaint` on the grid element.
2. zgui runs layout. `GridElement::layout` answers with the whole box CSS gave
   it (the style sheet makes it fill the window), in device pixels.
3. `GridElement::paint` measures the cell at the painter's scale and fits the
   grid to the box. It renders the `App` with the unchanged `vix::ui::draw`
   through a `ratatui::Terminal<GridBackend>`, then paints the grid:
   - one fill of the default background;
   - a quad per background run, on whole-pixel cell edges so neighbors meet
     without seams;
   - a glyph run per text span, then underline and strike-through quads;
   - a 2-pixel cursor bar.

zgui replays the element's recorded primitives on frames where nothing asked
it to repaint, so an idle window costs nothing. Any change asks: an event, a
tick that `App::poll_interval` says should redraw, or a resize.

### Text

Each span from `render::row_text` is shaped with the window's own `Shaper`, so
the face handles in the result are ones the renderer draws:

- The family list is the platform monospace face (Menlo, Consolas or DejaVu
  Sans Mono, or `VIX_GUI_FONT`), then "Symbols Nerd Font Mono" and "Symbols
  Nerd Font", then `monospace`. Families that are not installed are skipped.
  Beyond the list, parley's per-character fallback supplies CJK, symbols and
  color emoji (unverified on a live window).
- The weight is 700 for `BOLD` and the face is italic for `ITALIC`. These are
  real faces when the family has them; otherwise zgui synthesizes them.
- Ligatures are off, because a ligature draws two characters as one glyph in
  one cell.
- Every span starts at its exact column, and `render` gives each non-ASCII
  cell a span of its own. A fallback glyph with a different advance therefore
  cannot shift its row.
- Each run's origin is set so that its first glyph's baseline lands on the
  cell's baseline. A fallback face with a taller ascent sits on the same line.
  The baseline centers the face's ascent plus descent in the cell height, which
  is 14 px × 1.3, rounded.

Shaped spans are cached by (text, bold, italic). The cache is cleared when the
scale changes and when it passes 8192 entries. Most of a frame repeats the
last one, so most spans are cache hits.

### Input

- **Keys:** `key_down` on the element, which is also a window shortcut, so it
  hears keys before anything has focus. `keys::key_event` takes the key name
  from `key_without_modifiers` and the typed character from `key`, then
  applies `key_rules`:
  - Plain typing yields the layout's character.
  - A chord (Ctrl, Alt, Cmd) yields the unmodified key, so Option+S is `Alt+s`
    and not `ß`.
  - Meta becomes `SUPER`, which Vix folds into Ctrl on macOS.

  zgui 0.1 emits no separate text event for typing, so one press is one Vix
  key, and nothing is filtered twice. A handled key calls `prevent_default`,
  so Tab is Vix's and never moves zgui's focus.
- **IME:** `ime_commit` types the committed text as keys. Preedit text is not
  drawn.
- **Pointer:**
  - The pointer is captured on a button press, so a drag keeps reporting
    outside the window.
  - Motion is reported per cell, as a terminal reports it.
  - Wheel lines are passed on as they come, and trackpad pixels at 20 CSS px
    per line. Both accumulate fractions.
  - zgui's positive delta moves the content down, which is a Vix `ScrollDown`.
- **Close box:** `on_close_request` runs **File → Quit**. With unsaved
  changes, the window vetoes the close (`CloseResponse::Veto`) and Vix's prompt
  shows. When Vix later quits, the window closes through
  `use_windows().quit()`.
- **Menus (macOS):** `App::with_menu` builds the bar from `menu_tree`. zgui
  installs it with muda, so vix-zgui links no muda of its own. Clicks come back
  through `zgui::desktop::on_menu`. **File → Open with System Dialog…** opens
  `rfd`'s panel.
- **Clipboard:** the copy and paste keys reach Vix as plain keys, and Vix
  reads the system clipboard itself, as in every build.

`App::on_exit` (the session save) runs once, on the first of: Vix quitting,
the close box closing the window, or the event loop ending.

## Verified and unverified

**Verified (2026-11-09, macOS, arm64):**

- `cargo build` in debug and release.
- Pedantic `clippy --all-targets --all-features -D warnings`.
- `cargo test -p vix-zgui`: 26 tests. They cover:
  - the key mapping (typing, chords, named and function keys, dropped keys,
    Meta);
  - the model driven through it: typing reaches the buffer; `Ctrl+P` opens the
    palette and `Escape` closes it; clicks, drags and the wheel; cell lookup
    at scale 2; IME commits; files from the dialog; quit;
  - the verbatim modules' own tests.
- The license check (`scripts/check-binary-licenses`).
- The release binary opens its window and runs with nothing logged.

**Unverified:**

- What the window draws. Screen capture was not available where this was
  built, so text placement, the baseline, colors, the cursor and the Nerd Font
  fallback have not been seen.
- The native menu bar and the system dialog.
- The close-box veto and IME on a live window.
- Linux (Wayland and X11) and Windows builds. The `gui.yml` CI matrix builds
  and tests every platform.

There is **no screenshot smoke test**. zgui 0.1.0's published crates expose no
off-screen window or pixel readback, and its headless platform
(`zgui-platform-headless`) is not on crates.io. So `gui.yml`'s macOS render
loop leaves vix-zgui out, and there is no `screenshot` feature.

## Limits (zgui 0.1)

- No inline IME preedit. Commits only, and only if zgui enables the input
  method for a focused element that is not a zgui text field (unverified).
- Light or dark comes from `VIX_GUI_THEME`, not from the desktop. zgui reports
  the desktop's scheme only once a window is open, after the `App` and its
  theme must exist.
- Window size and position are not remembered, as in every sibling.
- No prebuilt binaries: `dist` does not build it (no `[package.metadata.dist]`).
  crates.io and the Homebrew formula both compile from source.

## Distribution

- **crates.io:** `vix-zgui` is the one GUI crate with `publish = true`. Its
  workspace dependencies (`vix-tui`, `vix-input`, `vix-i18n`) carry
  `version = "2.0.2"` beside their paths, so the published crate builds
  against the published editor. Publish it after those crates, at the same
  version.
- **Homebrew:** `Formula/vix-zgui.rb` in `vixide/homebrew-tap` is
  hand-written, not generated by `dist`. It downloads the crate from
  `static.crates.io`, depends on `rust` to build (on Linux also `pkgconf`,
  `fontconfig`, `libxkbcommon` and `wayland`) and runs `cargo install
  --locked`. Its test runs `vix-zgui --version`, which `main` answers
  before loading settings or opening a window (`vix-zgui 2.0.2`). Bump its `url` and `sha256`
  with each publish.

## Tests

`cargo test --manifest-path gui/Cargo.toml -p vix-zgui`. The `headless`
module drives `Model` with zgui `Key` values through `keys::key_event`, the
same function the window's listener calls, and asserts on the grid's text.
Never translated strings are asserted. No window opens.
