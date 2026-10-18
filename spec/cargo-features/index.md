# Capability Cargo features

Vix makes some capabilities optional at build time: the **DB** workbench, inline
**image** preview, **script**ing, the integrated **terminal**, the **HTTP**
client, the **AI** assistant, **Git** and **Jujutsu** integration. Every
capability is **default-on**, so a released binary is unchanged; the features
exist for source builds that want a smaller or locked-down Vix.

This reverses T103/T111's "wired in = always compiled" decision, deliberately and
only for areas with a measured payoff (see `plan.md` § "Capability Cargo
features"): an area earns a flag when it removes many third-party crates (`sql`
48, `image` 81 before T583, `script` 9, `terminal` 10) or a whole outward surface
(`http`, `ai`, `git`, `jj`). `org`, `lsp` and `dap` are deliberately not flagged:
no payoff and the heaviest coupling.

## The table

| Feature | Owns action ids | Crates it will gate (T588+) |
| --- | --- | --- |
| `sql` | `db.*`, `tools.edit_sql` | `vix-db`, `vix-edit-sql` |
| `image` | (none; image tabs) | `image`, `ratatui-image` (43 exclusive crates; forwards to `vix-editor/image`) |
| `script` | `script.*`, `script:*` | `vix-script` |
| `terminal` | `tools.terminal` | `vix-terminal` |
| `http` | `tools.http_send` | `vix-http-client` |
| `ai` | `ai.*` | `vix-ai-core`, `-diff`, `-panel` |
| `git` | `git.*` | (surface only: no crates) |
| `jj` | `jj.*` | (surface only: no crates) |
| `full` | umbrella for every capability; `default` enables it, and whole-product tests gate on it | all of the above |
| `net` | umbrella for `http`, `ai`, `sql` | everything that can reach the network |

`syntax-*` / `lang-*` (Tree-sitter grammars) are a separate, older axis and are
unchanged.

## Rules

- **Default-on.** `default` enables `full`, which lists every capability feature. `--no-default-features`
  is a supported build.
- **Gate at the root, filter at runtime.** `cfg` lives only in the root package
  (`src/capability.rs` and, from T588, the gated modules). `vix-menu` and
  `vix-palette` stay `cfg`-free: `vix_menu::set_action_filter` hides any leaf the
  root's `capability::is_available` rejects, plus submenus and menus left empty
  and any stray separator. The palette applies the same predicate. With no
  filter set the menus are exactly as built.
- **Settings never lose data.** `Settings::save` writes back only the fields it
  knows, so a section can never be compiled out of `Settings` itself. The DB,
  DAP and org-capture types live in `vix-settings` (T584) and round-trip in
  every build.
- **A disabled id fails loudly.** `App::run_action` checks
  `Capability::for_action`; an id owned by a compiled-out capability — reached by
  keymap, a script's `bind_key`, or typed input — shows the translated
  *"X is not included in this build"* (`msg.capability_missing`, all locales).
- **Builds say what they are.** `vix --version --json` carries a `capabilities`
  array and `vix --doctor` / Help → Run Diagnostics a "build capabilities" row.
- **Prefixes are the contract.** `Capability::for_action` maps an action-id
  prefix to its capability; a new action under an owned prefix is gated for
  free. A new capability adds a feature, an enum variant, and prefix rows.

## Status

T586 (this framework) adds the features as empty (`sql = []`, …) and the runtime
filter; nothing is compiled out yet, so the build is unchanged. T587 (done) adds CI for
every feature combination (`spec/ci/index.md`); T588 (`sql`) and T589 (`image`) are done; T590–T593 attach each capability's crates and code;
T594 decides distribution.

Tests: `src/capability.rs` (prefix map), `vix-menu` (`filter_menus`), and, in a
`--no-default-features` build, `an_action_from_a_disabled_capability_warns_…`.
