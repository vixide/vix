# Vix™ documentation index

A map of the documentation in this repository. 

## User guides

Highlights:

- [Architecture](architecture/index.md) — workspace layout (App shell + `vix-*`
  member crates), event flow, rendering, theming, i18n, configuration, testing.
- [Keybindings](keybindings/index.md) — every keyboard shortcut and mouse
  gesture, including keymaps.
- [Configuration](configuration/index.md) — settings file, every key, CLI
  flags. · [EditorConfig](editorconfig/index.md) — per-project `.editorconfig`.
- [Themes](themes/index.md) · [Internationalization](internationalization/index.md)
  · [Keymaps](keymaps/index.md) · [Menus](menus/index.md)
- [Language Server Protocol](language-server-protocol/index.md) — diagnostics,
  hover, go-to-definition, completion.
- [Debugger](debugger/index.md) — Debug Adapter Protocol: breakpoints, stepping,
  call stack, variables, watches.
- Panels & tools: [Command Palette](command-palette/index.md),
  [File Explorer](file-explorer/index.md), [Find Panel](find-panel/index.md),
  [Outline](outline-panel/index.md), [Git Panel](git-panel/index.md),
  [Calendar](calendar-panel/index.md),
  [Nerd Font Picker](nerd-font-picker/index.md),
  [ASCII Picker](ascii-code-picker/index.md),
  [System Information](system-information-panel/index.md), and more under
  `docs/`.
- Edit surfaces (Edit → Mode): [Edit SQL](edit-sql/index.md) · [Edit Table](edit-table/index.md) ·
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
- DB: [Database Workbench](db/index.md) (SQLite/Postgres/MySQL — connections,
  schema tree, query editor, transactions, history, export).
- Workflow: [Tasks](tasks/index.md) (named `tasks.toml` commands) ·
  [Compare With File](diff-view/index.md) (diff against another file) ·
  [Integrated Terminal](terminal/index.md) (a shell in a panel) ·
  [Switch Project](switch-project/index.md) (re-root at a recent workspace) ·
  [Test Runner](test-runner/index.md) (pass/fail panel).
- Markup: [Org](org/index.md) (Org-mode basics — headlines, TODO, export).
- Reference: [Media Types](media-types/index.md) (MIME picker by type/extension).

## Specification — source of truth

Specs are **per crate**: each member crate owns `crates/<crate>/spec/index.md`
(multi-topic crates add `spec/<topic>/index.md` sub-specs). The repo-root
[`spec/`](spec/) holds only cross-cutting / app-level and build/meta specs.

- Overview: [spec/index](spec/index/index.md) — goals, dependency crates,
  build/run.
- Core: [menus](crates/vix-menu/spec/index.md) · [keyboard](crates/vix-keyboard-shortcut-panel/spec/index.md) ·
  [keymaps](crates/vix-keymap-model/spec/index.md) · [navigation](spec/navigation/index.md) ·
  [command-palette](crates/vix-palette/spec/index.md) ·
  [file-explorer](crates/vix-fileops/spec/index.md) · [editor](crates/vix-editor/spec/index.md) ·
  [find-and-replace](crates/vix-query/spec/index.md) · [hover](crates/vix-lsp/spec/hover/index.md)
- Features: [lsp](crates/vix-lsp/spec/index.md) ·
  [git-integration](crates/vix-git/spec/git-integration/index.md) ·
  [spellcheck](crates/vix-spellcheck/spec/index.md) · [case-change](crates/vix-case/spec/index.md) ·
  [themes](crates/vix-theme/spec/index.md) · [localization](crates/vix-i18n/spec/index.md) ·
  tools under [spec/tools/](spec/tools/) · [comparisons](spec/comparisons/index.md)
- Editor actions: one `spec/<action>/index.md` per action under
  [`crates/vix-editor/spec/`](crates/vix-editor/spec/), catalogued in
  [`crates/vix-editor-core/spec/index.md`](crates/vix-editor-core/spec/index.md).

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

---

Vix™ and Vix IDE™ are trademarks.
