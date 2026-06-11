# Vix documentation index

A map of all documentation in this repository. Start with the
[README](README.md) for an overview, or jump to a topic below.

## Getting started

- [README](README.md) — overview, features, install & run, examples.
- [CHANGELOG](CHANGELOG.md) — notable changes.

## User guides (`docs/`)

- [Architecture](docs/architecture.md) — workspace shape, modules, event flow,
  rendering, theming, i18n, configuration, dependency pinning, testing.
- [Keybindings](docs/keybindings.md) — every keyboard shortcut and mouse gesture.
- [Themes](docs/themes.md) — built-in monochrome themes and the custom JSON
  theme format.
- [Internationalization](docs/i18n.md) — bundled languages, switching, and adding
  a language.
- [Configuration](docs/configuration.md) — settings file, every key, CLI flags.

## Specification (`spec/`) — source of truth

- [index.md](spec/index.md) — overview, crate set, build/run, implementation status.
- [menus.md](spec/menus.md) · [keyboard.md](spec/keyboard.md) ·
  [navigation.md](spec/navigation.md)
- [command-palette.md](spec/command-palette.md) ·
  [file-explorer.md](spec/file-explorer.md) ·
  [search-and-replace.md](spec/search-and-replace.md)
- [code-editor.md](spec/code-editor.md) ·
  [theme-chooser.md](spec/theme-chooser.md) ·
  [locale-chooser.md](spec/locale-chooser.md) ·
  [keyway-chooser.md](spec/keyway-chooser.md) ·
  [nerd-font-palette.md](spec/nerd-font-palette.md) ·
  [vix-calendar.md](spec/vix-calendar.md) · [hover.md](spec/hover.md)
- [main-rs-and-lib-rs-boilerplate.md](spec/main-rs-and-lib-rs-boilerplate.md) ·
  [rust-clippy-pedantic.md](spec/rust-clippy-pedantic.md) ·
  [rust-cargo-config-toml-musl.md](spec/rust-cargo-config-toml-musl.md)
- [comparisons.md](spec/comparisons.md) · [test.md](spec/test.md)

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
