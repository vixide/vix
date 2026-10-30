# Spec-driven workflow

Vix is developed specification-first. Each member crate's `spec/index.md`
describes that crate's intended behavior and is the source of truth; the code
implements it. Cross-cutting / app-level specs live at the repo-root `spec/`.

## The loop

1. **Read the spec.** Find the owning crate's `spec/index.md` (or the relevant
   root `spec/` topic — see the map below). If the change alters intended
   behavior, edit the spec first so it stays authoritative.
2. **Implement** in the smallest fitting module. Keep editing/state logic in the
   library; keep rendering in `tui/src/ui.rs`.
3. **Internationalize** any new user-facing text: add the key(s) to
   the right `crates/vix-i18n/locales/*.yml` file (English at minimum; other locales fall back to English)
   and render with `t!`.
4. **Document** every new public item (the build denies missing docs).
5. **Test**: extend `tui/tests/integration/main.rs` (its `mod`s: `common.rs` for
   shared fixtures, plus one file per area — `ai`, `catalog`, `compare`,
   `cowork`, `db`, `editing`, `find`, `git`, `ide_parity`, `im`,
   `keybindings`, `keymaps`, `large_file`, `lsp`, `mcp`, `menu`, `modal`,
   `org`, `palette`, `panels`, `save`, `scripting`, `themes`, `workspace`,
   and others; `ls tui/tests/integration/`) or a module's unit tests. Remote
   editing has its own top-level suites (`tui/tests/authority_*.rs`,
   `remote_agent.rs`, `remote_ui.rs`). A pure
   transform gets unit tests next to it; a fuzz target if it parses untrusted
   text; a Criterion benchmark if it runs per keystroke or per frame.
6. **Verify**: run the whole `scripts/check` gate (fmt, build, clippy at
   pedantic with `-D warnings`, tests, feature combinations, headless
   examples, `cargo doc` with warnings denied, generated reference docs and
   man page current, semver, `scripts/check-docs` with CSpell, per-binary
   licenses). It ends with `OK: … all passed`. Run all of it right before
   every commit you intend to land, not a hand-picked subset: `cargo doc`,
   the reference-docs diff and `check-docs` catch things clippy and the tests
   never see.
7. **Record** user-visible changes in `CHANGELOG.md`.

## Landing a change

- **One branch per change**, off `main`; merge with `git merge --no-ff` so
  the feature boundary stays in history, then delete the branch. Push only
  when the maintainer asks (`git push origin main` reaches GitHub, GitLab
  and Codeberg at once).
- **Several agents share one checkout.** Work in your own git worktree
  (`.claude/worktrees/<name>`), and merge large branches in a separate
  worktree rather than switching the shared checkout's branch under another
  session. Check `git status` and `git log` fresh before starting: another
  session may already have landed the task.
- **Never use a bare `git stash`/`git stash pop`**: the stash stack is
  shared by every worktree. Prefer a temporary WIP commit.
- **Commits are SSH-signed** (`commit.gpgsign`). The first signature in a
  session can wait on the key's passphrase; run `git commit` under a timeout
  (for example `timeout 90 git commit …`) and stop to ask if it hangs,
  rather than disabling signing.
- **AI-assisted commits** end with a `Co-Authored-By:` trailer naming the
  model; the git author and committer stay human. See
  [`AI_STATEMENT.md`](../AI_STATEMENT.md) for what an agent may do without
  asking (it may run `cargo publish`; it may not tag or cut a release).

## Writing a spec

A spec says what the feature *is*, not how the code happens to be arranged
today. Keep them in this shape:

- **One page per crate** at `crates/<crate>/spec/index.md`; a crate with several
  distinct features adds `spec/<topic>/index.md` sub-specs and links them.
- **Open with the contract**: the action ids, the menu path, and the one-sentence
  behavior. A reader should be able to stop after the first paragraph.
- **Tables for enumerable things** — items, actions, keys, settings, formats.
  Prose for the rules that are not a list.
- **Say why, where it is surprising.** A spec that records the reason for a
  decision survives the next refactor; one that restates the code does not.
- **Name the code that implements it** (module, function, host method) at the
  end, so drift is visible from either side.
- **Point at neighbors** with relative links, and keep them resolvable —
  `scripts/check-docs` fails on a broken one.

Some documentation directories carry `index.md` *and* a byte-identical
`README.md` — the doc map links the first, a forge renders the second when you
browse to the directory. Where both exist they must match: make `README.md` a
symlink (`ln -s index.md README.md`) and edit only `index.md`.
`scripts/check-docs` fails if they drift.

