# Desktop GUIs (`vix-gpui`, `vix-egui`, `vix-iced`, `vix-gtk4`, `vix-slint`)

Native desktop windows for Vix on macOS, Windows and Linux, built five
times — on [GPUI](https://github.com/longbridge/gpui-kit) (via Longbridge's
GPUI Kit), [egui](https://github.com/emilk/egui) (via eframe),
[iced](https://github.com/iced-rs/iced), [GTK 4](https://gtk.org) (via
gtk4-rs) and [Slint](https://slint.dev) — each a complete, independent
implementation, so the toolkits can be tried and compared side by side
before committing to one.

**Status:** experimental. GUI Phase 0 (T640) and Phase 1 (T641, T642)
shipped, with iced, GTK 4 and Slint front ends added in T647; Phase 2 (native components) not started, and waits on choosing a
toolkit.

```sh
cargo run --manifest-path gui/Cargo.toml --release -p vix-gpui [-- FILE…]
cargo run --manifest-path gui/Cargo.toml --release -p vix-egui [-- FILE…]
cargo run --manifest-path gui/Cargo.toml --release -p vix-iced [-- FILE…]
cargo run --manifest-path gui/Cargo.toml --release -p vix-gtk4 [-- FILE…]   # needs GTK 4 installed
cargo run --manifest-path gui/Cargo.toml --release -p vix-slint [-- FILE…]
```

All take the same command line as `vix` (`vix::cli::Cli`).

## Layout

`gui/` is one Cargo workspace (separate from the repo root, like `fuzz/`)
with one crate per front end:

| Crate      | Contents |
| ---------- | -------- |
| `vix-gpui` | The GPUI front end: a GPUI view holding the `App`, painting via GPUI's text system, GPUI keystrokes, native menus and file dialog through GPUI itself. GPUI Kit pinned `=0.8.0`. |
| `vix-egui` | The egui front end: an `eframe::App`, painting via egui's painter, egui events, fonts loaded by family name through `fontique`, native menus (macOS) through `muda`, file dialog through `rfd`. eframe pinned `=0.37.0`. |
| `vix-iced` | The iced front end: an Elm-style application whose `canvas` paints the grid, a zero-size widget that enables the input method at the cursor, events through a subscription, native menus (macOS) through `muda`, file dialog through `rfd`. iced pinned `=0.14.0`. |
| `vix-gtk4` | The GTK 4 front end: a `DrawingArea` painted with Cairo and Pango, an `IMMulticontext` driven by the key controller, a GIO menu bar (native on every platform), `FileChooserNative`. gtk4-rs 0.11 against the GTK 4.0 API, so it builds with Ubuntu 22.04's GTK 4.6. |
| `vix-slint` | The Slint front end: `.slint` markup compiled at startup by `slint-interpreter` (Slint's generated Rust contains `unsafe` code, which this workspace forbids), per-row repeaters of background, text and line elements whose models are diffed after each frame, a generated `MenuBar` (native on macOS and Windows), file dialog through `rfd`. Slint pinned `=1.18.1`; used under the GPL-3.0-only, so `vix-slint` itself is `GPL-3.0-only`. |

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

## Licenses

Each front end is a distributed binary, so it carries the licenses its
framework and the rest of its linked dependencies allow, not automatically
all five of Vix's source licenses: its `Cargo.toml` `license` and its own
`LICENSE` file state that subset, and `scripts/check-binary-licenses`
recomputes it from `gui/Cargo.lock` and fails on any difference. Today
`vix-gpui`, `vix-egui`, `vix-iced` and `vix-gtk4` are `Apache-2.0 OR
BSD-3-Clause OR MIT OR GPL-3.0-only` (GPUI is Apache-2.0 only, and every
toolkit stack links Apache-2.0-only crates, which rule out GPL-2.0-only)
and `vix-slint` is `GPL-3.0-only` (Slint, used under its GPLv3 option).
A new front end, or a toolkit upgrade, follows the same rule. See
[`spec/license/index.md`](../../spec/license/index.md) ("Binary licenses").

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

## Where they differ

| | Text rendering | Native menu bar | File dialog | IME | Redraw model |
| --- | --- | --- | --- | --- | --- |
| `vix-gpui` | OS text stack (CoreText / DirectWrite / Linux); OS font fallback | macOS, Windows, Linux (GPUI) | GPUI | not yet (needs a GPUI input handler) | retained view, redrawn on `notify` |
| `vix-egui` | egui's own renderer (skrifa); fonts loaded explicitly via `fontique` | macOS only (`muda`: its Windows setup is an `unsafe fn`, forbidden here; Linux needs GTK) | `rfd` (macOS menu only) | commits type like text; candidate window at the cursor | immediate mode, `ui` per frame |
| `vix-iced` | cosmic-text with advanced shaping; system fonts and per-glyph fallback built in; icon spans drawn in the Nerd Font explicitly | macOS only (`muda`, clicks delivered as a subscription) | `rfd` (macOS menu only) | commits type like text, via a zero-size widget requesting the input method at the cursor | Elm-style: the grid is redrawn once per message (event, tick, menu click) |
| `vix-gtk4` | Pango over Cairo (software); OS font stack with per-glyph fallback | every platform (GIO menu model; the global bar on macOS) | `FileChooserNative`, every platform | commits through an `IMMulticontext`; candidate window at the cursor | retained `DrawingArea`, `queue_draw` |
| `vix-slint` | Slint's femtovg renderer (OpenGL) with parley / fontique; per-script fallback, icon spans given the Nerd Font family | macOS and Windows (Slint's `MenuBar`); none on Linux, where it would duplicate the in-grid bar | `rfd` (macOS menu only) | none (Slint enables IME only for a focused `TextInput`) | retained elements; per-row models diffed each frame |

Clipboard keys (`Cmd/Ctrl+X/C/V`) arrive as plain keys in every build
(egui needs `NativeOptions::clipboard_shortcuts = false` for that); Vix
reads the clipboard itself. In every build the in-grid Vix menu bar works
too.

## Known limitations (Phase 1)

- `Option`-typed characters (`ß`, `€`) are unavailable on macOS because
  `Option` is treated as Meta.
- No inline IME preedit (composition) text in any build; `vix-slint` has
  no IME at all.
- `vix-iced` and `vix-gtk4` have no OS paste event (`App::on_paste`);
  paste keys reach Vix, which reads the clipboard.
- `vix-gtk4` treats smooth (trackpad) scroll deltas as lines: telling
  pixels from lines needs GTK 4.8's `EventControllerScroll::unit`, outside
  the 4.0 API it targets.
- `vix-slint` drops dead keys without Option (e.g. US-International) and
  reports Ctrl+Shift+punctuation as the shifted character; it uses two
  `unstable-*` Slint features (raw winit events for Option-as-Meta, and
  Slint's font collection for the icon font), safe only because Slint is
  pinned exactly.
- Native menu labels are translated at startup; changing the locale at
  runtime does not relabel them until restart.
- Wide (CJK, emoji) glyphs are drawn at their cell position but sized by
  the fallback font.

## Distribution (T645, T647)

`vix-gpui`, `vix-egui`, `vix-iced` and `vix-slint` are released from the
same version tag as the terminal app, by `dist` (the repo-root `dist-workspace.toml` lists this workspace as
`cargo:gui`; `[package.metadata.dist]` in each front end's `Cargo.toml`):

```sh
brew install vixide/tap/vix-gpui     # or vix-egui, vix-iced, vix-slint; vix-tui for the terminal app
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
  driver; OpenGL/EGL for `vix-slint`) must be present.
- **`vix-gtk4` is not released yet.** It links the system GTK 4, so a
  prebuilt binary needs a matching GTK at run time, and on Windows the GTK
  DLLs would have to ship beside it. Build it from source (Homebrew `gtk4`
  on macOS, `libgtk-4-dev` on Debian/Ubuntu).
- **`vix-slint` is GPL-3.0-only**: Slint is used under the GPLv3, so its
  binaries carry that license rather than Vix's five-way choice.
- **Windows** release builds use the `windows` subsystem, so no console
  window opens beside the GUI (which also means `--help` prints nothing
  there); debug builds keep the console.
- **macOS** ships a plain executable, not an `.app` bundle; it opens its
  window when run from a terminal, Spotlight does not list it.
- **Version.** `gui/Cargo.toml`'s `workspace.package.version` must equal the
  root's: one tag releases every app, and dist refuses a tag that does
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

With the opt-in `screenshot` feature on any front end,
`VIX_GUI_SCREENSHOT=out.png` waits for startup, runs the comma-separated
action ids in `VIX_GUI_SCREENSHOT_ACTIONS`, types the space-separated
keystrokes in `VIX_GUI_SCREENSHOT_KEYS` (GPUI syntax: `ctrl-p`, `shift-t`,
`enter`) through that front end's real input handling, saves the window as a
PNG, and quits:

```sh
VIX_GUI_SCREENSHOT=out.png VIX_GUI_SCREENSHOT_KEYS="escape ctrl-p c a r g o" \
  cargo run --manifest-path gui/Cargo.toml -p vix-egui --features screenshot -- tui/src/main.rs
```

(The leading `escape` closes the Welcome overlay a fresh profile opens on.)
Each captures in its own way: GPUI draws explicitly (`Window::draw`, then
`render_to_image`) and egui asks to paint while hidden
(`ViewportCommand::RequestPaintWhileHidden`), because a window that is not
visible (a CI runner, or one behind other windows) otherwise stops
painting; iced takes `window::screenshot` (it redraws after every message,
hidden or not); GTK 4 paints the grid into a Cairo image surface with the
same paint function, no window capture at all; Slint uses
`Window::take_snapshot`. `vix-gpui`'s feature enables
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
