# Spec-driven workflow

Vix is developed specification-first. The `spec/*.md` files describe intended
behavior and are the source of truth; the code implements them.

## The loop

1. **Read the spec.** Find the relevant `spec/*.md` (see the map below). If the
   change alters intended behavior, edit the spec first so it stays authoritative.
2. **Implement** in the smallest fitting module. Keep editing/state logic in the
   library; keep rendering in `src/ui.rs`.
3. **Internationalize** any new user-facing text: add the key(s) to
   `locales/app.yml` for every bundled language (English at minimum; others fall
   back to English) and render with `t!`.
4. **Document** every new public item (the build denies missing docs).
5. **Test**: extend `tests/integration.rs` or a module's unit tests.
6. **Verify**: `cargo test` and `cargo clippy --all-targets -- -D warnings` clean.
7. **Record** user-visible changes in `CHANGELOG.md`.

## Auditing for drift

Drift is when code, spec, and docs disagree. To audit:

- Compare each `spec/*/index.md` against the implementation it describes.
- Compare `README.md` / `docs/*` against current features and module names.
- Check that every action id used by a menu/palette has a `run_action` arm and an
  i18n label key.
- Check that user-facing strings go through `t!`.

Fix drift by aligning all three (code, spec, docs).

## Spec map

Each spec lives at `spec/<name>/index.md`. Notable ones:

| Spec                              | Covers                                       |
| --------------------------------- | -------------------------------------------- |
| `spec/index.md`                   | Overview, build/run, status                  |
| `spec/menus`                      | Menu bar structure and items                 |
| `spec/keymaps`                    | Keymaps: Apple/VSCode/Emacs/Vi/Spacemacs/IntelliJ/Eclipse |
| `spec/navigation`                 | Position history, go-to-definition/symbol    |
| `spec/command-palette`            | Palette modes and behavior                   |
| `spec/file-explorer`              | Explorer tree and file ops                   |
| `spec/find-and-replace`           | Find/replace, workspace search, query-replace |
| `spec/editor`                     | The editor widget (soft wrap, brackets, folds, …) |
| `spec/lsp`, `spec/debugger`       | Language Server Protocol; DAP debugger        |
| `spec/git-integration`            | Git status/diff/staging/conflicts            |
| `spec/tools`                      | Tools menu (convert/generate/checksum/…)      |
| `spec/snippets`, `spec/media-types` | JSON snippet scopes; the media-type catalog |
| `spec/org`, `spec/edit-sql`       | Org-mode editing; the SQL edit surface        |
| `spec/themes`                     | Theme model + custom JSON format             |
| `spec/localization`               | Internationalization / languages             |
| `spec/rust-clippy-pedantic`       | `clippy::pedantic` on all targets            |
| `spec/comparisons`, `spec/test`   | Editor comparisons; manual test scenarios    |
