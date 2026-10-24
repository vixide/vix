# Desktop GUIs (`vix-gpui`, `vix-egui`)

Native desktop windows for Vix on macOS, Windows and Linux, built twice — on
[GPUI](https://github.com/longbridge/gpui-kit) (via Longbridge's GPUI Kit)
and on [egui](https://github.com/emilk/egui) (via eframe) — each a complete,
independent implementation, so the toolkits can be tried and compared side
by side before committing to one.

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
with one crate per front end:

| Crate      | Contents |
| ---------- | -------- |
| `vix-gpui` | The GPUI front end: a GPUI view holding the `App`, painting via GPUI's text system, GPUI keystrokes, native menus and file dialog through GPUI itself. GPUI Kit pinned `=0.8.0`. |
| `vix-egui` | The egui front end: an `eframe::App`, painting via egui's painter, egui events, fonts loaded by family name through `fontique`, native menus (macOS) through `muda`, file dialog through `rfd`. eframe pinned `=0.37.0`. |

Each crate also carries its own copy of the parts that do not depend on
the toolkit: a ratatui cell-grid `Backend` (`grid`), color resolution and
per-row background runs / text spans (`render`), the keystroke rules
(`key_rules`), the native menu tree from `vix-menu` (`menu_tree`),
font-family choice (`font_choice`), and startup plus the scripted
screenshot run (`startup`).

## Independent implementations

**Each GUI implementation has its own, independent GUI code; there is no
common GUI library.** A front end depends on the Vix App (package
`vix-tui`, library `vix`) and the shared Vix crates such as `vix-input` and
`vix-i18n`, and on nothing from another front end — no shared GUI crate and
no `path` into a sibling's sources. When two front ends need the same logic
(the cell grid, the key rules, the menu tree), each keeps its own copy and
may change it for its toolkit without touching the others. So each front
end can be kept, changed, or deleted on its own, and comparing toolkits
compares complete implementations. Logic that every front end needs and
that is not about the GUI belongs in the App (`tui/src`) or a `crates/`
member, as `App::start` / `App::tick` / `App::poll_interval` do.

## Phases

1. **Phase 0 — decouple input** (T640). Every input handler takes the
   backend-neutral `vix-input` types; only the terminal loop converts
   crossterm events. See
   [`crates/vix-input/spec/index.md`](../../crates/vix-input/spec/index.md).
   Startup and the loop's background work are `App::start`, `App::tick`,
   `App::poll_interval` (`tui/src/app/event_loop.rs`), shared by every front end.
2. **Phase 1 — the whole App in a window** (T641 GPUI, T642 egui; T646
   made each front end self-contained). The unchanged `ui::draw` renders
   each frame into a cell grid, which each front end paints with a monospace font. Every panel,
   overlay, picker and dock works identically to the terminal.
3. **Phase 2 — native components** (not started): docks, tables, Markdown,
   and a custom element drawing `vix-editor-core`'s buffer with proportional
   text, migrating one panel at a time with the cell grid as the fallback.

## Behavior every front end implements

- **Rendering.** Text is drawn in *spans* — maximal same-style stretches of
  a row — each placed at its exact cell column; any non-ASCII cell (box
  drawing, Nerd Font icons, CJK, emoji) is its own span, so a fallback glyph
  never shifts the rest of its row. Bold uses the primary family's bold face;
  italic, underline and strikethrough come from the cell's modifiers;
  `REVERSED` and `DIM` fold into the colors.
- **Input** is shaped like what crossterm delivers for the same key
  (each crate's `key_rules`): typed characters for plain typing, the
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

## Distribution (T645)

Both GUIs are released from the same version tag as the terminal app, by
`dist` (the repo-root `dist-workspace.toml` lists this workspace as
`cargo:gui`; `[package.metadata.dist]` in each front end's `Cargo.toml`):

```sh
brew install vixide/tap/vix-gpui     # or vix-egui; vix-tui for the terminal app
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/vixide/vix/releases/latest/download/vix-gpui-installer.sh | sh
powershell -ExecutionPolicy Bypass -c "irm https://github.com/vixide/vix/releases/latest/download/vix-gpui-installer.ps1 | iex"
```

- **Artifacts** per front end: `.tar.xz` archives for macOS (Apple silicon
  and Intel) and Linux glibc (x86_64 and aarch64), a `.zip` and an `.msi`
  for Windows, shell and PowerShell installers, a Homebrew formula pushed to
  `vixide/homebrew-tap`, and a CycloneDX SBOM. No musl build (a GUI links
  the system's windowing, GPU and font libraries) and no npm package.
- **Linux** release builds install the same system libraries as CI
  (`[package.metadata.dist.dependencies.apt]`); at run time the usual
  desktop libraries (xkbcommon, Wayland or X11, fontconfig, a Vulkan
  driver) must be present.
- **Windows** release builds use the `windows` subsystem, so no console
  window opens beside the GUI (which also means `--help` prints nothing
  there); debug builds keep the console.
- **macOS** ships a plain executable, not an `.app` bundle; it opens its
  window when run from a terminal, Spotlight does not list it.
- **Version.** `gui/Cargo.toml`'s `workspace.package.version` must equal the
  root's: one tag releases all three apps, and dist refuses a tag that does
  not match every app's version.

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
  cargo run --manifest-path gui/Cargo.toml -p vix-egui --features screenshot -- tui/src/main.rs
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
in each front end: its grid backend (drawing, cursor tracking, resize),
color and modifier resolution, row span/background planning, the keystroke
rules, the menu tree (every model leaf in order; File gains the system-dialog
item), font fallback choice, and its toolkit adapter
(GPUI keystroke parsing and modifier flags; egui's Key/Text/IME split,
Option-as-Meta, and scripted keystrokes round-tripping) and native menu
conversion.
