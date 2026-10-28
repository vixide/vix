# Format Tool

Reformat the selection (or buffer) within one data format: pretty-print or
minify JSON, and canonicalize YAML and TOML.

Each function parses the text into a generic value and re-serializes it, so
the document is normalized (consistent indentation, key handling, quoting)
without changing its meaning. Used by the Tools → Format menu via
`App::transform_selection_or_buffer_try`.

## Prettier (T683)

`prettier` formats a buffer with the user's own Prettier. Vix never bundles
Node or Prettier.

- **Languages.** `LANGUAGES` maps Prettier language ids (`javascript`,
  `typescript`, `json`, `css`, `html`, `markdown`, `yaml`, `graphql`,
  `handlebars`) to file extensions. A file is formatted by Prettier only when
  its language id is listed in the `save.prettier_languages` setting (empty by
  default, so it is opt-in). `language_of` and `is_enabled_for` decide.
- **Binary.** `resolve_binary` finds the nearest `node_modules/.bin/prettier`
  walking up from the file, then the first `prettier` on `PATH`. The
  project-local binary is project-controlled code, so it is skipped unless the
  caller says the workspace is trusted.
- **Run.** `format` (behind the `process` feature, enabled by the root `tasks`
  feature) runs `prettier --stdin-filepath <path>` in the file's directory,
  so Prettier itself reads `.prettierrc*`, `.editorconfig` and
  `.prettierignore` (an ignored file comes back unchanged). The process is
  killed after `TIMEOUT` (10 seconds). It runs synchronously, like Pipe
  Through Command.
- **Errors.** `PrettierError` is `NotFound`, `Io`, `TimedOut`, `Failed(stderr)`
  and `BadOutput`.
- **Hook.** In `tui/src/app/prettier.rs`, Format Document (`lsp.format`) and
  format-on-save try Prettier first for a listed language and fall back to the
  language server for any other. A missing binary, a timeout or a syntax error
  leaves the buffer unchanged and shows a translated status hint.
- **Trust.** A project-local Prettier runs only in a workspace the user has
  trusted (the persisted T132 decision, which T627 will generalize).
  Otherwise only a `PATH` Prettier is used.

Tests use a fake `prettier` shell script in a temp directory, never real Node.
