# Desktop GUI (`vix-gui`)

A native, GPU-rendered desktop window for Vix on macOS, Windows and Linux,
built on [GPUI Kit](https://github.com/longbridge/gpui-kit) (Longbridge's
toolkit over Zed's GPUI).

**Status:** Phase 0 and Phase 1 shipped (experimental). Phase 2 (native
components) not started.

## Phases

1. **Phase 0 — decouple input.** Every input handler takes the
   backend-neutral `vix-input` types instead of crossterm's; only the
   terminal loop converts crossterm events. See
   [`crates/vix-input/spec/index.md`](../../crates/vix-input/spec/index.md).
   Startup and the per-iteration background work moved into the App shell
   (`App::start`, `App::tick`, `App::poll_interval` in
   `src/app/event_loop.rs`), so both front ends share them.
2. **Phase 1 — the whole App in a window (this crate).** The unchanged
   `ui::draw` renders each frame into an in-memory ratatui cell grid
   (`src/grid.rs`, a ratatui `Backend`), which the window paints with a
   monospace font (`src/paint.rs`). Every panel, overlay, picker and dock
   therefore works on day one, identically to the terminal.
3. **Phase 2 — native components where they pay off** (not started): GPUI
   Kit's dock/tabs for layout, its table for the DB workbench and table
   editor, its Markdown view for preview; a custom element drawing
   `vix-editor-core`'s buffer with proportional text. Panels migrate one at a
   time; the cell grid stays the fallback for everything else.

## Phase 1 behavior

- **Input.** GPUI keystrokes become `vix-input` key events shaped like the
  ones crossterm delivers for the same key (`src/keys.rs`): typed characters
  for plain typing, the unmodified key for `Ctrl`/`Alt`/`Cmd` chords
  ("Option as Meta", so `Alt+letter` menu mnemonics work), `Shift+Tab` as
  `BackTab`, `Cmd`/Windows/Super as `SUPER` (Vix folds `Cmd` into `Ctrl` on
  macOS itself). Mouse positions map to cells; motion is reported per cell,
  as a terminal reports it; wheel/trackpad deltas become whole scroll lines.
- **Native menu bar.** Built from the `vix-menu` model (`src/menus.rs`);
  every leaf dispatches its action id to `App::run_action`, so the native
  menu, Vix's in-grid menu bar, the palette and shortcuts share one
  implementation. File gains one GUI-only item, **Open with System Dialog…**
  (`menu.item.file.open_native`), using the platform file dialog.
- **Background work** runs on a GPUI timer at `App::poll_interval`, calling
  `App::tick` and redrawing, exactly as the terminal loop does.
- **Fonts.** Menlo / Consolas / DejaVu Sans Mono by platform, or
  `VIX_GUI_FONT`. Vix's icons are Nerd Font glyphs; the window falls back,
  per glyph, to an installed "Symbols Nerd Font" or "… Nerd Font Mono"
  family. With none installed, icons show the missing-glyph box, as they do
  in a non-Nerd terminal.
- **Colors.** `Color::Reset` resolves to a dark or light default following
  the OS appearance; `theme = "auto"` also follows it (the window's
  equivalent of the terminal's OSC 11 query). Named/indexed colors use the
  xterm palette.
- **Not applicable in a window:** suspend-to-shell (`Ctrl+Z`) is ignored;
  inline terminal images (`image` feature) are not shown.

### Known limitations (Phase 1)

- No IME composition or dead keys: input arrives as completed key presses.
  `Option`-typed characters (`ß`, `€`) are unavailable on macOS because
  `Option` is treated as Meta.
- Native menu labels are translated at startup; changing the locale at
  runtime does not relabel them until restart.
- Wide (CJK, emoji) glyphs are drawn at their cell position but sized by
  the fallback font.

## Why a separate workspace

`gui/` is its own Cargo workspace (like `fuzz/`), not a `crates/*` member:

- GPUI pulls in a GPU stack (Metal / DirectX / Vulkan via wgpu), windowing
  and font libraries; the terminal build, `cargo test --workspace` and
  clippy at the repo root must never build them.
- GPUI Kit ships breaking changes between minor releases, so it is pinned
  to an exact version (`=0.8.0`, over `gpui-pre 0.3.9`). Upgrades are
  deliberate tasks, not drive-by `cargo update`s.
- The lint policy is restated in `gui/Cargo.toml` (a separate workspace
  cannot inherit `[workspace.lints]`) and must be kept in step with the root.

## Build, run, test

```sh
cargo run   --manifest-path gui/Cargo.toml --release [-- FILE…]
cargo test  --manifest-path gui/Cargo.toml
cargo clippy --manifest-path gui/Cargo.toml --all-targets -- -D warnings
```

It takes the same command line as `vix` (`vix::cli::Cli`). Linux needs the
usual GPUI system libraries (X11/Wayland, xkbcommon, fontconfig, Vulkan).

### Render smoke test

With the opt-in `screenshot` feature, `VIX_GUI_SCREENSHOT=out.png` waits for
startup, runs the comma-separated action ids in `VIX_GUI_SCREENSHOT_ACTIONS`,
types the space-separated keystrokes in `VIX_GUI_SCREENSHOT_KEYS` (GPUI
syntax: `ctrl-p`, `shift-t`, `enter`) through the window's real key dispatch,
renders the window to PNG, and quits:

```sh
VIX_GUI_SCREENSHOT=out.png VIX_GUI_SCREENSHOT_KEYS="ctrl-p c a r g o" \
  cargo run --manifest-path gui/Cargo.toml --features screenshot -- src/main.rs
```

The window is drawn explicitly before the capture: macOS pauses frame
callbacks for a window that is not visible, so the last presented frame can
predate the scripted input. The feature enables `gpui-pre-platform`'s
`test-support` directly, because GPUI Kit's own `test-support` also pulls in
its alternate GPUI Fast backend, whose exact version pins conflict with the
default backend's.

## Tests

Unit tests (`cargo test --manifest-path gui/Cargo.toml`) cover the keystroke
conversion, the grid backend (drawing, cursor tracking, resize), color and
modifier resolution, and the native menu tree (every model leaf becomes a
native item, in order; File gains the system-dialog item).
