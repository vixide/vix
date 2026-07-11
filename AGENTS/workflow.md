# Spec-driven workflow

Vix is developed specification-first. Each member crate's `spec/index.md`
describes that crate's intended behavior and is the source of truth; the code
implements it. Cross-cutting / app-level specs live at the repo-root `spec/`.

## The loop

1. **Read the spec.** Find the owning crate's `spec/index.md` (or the relevant
   root `spec/` topic — see the map below). If the change alters intended
   behavior, edit the spec first so it stays authoritative.
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

- Compare each crate's `spec/index.md` against the crate it describes (and the
  root `spec/` topics against the app shell).
- Compare `README.md` / `docs/*` against current features and crate/module names.
- Check that every action id used by a menu/palette has a `run_action` arm and an
  i18n label key.
- Check that user-facing strings go through `t!`.

Fix drift by aligning all three (code, spec, docs).

## Spec map

Feature specs live at `crates/<crate>/spec/index.md`; cross-cutting ones stay at
the repo-root `spec/`. Notable ones:

| Spec                              | Covers                                       |
| --------------------------------- | -------------------------------------------- |
| `spec/index`                      | Overview, dependency crates, build/run       |
| `crates/vix-menu/spec`                      | Menu bar structure and items                 |
| `crates/vix-keymap-model/spec`                    | Keymaps: Apple/VSCode/Emacs/Vi/Spacemacs/IntelliJ/Eclipse |
| `spec/navigation`                 | Position history, go-to-definition/symbol    |
| `crates/vix-palette/spec`            | Palette modes and behavior                   |
| `crates/vix-fileops/spec`              | Explorer tree and file ops                   |
| `crates/vix-query/spec`           | Find/replace, workspace search, query-replace |
| `crates/vix-editor/spec`                     | The editor widget (soft wrap, brackets, folds, …) |
| `crates/vix-lsp/spec`, `crates/vix-dap/spec`       | Language Server Protocol; DAP debugger        |
| `crates/vix-git/spec/git-integration`            | Git status/diff/staging/conflicts            |
| `spec/tools`                      | Tools menu (convert/generate/checksum/…)      |
| `crates/vix-snippets/spec`, `crates/vix-media-type/spec` | JSON snippet scopes; the media-type catalog |
| `crates/vix-org/spec`, `crates/vix-edit-sql/spec`       | Org-mode editing; the SQL edit surface        |
| `crates/vix-theme/spec`                     | Theme model + custom JSON format             |
| `crates/vix-i18n/spec`               | Internationalization / languages             |
| `spec/rust-clippy-pedantic`       | `clippy::pedantic` on all targets            |
| `spec/comparisons`, `spec/test`   | Editor comparisons; manual test scenarios    |
