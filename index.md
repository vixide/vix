# Vix documentation index

A map of the documentation in this repository. Start with the
[README](README.md) for an overview, or jump to a topic below.

## Getting started

- [README](README.md) — overview, features, install & run, examples.
- [CHANGELOG](CHANGELOG.md) — notable changes.

## User guides (`docs/`)

Each guide is `docs/<topic>/index.md`. Highlights:

- [Architecture](docs/architecture/index.md) — workspace shape, modules, event
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

App-level specs live in `spec/`; each crate's own behavior is specified in
`vix-<crate>/spec/index.md`.

- App: [index.md](spec/index.md) · [menus.md](spec/menus.md) ·
  [keyboard.md](spec/keyboard.md) · [navigation.md](spec/navigation.md) ·
  [command-palette.md](spec/command-palette.md) ·
  [file-explorer.md](spec/file-explorer.md) · [hover.md](spec/hover.md) ·
  [lsp.md](spec/lsp.md) · [git-integration.md](spec/git-integration.md) ·
  [spellcheck.md](spec/spellcheck.md) · [case-change.md](spec/case-change.md) ·
  [comparisons.md](spec/comparisons.md)
- Per-crate: each `vix-*/spec/index.md` (e.g.
  [vix-editor](vix-editor/spec/index.md),
  [vix-find-panel](vix-find-panel/spec/index.md),
  [vix-keymap-chooser](vix-keymap-chooser/spec/index.md),
  [vix-x11-color-picker](vix-x11-color-picker/spec/index.md),
  [vix-workspace-dashboard-panel](vix-workspace-dashboard-panel/spec/index.md)).

## Contributor & agent guidance (`AGENTS/`)

- [AGENTS.md](AGENTS.md) — entry point: build/test, hard rules, conventions.
- [AGENTS/conventions.md](AGENTS/conventions.md) — coding style and patterns.
- [AGENTS/workflow.md](AGENTS/workflow.md) — the spec-driven workflow and drift
  audits.
- [AGENTS/share/crate-map.md](AGENTS/share/crate-map.md) — every crate and file.
- [AGENTS/share/glossary.md](AGENTS/share/glossary.md) — shared terminology.

## Code documentation

The crates are documented inline (the build denies missing docs). Browse it with:

```sh
cargo doc --workspace --no-deps --open
```
