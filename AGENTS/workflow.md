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
5. **Test**: extend `tests/integration.rs` or the relevant crate's unit tests.
6. **Verify**: `cargo test --workspace` and `cargo clippy --workspace` clean.
7. **Record** user-visible changes in `CHANGELOG.md`.

## Auditing for drift

Drift is when code, spec, and docs disagree. To audit:

- Compare each `spec/*.md` against the implementation it describes.
- Compare `README.md` / `docs/*` against current features and crate names.
- Check that every action id used by a menu/palette has a `run_action` arm and an
  i18n label key.
- Check that user-facing strings go through `t!`.

Fix drift by aligning all three (code, spec, docs).

## Spec map

| Spec file                                | Covers                                       |
| ---------------------------------------- | -------------------------------------------- |
| `spec/index.md`                          | Overview, crate set, build/run, status       |
| `spec/menus.md`                          | Menu bar structure and items                 |
| `spec/keyboard.md`                       | Keyboard shortcuts (roadmap: bindings browser) |
| `spec/navigation.md`                     | Position history, go-to-definition/symbol    |
| `spec/command-palette.md`                | Palette modes and behavior                   |
| `spec/file-explorer.md`                  | Explorer tree and file ops                   |
| `spec/search-and-replace.md`             | Find/replace, project search, query-replace  |
| `spec/code-editor.md`                    | The `vix-editor` widget (soft wrap, brackets, …) |
| `spec/theme-chooser.md`                  | Theme model + custom JSON format             |
| `spec/locale-chooser.md`                 | Internationalization / languages             |
| `spec/keyway-chooser.md`                 | Keyboard navigation styles (Apple/Emacs/Vim) |
| `spec/vix-calendar.md`                   | Calendar box                                 |
| `spec/hover.md`                          | Mouse any-motion (menu hover) tracking       |
| `spec/main-rs-and-lib-rs-boilerplate.md` | Entry-point conventions                      |
| `spec/rust-clippy-pedantic.md`           | `clippy::pedantic` on all targets            |
| `spec/rust-cargo-config-toml-musl.md`    | MUSL linker config (`.cargo/config.toml`)    |
| `spec/comparisons.md`                    | Comparisons to other editors                 |
| `spec/test.md`                           | Manual test scenarios                        |
