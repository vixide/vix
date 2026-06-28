# Vix documentation index

A map of the documentation in this repository. 

## User guides

Highlights:

- [Architecture](architecture/index.md) — single-crate module layout, event
  flow, rendering, theming, i18n, configuration, testing.
- [Keybindings](keybindings/index.md) — every keyboard shortcut and mouse
  gesture, including keymaps.
- [Configuration](configuration/index.md) — settings file, every key, CLI
  flags. · [EditorConfig](editorconfig/index.md) — per-project `.editorconfig`.
- [Themes](themes/index.md) · [Internationalization](internationalization/index.md)
  · [Keymaps](keymaps/index.md) · [Menus](menus/index.md)
- [Language Server Protocol](language-server-protocol/index.md) — diagnostics,
  hover, go-to-definition, completion.
- Panels & tools: [Command Palette](command-palette/index.md),
  [File Explorer](file-explorer/index.md), [Find Panel](find-panel/index.md),
  [Outline](outline-panel/index.md), [Git Panel](git-panel/index.md),
  [Calendar](calendar-panel/index.md),
  [Nerd Font Picker](nerd-font-picker/index.md),
  [ASCII Picker](ascii-code-picker/index.md),
  [System Information](system-information-panel/index.md), and more under
  `docs/`.
- Edit surfaces (Tools menu): [Edit Table](edit-table/index.md) ·
  [Edit Outline](edit-outline/index.md) · [Edit JSON](edit-json/index.md) ·
  [Edit YAML](edit-yaml/index.md) · [Edit Bytes](edit-bytes/index.md).
- Generators & editing: [Insert](insert/index.md) (UUID, ZID, Markdown, HTML,
  Lorem ipsum, Date/Time) · [Snippets](snippets/index.md) (with tabstops) ·
  [QR Code](qr-code/index.md) ·
  [Multiple Cursors](multiple-cursors/index.md) (select-all-occurrences, column
  selection) · [Macros](macros/index.md) (record, save, replay).
- View: [Zen Mode](zen-mode/index.md) · [Breadcrumb Bar](breadcrumb/index.md).
- AI: [AI Chat Panel](agent-panel/index.md) (conversation with the configurable
  `ai_command` assistant).
- Workflow: [Tasks](tasks/index.md) (named `tasks.toml` commands) ·
  [Compare With File](diff-view/index.md) (diff against another file) ·
  [Integrated Terminal](terminal/index.md) (a shell in a panel) ·
  [Switch Project](switch-project/index.md) (re-root at a recent workspace).

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
