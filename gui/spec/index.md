# Desktop GUIs (`vix-gpui`, `vix-egui`)

Native desktop windows for Vix on macOS, Windows and Linux, built twice — on
[GPUI](https://github.com/longbridge/gpui-kit) (via Longbridge's GPUI Kit)
and on [egui](https://github.com/emilk/egui) (via eframe) — over one shared,
toolkit-agnostic core, so the two can be tried and compared side by side
before committing to one.

**Status:** experimental. GUI Phase 0 (T640) and Phase 1 (T641, T642)
shipped; Phase 2 (native components) not started, and waits on choosing a
toolkit.

```sh
cargo run --manifest-path gui/Cargo.toml --release -p vix-gpui [-- FILE…]
cargo run --manifest-path gui/Cargo.toml --release -p vix-egui [-- FILE…]
```

Both take the same command line as `vix` (`vix::cli::Cli`).

## Layout

`gui/` is one Cargo workspace (separate from the repo root, like `fuzz/`)
with three crates:

| Crate            | Contents |
| ---------------- | -------- |
| `vix-gui-common` | Everything that does not depend on the toolkit: the ratatui cell-grid `Backend` (`grid`), color resolution and per-row background runs / text spans (`render`), the keystroke rules (`keys`), the native menu tree from `vix-menu` (`menu`), font-family choice (`fonts`), and startup plus the scripted screenshot run (`startup`). |
| `vix-gpui`       | The GPUI front end: a GPUI view holding the `App`, painting via GPUI's text system, GPUI keystrokes → shared key rules, native menus and file dialog through GPUI itself. GPUI Kit pinned `=0.8.0`. |
| `vix-egui`       | The egui front end: an `eframe::App`, painting via egui's painter, egui events → shared key rules, fonts loaded by family name through `fontique`, native menus (macOS) through `muda`, file dialog through `rfd`. eframe pinned `=0.37.0`. |

Each front end is only the toolkit glue (window, painting, event wiring);
the comparison is between toolkits, not between two implementations of Vix.

## Phases

1. **Phase 0 — decouple input** (T640). Every input handler takes the
   backend-neutral `vix-input` types; only the terminal loop converts
   crossterm events. See
   [`crates/vix-input/spec/index.md`](../../crates/vix-input/spec/index.md).
   Startup and the loop's background work are `App::start`, `App::tick`,
   `App::poll_interval` (`src/app/event_loop.rs`), shared by every front end.
2. **Phase 1 — the whole App in a window** (T641 GPUI, T642 egui + the
   shared core). The unchanged `ui::draw` renders each frame into the shared
   cell grid, which each front end paints with a monospace font. Every panel,
   overlay, picker and dock works identically to the terminal.
3. **Phase 2 — native components** (not started): docks, tables, Markdown,
   and a custom element drawing `vix-editor-core`'s buffer with proportional
   text, migrating one panel at a time with the cell grid as the fallback.

## Shared behavior (both front ends)

- **Rendering.** Text is drawn in *spans* — maximal same-style stretches of
  a row — each placed at its exact cell column; any non-ASCII cell (box
  drawing, Nerd Font icons, CJK, emoji) is its own span, so a fallback glyph
  never shifts the rest of its row. Bold uses the primary family's bold face;
  italic, underline and strikethrough come from the cell's modifiers;
  `REVERSED` and `DIM` fold into the colors.
- **Input** is shaped like what crossterm delivers for the same key
  (`vix_gui_common::keys`): typed characters for plain typing, the
  unmodified key for `Ctrl`/`Alt`/`Cmd` chords ("Option as Meta", so
  `Alt+letter` menu mnemonics work), `Shift+Tab` as `BackTab`,
  `Cmd`/Windows/Super as `SUPER` (Vix folds `Cmd` into `Ctrl` on macOS).
  Mouse positions map to cells; motion is reported per cell; wheel and
  trackpad deltas become whole scroll lines.
- **Menus.** The native menu bar is built from the `vix-menu` model; every
  leaf dispatches its action id to `App::run_action` (one action, one
  implementation). File gains one GUI-only item, **Open with System
  Dialog…** (`menu.item.file.open_native`).
- **Closing the window** runs Vix's own `file.quit` flow (unsaved-changes
  prompt, session save) instead of closing outright.
- **Background work** runs at `App::poll_interval`, calling `App::tick`, as
  the terminal loop does.
- **Fonts.** Menlo / Consolas / DejaVu Sans Mono by platform, or
  `VIX_GUI_FONT`. Vix's icons are Nerd Font glyphs, so both builds fall back
  per glyph to an installed "Symbols Nerd Font" or "… Nerd Font Mono" family;
  with none installed, icons show the missing-glyph box, as in a non-Nerd
  terminal.
- **Colors.** `Color::Reset` resolves to a dark or light default following
  the OS appearance; `theme = "auto"` follows it too.
- **Not applicable in a window:** suspend-to-shell (`Ctrl+Z`) is ignored;
  inline terminal images (`image` feature) are not shown.

## Where the two differ

| | `vix-gpui` | `vix-egui` |
| --- | --- | --- |
| Text rendering | OS text stack (CoreText / DirectWrite / Linux); OS font fallback | egui's own renderer (skrifa); fonts loaded explicitly via `fontique` |
| Native menu bar | macOS, Windows, Linux (GPUI) | macOS only (`muda`: its Windows setup is an `unsafe fn`, forbidden here; Linux needs GTK) — the in-grid menu bar works everywhere |
| File dialog | GPUI | `rfd` (macOS menu only) |
| IME | not yet (needs a GPUI input handler) | composed input (`Ime::Commit`) types like text; the candidate window is placed at the cursor |
| Clipboard keys | arrive as keys | `NativeOptions::clipboard_shortcuts = false` makes `Cmd/Ctrl+X/C/V` arrive as keys |
| Redraw model | retained view, redrawn on `notify` | immediate mode, `ui` per frame |

## Known limitations (Phase 1)

- `Option`-typed characters (`ß`, `€`) are unavailable on macOS because
  `Option` is treated as Meta.
- No inline IME preedit (composition) text in either build.
- Native menu labels are translated at startup; changing the locale at
  runtime does not relabel them until restart.
- Wide (CJK, emoji) glyphs are drawn at their cell position but sized by
  the fallback font.

## Why a separate workspace

GPUI and egui each pull in a GPU stack (Metal / DirectX / Vulkan via wgpu),
windowing and font libraries; the terminal build, `cargo test --workspace`
and clippy at the repo root must never build them. Both toolkits are pinned
to exact versions (GPUI Kit ships breaking changes between minor releases),
so upgrades are deliberate tasks. The lint policy is restated in
`gui/Cargo.toml` (a separate workspace cannot inherit the root's
`[workspace.lints]`) and must be kept in step with the root.

```sh
cargo test   --manifest-path gui/Cargo.toml --workspace
cargo clippy --manifest-path gui/Cargo.toml --workspace --all-targets --all-features -- -D warnings
cargo fmt    --manifest-path gui/Cargo.toml --all --check
```

Linux needs the usual windowing/font system libraries (X11/Wayland,
xkbcommon, fontconfig, Vulkan); see `.github/workflows/gui.yml`.

### Render smoke test

With the opt-in `screenshot` feature on either front end,
`VIX_GUI_SCREENSHOT=out.png` waits for startup, runs the comma-separated
action ids in `VIX_GUI_SCREENSHOT_ACTIONS`, types the space-separated
keystrokes in `VIX_GUI_SCREENSHOT_KEYS` (GPUI syntax: `ctrl-p`, `shift-t`,
`enter`) through that front end's real input handling, saves the window as a
PNG, and quits:

```sh
VIX_GUI_SCREENSHOT=out.png VIX_GUI_SCREENSHOT_KEYS="ctrl-p c a r g o" \
  cargo run --manifest-path gui/Cargo.toml -p vix-egui --features screenshot -- src/main.rs
```

Both draw explicitly for the capture, because a window that is not visible
(a CI runner, or one behind other windows) otherwise stops painting: GPUI
via `Window::draw` before `render_to_image`, egui via
`ViewportCommand::RequestPaintWhileHidden`. `vix-gpui`'s feature enables
`gpui-pre-platform`'s `test-support` directly, because GPUI Kit's own
`test-support` also pulls in its alternate GPUI Fast backend, whose exact
version pins conflict with the default backend's.

## Tests

Unit tests (`cargo test --manifest-path gui/Cargo.toml --workspace`) cover,
in `vix-gui-common`: the grid backend (drawing, cursor tracking, resize),
color and modifier resolution, row span/background planning, the keystroke
rules, the menu tree (every model leaf in order; File gains the system-dialog
item) and font fallback choice; in each front end: its toolkit adapter
(GPUI keystroke parsing and modifier flags; egui's Key/Text/IME split,
Option-as-Meta, and scripted keystrokes round-tripping) and native menu
conversion.