## Auditing for drift

Drift is when code, spec, and docs disagree. `scripts/check-docs` catches the
mechanical half — broken links and paths, a crate with no spec, a crate missing
from the crate map, a `README.md` that no longer matches its twin. The rest is a
reading job:

- Compare each crate's `spec/index.md` against the crate it describes (and the
  root `spec/` topics against the app shell).
- Compare `README.md` / `index.md` / `docs/*` against current features, crate
  names, and key bindings — bindings rot fastest.
- Check that every action id used by a menu or the palette has a `run_action`
  arm and an i18n label key, and that no `run_action` arm is unreachable.
- Check that user-facing strings go through `t!`, that every `t!` key exists in
  `crates/vix-i18n/locales/`, and that each call fills exactly the `%{name}` placeholders
  its string declares — `tui/tests/i18n_keys.rs` now gates all three, because all
  three shipped broken at least once. (`locale` is reserved by `t!`: it selects
  the target locale, so it can never be a placeholder name.)
- Check that every menu item has a `<label>.help` tooltip key; `show_menu_tooltips`
  is on by default, so a missing one is a blank hover.
- Watch for counts and versions in prose (crate count, keymap count, language
  count, toolchain floor) — they are the first thing to go stale.

Fix drift by aligning all three (code, spec, docs).

## Spec map

Feature specs live at `crates/<crate>/spec/index.md`; cross-cutting ones stay at
the repo-root `spec/`. Notable ones:

| Spec                                    | Covers                                        |
| --------------------------------------- | --------------------------------------------- |
| `spec/index`                            | Overview, dependencies, architecture, status  |
| `spec/navigation`                       | Position history, go-to-definition/symbol     |
| `spec/tools`                            | The Tools menu and which crate owns each item |
| `spec/test`                             | Test strategy: unit, integration, fuzz, bench |
| `spec/ci`                               | The gate, and the three forges that run it    |
| `spec/rust-clippy-pedantic`             | `clippy::pedantic` on all targets             |
| `spec/rust-msrv-n-minus-2`              | The MSRV policy: current stable minus two   |
| `spec/agents-directory-name-is-lowercase` | Why the guidance lives in `agents/`, lowercase |
| `spec/dependabot`                       | Dependabot: security updates + scheduled PRs  |
| `spec/comparisons`, `spec/emacs-menus`  | Editor comparisons; Emacs menu parity         |
| `spec/cargo-features`                   | Capability features (`full`, `net`, opt-in `cowork`) |
| `spec/remote-editing`, `crates/vix-authority/spec` | Remote editing over SSH; the authority slot |
| `spec/cowork`, `spec/instant-message`   | Real-time co-editing; the IM menu             |
| `spec/mcp-server`, `spec/jupyter`       | `vix --mcp`; the Jupyter design (spike only)  |
| `spec/ide-parity`, `spec/open-container` | IDE-parity features; the OC container menu   |
| `spec/worldwide`                        | No feature may favor one country or region    |
| `spec/llms-json-and-llms-txt`, `spec/monorepo-github-pages` | AI maps; the website subproject |
| `crates/vix-menu/spec`                  | Menu bar structure and every item             |
| `crates/vix-keymap-model/spec`          | The ten keymaps and how keys dispatch         |
| `crates/vix-palette/spec`               | Palette modes and behavior                    |
| `crates/vix-editor/spec`                | The editor host: tabs, splits, per-action specs |
| `crates/vix-editor-core/spec`           | The editor widget: buffer, wrap, brackets, history |
| `spec/find-and-replace`, `crates/vix-find-panel/spec` | Find/replace, workspace search, query-replace |
| `crates/vix-fileops/spec`, `crates/vix-left-dock/spec` | Explorer tree and file operations |
| `crates/vix-lsp/spec`, `crates/vix-dap/spec` | Language Server Protocol; DAP debugger   |
| `crates/vix-git/spec`                   | Git status/diff/staging/conflicts             |
| `crates/vix-db/spec`                    | The database workbench                        |
| `crates/vix-org/spec`, `crates/vix-org-table/spec`, `crates/vix-org-capture/spec` | Org editing, tables, capture |
| `crates/vix-theme/spec`, `crates/vix-i18n/spec` | Themes; internationalization          |
| `crates/vix-settings/spec`, `crates/vix-session/spec` | Settings keys; session/workspace restore |
| `crates/vix-clipboard/spec`             | Clipboard access and its test isolation       |
