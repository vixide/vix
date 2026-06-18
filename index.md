# Vix documentation index

A map of the documentation in this repository. Start with the
[README](README.md) for an overview, or jump to a topic below.

## Getting started

- [README](README.md) — overview, features, install & run, examples.
- [CHANGELOG](CHANGELOG.md) — notable changes.

## User guides (`docs/`)

Each guide is `docs/<topic>/index.md`. Highlights:

- [Architecture](docs/architecture/index.md) — single-crate module layout, event
  flow, rendering, theming, i18n, configuration, testing.
- [Keybindings](docs/keybindings/index.md) — every keyboard shortcut and mouse
  gesture, including keymaps.
- [Configuration](docs/configuration/index.md) — settings file, every key, CLI
  flags.
- [Themes](docs/themes/index.md) · [Internationalization](docs/internationalization/index.md)
  · [Keymaps](docs/keymaps/index.md) · [Menus](docs/menus/index.md)
- [Language Server Protocol](docs/language-server-protocol/index.md) — diagnostics,
  hover, go-to-definition, completion.
- Panels & tools: [Command Palette](docs/command-palette/index.md),
  [File Explorer](docs/file-explorer/index.md), [Find Panel](docs/find-panel/index.md),
  [Outline](docs/outline-panel/index.md), [Git Panel](docs/git-panel/index.md),
  [Calendar](docs/calendar-panel/index.md),
  [Nerd Font Picker](docs/nerd-font-picker/index.md),
  [ASCII Picker](docs/ascii-code-picker/index.md),
  [System Information](docs/system-information-panel/index.md), and more under
  `docs/`.

## Specification (`spec/`) — source of truth

All specs live under `spec/`, one directory per topic or action
(`spec/<name>/index.md`).

- Overview: [spec/index.md](spec/index.md). Each spec lives at
  `spec/<name>/index.md`.
- Core: [menus](spec/menus/index.md) · [keyboard](spec/keyboard/index.md) ·
  [keymaps](spec/keymaps/index.md) · [navigation](spec/navigation/index.md) ·
  [command-palette](spec/command-palette/index.md) ·
  [file-explorer](spec/file-explorer/index.md) · [editor](spec/editor/index.md) ·
  [find-and-replace](spec/find-and-replace/index.md) · [hover](spec/hover/index.md)
- Features: [lsp](spec/lsp/index.md) ·
  [git-integration](spec/git-integration/index.md) ·
  [spellcheck](spec/spellcheck/index.md) · [case-change](spec/case-change/index.md) ·
  [themes](spec/themes/index.md) · [localization](spec/localization/index.md) ·
  tools under [spec/tools/](spec/tools/) · [comparisons](spec/comparisons/index.md)
- Per-action: one `spec/<action>/index.md` for each editor action in the catalog
  (`spec/actions/index.md`).

## Contributor & agent guidance (`AGENTS/`)

- [AGENTS.md](AGENTS.md) — entry point: build/test, hard rules, conventions.
- [AGENTS/conventions.md](AGENTS/conventions.md) — coding style and patterns.
- [AGENTS/workflow.md](AGENTS/workflow.md) — the spec-driven workflow and drift
  audits.
- [AGENTS/share/crate-map.md](AGENTS/share/crate-map.md) — every module and file.
- [AGENTS/share/glossary.md](AGENTS/share/glossary.md) — shared terminology.

## Code documentation

The crates are documented inline (the build denies missing docs). Browse it with:

```sh
cargo doc --workspace --no-deps --open
```
